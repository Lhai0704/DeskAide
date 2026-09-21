use super::*;
use deskaide_ai_provider::{ModelError, ProviderEventSender};
use deskaide_tool_core::{LocalTools, ToolExecutor};
use serde_json::{Value, json};

#[derive(Default)]
struct MemoryStore(Mutex<Vec<Session>>);
#[async_trait]
impl SessionRepository for MemoryStore {
    async fn load(&self, id: &str) -> Result<Option<Session>, ToolError> {
        Ok(self.0.lock().unwrap().iter().find(|s| s.id == id).cloned())
    }
    async fn save(&self, s: &Session, revision: u64) -> Result<Session, ToolError> {
        let mut sessions = self.0.lock().unwrap();
        let old = sessions.iter().position(|o| o.id == s.id);
        if old.map_or(0, |i| sessions[i].revision) != revision {
            return Err(ToolError::new("revision_conflict", "conflict"));
        }
        let mut s = s.clone();
        s.revision += 1;
        if let Some(i) = old {
            sessions[i] = s.clone();
        } else {
            sessions.push(s.clone());
        }
        Ok(s)
    }
}
struct Contexts;
#[async_trait]
impl TurnContextProvider for Contexts {
    async fn collect(
        &self,
        selection: ContextSelection,
        _: bool,
        _: Option<u64>,
    ) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
        (
            selection
                .drafts
                .into_iter()
                .map(|d| ContextPayload {
                    source_type: d.source,
                    application_name: None,
                    process_name: None,
                    window_title: None,
                    url: None,
                    selected_text: Some(d.content),
                    main_text: None,
                    metadata: Value::Null,
                    images: vec![],
                    warnings: vec![],
                })
                .collect(),
            vec![],
        )
    }
}
struct FakeProvider {
    responses: Mutex<VecDeque<Result<ModelResponse, ModelError>>>,
    requests: Mutex<Vec<ModelRequest>>,
    stream: bool,
}
#[async_trait]
impl ModelProvider for FakeProvider {
    fn id(&self) -> &str {
        "fake"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            supports_tools: true,
            supports_text: true,
            supports_images: false,
            supports_streaming: self.stream,
            supports_system_message: true,
            max_images: None,
            context_window: Some(32000),
        }
    }
    async fn complete(
        &self,
        r: ModelRequest,
        s: ProviderEventSender,
    ) -> Result<ModelResponse, ModelError> {
        self.requests.lock().unwrap().push(r);
        let result = self.responses.lock().unwrap().pop_front();
        match result {
            Some(Ok(r)) => {
                if self.stream {
                    s.send(ProviderEvent::TextDelta(r.content.clone()))
                        .await
                        .unwrap();
                }
                Ok(r)
            }
            Some(Err(e)) => Err(e),
            None => std::future::pending().await,
        }
    }
}
struct Echo(bool);
#[async_trait]
impl ToolExecutor for Echo {
    async fn execute(&self, v: Value, _: CancellationToken) -> ToolResult {
        if self.0 {
            ToolResult::failure("fake_error", "failed")
        } else {
            ToolResult::success(v)
        }
    }
}
fn response(text: &str, calls: Vec<ToolCall>) -> Result<ModelResponse, ModelError> {
    Ok(ModelResponse {
        content: text.into(),
        finish_reason: if calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
        .into(),
        tool_calls: calls,
        usage: None,
    })
}
fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "echo".into(),
        arguments: json!({"text":"secret-context"}).to_string(),
    }
}
fn setup(
    responses: Vec<Result<ModelResponse, ModelError>>,
    approval: bool,
    fail: bool,
) -> (Arc<AssistantRuntime>, Arc<FakeProvider>, Arc<MemoryStore>) {
    let registry = Arc::new(ToolRegistry::default());
    registry
        .register(
            ToolDefinition {
                display_name: None,
                id: "echo".into(),
                name: "echo".into(),
                description: "echo".into(),
                parameters: json!({"type":"object"}),
                source: if approval {
                    ToolSource::Mcp {
                        server_id: "s".into(),
                        server_name: "test".into(),
                    }
                } else {
                    ToolSource::Internal
                },
                risk: ToolRisk::ReadOnly,
                revision: 1,
            },
            Arc::new(Echo(fail)),
        )
        .unwrap();
    let store = Arc::new(MemoryStore::default());
    let runtime = Arc::new(AssistantRuntime::new(
        store.clone(),
        Arc::new(Contexts),
        registry.clone(),
        Arc::new(LocalTools(registry)),
    ));
    (
        runtime,
        Arc::new(FakeProvider {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(vec![]),
            stream: true,
        }),
        store,
    )
}
fn input(sensitive: bool) -> TurnInput {
    TurnInput {
        conversation_id: Uuid::new_v4().to_string(),
        turn_id: Uuid::new_v4().to_string(),
        expected_revision: 0,
        prompt: "question".into(),
        profile_id: "fake".into(),
        context: ContextSelection {
            drafts: if sensitive {
                vec![TextContextDraft {
                    id: "d".into(),
                    source: ContextSourceType::SelectedText,
                    target: None,
                    content: "secret-context".into(),
                }]
            } else {
                vec![]
            },
            ..Default::default()
        },
    }
}
async fn drain(
    runtime: &AssistantRuntime,
    mut rx: mpsc::Receiver<AssistantEvent>,
    decision: ApprovalDecision,
) -> Vec<AssistantEvent> {
    let mut events = vec![];
    while let Some(e) = rx.recv().await {
        if let AssistantEventKind::ToolApprovalRequired { approval } = &e.kind {
            runtime
                .approve(
                    &e.conversation_id,
                    &e.turn_id,
                    &approval.approval_id,
                    decision,
                )
                .unwrap();
        }
        let terminal = e.kind.terminal();
        events.push(e);
        if terminal {
            break;
        }
    }
    events
}
#[tokio::test]
async fn text_stream_completion_and_reloaded_multiturn() {
    let (runtime, provider, store) = setup(
        vec![response("hello", vec![]), response("again", vec![])],
        false,
        false,
    );
    let first = input(false);
    let conversation = first.conversation_id.clone();
    let (tx, rx) = mpsc::channel(256);
    runtime.start(first, provider.clone(), tx).await.unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert!(matches!(
        events.last().unwrap().kind,
        AssistantEventKind::TurnCompleted { .. }
    ));
    assert_eq!(events.iter().filter(|e| e.kind.terminal()).count(), 1);
    let saved = store.load(&conversation).await.unwrap().unwrap();
    let mut second = input(false);
    second.conversation_id = conversation;
    second.expected_revision = saved.revision;
    let (tx, rx) = mpsc::channel(256);
    runtime.start(second, provider.clone(), tx).await.unwrap();
    drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert_eq!(provider.requests.lock().unwrap()[1].messages.len(), 3);
}
#[tokio::test]
async fn multiple_tools_and_rounds_are_paired_and_continue() {
    let (runtime, provider, store) = setup(
        vec![
            response("checking", vec![call("a"), call("b")]),
            response("next", vec![call("c")]),
            response("done", vec![]),
        ],
        false,
        false,
    );
    let input = input(false);
    let id = input.conversation_id.clone();
    let (tx, rx) = mpsc::channel(256);
    runtime.start(input, provider.clone(), tx).await.unwrap();
    drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert_eq!(provider.requests.lock().unwrap().len(), 3);
    let saved = store.load(&id).await.unwrap().unwrap();
    assert_eq!(
        saved
            .transcript
            .iter()
            .filter(|m| m.message.role == MessageRole::Tool)
            .count(),
        3
    );
    assert!(
        saved
            .transcript
            .iter()
            .filter(|m| m.message.role == MessageRole::Tool)
            .all(|m| m.message.text_content().contains("secret-context"))
    );
}
#[tokio::test]
async fn denial_failure_and_retention_are_structured() {
    for (allow, persist, fail) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let (runtime, provider, store) = setup(
            vec![response("", vec![call("a")]), response("done", vec![])],
            true,
            fail,
        );
        let input = input(true);
        let id = input.conversation_id.clone();
        let (tx, rx) = mpsc::channel(256);
        runtime.start(input, provider.clone(), tx).await.unwrap();
        let events = drain(&runtime, rx, ApprovalDecision { allow, persist }).await;
        assert!(matches!(
            events.last().unwrap().kind,
            AssistantEventKind::TurnCompleted { .. }
        ));
        let serialized = serde_json::to_string(&store.load(&id).await.unwrap().unwrap()).unwrap();
        assert_eq!(serialized.contains("secret-context"), allow && persist);
        let req = provider.requests.lock().unwrap();
        let tool = req[1]
            .messages
            .iter()
            .find(|m| m.role == MessageRole::Tool)
            .unwrap()
            .text_content();
        if !allow {
            assert!(tool.contains("denied"));
        } else if fail {
            assert!(tool.contains("fake_error"));
        } else {
            assert!(tool.contains("secret-context"));
        }
    }
}
#[tokio::test]
async fn cancellation_during_approval_clears_waiter_and_stale_events() {
    let (runtime, provider, _) = setup(vec![response("", vec![call("a")])], true, false);
    let first = input(false);
    let turn = first.turn_id.clone();
    let (tx, mut rx) = mpsc::channel(256);
    runtime.start(first, provider, tx).await.unwrap();
    let event = loop {
        let e = rx.recv().await.unwrap();
        if matches!(e.kind, AssistantEventKind::ToolApprovalRequired { .. }) {
            break e;
        }
    };
    assert!(!runtime.cancel("stale"));
    runtime.cancel_and_wait(&turn).await;
    if let AssistantEventKind::ToolApprovalRequired { approval } = event.kind {
        assert!(
            runtime
                .approve(
                    &event.conversation_id,
                    &turn,
                    &approval.approval_id,
                    ApprovalDecision {
                        allow: true,
                        persist: true
                    }
                )
                .is_err()
        );
    }
    assert_eq!(
        runtime.snapshot(&turn).unwrap().status,
        TurnStatus::Cancelled
    );
}
#[tokio::test]
async fn provider_failure_and_step_limit_end_once() {
    let (runtime, provider, _) = setup(vec![Err(ModelError::MissingUserText)], false, false);
    let (tx, rx) = mpsc::channel(256);
    runtime.start(input(false), provider, tx).await.unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert!(matches!(
        events.last().unwrap().kind,
        AssistantEventKind::TurnFailed { .. }
    ));
    let (mut runtime, provider, _) = setup(
        (0..8)
            .map(|i| response("", vec![call(&i.to_string())]))
            .collect(),
        false,
        false,
    );
    Arc::get_mut(&mut runtime).unwrap().limits.model_steps = 2;
    let (tx, rx) = mpsc::channel(256);
    runtime
        .start(input(false), provider.clone(), tx)
        .await
        .unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert!(
        matches!(&events.last().unwrap().kind,AssistantEventKind::TurnFailed{code,..} if code=="step_limit")
    );
    assert_eq!(provider.requests.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn new_turn_replaces_in_flight_model_without_cross_session_context() {
    let (runtime, provider, _) = setup(vec![], false, false);
    let first = input(true);
    let old = first.turn_id.clone();
    let (tx, mut rx) = mpsc::channel(256);
    runtime.start(first, provider.clone(), tx).await.unwrap();
    while !matches!(
        rx.recv().await.unwrap().kind,
        AssistantEventKind::MessageStarted { .. }
    ) {}
    provider
        .responses
        .lock()
        .unwrap()
        .push_back(response("new", vec![]));
    let (tx, rx) = mpsc::channel(256);
    runtime
        .start(input(false), provider.clone(), tx)
        .await
        .unwrap();
    drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: true,
            persist: false,
        },
    )
    .await;
    assert_eq!(
        runtime.snapshot(&old).unwrap().status,
        TurnStatus::Cancelled
    );
    assert!(
        !provider.requests.lock().unwrap().last().unwrap().messages[0]
            .text_content()
            .contains("secret-context")
    );
}

