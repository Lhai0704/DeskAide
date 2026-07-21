mod credentials;
mod model_profiles;
mod positioning;

use std::sync::{Arc, Mutex};

use credentials::{CredentialStore, SystemCredentialStore};
use deskaide_ai_provider::{
    MockProvider, ModelError, ModelProvider, OpenAiCompatibleProvider,
    openai_compatible::{OpenAiCompatibleConfig, SecretString},
};
use deskaide_assistant_core::{GenerationOptions, ModelMessage, ModelRequest, ResponseEvent};
use deskaide_context_core::PlatformIntegration;
use model_profiles::{
    AssistantBootstrap, ModelProfile, ModelProfileInput, ModelProfileView, ProfileCollection,
    ProviderType, SavedProfiles,
};
use positioning::{Rect, Size, assistant_position, clamp_rect};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size as TauriSize,
    State, WebviewWindow, WindowEvent,
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
    movement: Mutex<MovementState>,
    active_request: Arc<Mutex<Option<ActiveRequest>>>,
}

#[derive(Debug)]
struct ActiveRequest {
    request_id: String,
    abort_handle: tokio::task::AbortHandle,
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
            profiles: Mutex::new(ProfileCollection::default()),
            credentials: Arc::new(SystemCredentialStore::new()),
            platform: Arc::new(WindowsPlatformIntegration::new()),
            movement: Mutex::new(MovementState::default()),
            active_request: Arc::new(Mutex::new(None)),
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
fn submit_model_request(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    messages: Vec<ModelMessage>,
) -> Result<String, String> {
    if !messages.iter().any(|message| {
        matches!(message.role, deskaide_assistant_core::MessageRole::User)
            && message.content.iter().any(|block| {
                matches!(block, deskaide_assistant_core::ContentBlock::Text { text } if !text.trim().is_empty())
            })
    }) {
        return Err("问题不能为空".to_owned());
    }

    let profile = {
        state
            .profiles
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active()
            .clone()
    };
    let provider = provider_for_profile(&profile, state.credentials.as_ref())
        .map_err(|error| error.to_string())?;

    let request_id = Uuid::new_v4().to_string();
    let request = ModelRequest {
        request_id: request_id.clone(),
        model_profile_id: profile.id,
        conversation_id,
        system_prompt: Some("You are DeskAide, a helpful desktop assistant.".to_owned()),
        messages,
        context: Vec::new(),
        generation_options: GenerationOptions {
            max_output_tokens: None,
            temperature: Some(0.7),
        },
    };
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

    let failed_app = app.clone();
    let active_request = Arc::clone(&state.active_request);
    let task_registry = Arc::clone(&active_request);
    let completed_request_id = request_id.clone();
    let (start_sender, start_receiver) = tokio::sync::oneshot::channel();
    let provider_task = tauri::async_runtime::spawn(async move {
        let _ = start_receiver.await;
        if let Err(error) = provider.complete(request, sender).await {
            let _ = failed_app.emit_to(
                ASSISTANT_LABEL,
                "model-response",
                ResponseEvent::Failed {
                    request_id: failed_request_id,
                    code: error.code().to_owned(),
                    message: error.to_string(),
                },
            );
        }

        let mut active = task_registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if active
            .as_ref()
            .is_some_and(|request| request.request_id == completed_request_id)
        {
            *active = None;
        }
    });

    let replaced_request = {
        let mut active = active_request
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        active.replace(ActiveRequest {
            request_id: request_id.clone(),
            abort_handle: provider_task.inner().abort_handle(),
        })
    };
    if let Some(replaced) = replaced_request {
        replaced.abort_handle.abort();
        let _ = app.emit_to(
            ASSISTANT_LABEL,
            "model-response",
            ResponseEvent::Cancelled {
                request_id: replaced.request_id,
            },
        );
    }
    let _ = start_sender.send(());

    Ok(request_id)
}

fn provider_for_profile(
    profile: &ModelProfile,
    credentials: &dyn CredentialStore,
) -> Result<Arc<dyn ModelProvider>, ModelError> {
    match profile.provider_type {
        ProviderType::Mock => Ok(Arc::new(MockProvider::new())),
        ProviderType::OpenAiCompatible => {
            let api_key = credentials
                .get(&profile.id)
                .map_err(|_| ModelError::ApiKeyMissing)?
                .ok_or(ModelError::ApiKeyMissing)?;
            Ok(Arc::new(openai_provider(profile, api_key)?))
        }
    }
}

fn openai_provider(
    profile: &ModelProfile,
    api_key: String,
) -> Result<OpenAiCompatibleProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        profile_id: profile.id.clone(),
        base_url: profile.base_url.clone(),
        model_id: profile.model_id.clone(),
        api_key: SecretString::new(api_key),
        capabilities: profile.capabilities,
        max_output_tokens: profile.max_output_tokens,
        timeout_seconds: profile.timeout_seconds,
        custom_headers: profile.custom_headers.clone(),
    })
}

#[tauri::command]
fn stop_generation(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
) -> Result<bool, String> {
    let active = {
        let mut active_request = state
            .active_request
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        take_matching_request(&mut active_request, &request_id)
    };

    let Some(active) = active else {
        return Ok(false);
    };
    active.abort_handle.abort();
    app.emit_to(
        ASSISTANT_LABEL,
        "model-response",
        ResponseEvent::Cancelled { request_id },
    )
    .map_err(display_error)?;
    Ok(true)
}

fn take_matching_request(
    active_request: &mut Option<ActiveRequest>,
    request_id: &str,
) -> Option<ActiveRequest> {
    if active_request
        .as_ref()
        .is_some_and(|request| request.request_id == request_id)
    {
        active_request.take()
    } else {
        None
    }
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
            get_assistant_bootstrap,
            save_model_profile,
            delete_model_profile,
            set_active_model_profile,
            test_model_connection,
            toggle_assistant,
            hide_assistant,
            submit_model_request,
            stop_generation,
            set_assistant_expanded
        ])
        .on_window_event(|window, event| {
            if window.label() == AVATAR_LABEL && let WindowEvent::Moved(position) = event {
                handle_avatar_moved(window.app_handle(), *position);
            }
        })
        .setup(move |app| {
            load_profiles(app.handle())?;
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;
    use crate::credentials::MemoryCredentialStore;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn matching_request_can_be_aborted() {
        let completed = Arc::new(AtomicBool::new(false));
        let task_completed = Arc::clone(&completed);
        let task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            task_completed.store(true, Ordering::SeqCst);
        });
        let mut active_request = Some(ActiveRequest {
            request_id: "request-1".to_owned(),
            abort_handle: task.abort_handle(),
        });

        let active = take_matching_request(&mut active_request, "request-1").unwrap();
        active.abort_handle.abort();
        let result = task.await;

        assert!(result.unwrap_err().is_cancelled());
        assert!(!completed.load(Ordering::SeqCst));
        assert!(active_request.is_none());
    }

    #[test]
    fn stale_request_id_cannot_cancel_the_active_request() {
        let task = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        let mut active_request = Some(ActiveRequest {
            request_id: "request-2".to_owned(),
            abort_handle: task.abort_handle(),
        });

        assert!(take_matching_request(&mut active_request, "request-1").is_none());
        assert_eq!(
            active_request
                .as_ref()
                .map(|request| request.request_id.as_str()),
            Some("request-2")
        );
    }

    #[test]
    fn bootstrap_exposes_key_presence_but_not_key_material() {
        let mut profiles = ProfileCollection::default();
        let remote = profiles
            .save(ModelProfileInput {
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
