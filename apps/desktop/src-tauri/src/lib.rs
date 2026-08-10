mod credentials;
mod model_profiles;
mod positioning;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use credentials::{CredentialStore, SystemCredentialStore};
use deskaide_ai_provider::{
    MockProvider, ModelError, ModelProvider, OpenAiCompatibleProvider,
    openai_compatible::{OpenAiCompatibleConfig, SecretString},
};
use deskaide_assistant_core::{
    ContextPayload, ContextSourceType, GenerationOptions, ModelMessage, ModelRequest,
    ResponseEvent, TargetWindow,
};
use deskaide_context_core::{
    ActiveWindowTextContextProvider, ContextError, ContextProvider, ContextRequest,
    PlatformIntegration, SelectedTextContextProvider,
};
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
const CONTEXT_EDITOR_LABEL: &str = "context-editor";
const SETTINGS_FILE: &str = "settings.json";
const POSITION_KEY: &str = "avatarPosition";
const MODEL_PROFILES_KEY: &str = "modelProfiles";
const WINDOW_GAP: i32 = 12;
const COMPACT_ASSISTANT_WIDTH: f64 = 420.0;
const COMPACT_ASSISTANT_HEIGHT: f64 = 460.0;
const EXPANDED_ASSISTANT_WIDTH: f64 = 720.0;
const EXPANDED_ASSISTANT_HEIGHT: f64 = 720.0;
const MAX_CONTEXT_CHARS: usize = 64_000;
const DEFAULT_CONTEXT_CHARS: usize = 32_000;