struct Recorder {
    log: Arc<Mutex<Vec<u8>>>,
    id: u8,
    fail: bool,
}
#[async_trait]
impl hooks::AssistantHook for Recorder {
    async fn run(&self, _: &HookContext) -> Result<(), ToolError> {
        self.log.lock().unwrap().push(self.id);
        if self.fail {
            Err(ToolError::new("test", "test"))
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn hooks_order_unsubscribe_cleanup_and_error_policy() {
    let h = HookRegistry::default();
    let log = Arc::new(Mutex::new(vec![]));
    let mut guards = vec![];
    for id in 0..3 {
        guards.push(
            h.register(
                HookPhase::TextDelta,
                false,
                Arc::new(Recorder {
                    log: log.clone(),
                    id,
                    fail: id == 1,
                }),
            )
            .unwrap(),
        );
    }
    let ctx = HookContext {
        conversation_id: "c".into(),
        turn_id: "t".into(),
        phase: HookPhase::TextDelta,
        data: HookData::Empty,
    };
    let mut disabled = HashSet::new();
    h.dispatch(&ctx, &mut disabled, &CancellationToken::new())
        .await
        .unwrap();
    guards[0].unsubscribe();
    h.dispatch(&ctx, &mut disabled, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(*log.lock().unwrap(), vec![0, 1, 2, 2]);
    drop(guards);
    h.dispatch(&ctx, &mut disabled, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(log.lock().unwrap().len(), 4);
    let _guard = h
        .register(
            HookPhase::BeforeModel,
            true,
            Arc::new(Recorder {
                log,
                id: 4,
                fail: true,
            }),
        )
        .unwrap();
    let ctx = HookContext {
        phase: HookPhase::BeforeModel,
        ..ctx
    };
    assert!(
        h.dispatch(&ctx, &mut disabled, &CancellationToken::new())
            .await
            .is_err()
    );
    h.clear();
    assert!(
        h.dispatch(&ctx, &mut disabled, &CancellationToken::new())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn changing_a_tool_ends_approval_without_executing() {
    let (runtime, provider, _) = setup(
        vec![response("", vec![call("a")]), response("continued", vec![])],
        true,
        false,
    );
    let (tx, mut rx) = mpsc::channel(256);
    runtime
        .start(input(false), provider.clone(), tx)
        .await
        .unwrap();
    loop {
        if matches!(
            rx.recv().await.unwrap().kind,
            AssistantEventKind::ToolApprovalRequired { .. }
        ) {
            break;
        }
    }
    runtime.tools.unregister("echo");
    let events = tokio::time::timeout(
        Duration::from_secs(2),
        drain(
            &runtime,
            rx,
            ApprovalDecision {
                allow: true,
                persist: false,
            },
        ),
    )
    .await
    .unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, AssistantEventKind::ToolStarted { .. }))
    );
    assert!(matches!(
        events.last().unwrap().kind,
        AssistantEventKind::TurnCompleted { .. }
    ));
}

struct WaitingContext(Arc<tokio::sync::Notify>);
#[async_trait]
impl TurnContextProvider for WaitingContext {
    async fn collect(
        &self,
        _: ContextSelection,
        _: bool,
        _: Option<u64>,
    ) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
        self.0.notify_one();
        std::future::pending().await
    }
}
#[tokio::test]
async fn preparation_is_cancellable_before_any_model_request() {
    let (mut runtime, provider, store) = setup(vec![], false, false);
    let entered = Arc::new(tokio::sync::Notify::new());
    Arc::get_mut(&mut runtime).unwrap().contexts = Arc::new(WaitingContext(entered.clone()));
    let first = input(true);
    let turn = first.turn_id.clone();
    let conversation = first.conversation_id.clone();
    let (tx, _rx) = mpsc::channel(256);
    runtime.start(first, provider.clone(), tx).await.unwrap();
    entered.notified().await;
    runtime.cancel_and_wait(&turn).await;
    assert_eq!(
        runtime.snapshot(&turn).unwrap().status,
        TurnStatus::Cancelled
    );
    assert!(provider.requests.lock().unwrap().is_empty());
    assert!(
        !serde_json::to_string(&store.load(&conversation).await.unwrap())
            .unwrap()
            .contains("secret-context")
    );
}
struct WaitingTool(Arc<tokio::sync::Notify>);
#[async_trait]
impl ToolExecutor for WaitingTool {
    async fn execute(&self, _: Value, _: CancellationToken) -> ToolResult {
        self.0.notify_one();
        std::future::pending().await
    }
}
#[tokio::test]
async fn cancellation_during_execution_preserves_unknown_result_and_pairing() {
    let (runtime, provider, store) = setup(vec![response("", vec![call("a")])], false, false);
    let definition = runtime.tools.lookup("echo").unwrap();
    runtime.tools.unregister("echo");
    let entered = Arc::new(tokio::sync::Notify::new());
    runtime
        .tools
        .register(definition, Arc::new(WaitingTool(entered.clone())))
        .unwrap();
    let first = input(false);
    let turn = first.turn_id.clone();
    let conversation = first.conversation_id.clone();
    let (tx, _rx) = mpsc::channel(256);
    runtime.start(first, provider.clone(), tx).await.unwrap();
    entered.notified().await;
    runtime.cancel_and_wait(&turn).await;
    let saved = store.load(&conversation).await.unwrap().unwrap();
    assert_eq!(
        saved.transcript[2].message.tool_call_id.as_deref(),
        Some("a")
    );
    assert!(
        saved.transcript[2]
            .message
            .text_content()
            .contains("interrupted_unknown")
    );
    assert_eq!(
        runtime.snapshot(&turn).unwrap().status,
        TurnStatus::Cancelled
    );
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
}
struct SlowHook;
#[async_trait]
impl hooks::AssistantHook for SlowHook {
    async fn run(&self, _: &HookContext) -> Result<(), ToolError> {
        std::future::pending().await
    }
}
#[tokio::test(start_paused = true)]
async fn observer_timeout_isolated_and_guard_prevents_provider_call() {
    let (runtime, provider, _) = setup(vec![response("ok", vec![])], false, false);
    let _observer = runtime
        .hooks
        .register(HookPhase::TextDelta, false, Arc::new(SlowHook))
        .unwrap();
    let (tx, rx) = mpsc::channel(256);
    runtime
        .start(input(false), provider.clone(), tx)
        .await
        .unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: false,
            persist: false,
        },
    )
    .await;
    assert!(matches!(
        events.last().unwrap().kind,
        AssistantEventKind::TurnCompleted { .. }
    ));
    let _guard = runtime
        .hooks
        .register(HookPhase::BeforeModel, true, Arc::new(SlowHook))
        .unwrap();
    let (tx, rx) = mpsc::channel(256);
    runtime
        .start(input(false), provider.clone(), tx)
        .await
        .unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: false,
            persist: false,
        },
    )
    .await;
    assert!(
        matches!(&events.last().unwrap().kind,AssistantEventKind::TurnFailed{code,..} if code=="hook_guard_failed")
    );
    assert_eq!(provider.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn nonstreaming_plain_chat_needs_no_tools() {
    let (runtime, mut provider, _) = setup(vec![response("nonstream", vec![])], false, false);
    Arc::get_mut(&mut provider).unwrap().stream = false;
    let (tx, rx) = mpsc::channel(256);
    runtime.start(input(false), provider, tx).await.unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: false,
            persist: false,
        },
    )
    .await;
    assert_eq!(
        events
            .iter()
            .filter(
                |e| matches!(&e.kind,AssistantEventKind::TextDelta{text,..} if text=="nonstream")
            )
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn mock_provider_remains_offline_and_does_not_discover_tools() {
    let (runtime, _, _) = setup(vec![], false, false);
    let (tx, rx) = mpsc::channel(256);
    runtime
        .start(
            input(false),
            Arc::new(deskaide_ai_provider::MockProvider::new()),
            tx,
        )
        .await
        .unwrap();
    let events = drain(
        &runtime,
        rx,
        ApprovalDecision {
            allow: false,
            persist: false,
        },
    )
    .await;
    assert!(matches!(
        events.last().unwrap().kind,
        AssistantEventKind::TurnCompleted { .. }
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.kind, AssistantEventKind::ToolProposed { .. }))
    );
}
