mod positioning;

use std::sync::{Arc, Mutex};

use deskaide_ai_provider::{MockProvider, ModelProvider};
use deskaide_assistant_core::{
    ContentBlock, GenerationOptions, MessageRole, ModelMessage, ModelRequest, ResponseEvent,
};
use deskaide_context_core::PlatformIntegration;
use positioning::{Rect, Size, assistant_position, clamp_rect};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Position, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

#[cfg(windows)]
use deskaide_platform_windows::WindowsPlatformIntegration;

const AVATAR_LABEL: &str = "avatar";
const ASSISTANT_LABEL: &str = "assistant";
const SETTINGS_FILE: &str = "settings.json";
const POSITION_KEY: &str = "avatarPosition";
const WINDOW_GAP: i32 = 12;

struct AppState {
    model_provider: Arc<dyn ModelProvider>,
    #[allow(dead_code)]
    platform: Arc<dyn PlatformIntegration>,
    movement: Mutex<MovementState>,
}

#[derive(Debug, Default)]
struct MovementState {
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedAvatarPosition {
    x: i32,
    y: i32,
    monitor_name: Option<String>,
}

impl AppState {
    #[cfg(windows)]
    fn new() -> Self {
        Self {
            model_provider: Arc::new(MockProvider::new()),
            platform: Arc::new(WindowsPlatformIntegration::new()),
            movement: Mutex::new(MovementState::default()),
        }
    }
}

#[tauri::command]
fn toggle_assistant(app: AppHandle) -> Result<(), String> {
    let assistant = get_window(&app, ASSISTANT_LABEL)?;
    if assistant.is_visible().map_err(display_error)? {
        assistant.hide().map_err(display_error)
    } else {
        position_assistant(&app)?;
        assistant.show().map_err(display_error)?;
        assistant.set_focus().map_err(display_error)?;
        assistant.emit("assistant-shown", ()).map_err(display_error)
    }
}

#[tauri::command]
fn hide_assistant(app: AppHandle) -> Result<(), String> {
    get_window(&app, ASSISTANT_LABEL)?
        .hide()
        .map_err(display_error)
}

#[tauri::command]
fn submit_mock_request(
    app: AppHandle,
    state: State<'_, AppState>,
    prompt: String,
) -> Result<String, String> {
    let prompt = prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("问题不能为空".to_owned());
    }

    let request_id = Uuid::new_v4().to_string();
    let request = ModelRequest {
        request_id: request_id.clone(),
        model_profile_id: state.model_provider.id().to_owned(),
        conversation_id: "phase-one".to_owned(),
        system_prompt: Some("You are the local DeskAide phase-one mock provider.".to_owned()),
        messages: vec![ModelMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: prompt }],
        }],
        context: Vec::new(),
        generation_options: GenerationOptions::default(),
    };
    let provider = Arc::clone(&state.model_provider);
    let event_app = app.clone();
    let failed_request_id = request_id.clone();
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if let Err(error) = event_app.emit_to(ASSISTANT_LABEL, "model-response", event) {
                eprintln!("failed to emit model response event: {error}");
            }
        }
    });

    let failed_app = app;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = provider.complete(request, sender).await {
            let _ = failed_app.emit_to(
                ASSISTANT_LABEL,
                "model-response",
                ResponseEvent::Failed {
                    request_id: failed_request_id,
                    message: error.to_string(),
                },
            );
        }
    });

    Ok(request_id)
}

fn get_window(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("窗口 '{label}' 不存在"))
}

fn position_assistant(app: &AppHandle) -> Result<(), String> {
    let avatar = get_window(app, AVATAR_LABEL)?;
    let assistant = get_window(app, ASSISTANT_LABEL)?;
    let avatar_position = avatar.outer_position().map_err(display_error)?;
    let avatar_size = avatar.outer_size().map_err(display_error)?;
    let assistant_size = assistant.outer_size().map_err(display_error)?;
    let monitor = avatar
        .current_monitor()
        .map_err(display_error)?
        .or(avatar.primary_monitor().map_err(display_error)?)
        .ok_or_else(|| "无法确定当前显示器".to_owned())?;
    let work_area = monitor.work_area();
    let position = assistant_position(
        Rect {
            x: avatar_position.x,
            y: avatar_position.y,
            width: avatar_size.width,
            height: avatar_size.height,
        },
        Size {
            width: assistant_size.width,
            height: assistant_size.height,
        },
        Rect {
            x: work_area.position.x,
            y: work_area.position.y,
            width: work_area.size.width,
            height: work_area.size.height,
        },
        WINDOW_GAP,
    );

    assistant
        .set_position(Position::Physical(PhysicalPosition::new(
            position.0, position.1,
        )))
        .map_err(display_error)
}

