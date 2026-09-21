use deskaide_assistant_core::ToolError;
use std::{collections::HashMap, sync::Mutex};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy)]
pub struct ApprovalDecision {
    pub allow: bool,
    pub persist: bool,
}
struct Waiter {
    conversation: String,
    turn: String,
    sender: oneshot::Sender<ApprovalDecision>,
}
#[derive(Default)]
pub struct ApprovalService {
    waiters: Mutex<HashMap<String, Waiter>>,
}
impl ApprovalService {
    pub fn register(
        &self,
        id: &str,
        conversation: &str,
        turn: &str,
    ) -> Result<oneshot::Receiver<ApprovalDecision>, ToolError> {
        let mut waiters = self.waiters.lock().unwrap_or_else(|e| e.into_inner());
        if waiters.len() >= 32 || waiters.contains_key(id) {
            return Err(ToolError::new("approval_limit", "批准请求数量超过限制"));
        }
        let (sender, receiver) = oneshot::channel();
        waiters.insert(
            id.into(),
            Waiter {
                conversation: conversation.into(),
                turn: turn.into(),
                sender,
            },
        );
        Ok(receiver)
    }
    pub fn resolve(
        &self,
        id: &str,
        conversation: &str,
        turn: &str,
        decision: ApprovalDecision,
    ) -> Result<(), ToolError> {
        let mut waiters = self.waiters.lock().unwrap_or_else(|e| e.into_inner());
        if !waiters
            .get(id)
            .is_some_and(|w| w.conversation == conversation && w.turn == turn)
        {
            return Err(ToolError::new("stale_approval", "此批准请求已失效"));
        }
        if let Some(w) = waiters.remove(id) {
            w.sender
                .send(decision)
                .map_err(|_| ToolError::new("stale_approval", "此批准请求已结束"))?;
        }
        Ok(())
    }
    pub fn remove(&self, id: &str) {
        self.waiters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }
    pub fn cancel_turn(&self, turn: &str) {
        self.waiters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, w| w.turn != turn);
    }
}
