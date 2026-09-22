use crate::{ContentBlock, ContextSourceType, MessageRole, ModelMessage, TargetWindow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EVENT_VERSION: u32 = 1;
pub const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1024;
pub const MAX_TOOL_RESULT_BYTES: usize = 128 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_TOOLS_PER_STEP: usize = 16;
pub const EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolRisk {
    ReadOnly,
    UserData,
    Mutating,
    ExternalSideEffect,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolSource {
    Internal,
    Mcp {
        server_id: String,
        server_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub source: ToolSource,
    pub risk: ToolRisk,
    pub revision: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}
// Never accidentally log tool arguments through Debug.
impl std::fmt::Debug for ToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCall")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolError {
    pub code: String,
    pub message: String,
}
impl ToolError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub content: Value,
    pub error: Option<ToolError>,
}
impl ToolResult {
    pub fn failure(code: &str, message: &str) -> Self {
        Self {
            content: Value::Null,
            error: Some(ToolError::new(code, message)),
        }
    }
    pub fn success(content: Value) -> Self {
        Self {
            content,
            error: None,
        }
    }
    pub fn model_text(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| "{\"error\":\"serialization_failed\"}".into())
    }
}
impl std::fmt::Debug for ToolResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolResult")
            .field("error_code", &self.error.as_ref().map(|e| &e.code))
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    TextDelta(String),
    ReasoningDelta(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextContextDraft {
    pub id: String,
    pub source: ContextSourceType,
    pub target: Option<TargetWindow>,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContextSelection {
    pub target: Option<TargetWindow>,
    pub sources: Vec<ContextSourceType>,
    pub drafts: Vec<TextContextDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContextCollectionStatus {
    Added,
    Unavailable,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextCollectionResult {
    pub source: ContextSourceType,
    pub status: ContextCollectionStatus,
    pub character_count: usize,
    pub truncated: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Non-secret provenance retained even when call arguments/results are omitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolProvenance {
    pub call_id: String,
    pub source: ToolSource,
    pub risk: ToolRisk,
    pub definition_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMessage {
    pub id: String,
    pub turn_id: String,
    #[serde(flatten)]
    pub message: ModelMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub omitted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_status: Option<TurnStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_provenance: Vec<ToolProvenance>,
}

impl ModelMessage {
    pub fn text(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub model_profile_id: String,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub transcript: Vec<TranscriptMessage>,
    pub active_turn: Option<String>,
}
impl Session {
    pub fn new(id: String, profile: String) -> Self {
        Self {
            id,
            title: String::new(),
            model_profile_id: profile,
            revision: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
            transcript: vec![],
            active_turn: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolApproval {
    pub approval_id: String,
    pub call: ToolCall,
    pub definition: ToolDefinition,
    pub sensitive_context: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantEvent {
    pub version: u32,
    pub conversation_id: String,
    pub turn_id: String,
    pub sequence: u64,
    #[serde(flatten)]
    pub kind: AssistantEventKind,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AssistantEventKind {
    TurnStarted,
    ContextPrepared {
        results: Vec<ContextCollectionResult>,
    },
    MessageStarted {
        message_id: String,
        model_step: usize,
    },
    TextDelta {
        message_id: String,
        text: String,
    },
    ReasoningDelta {
        message_id: String,
        text: String,
    },
    MessageCompleted {
        message_id: String,
        content: String,
    },
    ToolProposed {
        call: ToolCall,
    },
    ToolApprovalRequired {
        approval: Box<ToolApproval>,
    },
    ToolStarted {
        tool_call_id: String,
    },
    ToolCompleted {
        tool_call_id: String,
    },
    ToolFailed {
        tool_call_id: String,
        code: String,
    },
    Usage {
        model_step: usize,
        usage: TokenUsage,
    },
    TurnCompleted {
        revision: u64,
    },
    TurnFailed {
        revision: u64,
        code: String,
        message: String,
    },
    TurnCancelled {
        revision: u64,
    },
    Warning {
        code: String,
        message: String,
    },
}
impl AssistantEventKind {
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::TurnCompleted { .. } | Self::TurnFailed { .. } | Self::TurnCancelled { .. }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnSnapshot {
    pub phase: TurnPhase,
    pub conversation_id: String,
    pub turn_id: String,
    pub sequence: u64,
    pub status: TurnStatus,
    pub content: String,
    pub approval: Option<ToolApproval>,
    pub revision: u64,
    pub error: Option<ToolError>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TurnPhase {
    #[default]
    Preparing,
    Generating,
    Responding,
    Tool,
    Approval,
    Terminal,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_contract() {
        let value = serde_json::to_value(AssistantEvent {
            version: EVENT_VERSION,
            conversation_id: "c".into(),
            turn_id: "t".into(),
            sequence: 1,
            kind: AssistantEventKind::TextDelta {
                message_id: "m".into(),
                text: "ok".into(),
            },
        })
        .unwrap();
        assert_eq!(value["type"], "textDelta");
        assert_eq!(value["turnId"], "t");
        assert_eq!(value["messageId"], "m");
    }
}
