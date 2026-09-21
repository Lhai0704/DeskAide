use async_trait::async_trait;
use deskaide_assistant_core::{
    ContentBlock, ModelCapabilities, ModelRequest, ModelResponse, ProviderEvent,
};
use tokio::time::Duration;

use crate::{ModelError, ModelProvider, ProviderEventSender, send};

#[derive(Debug, Default)]
pub struct MockProvider;

impl MockProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    fn id(&self) -> &str {
        "mock-local"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            supports_tools: false,
            supports_text: true,
            supports_images: false,
            supports_streaming: true,
            supports_system_message: true,
            max_images: Some(0),
            context_window: Some(4_096),
        }
    }

    async fn complete(
        &self,
        request: ModelRequest,
        event_sender: ProviderEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let _prompt = request
            .messages
            .iter()
            .rev()
            .flat_map(|message| message.content.iter())
            .find_map(|block| match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.trim()),
                _ => None,
            })
            .ok_or(ModelError::MissingUserText)?;

        // Do not echo the composed prompt: it may contain turn-only desktop context.
        let response_text = "Mock 助手已收到你的问题。\n\n切换到已配置的 OpenAI-Compatible Profile 后即可使用真实模型。".to_owned();

        for chunk in response_text.chars().collect::<Vec<_>>().chunks(5) {
            tokio::time::sleep(Duration::from_millis(30)).await;
            send(
                &event_sender,
                ProviderEvent::TextDelta(chunk.iter().collect()),
            )
            .await?;
        }

        let response = ModelResponse {
            tool_calls: vec![],
            usage: None,
            content: response_text,
            finish_reason: "stop".to_owned(),
        };
        Ok(response)
    }
}
