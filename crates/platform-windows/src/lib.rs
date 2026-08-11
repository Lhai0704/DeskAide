//! Windows foreground-window tracking and accessibility integration.

use std::{
    collections::HashSet,
    path::Path,
    sync::mpsc::{self, Sender},
    time::Duration,
};

use async_trait::async_trait;
use deskaide_assistant_core::TargetWindow;
use deskaide_context_core::{CapturedImage, DisplayTarget, PlatformError, PlatformIntegration};
use tokio::sync::oneshot;
use uiautomation::{
    UIAutomation, UIElement,
    patterns::{UITextPattern, UIValuePattern},
    types::{Handle, TreeScope},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HGLOBAL, HWND, LPARAM},
        System::{
            DataExchange::{
                CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
            },
            Memory::{GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongW,
            GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
            WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
        },
    },
    core::{BOOL, PCWSTR, PWSTR},
};

const PLATFORM: &str = "windows";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RAW_TEXT_CHARS: usize = 64_000;
const MAX_ANCESTORS: usize = 16;
const MAX_TEXT_ELEMENTS: usize = 256;

pub struct WindowsPlatformIntegration {
    sender: Sender<WorkerCommand>,
}

impl std::fmt::Debug for WindowsPlatformIntegration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowsPlatformIntegration")
            .finish_non_exhaustive()
    }
}

impl Default for WindowsPlatformIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPlatformIntegration {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("deskaide-uia".to_owned())
            .spawn(move || worker_loop(receiver))
            .expect("failed to start the Windows accessibility worker");
        Self { sender }
    }

    async fn text_request(
        &self,
        target: &TargetWindow,
        kind: TextRequestKind,
    ) -> Result<Option<String>, PlatformError> {
        let capability = kind.capability();
        let (reply, response) = oneshot::channel();
        self.sender
            .send(WorkerCommand::Text {
                target: target.clone(),
                kind,
                reply,
            })
            .map_err(|_| {
                PlatformError::Integration("Windows accessibility worker is not running".to_owned())
            })?;
        receive_with_timeout(response, capability).await
    }
}

#[async_trait]
impl PlatformIntegration for WindowsPlatformIntegration {
    async fn get_last_active_window(&self) -> Result<TargetWindow, PlatformError> {
        let candidate = foreground_candidate();
        let (reply, response) = oneshot::channel();
        self.sender
            .send(WorkerCommand::Snapshot { candidate, reply })
            .map_err(|_| {
                PlatformError::Integration("Windows accessibility worker is not running".to_owned())
            })?;
        receive_with_timeout(response, "get_last_active_window").await
    }

    async fn list_windows(&self) -> Result<Vec<TargetWindow>, PlatformError> {
        enumerate_windows()
    }

    async fn get_selected_text(
        &self,
        target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError> {
        self.text_request(target, TextRequestKind::Selected).await
    }

    async fn get_accessible_text(
        &self,
        target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError> {
        self.text_request(target, TextRequestKind::Accessible).await
    }

    async fn get_clipboard_text(&self) -> Result<Option<String>, PlatformError> {
        read_clipboard_text()
    }

    async fn capture_window(&self, _target: &TargetWindow) -> Result<CapturedImage, PlatformError> {
        unsupported("capture_window")
    }

    async fn capture_screen(
        &self,
        _display: DisplayTarget,
    ) -> Result<CapturedImage, PlatformError> {
        unsupported("capture_screen")
    }

    async fn select_region(&self) -> Result<CapturedImage, PlatformError> {
        unsupported("select_region")
    }
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn read_clipboard_text() -> Result<Option<String>, PlatformError> {
    unsafe {
        OpenClipboard(None).map_err(|error| PlatformError::Unavailable {
            platform: PLATFORM,
            capability: "clipboard_text",
            reason: format!("无法打开剪贴板：{error}"),
        })?;
        let _guard = ClipboardGuard;

        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return Ok(None);
        }

        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32)
            .map_err(|error| PlatformError::Integration(format!("读取剪贴板文字失败：{error}")))?;
        let memory = HGLOBAL(handle.0);
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            return Err(PlatformError::Integration(
                "无法访问剪贴板文字内存".to_owned(),
            ));
        }
        let text = PCWSTR(pointer.cast())
            .to_string()
            .map_err(|error| PlatformError::Integration(format!("解析剪贴板文字失败：{error}")));
        let _ = GlobalUnlock(memory);
        text.map(Some)
    }
}

