mod assistant;
mod avatar;
mod context_commands;
mod conversation_history;
mod credentials;
mod model_profiles;
mod positioning;
mod providers;
mod shortcuts;
mod speech;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use credentials::{CredentialStore, SystemCredentialStore};
use deskaide_assistant_core::{ContextSourceType, TargetWindow, TextContextDraft};
use deskaide_context_core::{
    ActiveWindowTextContextProvider, ContextProvider, ContextRequest, PlatformIntegration,
    SelectedTextContextProvider,
};
use model_profiles::{
    AssistantBootstrap, ModelProfileInput, ModelProfileView, ProfileCollection, ProviderType,
    SavedProfiles,
};
use positioning::{Rect, Size, assistant_position, clamp_rect};
use providers::openai_provider;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size as TauriSize,
    State, WebviewWindow, WindowEvent,
};
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

#[cfg(windows)]
use deskaide_platform_windows::WindowsPlatformIntegration;

const AVATAR_LABEL: &str = "avatar";
const ASSISTANT_LABEL: &str = "assistant";
const CONTEXT_EDITOR_LABEL: &str = "context-editor";
const SETTINGS_FILE: &str = "settings.json";
const POSITION_KEY: &str = "avatarPosition";
const MODEL_PROFILES_KEY: &str = "modelProfiles";
const WINDOW_GAP: i32 = 12;
const COMPACT_ASSISTANT_WIDTH: f64 = 420.0;
const COMPACT_ASSISTANT_HEIGHT: f64 = 460.0;
const EXPANDED_ASSISTANT_WIDTH: f64 = 720.0;
const EXPANDED_ASSISTANT_HEIGHT: f64 = 720.0;

struct AppState {
    profiles: Mutex<ProfileCollection>,
    credentials: Arc<dyn CredentialStore>,
    #[allow(dead_code)]
    platform: Arc<dyn PlatformIntegration>,
    selected_text_provider: Arc<dyn ContextProvider>,
    active_window_text_provider: Arc<dyn ContextProvider>,
    context_target: Mutex<Option<TargetWindow>>,
    editing_context: Mutex<Option<TextContextDraft>>,
    movement: Mutex<MovementState>,
    /// When true, assistant stays always-on-top and does not hide on blur.
    assistant_pinned: AtomicBool,
    /// Suppress blur-hide while the user is interacting with the avatar (click/drag).
    avatar_interacting: AtomicBool,
}

#[derive(Debug, Default)]
struct MovementState {
    generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistantShownPayload {
    target: Option<TargetWindow>,
    warning: Option<String>,
    pinned: bool,
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
        let platform: Arc<dyn PlatformIntegration> = Arc::new(WindowsPlatformIntegration::new());
        Self {
            profiles: Mutex::new(ProfileCollection::default()),
            credentials: Arc::new(SystemCredentialStore::new()),
            selected_text_provider: Arc::new(SelectedTextContextProvider::new(Arc::clone(
                &platform,
            ))),
            active_window_text_provider: Arc::new(ActiveWindowTextContextProvider::new(
                Arc::clone(&platform),
            )),
            platform,
            context_target: Mutex::new(None),
            editing_context: Mutex::new(None),
            movement: Mutex::new(MovementState::default()),
            assistant_pinned: AtomicBool::new(false),
            avatar_interacting: AtomicBool::new(false),
        }
    }
}

#[tauri::command]
fn get_assistant_bootstrap(state: State<'_, AppState>) -> Result<AssistantBootstrap, String> {
    let profiles = state
        .profiles
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    profile_bootstrap(&profiles, state.credentials.as_ref())
}

