//! Shared domain types for DeskAide.

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContextSourceType {
    SelectedText,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
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
    pub context: Vec<ContextPayload>,
    #[serde(default)]
    pub generation_options: GenerationOptions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResponseEvent {
    Started {
        request_id: String,
    },
    Delta {
        request_id: String,
        text: String,
    },
    Completed {
        request_id: String,
        response: ModelResponse,
    },
    Failed {
        request_id: String,
        code: String,
        message: String,
    },
    Cancelled {
        request_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_events_use_the_frontend_wire_format() {
        let event = ResponseEvent::Completed {
            request_id: "request-1".to_owned(),
            response: ModelResponse {
                content: "done".to_owned(),
                finish_reason: "stop".to_owned(),
            },
        };
        let value = serde_json::to_value(event).unwrap();

        assert_eq!(value["type"], "completed");
        assert_eq!(value["requestId"], "request-1");
        assert_eq!(value["response"]["finishReason"], "stop");
        assert!(value.get("request_id").is_none());
    }

    #[test]
    fn cancelled_events_use_the_frontend_wire_format() {
        let value = serde_json::to_value(ResponseEvent::Cancelled {
            request_id: "request-2".to_owned(),
        })
        .unwrap();

        assert_eq!(value["type"], "cancelled");
        assert_eq!(value["requestId"], "request-2");
    }

    #[test]
    fn failed_events_include_a_machine_readable_error_code() {
        let value = serde_json::to_value(ResponseEvent::Failed {
            request_id: "request-3".to_owned(),
            code: "rate_limited".to_owned(),
            message: "try later".to_owned(),
        })
        .unwrap();
        assert_eq!(value["code"], "rate_limited");
        assert_eq!(value["message"], "try later");
    }

    #[test]
    fn context_sources_use_frontend_wire_names() {
        assert_eq!(
            serde_json::to_string(&ContextSourceType::SelectedText).unwrap(),
            "\"selectedText\""
        );
        assert_eq!(
            serde_json::to_string(&ContextSourceType::ActiveWindowText).unwrap(),
            "\"activeWindowText\""
        );
    }
}
