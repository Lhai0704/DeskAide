//! Model API adapters. Turn lifecycle and tool execution live in assistant-runtime.
mod error;
mod mock;
pub mod openai_compatible;
use async_trait::async_trait;
use deskaide_assistant_core::{ModelCapabilities, ModelRequest, ModelResponse, ProviderEvent};
pub use error::{ModelError, ProviderErrorDetails};
pub use mock::MockProvider;
pub use openai_compatible::OpenAiCompatibleProvider;
use tokio::sync::mpsc::Sender;
pub type ProviderEventSender = Sender<ProviderEvent>;
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities;
    async fn complete(
        &self,
        request: ModelRequest,
        events: ProviderEventSender,
    ) -> Result<ModelResponse, ModelError>;
}
pub(crate) async fn send(
    sender: &ProviderEventSender,
    event: ProviderEvent,
) -> Result<(), ModelError> {
    sender
        .send(event)
        .await
        .map_err(|_| ModelError::ResponseReceiverClosed)
}
