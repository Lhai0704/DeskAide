//! Replaceable model-provider contracts and the phase-one mock implementation.

use async_trait::async_trait;
use deskaide_assistant_core::{
    ContentBlock, ModelCapabilities, ModelRequest, ModelResponse, ResponseEvent,
};
use thiserror::Error;
use tokio::{sync::mpsc::UnboundedSender, time::Duration};

pub type ResponseEventSender = UnboundedSender<ResponseEvent>;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("the model request contains no user text")]
    MissingUserText,
    #[error("the response receiver was closed")]
    ResponseReceiverClosed,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;

    fn capabilities(&self) -> ModelCapabilities;

    async fn complete(
        &self,
        request: ModelRequest,
        event_sender: ResponseEventSender,
    ) -> Result<ModelResponse, ModelError>;
}

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
        event_sender: ResponseEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let prompt = request
            .messages
            .iter()
            .rev()
            .flat_map(|message| message.content.iter())
            .find_map(|block| match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.trim()),
                _ => None,
            })
            .ok_or(ModelError::MissingUserText)?;

        let response_text = format!(
            "Mock 助手已收到你的问题：\u{300c}{prompt}\u{300d}\n\n真实模型 Provider 将在后续阶段接入。"
        );
        let request_id = request.request_id;

        send(
            &event_sender,
            ResponseEvent::Started {
                request_id: request_id.clone(),
            },
        )?;

        for chunk in response_text.chars().collect::<Vec<_>>().chunks(5) {
            tokio::time::sleep(Duration::from_millis(18)).await;
            send(
                &event_sender,
                ResponseEvent::Delta {
                    request_id: request_id.clone(),
                    text: chunk.iter().collect(),
                },
            )?;
        }

        let response = ModelResponse {
            content: response_text,
            finish_reason: "stop".to_owned(),
        };
        send(
            &event_sender,
            ResponseEvent::Completed {
                request_id,
                response: response.clone(),
            },
        )?;

        Ok(response)
    }
}

fn send(sender: &ResponseEventSender, event: ResponseEvent) -> Result<(), ModelError> {
    sender
        .send(event)
        .map_err(|_| ModelError::ResponseReceiverClosed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deskaide_assistant_core::{GenerationOptions, MessageRole, ModelMessage};

    #[tokio::test]
    async fn mock_provider_streams_a_complete_response() {
        let provider = MockProvider::new();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let request = ModelRequest {
            request_id: "request-1".to_owned(),
            model_profile_id: provider.id().to_owned(),
            conversation_id: "conversation-1".to_owned(),
            system_prompt: None,
            messages: vec![ModelMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "你好".to_owned(),
                }],
            }],
            context: Vec::new(),
            generation_options: GenerationOptions::default(),
        };

        let response = provider.complete(request, sender).await.unwrap();
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            events.push(event);
        }

        assert!(response.content.contains("你好"));
        assert!(matches!(
            events.first(),
            Some(ResponseEvent::Started { .. })
        ));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ResponseEvent::Delta { .. }))
        );
        assert!(matches!(
            events.last(),
            Some(ResponseEvent::Completed { .. })
        ));
    }

    #[test]
    fn mock_provider_declares_text_only_capabilities() {
        let capabilities = MockProvider::new().capabilities();
        assert!(capabilities.supports_text);
        assert!(capabilities.supports_streaming);
        assert!(!capabilities.supports_images);
    }
}