fn restore_avatar_position(app: &AppHandle) -> Result<(), String> {
    let avatar = get_window(app, AVATAR_LABEL)?;
    let store = app.store(SETTINGS_FILE).map_err(display_error)?;
    let saved = store
        .get(POSITION_KEY)
        .and_then(|value| serde_json::from_value::<SavedAvatarPosition>(value).ok());

    let (desired, preferred_monitor) = if let Some(saved) = saved {
        ((saved.x, saved.y), saved.monitor_name)
    } else {
        let monitor = avatar
            .primary_monitor()
            .map_err(display_error)?
            .ok_or_else(|| "无法确定主显示器".to_owned())?;
        let work = monitor.work_area();
        (
            (
                work.position.x + work.size.width as i32 - 160 - 24,
                work.position.y + work.size.height as i32 - 160 - 24,
            ),
            monitor.name().cloned(),
        )
    };

    let monitors = avatar.available_monitors().map_err(display_error)?;
    let monitor = preferred_monitor
        .as_ref()
        .and_then(|name| monitors.iter().find(|monitor| monitor.name() == Some(name)))
        .cloned()
        .or(avatar.primary_monitor().map_err(display_error)?)
        .ok_or_else(|| "无法确定用于恢复位置的显示器".to_owned())?;
    let work = monitor.work_area();
    let size = avatar.outer_size().map_err(display_error)?;
    let position = clamp_rect(
        desired,
        Size {
            width: size.width,
            height: size.height,
        },
        Rect {
            x: work.position.x,
            y: work.position.y,
            width: work.size.width,
            height: work.size.height,
        },
    );

    avatar
        .set_position(Position::Physical(PhysicalPosition::new(
            position.0, position.1,
        )))
        .map_err(display_error)
}

fn handle_avatar_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    if let Ok(assistant) = get_window(app, ASSISTANT_LABEL)
        && assistant.is_visible().unwrap_or(false)
        && let Err(error) = position_assistant(app)
    {
        eprintln!("failed to follow the avatar window: {error}");
    }

    let generation = {
        let state = app.state::<AppState>();
        let mut movement = state
            .movement
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        movement.generation = movement.generation.wrapping_add(1);
        movement.generation
    };
    let delayed_app = app.clone();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        let should_save = {
            let state = delayed_app.state::<AppState>();
            let movement = state
                .movement
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            movement.generation == generation
        };
        if should_save && let Err(error) = save_avatar_position(&delayed_app, position) {
            eprintln!("failed to save avatar position: {error}");
        }
    });
}

fn save_avatar_position(app: &AppHandle, position: PhysicalPosition<i32>) -> Result<(), String> {
    let avatar = get_window(app, AVATAR_LABEL)?;
    let monitor_name = avatar
        .current_monitor()
        .map_err(display_error)?
        .and_then(|monitor| monitor.name().cloned());
    let saved = SavedAvatarPosition {
        x: position.x,
        y: position.y,
        monitor_name,
    };
    let value = serde_json::to_value(saved).map_err(display_error)?;
    let store = app.store(SETTINGS_FILE).map_err(display_error)?;
    store.set(POSITION_KEY, value);
    store.save().map_err(display_error)
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(windows)]
pub fn run() {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    let handler_shortcut = shortcut;

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, triggered, event| {
                    if triggered == &handler_shortcut
                        && event.state() == ShortcutState::Pressed
                        && let Err(error) = toggle_assistant(app.clone())
                    {
                        eprintln!("failed to toggle assistant from shortcut: {error}");
                    }
                })
                .build(),
        )
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            toggle_assistant,
            hide_assistant,
            submit_mock_request
        ])
        .on_window_event(|window, event| {
            if window.label() == AVATAR_LABEL && let WindowEvent::Moved(position) = event {
                handle_avatar_moved(window.app_handle(), *position);
            }
        })
        .setup(move |app| {
            restore_avatar_position(app.handle())?;
            position_assistant(app.handle())?;
            if let Err(error) = app.global_shortcut().register(shortcut) {
                eprintln!(
                    "Ctrl+Shift+Space could not be registered; click activation remains available: {error}"
                );
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("DeskAide failed to start");
}