struct AppState {
    profiles: Mutex<ProfileCollection>,
    credentials: Arc<dyn CredentialStore>,
    #[allow(dead_code)]
    platform: Arc<dyn PlatformIntegration>,
    selected_text_provider: Arc<dyn ContextProvider>,
    active_window_text_provider: Arc<dyn ContextProvider>,
    context_target: Mutex<Option<TargetWindow>>,
    editing_context: Mutex<Option<WindowContextDraft>>,
    movement: Mutex<MovementState>,
    active_request: Arc<Mutex<Option<ActiveRequest>>>,
    /// When true, assistant stays always-on-top and does not hide on blur.
    assistant_pinned: AtomicBool,
    /// Suppress blur-hide while the user is interacting with the avatar (click/drag).
    avatar_interacting: AtomicBool,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistantShownPayload {
    target: Option<TargetWindow>,
    warning: Option<String>,
    pinned: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ContextCollectionStatus {
    Added,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ContextCollectionResult {
    source: ContextSourceType,
    status: ContextCollectionStatus,
    character_count: usize,
    truncated: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmitModelRequestResult {
    request_id: String,
    context_results: Vec<ContextCollectionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct WindowContextDraft {
    id: String,
    target: TargetWindow,
    content: String,
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
            active_request: Arc::new(Mutex::new(None)),
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
async fn list_available_windows(state: State<'_, AppState>) -> Result<Vec<TargetWindow>, String> {
    state.platform.list_windows().await.map_err(display_error)
}

#[tauri::command]
async fn collect_window_context(
    state: State<'_, AppState>,
    target: TargetWindow,
) -> Result<WindowContextDraft, String> {
    let payload = state
        .active_window_text_provider
        .collect(&ContextRequest {
            target: target.clone(),
            sources: vec![ContextSourceType::ActiveWindowText],
        })
        .await
        .map_err(display_error)?;
    let content = payload
        .main_text
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "该窗口没有公开可读取的文字内容".to_owned())?;
    Ok(WindowContextDraft {
        id: Uuid::new_v4().to_string(),
        target,
        content,
    })
}

#[tauri::command]
fn open_context_editor(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: WindowContextDraft,
) -> Result<(), String> {
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(draft.clone());
    let editor = get_window(&app, CONTEXT_EDITOR_LABEL)?;
    editor.show().map_err(display_error)?;
    editor.set_focus().map_err(display_error)?;
    editor
        .emit("context-editor-opened", draft)
        .map_err(display_error)
}

#[tauri::command]
fn get_context_editor_draft(state: State<'_, AppState>) -> Option<WindowContextDraft> {
    state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[tauri::command]
fn save_context_editor_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: WindowContextDraft,
) -> Result<(), String> {
    if draft.content.trim().is_empty() {
        return Err("上下文内容不能为空".to_owned());
    }
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(draft.clone());
    app.emit_to(ASSISTANT_LABEL, "context-draft-updated", draft)
        .map_err(display_error)?;
    close_context_editor(app, state)
}

#[tauri::command]
fn close_context_editor(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    *state
        .editing_context
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    get_window(&app, CONTEXT_EDITOR_LABEL)?
        .hide()
        .map_err(display_error)?;
    if let Ok(assistant) = get_window(&app, ASSISTANT_LABEL)
        && assistant.is_visible().unwrap_or(false)
    {
        let _ = assistant.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn submit_model_request(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    messages: Vec<ModelMessage>,
    context_sources: Vec<ContextSourceType>,
    window_contexts: Vec<WindowContextDraft>,
) -> Result<SubmitModelRequestResult, String> {
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
    let target = state
        .context_target
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let (context, context_results) = collect_context(
        &state,
        target.as_ref(),
        &context_sources,
        &window_contexts,
        profile.capabilities.supports_text,
        context_char_budget(profile.capabilities.context_window),
    )
    .await;

    let request_id = Uuid::new_v4().to_string();
    let request = ModelRequest {
        request_id: request_id.clone(),
        model_profile_id: profile.id,
        conversation_id,
        system_prompt: Some("You are DeskAide, a helpful desktop assistant.".to_owned()),
        messages,
        context,
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

    Ok(SubmitModelRequestResult {
        request_id,
        context_results,
    })
}

async fn collect_context(
    state: &AppState,
    target: Option<&TargetWindow>,
    requested: &[ContextSourceType],
    window_contexts: &[WindowContextDraft],
    supports_text: bool,
    mut remaining_chars: usize,
) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
    let mut payloads = Vec::new();
    let mut results = Vec::new();
    let mut ordered_sources = Vec::new();
    for source in [
        ContextSourceType::SelectedText,
        ContextSourceType::ActiveWindowText,
    ] {
        if requested.contains(&source) {
            ordered_sources.push(source);
        }
    }
    for source in requested {
        if !ordered_sources.contains(source) {
            ordered_sources.push(source.clone());
        }
    }

    for source in ordered_sources {
        if matches!(
            source,
            ContextSourceType::SelectedText | ContextSourceType::ActiveWindowText
        ) && !supports_text
        {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "当前模型不支持文字上下文",
            ));
            continue;
        }
        let Some(target) = target else {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "未记录到本次激活前的外部窗口",
            ));
            continue;
        };
        let provider = match source {
            ContextSourceType::SelectedText => Some(Arc::clone(&state.selected_text_provider)),
            ContextSourceType::ActiveWindowText => {
                Some(Arc::clone(&state.active_window_text_provider))
            }
            _ => None,
        };
        let Some(provider) = provider else {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "该上下文类型尚未实现",
            ));
            continue;
        };
        if remaining_chars == 0 {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                true,
                "本次请求的上下文预算已用完",
            ));
            continue;
        }