async fn receive_with_timeout<T>(
    response: oneshot::Receiver<Result<T, PlatformError>>,
    capability: &'static str,
) -> Result<T, PlatformError> {
    match tokio::time::timeout(REQUEST_TIMEOUT, response).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(PlatformError::Integration(
            "Windows accessibility worker stopped unexpectedly".to_owned(),
        )),
        Err(_) => Err(PlatformError::Timeout {
            platform: PLATFORM,
            capability,
        }),
    }
}

enum WorkerCommand {
    Snapshot {
        candidate: Option<WindowCandidate>,
        reply: oneshot::Sender<Result<TargetWindow, PlatformError>>,
    },
    Text {
        target: TargetWindow,
        kind: TextRequestKind,
        reply: oneshot::Sender<Result<Option<String>, PlatformError>>,
    },
}

#[derive(Clone, Copy)]
enum TextRequestKind {
    Selected,
    Accessible,
}

impl TextRequestKind {
    fn capability(self) -> &'static str {
        match self {
            Self::Selected => "get_selected_text",
            Self::Accessible => "get_accessible_text",
        }
    }
}

#[derive(Debug)]
struct WindowCandidate {
    hwnd: isize,
    process_id: u32,
    target: TargetWindow,
}

struct TrackedWindow {
    target: TargetWindow,
    process_id: u32,
    root: UIElement,
    focused: Option<UIElement>,
}

fn worker_loop(receiver: mpsc::Receiver<WorkerCommand>) {
    let automation = match UIAutomation::new() {
        Ok(automation) => automation,
        Err(error) => {
            let message = format!("failed to initialize Windows UI Automation: {error}");
            for command in receiver {
                match command {
                    WorkerCommand::Snapshot { reply, .. } => {
                        let _ = reply.send(Err(PlatformError::Integration(message.clone())));
                    }
                    WorkerCommand::Text { reply, .. } => {
                        let _ = reply.send(Err(PlatformError::Integration(message.clone())));
                    }
                }
            }
            return;
        }
    };
    let mut tracked: Option<TrackedWindow> = None;

    for command in receiver {
        match command {
            WorkerCommand::Snapshot { candidate, reply } => {
                if let Some(candidate) = candidate {
                    match track_candidate(&automation, candidate) {
                        Ok(next) => tracked = Some(next),
                        Err(error) => {
                            let _ = reply.send(Err(error));
                            continue;
                        }
                    }
                }
                let result = tracked
                    .as_ref()
                    .map(|window| window.target.clone())
                    .ok_or_else(|| {
                        unavailable(
                            "get_last_active_window",
                            "尚未记录到 DeskAide 之外的活动窗口",
                        )
                    });
                let _ = reply.send(result);
            }
            WorkerCommand::Text {
                target,
                kind,
                reply,
            } => {
                let result = collect_target_text(&automation, tracked.as_ref(), &target, kind);
                let _ = reply.send(result);
            }
        }
    }
}

fn track_candidate(
    automation: &UIAutomation,
    candidate: WindowCandidate,
) -> Result<TrackedWindow, PlatformError> {
    let root = automation
        .element_from_handle(Handle::from(candidate.hwnd))
        .map_err(|error| {
            PlatformError::Integration(format!("failed to inspect the target window: {error}"))
        })?;
    let focused = automation
        .get_focused_element()
        .ok()
        .filter(|element| element.get_process_id().ok() == Some(candidate.process_id));
    Ok(TrackedWindow {
        target: candidate.target,
        process_id: candidate.process_id,
        root,
        focused,
    })
}

