//! Live2D-only click-through. The page publishes a coarse mask a few times a second.
//! This loop reads the cursor about 20 times a second and toggles WS_EX_TRANSPARENT
//! only when the result changes. No pixel readback, global hook, or window subclass.
use std::time::Duration;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::AVATAR_LABEL;

const HIT_INTERVAL: Duration = Duration::from_millis(50);
const IDLE_INTERVAL: Duration = Duration::from_millis(250);
const WS_EX_LAYERED: i32 = 0x0008_0000;
const WS_EX_TRANSPARENT: i32 = 0x0000_0020;

#[derive(Default)]
pub struct ClickThroughState {
    inner: std::sync::Mutex<ClickThrough>,
}

#[derive(Default)]
struct ClickThrough {
    seq: u64,
    mask: Option<HitMask>,
    /// Last style we applied. The window starts without WS_EX_TRANSPARENT.
    passthrough: bool,
}

struct HitMask {
    cols: u16,
    rows: u16,
    bits: Vec<u8>,
}

fn point_hits(mask: &HitMask, x: f64, y: f64, width: f64, height: f64) -> bool {
    if !(width > 0. && height > 0.) || x < 0. || y < 0. || x >= width || y >= height {
        return false;
    }
    let col = ((x / width) * f64::from(mask.cols)).floor() as usize;
    let row = ((y / height) * f64::from(mask.rows)).floor() as usize;
    let col = col.min(mask.cols as usize - 1);
    let row = row.min(mask.rows as usize - 1);
    let index = row * mask.cols as usize + col;
    mask.bits
        .get(index / 8)
        .is_some_and(|byte| byte & (1 << (index % 8)) != 0)
}

fn with_passthrough(style: i32, passthrough: bool) -> i32 {
    // This DWM WebView ignores WS_EX_TRANSPARENT unless WS_EX_LAYERED is also set.
    // Layered stays on so toggling hit-testing does not drop the character.
    let style = style | WS_EX_LAYERED;
    if passthrough {
        style | WS_EX_TRANSPARENT
    } else {
        style & !WS_EX_TRANSPARENT
    }
}

fn parse_mask(cols: u16, rows: u16, bits: Vec<u8>) -> Result<HitMask, String> {
    if !(1..=64).contains(&cols) || !(1..=96).contains(&rows) {
        return Err("命中遮罩尺寸无效".into());
    }
    let bytes = (cols as usize * rows as usize).div_ceil(8);
    if bits.len() != bytes {
        return Err("命中遮罩长度无效".into());
    }
    Ok(HitMask { cols, rows, bits })
}

fn lock(state: &ClickThroughState) -> std::sync::MutexGuard<'_, ClickThrough> {
    state
        .inner
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

#[tauri::command]
pub fn set_avatar_hit_mask(
    window: WebviewWindow,
    state: State<'_, ClickThroughState>,
    seq: u64,
    cols: u16,
    rows: u16,
    bits: Vec<u8>,
) -> Result<(), String> {
    if window.label() != AVATAR_LABEL {
        return Err("禁止访问".into());
    }
    if seq == 0 {
        return Err("命中序号无效".into());
    }
    let mask = parse_mask(cols, rows, bits)?;
    let mut guard = lock(&state);
    if seq < guard.seq {
        return Ok(());
    }
    guard.seq = seq;
    guard.mask = Some(mask);
    Ok(())
}

#[tauri::command]
pub fn clear_avatar_hit_mask(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, ClickThroughState>,
    seq: u64,
) -> Result<(), String> {
    if window.label() != AVATAR_LABEL {
        return Err("禁止访问".into());
    }
    if seq == 0 {
        return Err("命中序号无效".into());
    }
    {
        let mut guard = lock(&state);
        if seq < guard.seq {
            return Ok(());
        }
        guard.seq = seq;
        guard.mask = None;
    }
    if let Some(avatar) = app.get_webview_window(AVATAR_LABEL) {
        apply_passthrough(&app, &avatar, false);
    }
    Ok(())
}

#[tauri::command]
pub fn set_avatar_passthrough(
    app: AppHandle,
    window: WebviewWindow,
    passthrough: bool,
) -> Result<(), String> {
    if window.label() != AVATAR_LABEL {
        return Err("禁止访问".into());
    }
    // A release that raced a move back onto the character must not drop the press.
    if passthrough
        && (app
            .state::<crate::AppState>()
            .avatar_interacting
            .load(std::sync::atomic::Ordering::SeqCst)
            || cursor_hits_model(&app, &window) != Some(false))
    {
        return Ok(());
    }
    apply_passthrough(&app, &window, passthrough);
    Ok(())
}