        let request = ContextRequest {
            target: target.clone(),
            sources: vec![source.clone()],
        };
        match provider.collect(&request).await {
            Ok(mut payload) => {
                let text = match source {
                    ContextSourceType::SelectedText => payload.selected_text.take(),
                    ContextSourceType::ActiveWindowText => payload.main_text.take(),
                    _ => None,
                };
                let Some(text) = text else {
                    results.push(context_result(
                        source,
                        ContextCollectionStatus::Unavailable,
                        0,
                        false,
                        "未获取到可用文字",
                    ));
                    continue;
                };
                let (text, truncated) = truncate_chars(&text, remaining_chars);
                let character_count = text.chars().count();
                remaining_chars = remaining_chars.saturating_sub(character_count);
                if truncated {
                    payload
                        .warnings
                        .push("文字已按当前模型的上下文预算截断".to_owned());
                }
                match source {
                    ContextSourceType::SelectedText => payload.selected_text = Some(text),
                    ContextSourceType::ActiveWindowText => payload.main_text = Some(text),
                    _ => {}
                }
                results.push(context_result(
                    source,
                    ContextCollectionStatus::Added,
                    character_count,
                    truncated,
                    if truncated {
                        "已添加，内容已按模型预算截断"
                    } else {
                        "已添加到本次提问"
                    },
                ));
                payloads.push(payload);
            }
            Err(error) => {
                let (status, message) = context_error_result(&error);
                results.push(context_result(source, status, 0, false, message));
            }
        }
    }

    if supports_text {
        for draft in window_contexts {
            let content = draft.content.trim();
            if content.is_empty() {
                results.push(context_result(
                    ContextSourceType::ActiveWindowText,
                    ContextCollectionStatus::Unavailable,
                    0,
                    false,
                    format!("{}：内容为空，已跳过", window_context_label(draft)),
                ));
                continue;
            }
            if remaining_chars == 0 {
                results.push(context_result(
                    ContextSourceType::ActiveWindowText,
                    ContextCollectionStatus::Unavailable,
                    0,
                    true,
                    format!("{}：上下文预算已用完", window_context_label(draft)),
                ));
                continue;
            }
            let (text, truncated) = truncate_chars(content, remaining_chars);
            let character_count = text.chars().count();
            remaining_chars = remaining_chars.saturating_sub(character_count);
            let mut warnings = Vec::new();
            if truncated {
                warnings.push("文字已按当前模型的上下文预算截断".to_owned());
            }
            payloads.push(ContextPayload {
                source_type: ContextSourceType::ActiveWindowText,
                application_name: draft.target.application_name.clone(),
                process_name: draft.target.process_name.clone(),
                window_title: draft.target.title.clone(),
                url: None,
                selected_text: None,
                main_text: Some(text),
                metadata: serde_json::json!({ "draftId": draft.id, "edited": true }),
                images: Vec::new(),
                warnings,
            });
            results.push(context_result(
                ContextSourceType::ActiveWindowText,
                ContextCollectionStatus::Added,
                character_count,
                truncated,
                if truncated {
                    format!("{}：已添加，内容已截断", window_context_label(draft))
                } else {
                    format!("{}：已添加", window_context_label(draft))
                },
            ));
        }
    } else if !window_contexts.is_empty() {
        results.push(context_result(
            ContextSourceType::ActiveWindowText,
            ContextCollectionStatus::Unavailable,
            0,
            false,
            "当前模型不支持文字上下文",
        ));
    }

    (payloads, results)
}

fn window_context_label(draft: &WindowContextDraft) -> &str {
    draft
        .target
        .title
        .as_deref()
        .or(draft.target.application_name.as_deref())
        .or(draft.target.process_name.as_deref())
        .unwrap_or("窗口")
}