fn collect_target_text(
    automation: &UIAutomation,
    tracked: Option<&TrackedWindow>,
    target: &TargetWindow,
    kind: TextRequestKind,
) -> Result<Option<String>, PlatformError> {
    let temporary;
    let window = if let Some(window) = tracked.filter(|window| window.target.id == target.id) {
        window
    } else if matches!(kind, TextRequestKind::Accessible) {
        temporary = track_candidate(automation, candidate_from_target(target)?)?;
        &temporary
    } else {
        return Err(unavailable(
            kind.capability(),
            "助手激活前窗口中的选中文字已失效",
        ));
    };
    let text = match kind {
        TextRequestKind::Selected => selected_text(automation, window),
        TextRequestKind::Accessible => accessible_text(automation, window),
    };
    Ok(text.filter(|value| !value.trim().is_empty()))
}

fn selected_text(automation: &UIAutomation, window: &TrackedWindow) -> Option<String> {
    for element in element_chain(automation, window) {
        let Ok(pattern) = element.get_pattern::<UITextPattern>() else {
            continue;
        };
        let Ok(ranges) = pattern.get_selection() else {
            continue;
        };
        let mut parts = Vec::new();
        let mut remaining = MAX_RAW_TEXT_CHARS;
        for range in ranges {
            if remaining == 0 {
                break;
            }
            if let Ok(text) = range.get_text((remaining + 1).min(i32::MAX as usize) as i32) {
                let text = normalize_text(&text);
                if !text.is_empty() {
                    remaining = remaining.saturating_sub(text.chars().count());
                    parts.push(text);
                }
            }
        }
        if !parts.is_empty() {
            return Some(truncate_chars(&parts.join("\n"), MAX_RAW_TEXT_CHARS).0);
        }
    }
    None
}

fn accessible_text(automation: &UIAutomation, window: &TrackedWindow) -> Option<String> {
    let chain = element_chain(automation, window);
    for element in &chain {
        if let Some(text) = document_text(element, MAX_RAW_TEXT_CHARS) {
            return Some(text);
        }
    }
    for element in &chain {
        if let Ok(pattern) = element.get_pattern::<UIValuePattern>()
            && let Ok(value) = pattern.get_value()
        {
            let value = normalize_text(&value);
            if !value.is_empty() {
                return Some(truncate_chars(&value, MAX_RAW_TEXT_CHARS).0);
            }
        }
    }

    let condition = automation.create_true_condition().ok()?;
    let elements = window
        .root
        .find_all(TreeScope::Descendants, &condition)
        .ok()?;
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    let mut remaining = MAX_RAW_TEXT_CHARS;
    for element in elements.into_iter().take(MAX_TEXT_ELEMENTS) {
        if remaining == 0 {
            break;
        }
        let Some(text) = document_text(&element, remaining) else {
            continue;
        };
        if seen.insert(text.clone()) {
            remaining = remaining.saturating_sub(text.chars().count());
            parts.push(text);
        }
    }
    (!parts.is_empty()).then(|| truncate_chars(&parts.join("\n\n"), MAX_RAW_TEXT_CHARS).0)
}

fn element_chain(automation: &UIAutomation, window: &TrackedWindow) -> Vec<UIElement> {
    let mut elements = Vec::new();
    let mut current = window
        .focused
        .clone()
        .unwrap_or_else(|| window.root.clone());
    let walker = automation.get_control_view_walker().ok();
    for _ in 0..MAX_ANCESTORS {
        if current.get_process_id().ok() != Some(window.process_id) {
            break;
        }
        elements.push(current.clone());
        if automation
            .compare_elements(&current, &window.root)
            .unwrap_or(false)
        {
            break;
        }
        let Some(parent) = walker
            .as_ref()
            .and_then(|walker| walker.get_parent(&current).ok())
        else {
            break;
        };
        current = parent;
    }
    if !elements.iter().any(|element| {
        automation
            .compare_elements(element, &window.root)
            .unwrap_or(false)
    }) {
        elements.push(window.root.clone());
    }
    elements
}

fn document_text(element: &UIElement, max_chars: usize) -> Option<String> {
    let pattern = element.get_pattern::<UITextPattern>().ok()?;
    let range = pattern.get_document_range().ok()?;
    let text = range
        .get_text((max_chars + 1).min(i32::MAX as usize) as i32)
        .ok()?;
    let text = normalize_text(&text);
    (!text.is_empty()).then(|| truncate_chars(&text, max_chars).0)
}

