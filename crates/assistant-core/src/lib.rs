//! Shared domain types for DeskAide.
mod agent;
pub use agent::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TargetWindow {
    pub id: String,
    pub application_name: Option<String>,
    pub process_name: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContextSourceType {
    SelectedText,
    Clipboard,
    WebPage,
    ActiveWindowText,
    ActiveWindowScreenshot,
    RegionScreenshot,
    ScreenScreenshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachment {
    pub mime_type: String,
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextPayload {
    pub source_type: ContextSourceType,
    pub application_name: Option<String>,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub selected_text: Option<String>,
    pub main_text: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub images: Vec<ImageAttachment>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        path: String,
        width: Option<u32>,
        height: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelMessage {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            max_output_tokens: Some(512),
            temperature: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequest {
    pub request_id: String,
    pub model_profile_id: String,
    pub conversation_id: String,
    pub system_prompt: Option<String>,
    pub messages: Vec<ModelMessage>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub generation_options: GenerationOptions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    #[serde(default)]
    pub supports_tools: bool,
    pub supports_text: bool,
    pub supports_images: bool,
    pub supports_streaming: bool,
    pub supports_system_message: bool,
    pub max_images: Option<usize>,
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelResponse {
    pub content: String,
    pub finish_reason: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}