fn context_char_budget(context_window: Option<u64>) -> usize {
    context_window
        .map(|tokens| usize::try_from(tokens / 2).unwrap_or(MAX_CONTEXT_CHARS))
        .unwrap_or(DEFAULT_CONTEXT_CHARS)
        .clamp(1, MAX_CONTEXT_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    let mut chars = text.chars();
    let result: String = chars.by_ref().take(max_chars).collect();
    let truncated = chars.next().is_some();
    (result, truncated)
}

fn context_result(
    source: ContextSourceType,
    status: ContextCollectionStatus,
    character_count: usize,
    truncated: bool,
    message: impl Into<String>,
) -> ContextCollectionResult {
    ContextCollectionResult {
        source,
        status,
        character_count,
        truncated,
        message: message.into(),
    }
}

fn context_error_result(error: &ContextError) -> (ContextCollectionStatus, String) {
    match error {
        ContextError::Unsupported { .. } => (
            ContextCollectionStatus::Unavailable,
            "该上下文类型在当前平台不可用".to_owned(),
        ),
        ContextError::Unavailable { reason, .. } => {
            (ContextCollectionStatus::Unavailable, reason.clone())
        }
        ContextError::Timeout { .. } => (
            ContextCollectionStatus::Failed,
            "采集超过 3 秒，已跳过该上下文".to_owned(),
        ),
        ContextError::Collection(message) => (
            ContextCollectionStatus::Failed,
            format!("采集失败：{message}"),
        ),
    }
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
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    let handler_shortcut = shortcut;

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, triggered, event| {
                    if triggered == &handler_shortcut
                        && event.state() == ShortcutState::Pressed
                    {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<AppState>();
                            if let Err(error) = toggle_assistant(app.clone(), state).await {
                                eprintln!("failed to toggle assistant from shortcut: {error}");
                            }
                        });
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
            set_assistant_pinned,
            set_avatar_interacting,
            list_available_windows,
            collect_window_context,
            open_context_editor,
            get_context_editor_draft,
            save_context_editor_draft,
            close_context_editor,
            submit_model_request,
            stop_generation,
            set_assistant_expanded
        ])
        .on_window_event(|window, event| {
            match event {
                WindowEvent::Moved(position) if window.label() == AVATAR_LABEL => {
                    handle_avatar_moved(window.app_handle(), *position);
                }
                WindowEvent::Focused(false) if window.label() == ASSISTANT_LABEL => {
                    handle_assistant_blur(window.app_handle());
                }
                _ => {}
            }
        })
        .setup(move |app| {
            load_profiles(app.handle())?;
            restore_avatar_position(app.handle())?;
            // Default to the large (expanded) assistant size on startup.
            set_assistant_expanded(app.handle().clone(), true)?;
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

    #[test]
    fn context_budget_uses_half_the_model_window_with_a_hard_cap() {
        assert_eq!(context_char_budget(Some(4_096)), 2_048);
        assert_eq!(context_char_budget(None), DEFAULT_CONTEXT_CHARS);
        assert_eq!(context_char_budget(Some(1_000_000)), MAX_CONTEXT_CHARS);
        assert_eq!(context_char_budget(Some(1)), 1);
    }

    #[test]
    fn context_truncation_preserves_unicode_boundaries() {
        assert_eq!(truncate_chars("助手ABC", 3), ("助手A".to_owned(), true));
        assert_eq!(truncate_chars("助手", 3), ("助手".to_owned(), false));
    }

    #[test]
    fn context_results_use_the_frontend_wire_format() {
        let result = SubmitModelRequestResult {
            request_id: "request-1".to_owned(),
            context_results: vec![context_result(
                ContextSourceType::SelectedText,
                ContextCollectionStatus::Added,
                12,
                false,
                "added",
            )],
        };
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["requestId"], "request-1");
        assert_eq!(value["contextResults"][0]["source"], "selectedText");
        assert_eq!(value["contextResults"][0]["status"], "added");
        assert_eq!(value["contextResults"][0]["characterCount"], 12);
    }

    #[test]
    fn context_errors_map_to_non_blocking_user_statuses() {
        let unavailable = ContextError::Unavailable {
            capability: "selected_text",
            reason: "没有选中文字".to_owned(),
        };
        let timeout = ContextError::Timeout {
            capability: "active_window_text",
        };
        assert_eq!(
            context_error_result(&unavailable),
            (
                ContextCollectionStatus::Unavailable,
                "没有选中文字".to_owned()
            )
        );
        assert_eq!(
            context_error_result(&timeout).0,
            ContextCollectionStatus::Failed
        );
    }
}
