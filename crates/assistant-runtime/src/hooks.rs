use async_trait::async_trait;
use deskaide_assistant_core::{ModelMessage, ModelRequest, ToolCall, ToolError, ToolResult};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPhase {
    BeforeCompose,
    AfterCompose,
    BeforeModel,
    TextDelta,
    ReasoningDelta,
    ToolProposed,
    ToolStarted,
    ToolFinished,
    TurnCompleted,
    TurnFailed,
    TurnCancelled,
}
#[derive(Debug, Clone, Default)]
pub enum HookData {
    #[default]
    Empty,
    Text(String),
    Prompt(String),
    Composed(Vec<ModelMessage>),
    ModelRequest(ModelRequest),
    Tool(ToolCall),
    ToolResult {
        call: ToolCall,
        result: ToolResult,
    },
    Error(ToolError),
}
#[derive(Debug, Clone)]
pub struct HookContext {
    pub conversation_id: String,
    pub turn_id: String,
    pub phase: HookPhase,
    pub data: HookData,
}
#[async_trait]
pub trait AssistantHook: Send + Sync {
    async fn run(&self, context: &HookContext) -> Result<(), ToolError>;
}
struct Entry {
    id: u64,
    phase: HookPhase,
    guard: bool,
    hook: Arc<dyn AssistantHook>,
}
#[derive(Default)]
struct Entries {
    next: u64,
    entries: Vec<Arc<Entry>>,
}
#[derive(Clone, Default)]
pub struct HookRegistry(Arc<Mutex<Entries>>);
pub struct Subscription {
    id: u64,
    registry: Weak<Mutex<Entries>>,
}
impl Subscription {
    pub fn unsubscribe(&self) {
        if let Some(r) = self.registry.upgrade() {
            r.lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .retain(|e| e.id != self.id);
        }
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}
impl HookRegistry {
    pub fn register(
        &self,
        phase: HookPhase,
        guard: bool,
        hook: Arc<dyn AssistantHook>,
    ) -> Result<Subscription, ToolError> {
        if guard
            && !matches!(
                phase,
                HookPhase::BeforeCompose | HookPhase::AfterCompose | HookPhase::BeforeModel
            )
        {
            return Err(ToolError::new("invalid_hook", "此阶段不能注册 guard"));
        }
        let mut r = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if r.entries.len() >= 128 {
            return Err(ToolError::new("hook_limit", "Hook 数量超过限制"));
        }
        r.next += 1;
        let id = r.next;
        r.entries.push(Arc::new(Entry {
            id,
            phase,
            guard,
            hook,
        }));
        Ok(Subscription {
            id,
            registry: Arc::downgrade(&self.0),
        })
    }
    pub fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .clear();
    }
    pub async fn dispatch(
        &self,
        ctx: &HookContext,
        disabled: &mut HashSet<u64>,
        cancel: &CancellationToken,
    ) -> Result<(), ToolError> {
        let entries = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .clone();
        for e in entries.into_iter().filter(|e| e.phase == ctx.phase) {
            if disabled.contains(&e.id) {
                continue;
            }
            let result = tokio::select! {
                _=cancel.cancelled()=>return Err(ToolError::new("cancelled","已取消")),
                r=tokio::time::timeout(Duration::from_millis(200),e.hook.run(ctx))=>r.unwrap_or_else(|_|Err(ToolError::new("hook_timeout","Hook 超时"))),
            };
            if let Err(error) = result {
                eprintln!(
                    "assistant hook {} failed at {:?}: {}",
                    e.id,
                    ctx.phase,
                    if error.code == "hook_timeout" {
                        "timeout"
                    } else {
                        "listener_error"
                    }
                );
                if e.guard {
                    return Err(ToolError::new("hook_guard_failed", "请求前检查失败"));
                }
                disabled.insert(e.id);
            }
        }
        Ok(())
    }
}
