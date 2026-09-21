//! Tauri composition root and IPC adapters. No model/tool orchestration here.
use crate::{
    ASSISTANT_LABEL, AppState, SETTINGS_FILE,
    conversation_history::{ConversationRecord, ConversationSummary, HistoryRepository},
    providers::provider_for_profile,
};
use deskaide_assistant_core::*;
use deskaide_assistant_runtime::{AssistantRuntime, TurnInput, approval::ApprovalDecision};
use deskaide_context_core::DesktopContextCollector;
use deskaide_mcp_client::{McpManager, McpServerConfig, McpStatus};
use deskaide_tool_core::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;

pub struct AssistantServices {
    runtime: Arc<AssistantRuntime>,
    history: Arc<HistoryRepository>,
    mcp: Arc<McpManager>,
    settings_gate: tokio::sync::Mutex<()>,
    settings_error: Mutex<Option<String>>,
}
pub fn presentation_snapshot(app: &AppHandle) -> Option<TurnSnapshot> {
    app.try_state::<AssistantServices>()?
        .runtime
        .latest_snapshot()
}
fn message(error: ToolError) -> String {
    error.message
}
pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<AppState>();
    let history = Arc::new(HistoryRepository::new(app.path().app_data_dir()?));
    if let Err(e) = history.recover() {
        eprintln!("history startup: {}", e.code);
    }
    let registry = Arc::new(ToolRegistry::default());
    let mcp = Arc::new(McpManager::new(Arc::clone(&registry)));
    let contexts = Arc::new(DesktopContextCollector {
        selected_text_provider: Arc::clone(&state.selected_text_provider),
        active_window_text_provider: Arc::clone(&state.active_window_text_provider),
    });
    let runtime = Arc::new(AssistantRuntime::new(
        history.clone(),
        contexts,
        registry,
        mcp.clone(),
    ));
    let configs = app.store(SETTINGS_FILE)?.get("mcpServers");
    let error = if let Some(value) = configs {
        match serde_json::from_value::<Vec<McpServerConfig>>(value) {
            Ok(configs) => tauri::async_runtime::block_on(mcp.configure(configs))
                .err()
                .map(message),
            Err(_) => Some("MCP 设置无法读取；原设置已保留，请修正后重试".into()),
        }
    } else {
        None
    };
    app.manage(AssistantServices {
        runtime,
        history,
        mcp,
        settings_gate: tokio::sync::Mutex::new(()),
        settings_error: Mutex::new(error),
    });
    Ok(())
}
pub async fn shutdown(app: &AppHandle) {
    let s = app.state::<AssistantServices>();
    s.runtime.shutdown().await;
    s.mcp.shutdown().await;
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTurn {
    conversation_id: String,
    turn_id: String,
    expected_revision: u64,
    prompt: String,
    context_drafts: Vec<TextContextDraft>,
}
#[tauri::command]
pub async fn submit_turn(
    app: AppHandle,
    state: State<'_, AppState>,
    services: State<'_, AssistantServices>,
    input: SubmitTurn,
) -> Result<(), String> {
    let profile = state
        .profiles
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .active()
        .clone();
    let provider =
        provider_for_profile(&profile, state.credentials.as_ref()).map_err(|e| e.to_string())?;
    let target = state
        .context_target
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(EVENT_CAPACITY);
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if app
                .emit_to(ASSISTANT_LABEL, "assistant-event", event)
                .is_err()
            {
                eprintln!("assistant event delivery failed");
            }
        }
    });
    services
        .runtime
        .start(
            TurnInput {
                conversation_id: input.conversation_id,
                turn_id: input.turn_id,
                expected_revision: input.expected_revision,
                prompt: input.prompt,
                profile_id: profile.id,
                context: ContextSelection {
                    target,
                    sources: vec![],
                    drafts: input.context_drafts,
                },
            },
            provider,
            sender,
        )
        .await
        .map_err(message)
}
#[tauri::command]
pub async fn cancel_turn(
    services: State<'_, AssistantServices>,
    turn_id: String,
) -> Result<(), String> {
    services.runtime.cancel_and_wait(&turn_id).await;
    Ok(())
}
#[tauri::command]
pub fn get_active_turn_snapshot(services: State<'_, AssistantServices>) -> Option<TurnSnapshot> {
    services.runtime.active_snapshot()
}
#[tauri::command]
pub fn get_turn_snapshot(
    services: State<'_, AssistantServices>,
    turn_id: String,
) -> Option<TurnSnapshot> {
    services.runtime.snapshot(&turn_id)
}
#[tauri::command]
pub fn approve_tool(
    services: State<'_, AssistantServices>,
    conversation_id: String,
    turn_id: String,
    approval_id: String,
    allow: bool,
    persist: bool,
) -> Result<(), String> {
    services
        .runtime
        .approve(
            &conversation_id,
            &turn_id,
            &approval_id,
            ApprovalDecision { allow, persist },
        )
        .map_err(message)
}
#[tauri::command]
pub fn list_conversation_summaries(
    services: State<'_, AssistantServices>,
) -> Result<Vec<ConversationSummary>, String> {
    services.history.list().map_err(message)
}
#[tauri::command]
pub async fn load_conversation(
    services: State<'_, AssistantServices>,
    conversation_id: String,
) -> Result<Option<ConversationRecord>, String> {
    services
        .history
        .get(&conversation_id)
        .await
        .map_err(message)
}
#[tauri::command]
pub fn rename_conversation(
    services: State<'_, AssistantServices>,
    conversation_id: String,
    title: String,
) -> Result<ConversationSummary, String> {
    services
        .history
        .rename(&conversation_id, &title)
        .map_err(message)
}
#[tauri::command]
pub fn delete_conversation(
    services: State<'_, AssistantServices>,
    conversation_id: String,
) -> Result<bool, String> {
    services.history.delete(&conversation_id).map_err(message)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    servers: Vec<McpServerConfig>,
    statuses: Vec<McpStatus>,
    error: Option<String>,
}
#[tauri::command]
pub fn get_mcp_settings(services: State<'_, AssistantServices>) -> McpSettings {
    McpSettings {
        servers: services.mcp.configurations(),
        statuses: services.mcp.statuses(),
        error: services
            .settings_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
    }
}
async fn save_configs(
    app: &AppHandle,
    s: &AssistantServices,
    configs: Vec<McpServerConfig>,
) -> Result<(), String> {
    if s.settings_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return Err("原 MCP 设置无效，已阻止覆盖；请修复 settings.json 中的 mcpServers".into());
    }
    for c in &configs {
        c.validate().map_err(message)?;
    }
    if configs.len() > 16 {
        return Err("最多配置 16 个 MCP server".into());
    }
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    let previous = store.get("mcpServers");
    store.set(
        "mcpServers",
        serde_json::to_value(&configs).map_err(|e| e.to_string())?,
    );
    if let Err(e) = store.save() {
        if let Some(old) = previous {
            store.set("mcpServers", old);
        } else {
            store.delete("mcpServers");
        }
        return Err(e.to_string());
    }
    s.mcp.configure(configs).await.map_err(message)
}
#[tauri::command]
pub async fn save_mcp_server(
    app: AppHandle,
    services: State<'_, AssistantServices>,
    mut server: McpServerConfig,
) -> Result<(), String> {
    let _gate = services.settings_gate.lock().await;
    server.validate().map_err(message)?;
    let mut configs = services.mcp.configurations();
    if let Some(old) = configs.iter_mut().find(|c| c.id == server.id) {
        server.revision = old.revision + 1;
        *old = server;
    } else {
        server.revision = 1;
        configs.push(server);
    }
    save_configs(&app, &services, configs).await
}
#[tauri::command]
pub async fn delete_mcp_server(
    app: AppHandle,
    services: State<'_, AssistantServices>,
    server_id: String,
) -> Result<(), String> {
    let _gate = services.settings_gate.lock().await;
    let mut configs = services.mcp.configurations();
    configs.retain(|s| s.id != server_id);
    save_configs(&app, &services, configs).await
}
#[tauri::command]
pub async fn test_mcp_server(server: McpServerConfig) -> Result<usize, String> {
    McpManager::test(server).await.map_err(message)
}
#[tauri::command]
pub async fn reconnect_mcp_server(
    services: State<'_, AssistantServices>,
    server_id: String,
) -> Result<(), String> {
    services.mcp.reconnect(&server_id).await.map_err(message)
}
