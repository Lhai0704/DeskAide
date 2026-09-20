use serde::{Deserialize, Serialize};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_store::StoreExt;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShortcutSettings {
    pub shortcut: String,
    pub copilot_enabled: bool,
}
impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            shortcut: "Control+Shift+Space".into(),
            copilot_enabled: true,
        }
    }
}
#[derive(Default)]
pub struct ShortcutState {
    settings: Mutex<ShortcutSettings>,
    pub error: Mutex<Option<String>>,
}
static COPILOT_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn activate(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(window) = super::get_window(&app, super::ASSISTANT_LABEL) else {
            return;
        };
        if window.is_visible().unwrap_or(false) {
            let _ = window.set_focus();
        } else {
            let state = app.state::<super::AppState>();
            if let Err(error) = super::toggle_assistant(app.clone(), state).await {
                eprintln!("shortcut activation failed: {error}");
            }
        }
    });
}

fn parse(value: &str) -> Result<Shortcut, String> {
    let shortcut: Shortcut = value.parse().map_err(|_| "快捷键格式无效".to_owned())?;
    if shortcut.mods.is_empty() {
        return Err("请至少包含 Ctrl、Alt、Shift 或 Win 中的一个修饰键".into());
    }
    if shortcut.key == Code::F23 && shortcut.mods == (Modifiers::SUPER | Modifiers::SHIFT) {
        return Err("Copilot 键请使用上方开关设置，备用键请选择其他组合".into());
    }
    Ok(shortcut)
}

#[tauri::command]
pub fn get_shortcut_settings(state: State<'_, ShortcutState>) -> ShortcutSettings {
    state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}
#[tauri::command]
pub fn get_shortcut_error(state: State<'_, ShortcutState>) -> Option<String> {
    state
        .error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}
#[tauri::command]
pub fn save_shortcut_settings(
    app: AppHandle,
    state: State<'_, ShortcutState>,
    settings: ShortcutSettings,
) -> Result<(), String> {
    let next = parse(settings.shortcut.trim())?;
    let mut current = state.settings.lock().unwrap_or_else(|e| e.into_inner());
    let previous = parse(&current.shortcut)?;
    let changed = previous != next;
    if changed || !app.global_shortcut().is_registered(next) {
        app.global_shortcut()
            .register(next)
            .map_err(|e| format!("快捷键被占用或无法注册：{e}"))?;
    }
    let store = app.store(super::SETTINGS_FILE).map_err(|e| e.to_string())?;
    let old_value = store.get("shortcuts");
    store.set(
        "shortcuts",
        serde_json::to_value(&settings).map_err(|e| e.to_string())?,
    );
    if let Err(error) = store.save() {
        if let Some(value) = old_value {
            store.set("shortcuts", value);
        } else {
            store.delete("shortcuts");
        }
        if changed {
            let _ = app.global_shortcut().unregister(next);
        }
        return Err(error.to_string());
    }
    if changed {
        let _ = app.global_shortcut().unregister(previous);
    }
    COPILOT_ENABLED.store(settings.copilot_enabled, Ordering::SeqCst);
    *current = settings;
    Ok(())
}

pub fn setup(app: &AppHandle) -> Result<(), String> {
    let store = app.store(super::SETTINGS_FILE).map_err(|e| e.to_string())?;
    let settings: ShortcutSettings = store
        .get("shortcuts")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let state = app.state::<ShortcutState>();
    if let Err(error) = parse(&settings.shortcut)
        .and_then(|s| app.global_shortcut().register(s).map_err(|e| e.to_string()))
    {
        *state.error.lock().unwrap() = Some(format!("备用快捷键注册失败：{error}"));
    }
    COPILOT_ENABLED.store(settings.copilot_enabled, Ordering::SeqCst);
    *state.settings.lock().unwrap() = settings;
    #[cfg(windows)]
    if let Err(error) = copilot::start(app.clone()) {
        COPILOT_ENABLED.store(false, Ordering::SeqCst);
        *state.error.lock().unwrap() = Some(format!("Copilot 键监听失败：{error}"));
    }
    Ok(())
}

#[cfg(windows)]
mod copilot {
    use super::*;
    use std::sync::{OnceLock, mpsc};
    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN, VK_SHIFT},
            WindowsAndMessaging::*,
        },
    };
    static SIGNAL: OnceLock<mpsc::Sender<()>> = OnceLock::new();
    static DOWN: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            // Windows owns this pointer for the duration of the hook callback.
            let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if key.vkCode == 0x86 {
                // F23: the Copilot key emits Win+Shift+F23.
                let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
                if up && DOWN.swap(false, Ordering::SeqCst) {
                    return LRESULT(1);
                }
                let modifiers = unsafe {
                    GetAsyncKeyState(VK_SHIFT.0 as i32) < 0
                        && (GetAsyncKeyState(VK_LWIN.0 as i32) < 0
                            || GetAsyncKeyState(VK_RWIN.0 as i32) < 0)
                };
                if !up
                    && (DOWN.load(Ordering::SeqCst)
                        || (COPILOT_ENABLED.load(Ordering::SeqCst) && modifiers))
                {
                    if !DOWN.swap(true, Ordering::SeqCst) {
                        if let Some(sender) = SIGNAL.get() {
                            let _ = sender.send(());
                        }
                    }
                    return LRESULT(1);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    pub fn start(app: AppHandle) -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        SIGNAL.set(sender).map_err(|_| "监听已启动".to_owned())?;
        std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                activate(&app);
            }
        });
        let (ready, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || unsafe {
            let hook_handle = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    return;
                }
            };
            let _ = ready.send(Ok(()));
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = UnhookWindowsHookEx(hook_handle);
        });
        result.recv().map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_requires_modifier_and_valid_key() {
        assert!(parse("Control+Shift+Space").is_ok());
        assert!(parse("F23").is_err());
        assert!(parse("nonsense").is_err());
        assert!(parse("Super+Shift+F23").is_err());
    }
}
