mod config;
mod request;
mod response;
mod stream;

use std::time::Duration;

#[cfg(debug_assertions)]
use std::time::Instant;

use async_trait::async_trait;
use deskaide_assistant_core::{ModelCapabilities, ModelRequest, ModelResponse, ResponseEvent};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};

use crate::{ModelError, ModelProvider, ProviderErrorDetails, ResponseEventSender, send};

pub use config::{OpenAiCompatibleConfig, SecretString};
use request::ChatCompletionRequest;
use response::{ChatCompletionResponse, ErrorEnvelope};
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
        request_id: String,
        response: reqwest::Response,
        sender: &ResponseEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let mut bytes = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut content = String::new();
        let mut finish_reason = None;
        let mut complete = false;

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(ModelError::from_stream_reqwest)?;
            for data in parser.push(&chunk)? {
                if data.trim() == "[DONE]" {
                    complete = true;
                    continue;
                }
                let event = response::parse_stream_event(&data)?;
                if let Some(text) = event.text {
                    content.push_str(&text);
                    send(
                        sender,
                        ResponseEvent::Delta {
                            request_id: request_id.clone(),
                            text,
                        },
                    )?;
                }
                if event.finish_reason.is_some() {
                    finish_reason = event.finish_reason;
                    complete = true;
                }
            }
        }
        for data in parser.finish()? {
            if data.trim() == "[DONE]" {
                complete = true;
            } else {
                let event = response::parse_stream_event(&data)?;
                if let Some(text) = event.text {
                    content.push_str(&text);
                    send(
                        sender,
                        ResponseEvent::Delta {
                            request_id: request_id.clone(),
                            text,
                        },
                    )?;
                }
                if event.finish_reason.is_some() {
                    finish_reason = event.finish_reason;
                    complete = true;
                }
            }
        }
        if !complete {
            return Err(ModelError::StreamInterrupted);
        }
        Ok(ModelResponse {
            content,
            finish_reason: finish_reason.unwrap_or_else(|| "stop".to_owned()),
        })
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
        event_sender: ResponseEventSender,
    ) -> Result<ModelResponse, ModelError> {
        let request_id = request.request_id.clone();
        let payload = ChatCompletionRequest::from_model_request(&self.config, request)?;
        let endpoint = self.config.chat_completions_url()?;
        #[cfg(debug_assertions)]
        {
            eprintln!(
                "{}\n",
                payload.debug_view(&request_id, &self.config.profile_id, &endpoint)
            );
        }
        #[cfg(debug_assertions)]
        let started_at = Instant::now();
        let builder = self
            .client
            .post(endpoint)
            .bearer_auth(self.config.api_key.expose())
            .headers(self.config.header_map()?)
            .json(&payload);
        let result = async {
            let response = builder.send().await.map_err(ModelError::from_reqwest)?;
            if !response.status().is_success() {
                return Err(parse_http_error(response, self.config.api_key.expose()).await);
            }

            send(
                &event_sender,
                ResponseEvent::Started {
                    request_id: request_id.clone(),
                },
            )?;
            let response = if self.config.capabilities.supports_streaming {
                self.complete_streaming(request_id.clone(), response, &event_sender)
                    .await?
            } else {
                let response: ChatCompletionResponse = response.json().await.map_err(|_| {
                    ModelError::IncompatibleResponse("invalid JSON body".to_owned())
                })?;
                response.into_model_response()?
            };
            send(
                &event_sender,
                ResponseEvent::Completed {
                    request_id: request_id.clone(),
                    response: response.clone(),
                },
            )?;
            Ok(response)
        }
        .await;

        #[cfg(debug_assertions)]
        log_model_result(&request_id, &result, started_at.elapsed());

        result
    }
}

#[cfg(debug_assertions)]
fn log_model_result(
    request_id: &str,
    result: &Result<ModelResponse, ModelError>,
    elapsed: Duration,
) {
    let elapsed_ms = elapsed.as_millis();
    match result {
        Ok(response) => eprintln!(
            "============================================================\n\
             [DeskAide] MODEL RESPONSE\n\
             ============================================================\n\
             Request ID   : {request_id}\n\
             Duration     : {elapsed_ms} ms\n\
             Finish reason: {}\n\
             \n\
             ---------------- RESPONSE CONTENT ----------------\n\
             {}\n\
             -------------- END RESPONSE CONTENT --------------\n\
             ============================================================\n",
            response.finish_reason, response.content
        ),
        Err(error) => eprintln!(
            "============================================================\n\
             [DeskAide] MODEL REQUEST FAILED\n\
             ============================================================\n\
             Request ID: {request_id}\n\
             Duration  : {elapsed_ms} ms\n\
             Error code: {}\n\
             Error     : {error}\n\
             ============================================================\n",
            error.code()
        ),
    }
}

async fn parse_http_error(response: reqwest::Response, api_key: &str) -> ModelError {
    let status = response.status();
    let retry_header = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let body = response.text().await.unwrap_or_default();
    let parsed = serde_json::from_str::<ErrorEnvelope>(&body).ok();
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
