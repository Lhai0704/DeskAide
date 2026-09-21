//! Presentation-only native adapters. Never sent to the assistant model.
use crate::{
    AVATAR_LABEL, SETTINGS_FILE,
    positioning::{Rect, Size, clamp_rect},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_store::StoreExt;

#[derive(Default)]
pub struct PresentationState {
    inner: Mutex<PresentationInner>,
}
#[derive(Default)]
struct PresentationInner {
    revision: u64,
    epoch: u64,
    sequence: u64,
    speech: Option<SpeechSignal>,
    received: Option<std::time::Instant>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSignal {
    session_id: String,
    turn_id: String,
    playing: bool,
    level: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    revision: u64,
    turn_id: Option<String>,
    phase: String,
    failed: bool,
    speech: Option<SpeechSignal>,
}
#[tauri::command]
pub fn begin_speech_presentation(
    window: WebviewWindow,
    state: State<'_, PresentationState>,
) -> Result<u64, String> {
    if window.label() != crate::ASSISTANT_LABEL {
        return Err("禁止访问".into());
    }
    let mut s = state.inner.lock().map_err(|e| e.to_string())?;
    s.epoch += 1;
    s.sequence = 0;
    s.speech = None;
    Ok(s.epoch)
}
#[tauri::command]
pub fn publish_speech_presentation(
    window: WebviewWindow,
    state: State<'_, PresentationState>,
    epoch: u64,
    sequence: u64,
    signal: Option<SpeechSignal>,
) -> Result<(), String> {
    if window.label() != crate::ASSISTANT_LABEL {
        return Err("禁止访问".into());
    }
    let mut s = state.inner.lock().map_err(|e| e.to_string())?;
    if epoch != s.epoch || sequence <= s.sequence {
        return Ok(());
    }
    s.sequence = sequence;
    s.received = Some(std::time::Instant::now());
    s.speech = signal.map(|mut x| {
        x.level = if x.level.is_finite() {
            x.level.clamp(0., 1.)
        } else {
            0.
        };
        x
    });
    Ok(())
}
#[tauri::command]
pub fn get_avatar_presentation(
    app: AppHandle,
    state: State<'_, PresentationState>,
) -> Result<Presentation, String> {
    let turn = crate::assistant::presentation_snapshot(&app);
    let mut s = state.inner.lock().map_err(|e| e.to_string())?;
    s.revision += 1;
    let speech = s.speech.clone().filter(|speech| {
        s.received.is_some_and(|time| time.elapsed().as_secs() < 5)
            && turn.as_ref().is_none_or(|t| {
                t.phase == deskaide_assistant_core::TurnPhase::Terminal
                    || t.turn_id == speech.turn_id
            })
    });
    Ok(Presentation {
        revision: s.revision,
        turn_id: turn.as_ref().map(|t| t.turn_id.clone()),
        phase: turn
            .as_ref()
            .map(|t| {
                serde_json::to_value(t.phase)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("terminal")
                    .to_owned()
            })
            .unwrap_or("terminal".into()),
        failed: turn.as_ref().is_some_and(|t| t.error.is_some()),
        speech,
    })
}
pub fn root(app: &AppHandle) -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../.local/live2d"));
    }
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("live2d"))
}
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains(['\\', ':', '%', '?', '#'])
        || relative
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("不安全的资源路径".into());
    }
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    let path = base
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(&base) || !path.is_file() {
        return Err("资源超出本地目录".into());
    }
    Ok(path)
}
pub fn resolve_resource(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = resolve(root, relative)?;
    let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("");
    let allowed = if relative.starts_with("packs/") {
        matches!(
            extension,
            "json" | "moc3" | "png" | "jpg" | "jpeg" | "webp" | "webm" | "mp4"
        )
    } else {
        matches!(
            relative,
            "runtime/bridge.js" | "runtime/live2dcubismcore.min.js"
        ) || (relative.starts_with("runtime/shaders/") && matches!(extension, "vert" | "frag"))
    };
    if !allowed {
        return Err("不允许的资源类型".into());
    }
    if path.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("资源超过大小限制".into());
    }
    Ok(path)
}
pub fn resolve_resource_uri(root: &Path, encoded: &str) -> Result<PathBuf, String> {
    let relative = percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .map_err(|e| e.to_string())?;
    resolve_resource(root, &relative)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPacks {
    directory: String,
    packs: Vec<String>,
    runtime_ready: bool,
}
#[tauri::command]
pub fn list_local_avatar_packs(app: AppHandle) -> Result<LocalPacks, String> {
    let root = root(&app)?;
    let mut packs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("packs")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && resolve(&root, &format!("packs/{name}/manifest.json")).is_ok()
            {
                packs.push(name);
            }
        }
    }
    packs.sort();
    Ok(LocalPacks {
        directory: root.to_string_lossy().to_string(),
        runtime_ready: resolve(&root, "runtime/bridge.js").is_ok()
            && resolve(&root, "runtime/live2dcubismcore.min.js").is_ok(),
        packs,
    })
}
#[tauri::command]
pub fn open_avatar_directory(app: AppHandle) -> Result<(), String> {
    let path = root(&app)?.join("packs");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn get_avatar_settings(app: AppHandle) -> Result<serde_json::Value, String> {
    Ok(app
        .store(SETTINGS_FILE)
        .map_err(|e| e.to_string())?
        .get("avatar")
        .unwrap_or(serde_json::json!({"packId":null,"preferences":{}})))
}
#[tauri::command]
pub fn save_avatar_settings(app: AppHandle, value: serde_json::Value) -> Result<(), String> {
    if !value.is_object() || serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("形象设置无效".into());
    }
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    store.set("avatar", value);
    store.save().map_err(|e| e.to_string())?;
    app.emit_to(AVATAR_LABEL, "avatar-settings-changed", ())
        .map_err(|e| e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSample {
    x: i32,
    y: i32,
    origin_x: i32,
    origin_y: i32,
    scale: f64,
}
#[tauri::command]
pub fn sample_avatar_cursor(window: WebviewWindow) -> Result<Option<CursorSample>, String> {
    if window.label() != AVATAR_LABEL || !window.is_visible().unwrap_or(false) {
        return Ok(None);
    }
    #[cfg(windows)]
    {
        use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }.map_err(|e| e.to_string())?;
        let origin = window.inner_position().map_err(|e| e.to_string())?;
        Ok(Some(CursorSample {
            x: p.x,
            y: p.y,
            origin_x: origin.x,
            origin_y: origin.y,
            scale: window.scale_factor().map_err(|e| e.to_string())?,
        }))
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}
#[tauri::command]
pub fn resize_avatar(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("无效窗口尺寸".into());
    }
    let w = app
        .get_webview_window(AVATAR_LABEL)
        .ok_or("形象窗口不存在")?;
    let monitor = w
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("显示器不存在")?;
    let area = monitor.work_area();
    let dpi = w.scale_factor().map_err(|e| e.to_string())?;
    let size = Size {
        width: ((width.min(360.) * dpi).round() as u32).min(area.size.width),
        height: ((height.min(480.) * dpi).round() as u32).min(area.size.height),
    };
    let old = w.outer_size().map_err(|e| e.to_string())?;
    let p = w.outer_position().map_err(|e| e.to_string())?;
    let pos = clamp_rect(
        (
            p.x + (old.width as i32 - size.width as i32) / 2,
            p.y + old.height as i32 - size.height as i32,
        ),
        size,
        Rect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        },
    );
    w.set_size(tauri::PhysicalSize::new(size.width, size.height))
        .map_err(|e| e.to_string())?;
    w.set_position(tauri::PhysicalPosition::new(pos.0, pos.1))
        .map_err(|e| e.to_string())?;
    crate::position_assistant(&app)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_paths() {
        let d = tempfile::tempdir().unwrap();
        for p in ["../a", "/a", "a\\b", "C:/a", "%2e%2e/a", "a//b"] {
            assert!(resolve(d.path(), p).is_err());
        }
    }
    #[test]
    fn resolves_only_existing_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("m.json"), b"{}").unwrap();
        assert!(resolve(d.path(), "m.json").is_ok());
        assert!(resolve(d.path(), "missing").is_err());
    }
    #[test]
    fn packs_cannot_supply_executable_resources() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("packs/a")).unwrap();
        std::fs::write(d.path().join("packs/a/model.json"), b"{}").unwrap();
        std::fs::write(d.path().join("packs/a/script.js"), b"alert(1)").unwrap();
        assert!(resolve_resource(d.path(), "packs/a/model.json").is_ok());
        assert!(resolve_resource(d.path(), "packs/a/script.js").is_err());
        std::fs::write(d.path().join("packs/a/角色.json"), b"{}").unwrap();
        assert!(resolve_resource_uri(d.path(), "packs/a/%E8%A7%92%E8%89%B2.json").is_ok());
        for path in ["packs/%2e%2e/secret.json", "packs/%252e%252e/secret.json"] {
            assert!(resolve_resource_uri(d.path(), path).is_err());
        }
    }
}
