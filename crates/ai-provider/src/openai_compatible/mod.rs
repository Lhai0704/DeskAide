mod config;
mod request;
mod response;
mod stream;

use std::time::Duration;

use async_trait::async_trait;
use deskaide_assistant_core::{ModelCapabilities, ModelRequest, ModelResponse, ProviderEvent};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};

use crate::{ModelError, ModelProvider, ProviderErrorDetails, ProviderEventSender, send};

pub use config::{OpenAiCompatibleConfig, SecretString};
use request::ChatCompletionRequest;
use response::ErrorEnvelope;
use stream::SseParser;

#[derive(Debug)]
pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, ModelError> {
        config.validate()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(ModelError::from_reqwest)?;
        Ok(Self { config, client })
    }

    pub async fn test_connection(&self) -> Result<(), ModelError> {
        let response = self
            .request_builder(self.config.model_url()?)?
            .send()
            .await
            .map_err(ModelError::from_reqwest)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(parse_http_error(response, self.config.api_key.expose()).await)
        }
    }

    fn request_builder(&self, url: url::Url) -> Result<reqwest::RequestBuilder, ModelError> {
        let builder = self
            .client
            .get(url)
            .bearer_auth(self.config.api_key.expose())
            .headers(self.config.header_map()?);
        Ok(builder)
    }

    async fn complete_streaming(
        &self,
        response: reqwest::Response,
        sender: &ProviderEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let mut bytes = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut output = response::StreamAccumulator::default();
        while let Some(chunk) = bytes.next().await {
            for data in parser.push(&chunk.map_err(ModelError::from_stream_reqwest)?)? {
                let (text, reasoning) = output.push(&data)?;
                if data.trim() == "[DONE]" {
                    return output.finish();
                }
                if let Some(text) = text {
                    send(sender, ProviderEvent::TextDelta(text)).await?;
                }
                if let Some(text) = reasoning {
                    send(sender, ProviderEvent::ReasoningDelta(text)).await?;
                }
            }
        }
        for data in parser.finish()? {
            let (text, reasoning) = output.push(&data)?;
            if let Some(text) = text {
                send(sender, ProviderEvent::TextDelta(text)).await?;
            }
            if let Some(text) = reasoning {
                send(sender, ProviderEvent::ReasoningDelta(text)).await?;
            }
        }
        output.finish()
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        &self.config.profile_id
    }

    fn capabilities(&self) -> ModelCapabilities {
        self.config.capabilities
    }

    async fn complete(
        &self,
        request: ModelRequest,
        event_sender: ProviderEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let payload = ChatCompletionRequest::from_model_request(&self.config, request)?;
        let response = self
            .client
            .post(self.config.chat_completions_url()?)
            .bearer_auth(self.config.api_key.expose())
            .headers(self.config.header_map()?)
            .json(&payload)
            .send()
            .await
            .map_err(ModelError::from_reqwest)?;
        if !response.status().is_success() {
            return Err(parse_http_error(response, self.config.api_key.expose()).await);
        }
        if self.config.capabilities.supports_streaming {
            self.complete_streaming(response, &event_sender).await
        } else {
            let mut stream = response.bytes_stream();
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(ModelError::from_reqwest)?;
                if body.len() + chunk.len() > deskaide_assistant_core::MAX_RESPONSE_BYTES {
                    return Err(ModelError::IncompatibleResponse(
                        "response exceeds size limit".into(),
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            response::parse_response(
                serde_json::from_slice(&body)
                    .map_err(|_| ModelError::IncompatibleResponse("invalid JSON body".into()))?,
            )
        }
    }
}

async fn parse_http_error(response: reqwest::Response, api_key: &str) -> ModelError {
    let status = response.status();
    let retry_header = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) if body.len() + chunk.len() <= 64 * 1024 => body.extend_from_slice(&chunk),
            _ => {
                body.clear();
                break;
            }
        }
    }
    let parsed = serde_json::from_slice::<ErrorEnvelope>(&body).ok();
    let message = parsed
        .as_ref()
        .and_then(|body| body.error.message.clone())
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("provider request failed")
                .to_owned()
        });
    let message = if api_key.is_empty() {
        message
    } else {
        message.replace(api_key, "[REDACTED]")
    };
    let details = ProviderErrorDetails {
        provider_type: parsed.as_ref().and_then(|body| body.error.kind.clone()),
        provider_code: parsed.as_ref().and_then(|body| body.error.code.clone()),
        retry_after_seconds: parsed
            .as_ref()
            .and_then(|body| body.error.retry_after)
            .or(retry_header),
    };
    let provider_code = details
        .provider_code
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let message_lower = message.to_ascii_lowercase();
    let missing_model = provider_code.contains("model_not_found")
        || provider_code.contains("invalid_model")
        || (message_lower.contains("model") && message_lower.contains("not found"));
    match status {
        StatusCode::UNAUTHORIZED => ModelError::Authentication { message, details },
        StatusCode::FORBIDDEN | StatusCode::PAYMENT_REQUIRED => {
            ModelError::Permission { message, details }
        }
        StatusCode::NOT_FOUND => ModelError::ModelNotFound { message, details },
        _ if missing_model => ModelError::ModelNotFound { message, details },
        StatusCode::TOO_MANY_REQUESTS => ModelError::RateLimited { message, details },
        status if status.is_server_error() => ModelError::ProviderServer {
            status: status.as_u16(),
            message,
            details,
        },
        _ => ModelError::ProviderRequest {
            status: status.as_u16(),
            message,
            details,
        },
    }
}

#[cfg(test)]
mod tests;