fn profile_bootstrap(
    profiles: &ProfileCollection,
    credentials: &dyn CredentialStore,
) -> Result<AssistantBootstrap, String> {
    let model_profiles = profiles
        .profiles()
        .iter()
        .cloned()
        .map(|profile| {
            let has_api_key = profile.provider_type == ProviderType::Mock
                || credentials.exists(&profile.id).map_err(display_error)?;
            Ok(ModelProfileView {
                profile,
                has_api_key,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AssistantBootstrap {
        active_model_profile_id: profiles.active_id().to_owned(),
        model_profiles,
    })
}

#[tauri::command]
fn save_model_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: ModelProfileInput,
) -> Result<ModelProfileView, String> {
    let api_key = profile
        .api_key
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let mut current = state
        .profiles
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut draft = current.clone();
    let saved = draft.save(profile).map_err(display_error)?;
    let previous_key = if api_key.is_some() {
        state.credentials.get(&saved.id).map_err(display_error)?
    } else {
        None
    };
    if let Some(api_key) = api_key.as_deref() {
        state
            .credentials
            .set(&saved.id, api_key)
            .map_err(display_error)?;
    }
    if let Err(error) = persist_profiles(&app, &draft) {
        if api_key.is_some() {
            if let Some(previous_key) = previous_key.as_deref() {
                let _ = state.credentials.set(&saved.id, previous_key);
            } else {
                let _ = state.credentials.delete(&saved.id);
            }
        }
        return Err(error);
    }
    let has_api_key = saved.provider_type == ProviderType::Mock
        || state.credentials.exists(&saved.id).map_err(display_error)?;
    *current = draft;
    Ok(ModelProfileView {
        profile: saved,
        has_api_key,
    })
}

#[tauri::command]
fn delete_model_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut current = state
        .profiles
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut draft = current.clone();
    draft.delete(&profile_id).map_err(display_error)?;
    let previous_key = state.credentials.get(&profile_id).map_err(display_error)?;
    state
        .credentials
        .delete(&profile_id)
        .map_err(display_error)?;
    if let Err(error) = persist_profiles(&app, &draft) {
        if let Some(previous_key) = previous_key.as_deref() {
            let _ = state.credentials.set(&profile_id, previous_key);
        }
        return Err(error);
    }
    *current = draft;
    Ok(())
}

#[tauri::command]
fn set_active_model_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut current = state
        .profiles
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut draft = current.clone();
    draft.set_active(&profile_id).map_err(display_error)?;
    persist_profiles(&app, &draft)?;
    *current = draft;
    Ok(())
}

#[tauri::command]
async fn test_model_connection(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        state
            .profiles
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&profile_id)
            .cloned()
            .map_err(display_error)?
    };
    if profile.provider_type == ProviderType::Mock {
        return Ok("Mock Provider 可用（无需网络）".to_owned());
    }
    let key = state
        .credentials
        .get(&profile.id)
        .map_err(display_error)?
        .ok_or_else(|| "API Key 未配置".to_owned())?;
    let provider = openai_provider(&profile, key).map_err(display_error)?;
    provider.test_connection().await.map_err(display_error)?;
    Ok("连接成功，模型可用".to_owned())
}

#[tauri::command]
async fn toggle_assistant(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let assistant = get_window(&app, ASSISTANT_LABEL)?;
    if assistant.is_visible().map_err(display_error)? {
        assistant.hide().map_err(display_error)
    } else {
        let (target, warning) = match state.platform.get_last_active_window().await {
            Ok(target) => {
                *state
                    .context_target
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(target.clone());
                (Some(target), None)
            }
            Err(error) => {
                let target = state
                    .context_target
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                (target, Some(error.to_string()))
            }
        };
        let pinned = state.assistant_pinned.load(Ordering::SeqCst);
        assistant.set_always_on_top(pinned).map_err(display_error)?;
        position_assistant(&app)?;
        assistant.show().map_err(display_error)?;
        let _ = app.emit_to(AVATAR_LABEL, "avatar-activated", ());
        assistant.set_focus().map_err(display_error)?;
        assistant
            .emit(
                "assistant-shown",
                AssistantShownPayload {
                    target,
                    warning,
                    pinned,
                },
            )
            .map_err(display_error)
    }
}

#[tauri::command]
fn hide_assistant(app: AppHandle) -> Result<(), String> {
    get_window(&app, ASSISTANT_LABEL)?
        .hide()
        .map_err(display_error)
}

#[tauri::command]
fn set_assistant_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    pinned: bool,
) -> Result<(), String> {
    state.assistant_pinned.store(pinned, Ordering::SeqCst);
    get_window(&app, ASSISTANT_LABEL)?
        .set_always_on_top(pinned)
        .map_err(display_error)
}

