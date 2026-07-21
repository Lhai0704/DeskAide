//! Replaceable model-provider contracts and implementations.

mod error;
mod mock;
pub mod openai_compatible;

use async_trait::async_trait;
use deskaide_assistant_core::{ModelCapabilities, ModelRequest, ModelResponse, ResponseEvent};
use tokio::sync::mpsc::UnboundedSender;

pub use error::{ModelError, ProviderErrorDetails};
pub use mock::MockProvider;
pub use openai_compatible::OpenAiCompatibleProvider;

pub type ResponseEventSender = UnboundedSender<ResponseEvent>;

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

pub(crate) fn send(sender: &ResponseEventSender, event: ResponseEvent) -> Result<(), ModelError> {
    sender
        .send(event)
        .map_err(|_| ModelError::ResponseReceiverClosed)
}
