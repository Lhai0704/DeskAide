//! Full product ports: HTTP provider -> runtime -> approval -> stdio -> atomic history -> reload.
#[path = "../../../apps/desktop/src-tauri/src/conversation_history.rs"]
mod history;
use async_trait::async_trait;
use axum::{Json, Router, extract::State, routing::post};
use deskaide_ai_provider::openai_compatible::{
    OpenAiCompatibleConfig, OpenAiCompatibleProvider, SecretString,
};
use deskaide_assistant_core::*;
use deskaide_assistant_runtime::{
    AssistantRuntime, SessionRepository, TurnInput, approval::ApprovalDecision,
};
use deskaide_context_core::TurnContextProvider;
use deskaide_mcp_client::{McpManager, McpServerConfig};
use deskaide_tool_core::ToolRegistry;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
struct NoContext;
#[async_trait]
impl TurnContextProvider for NoContext {
    async fn collect(
        &self,
        _: ContextSelection,
        _: bool,
        _: Option<u64>,
    ) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
        (vec![], vec![])
    }
}
async fn model(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut requests = requests.lock().unwrap();
    requests.push(body.clone());
    if requests.len() == 1 {
        let name = body["tools"][0]["function"]["name"].as_str().unwrap();
        Json(
            json!({"choices":[{"message":{"content":"Checking.","tool_calls":[{"id":"call-1","type":"function","function":{"name":name,"arguments":"{\"text\":\"fixture value\"}"}}]},"finish_reason":"tool_calls"}]}),
        )
    } else {
        assert!(
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["role"] == "tool"
                    && m["tool_call_id"] == "call-1"
                    && m["content"].as_str().unwrap().contains("fixture value"))
        );
        Json(
            json!({"choices":[{"message":{"content":"The tool returned fixture value."},"finish_reason":"stop"}],"usage":{"total_tokens":20}}),
        )
    }
}
#[tokio::test]
async fn approved_mcp_tool_roundtrip_and_real_history_reload() {
    let requests = Arc::new(Mutex::new(vec![]));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/v1/chat/completions", post(model))
        .with_state(requests.clone());
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let provider = Arc::new(
        OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            prefer_fast_response: true,
            profile_id: "fixture".into(),
            base_url: format!("http://{address}"),
            model_id: "fixture".into(),
            api_key: SecretString::new("fixture-not-a-real-key"),
            capabilities: ModelCapabilities {
                supports_text: true,
                supports_images: false,
                supports_tools: true,
                supports_streaming: false,
                supports_system_message: true,
                max_images: None,
                context_window: None,
            },
            max_output_tokens: None,
            timeout_seconds: 5,
            custom_headers: BTreeMap::new(),
        })
        .unwrap(),
    );
    let dir = tempfile::tempdir().unwrap();
    let registry = Arc::new(ToolRegistry::default());
    let mcp = Arc::new(McpManager::new(registry.clone()));
    mcp.configure(vec![McpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: "fixture".into(),
        enabled: true,
        command: env!("CARGO_BIN_EXE_mcp-fixture").into(),
        args: vec![],
        working_directory: None,
        revision: 1,
    }])
    .await
    .unwrap();
    let conversation = uuid::Uuid::new_v4().to_string();
    let mut revision = 0;
    for round in 0..2 {
        // New runtime AND disk repository each time, proving the UI isn't rebuilding context.
        let repository = Arc::new(history::HistoryRepository::new(dir.path().into()));
        repository.recover().unwrap();
        let runtime = Arc::new(AssistantRuntime::new(
            repository.clone(),
            Arc::new(NoContext),
            registry.clone(),
            mcp.clone(),
        ));
        let (tx, mut rx) = mpsc::channel(EVENT_CAPACITY);
        runtime
            .start(
                TurnInput {
                    conversation_id: conversation.clone(),
                    turn_id: uuid::Uuid::new_v4().to_string(),
                    expected_revision: revision,
                    prompt: if round == 0 { "check" } else { "continue" }.into(),
                    profile_id: "fixture".into(),
                    context: ContextSelection::default(),
                },
                provider.clone(),
                tx,
            )
            .await
            .unwrap();
        let mut approvals = 0;
        let mut completed = false;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
        {
            if let AssistantEventKind::ToolApprovalRequired { approval } = &event.kind {
                approvals += 1;
                assert!(matches!(approval.definition.source, ToolSource::Mcp { .. }));
                runtime
                    .approve(
                        &conversation,
                        &event.turn_id,
                        &approval.approval_id,
                        ApprovalDecision {
                            allow: true,
                            persist: false,
                        },
                    )
                    .unwrap();
            }
            if event.kind.terminal() {
                assert!(matches!(
                    event.kind,
                    AssistantEventKind::TurnCompleted { .. }
                ));
                completed = true;
                break;
            }
        }
        assert!(completed);
        assert_eq!(approvals, if round == 0 { 1 } else { 0 });
        runtime.shutdown().await;
        let saved = repository.load(&conversation).await.unwrap().unwrap();
        assert!(
            saved
                .transcript
                .iter()
                .any(|m| !m.tool_provenance.is_empty())
        );
        assert!(
            repository
                .get(&conversation)
                .await
                .unwrap()
                .unwrap()
                .messages
                .iter()
                .any(|m| m.content.contains("fixture value"))
        );
        assert_eq!(repository.list().unwrap().len(), 1);
        revision = repository
            .rename(&conversation, "Verified")
            .unwrap()
            .revision;
    }
    assert_eq!(requests.lock().unwrap().len(), 3);
    mcp.shutdown().await;
    assert!(registry.list().is_empty());
    http.abort();
}