#[tauri::command]
fn set_avatar_interacting(app: AppHandle, state: State<'_, AppState>, interacting: bool) {
    state
        .avatar_interacting
        .store(interacting, Ordering::SeqCst);
    if interacting {
        return;
    }
    // After avatar click/drag, if the assistant stayed open but lost focus,
    // restore focus so a later outside click can still blur-hide it.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let state = app.state::<AppState>();
        if state.avatar_interacting.load(Ordering::SeqCst)
            || state.assistant_pinned.load(Ordering::SeqCst)
        {
            return;
        }
        if let Ok(assistant) = get_window(&app, ASSISTANT_LABEL)
            && assistant.is_visible().unwrap_or(false)
            && !assistant.is_focused().unwrap_or(false)
        {
            let _ = assistant.set_focus();
        }
    });
}

#[tauri::command]
fn set_assistant_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    let avatar = get_window(&app, AVATAR_LABEL)?;
    let assistant = get_window(&app, ASSISTANT_LABEL)?;
    let monitor = avatar
        .current_monitor()
        .map_err(display_error)?
        .or(avatar.primary_monitor().map_err(display_error)?)
        .ok_or_else(|| "无法确定当前显示器".to_owned())?;
    let work_area = monitor.work_area();
    let scale_factor = assistant.scale_factor().map_err(display_error)?;
    let (logical_width, logical_height) = if expanded {
        (EXPANDED_ASSISTANT_WIDTH, EXPANDED_ASSISTANT_HEIGHT)
    } else {
        (COMPACT_ASSISTANT_WIDTH, COMPACT_ASSISTANT_HEIGHT)
    };
    let width = ((logical_width * scale_factor).round() as u32).min(work_area.size.width);
    let height = ((logical_height * scale_factor).round() as u32).min(work_area.size.height);
    assistant
        .set_size(TauriSize::Physical(PhysicalSize::new(width, height)))
        .map_err(display_error)?;
    position_assistant(&app)
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
        let avatar_size = avatar.outer_size().map_err(display_error)?;
        (
            (
                work.position.x + work.size.width as i32 - avatar_size.width as i32 - 24,
                work.position.y + work.size.height as i32 - avatar_size.height as i32 - 24,
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

fn load_profiles(app: &AppHandle) -> Result<(), String> {
    let store = app.store(SETTINGS_FILE).map_err(display_error)?;
    let profiles = store
        .get(MODEL_PROFILES_KEY)
        .and_then(|value| serde_json::from_value::<SavedProfiles>(value).ok())
        .map(ProfileCollection::from_saved)
        .unwrap_or_default();
    *app.state::<AppState>()
        .profiles
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = profiles;
    Ok(())
}

fn persist_profiles(app: &AppHandle, profiles: &ProfileCollection) -> Result<(), String> {
    let value = serde_json::to_value(profiles.saved()).map_err(display_error)?;
    let store = app.store(SETTINGS_FILE).map_err(display_error)?;
    store.set(MODEL_PROFILES_KEY, value);
    store.save().map_err(display_error)
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

fn handle_assistant_blur(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Delay so avatar click/drag can mark interaction and so brief focus
        // transitions do not flash-hide the panel.
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let state = app.state::<AppState>();
        if state.assistant_pinned.load(Ordering::SeqCst) {
            return;
        }
        if state.avatar_interacting.load(Ordering::SeqCst) {
            return;
        }
        let Ok(assistant) = get_window(&app, ASSISTANT_LABEL) else {
            return;
        };
        if assistant.is_focused().unwrap_or(false) {
            return;
        }
        if let Ok(editor) = get_window(&app, CONTEXT_EDITOR_LABEL)
            && editor.is_visible().unwrap_or(false)
        {
            return;
        }
        if assistant.is_visible().unwrap_or(false)
            && let Err(error) = assistant.hide()
        {
            eprintln!("failed to hide assistant after blur: {error}");
        }
    });
}

#[cfg(windows)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        shortcuts::activate(app);
                    }
                })
                .build(),
        )
        .manage(AppState::new())
        .manage(speech::SpeechState::default())
        .manage(avatar::PresentationState::default())
        .register_uri_scheme_protocol("avatar-local", |ctx, request| {
            let result = avatar::root(ctx.app_handle()).and_then(|root| {
                avatar::resolve_resource_uri(&root, request.uri().path().trim_start_matches('/'))
            });
            match result {
                Ok(path) => {
                    let mime = match path.extension().and_then(|x| x.to_str()).unwrap_or("") {
                        "js" => "text/javascript",
                        "json" => "application/json",
                        "png" => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "webp" => "image/webp",
                        "webm" => "video/webm",
                        "mp4" => "video/mp4",
                        "wasm" => "application/wasm",
                        _ => "application/octet-stream",
                    };
                    match std::fs::read(path) {
                        Ok(bytes) => tauri::http::Response::builder()
                            .header("Content-Type", mime)
                            .header("X-Content-Type-Options", "nosniff")
                            .header("Access-Control-Allow-Origin", "*")
                            .body(bytes)
                            .unwrap(),
                        Err(_) => tauri::http::Response::builder()
                            .status(404)
                            .body(Vec::new())
                            .unwrap(),
                    }
                }
                Err(_) => tauri::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .manage(shortcuts::ShortcutState::default())
        .invoke_handler(tauri::generate_handler![
            shortcuts::get_shortcut_settings,
            shortcuts::get_shortcut_error,
            shortcuts::save_shortcut_settings,
            speech::get_speech_settings,
            avatar::begin_speech_presentation,
            avatar::publish_speech_presentation,
            avatar::get_avatar_presentation,
            avatar::list_local_avatar_packs,
            avatar::open_avatar_directory,
            avatar::get_avatar_settings,
            avatar::save_avatar_settings,
            avatar::sample_avatar_cursor,
            avatar::resize_avatar,
            speech::save_speech_settings,
            speech::check_speech_service,
            speech::speech_references,
            speech::speak_segment,
            speech::cancel_speech,
            get_assistant_bootstrap,
            save_model_profile,
            delete_model_profile,
            set_active_model_profile,
            test_model_connection,
            toggle_assistant,
            hide_assistant,
            set_assistant_pinned,
            set_avatar_interacting,
            context_commands::list_available_windows,
            context_commands::collect_window_context,
            context_commands::preview_selected_text_context,
            context_commands::preview_clipboard_context,
            context_commands::open_context_editor,
            context_commands::get_context_editor_draft,
            context_commands::save_context_editor_draft,
            context_commands::close_context_editor,
            assistant::submit_turn,
            assistant::cancel_turn,
            set_assistant_expanded,
            assistant::list_conversation_summaries,
            assistant::load_conversation,
            assistant::get_turn_snapshot,
            assistant::get_active_turn_snapshot,
            assistant::approve_tool,
            assistant::get_mcp_settings,
            assistant::save_mcp_server,
            assistant::delete_mcp_server,
            assistant::test_mcp_server,
            assistant::reconnect_mcp_server,
            assistant::rename_conversation,
            assistant::delete_conversation
        ])
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(position) if window.label() == AVATAR_LABEL => {
                handle_avatar_moved(window.app_handle(), *position);
            }
            WindowEvent::Focused(false) if window.label() == ASSISTANT_LABEL => {
                handle_assistant_blur(window.app_handle());
            }
            _ => {}
        })
        .setup(move |app| {
            load_profiles(app.handle())?;
            assistant::setup(app.handle())?;
            restore_avatar_position(app.handle())?;
            // Default to the large (expanded) assistant size on startup.
            set_assistant_expanded(app.handle().clone(), true)?;
            position_assistant(app.handle())?;
            shortcuts::setup(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("DeskAide failed to start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                tauri::async_runtime::block_on(assistant::shutdown(app));
                app.state::<speech::SpeechState>().shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryCredentialStore;
    use std::collections::BTreeMap;
    #[test]
    fn bootstrap_exposes_key_presence_but_not_key_material() {
        let mut profiles = ProfileCollection::default();
        let remote = profiles
            .save(ModelProfileInput {
                prefer_fast_response: true,
                id: Some("remote".to_owned()),
                name: "Remote".to_owned(),
                provider_type: ProviderType::OpenAiCompatible,
                base_url: "https://example.com/v1".to_owned(),
                model_id: "model".to_owned(),
                capabilities: model_profiles::mock_profile().capabilities,
                max_output_tokens: Some(100),
                timeout_seconds: 30,
                custom_headers: BTreeMap::new(),
                api_key: None,
            })
            .unwrap();
        let credentials = MemoryCredentialStore::default();
        let without_key = profile_bootstrap(&profiles, &credentials).unwrap();
        assert!(!without_key.model_profiles[1].has_api_key);

        credentials.set(&remote.id, "never-return-this").unwrap();
        let with_key = profile_bootstrap(&profiles, &credentials).unwrap();
        assert!(with_key.model_profiles[1].has_api_key);
        let json = serde_json::to_string(&with_key).unwrap();
        assert!(!json.contains("never-return-this"));
    }
}