pub fn capture_for_interaction(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(AVATAR_LABEL) {
        apply_passthrough(app, &window, false);
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let tick_app = app.clone();
            if app
                .run_on_main_thread(move || {
                    let _ = tx.send(tick(&tick_app));
                })
                .is_err()
            {
                break;
            }
            let Ok(wait) = rx.await else { break };
            tokio::time::sleep(wait).await;
        }
    });
}

fn tick(app: &AppHandle) -> Duration {
    let Some(window) = app.get_webview_window(AVATAR_LABEL) else {
        return IDLE_INTERVAL;
    };
    if !window.is_visible().unwrap_or(false) {
        return IDLE_INTERVAL;
    }
    let mask = lock(&app.state::<ClickThroughState>()).mask.is_some();
    if !mask {
        apply_passthrough(app, &window, false);
        return IDLE_INTERVAL;
    }
    if app
        .state::<crate::AppState>()
        .avatar_interacting
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        apply_passthrough(app, &window, false);
        return HIT_INTERVAL;
    }
    let passthrough = cursor_hits_model(app, &window) == Some(false);
    apply_passthrough(app, &window, passthrough);
    HIT_INTERVAL
}

/// `Some(true)` is on the model, `Some(false)` is over this window's empty area,
/// `None` when the cursor or the mask is unavailable.
fn cursor_hits_model(app: &AppHandle, window: &WebviewWindow) -> Option<bool> {
    let (x, y, width, height) = client_cursor(window)?;
    let state = app.state::<ClickThroughState>();
    let guard = lock(&state);
    let mask = guard.mask.as_ref()?;
    Some(point_hits(mask, x, y, width, height))
}

fn apply_passthrough(app: &AppHandle, window: &WebviewWindow, passthrough: bool) {
    let state = app.state::<ClickThroughState>();
    if lock(&state).passthrough == passthrough {
        return;
    }
    if set_window_passthrough(window, passthrough).is_err() {
        return;
    }
    lock(&state).passthrough = passthrough;
}

fn client_cursor(window: &WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    #[cfg(windows)]
    {
        use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.ok()?;
        let origin = window.inner_position().ok()?;
        let size = window.inner_size().ok()?;
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some((
            f64::from(cursor.x - origin.x),
            f64::from(cursor.y - origin.y),
            f64::from(size.width),
            f64::from(size.height),
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        None
    }
}

fn set_window_passthrough(window: &WebviewWindow, passthrough: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                GWL_EXSTYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOSIZE, SWP_NOZORDER, SetWindowLongW, SetWindowPos,
            },
        };
        // Tauri links a different windows crate; both HWNDs are a public pointer.
        let foreign = window.hwnd().map_err(|error| error.to_string())?;
        let hwnd = HWND(foreign.0);
        unsafe {
            let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let next = with_passthrough(current, passthrough);
            if next != current {
                SetWindowLongW(hwnd, GWL_EXSTYLE, next);
                // The hit-test cache ignores the new extended style until the frame is refreshed.
                SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (window, passthrough);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(cols: u16, rows: u16, bits: Vec<u8>) -> HitMask {
        parse_mask(cols, rows, bits).unwrap()
    }

    #[test]
    fn top_left_cell_is_the_low_bit() {
        let mask = mask(8, 1, vec![0b0000_0001]);
        assert!(point_hits(&mask, 0.0, 0.0, 80.0, 10.0));
        assert!(point_hits(&mask, 9.0, 0.0, 80.0, 10.0));
        assert!(!point_hits(&mask, 10.0, 0.0, 80.0, 10.0));
        assert!(!point_hits(&mask, 80.0, 0.0, 80.0, 10.0));
        assert!(!point_hits(&mask, -0.1, 0.0, 80.0, 10.0));
    }

    #[test]
    fn passthrough_toggles_hits_and_keeps_layered() {
        let noactivate = 0x0800_0000;
        let on = with_passthrough(noactivate, true);
        assert_eq!(on & WS_EX_TRANSPARENT, WS_EX_TRANSPARENT);
        assert_eq!(on & WS_EX_LAYERED, WS_EX_LAYERED);
        assert_eq!(on & noactivate, noactivate);
        let off = with_passthrough(on, false);
        assert_eq!(off & WS_EX_TRANSPARENT, 0);
        assert_eq!(off & WS_EX_LAYERED, WS_EX_LAYERED);
        assert_eq!(off & noactivate, noactivate);
        assert_eq!(
            with_passthrough(off, true) & WS_EX_TRANSPARENT,
            WS_EX_TRANSPARENT
        );
    }

    #[test]
    fn mask_bytes_must_cover_every_cell() {
        assert!(parse_mask(32, 48, vec![0; 192]).is_ok());
        assert!(parse_mask(32, 48, vec![0; 191]).is_err());
        assert!(parse_mask(0, 48, vec![]).is_err());
        assert!(parse_mask(65, 1, vec![0; 9]).is_err());
    }
}
