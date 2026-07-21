use deskaide_assistant_core::{ContentBlock, MessageRole, ModelRequest};
use serde::Serialize;

use crate::{ModelError, openai_compatible::OpenAiCompatibleConfig};

#[derive(Debug, Serialize)]
pub(crate) struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

impl ChatCompletionRequest {
    pub(crate) fn from_model_request(
        config: &OpenAiCompatibleConfig,
        request: ModelRequest,
    ) -> Result<Self, ModelError> {
        let mut messages = Vec::new();
        if let Some(system) = request
            .system_prompt
            .filter(|value| !value.trim().is_empty())
        {
            messages.push(ChatMessage {
                role: "system",
                content: system,
            });
        }
        for message in request.messages {
            let content = message
                .content
                .into_iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text),
                    ContentBlock::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !content.trim().is_empty() {
                messages.push(ChatMessage {
                    role: match message.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                    },
                    content,
                });
            }
        }
        if !messages.iter().any(|message| message.role == "user") {
            return Err(ModelError::MissingUserText);
        }
        Ok(Self {
            model: config.model_id.clone(),
            messages,
            stream: config.capabilities.supports_streaming,
            temperature: request.generation_options.temperature,
            max_tokens: request
                .generation_options
                .max_output_tokens
                .or(config.max_output_tokens),
        })
    }
}