fn foreground_candidate() -> Option<WindowCandidate> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if !should_track_process(process_id, std::process::id()) {
        return None;
    }

    Some(window_candidate(hwnd, process_id))
}

fn enumerate_windows() -> Result<Vec<TargetWindow>, PlatformError> {
    unsafe extern "system" fn callback(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(parameter.0 as *mut Vec<TargetWindow>) };
        if !is_selectable_window(hwnd) {
            return BOOL::from(true);
        }
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if should_track_process(process_id, std::process::id()) {
            windows.push(window_candidate(hwnd, process_id).target);
        }
        BOOL::from(true)
    }

    let mut windows = Vec::new();
    unsafe { EnumWindows(Some(callback), LPARAM(&mut windows as *mut _ as isize)) }.map_err(
        |error| PlatformError::Integration(format!("failed to enumerate windows: {error}")),
    )?;
    Ok(windows)
}

fn is_selectable_window(hwnd: HWND) -> bool {
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { GetWindowTextLengthW(hwnd) } <= 0 {
        return false;
    }
    let extended_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    if extended_style & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    let has_owner = unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok();
    !has_owner || extended_style & WS_EX_APPWINDOW.0 != 0
}

fn window_candidate(hwnd: HWND, process_id: u32) -> WindowCandidate {
    let title = window_title(hwnd);
    let process_path = process_path(process_id);
    let process_name = process_path.as_ref().and_then(|path| {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    });
    let application_name = process_name.as_ref().map(|name| {
        Path::new(name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    let hwnd_value = hwnd.0.addr() as isize;
    WindowCandidate {
        hwnd: hwnd_value,
        process_id,
        target: TargetWindow {
            id: format!("windows-hwnd:{hwnd_value:X}"),
            application_name,
            process_name,
            title,
        },
    }
}

fn candidate_from_target(target: &TargetWindow) -> Result<WindowCandidate, PlatformError> {
    let value = target
        .id
        .strip_prefix("windows-hwnd:")
        .and_then(|value| isize::from_str_radix(value, 16).ok())
        .ok_or_else(|| unavailable("get_accessible_text", "目标窗口标识无效"))?;
    let hwnd = HWND(value as *mut _);
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id == 0 {
        return Err(unavailable("get_accessible_text", "目标窗口已经关闭"));
    }
    Ok(WindowCandidate {
        hwnd: value,
        process_id,
        target: target.clone(),
    })
}

fn should_track_process(process_id: u32, own_process_id: u32) -> bool {
    process_id != 0 && process_id != own_process_id
}

fn window_title(hwnd: HWND) -> Option<String> {
    let mut buffer = vec![0_u16; 2_048];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn process_path(process_id: u32) -> Option<String> {
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    result
        .ok()
        .map(|_| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches(['\0', ' ', '\t', '\n'])
        .to_owned()
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    let mut chars = text.chars();
    let result: String = chars.by_ref().take(max_chars).collect();
    let truncated = chars.next().is_some();
    (result, truncated)
}

fn unavailable(capability: &'static str, reason: impl Into<String>) -> PlatformError {
    PlatformError::Unavailable {
        platform: PLATFORM,
        capability,
        reason: reason.into(),
    }
}

fn unsupported<T>(capability: &'static str) -> Result<T, PlatformError> {
    Err(PlatformError::Unsupported {
        platform: PLATFORM,
        capability,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_own_and_invalid_processes() {
        assert!(!should_track_process(0, 42));
        assert!(!should_track_process(42, 42));
        assert!(should_track_process(43, 42));
    }

    #[test]
    fn truncates_on_unicode_character_boundaries() {
        assert_eq!(truncate_chars("助手ABC", 3), ("助手A".to_owned(), true));
        assert_eq!(truncate_chars("助手", 3), ("助手".to_owned(), false));
    }

    #[test]
    fn normalizes_platform_newlines_and_padding() {
        assert_eq!(normalize_text(" \r\nhello\r\n "), "hello");
    }
}
