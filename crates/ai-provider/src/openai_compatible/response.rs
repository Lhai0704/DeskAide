use deskaide_assistant_core::ModelResponse;
use serde::Deserialize;

use crate::ModelError;

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorBody {
    pub message: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "string_or_number", default)]
    pub code: Option<String>,
    pub retry_after: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionResponse {
    choices: Vec<ResponseChoice>,
}

#[derive(Debug, Deserialize)]
struct ResponseChoice {
    message: ResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

impl ChatCompletionResponse {
    pub(crate) fn into_model_response(self) -> Result<ModelResponse, ModelError> {
        let choice = self.choices.into_iter().next().ok_or_else(|| {
            ModelError::IncompatibleResponse("response contains no choices".to_owned())
        })?;
        Ok(ModelResponse {
            content: choice.message.content.ok_or_else(|| {
                ModelError::IncompatibleResponse("response contains no assistant text".to_owned())
            })?,
            finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".to_owned()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct StreamEnvelope {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

pub(crate) struct ParsedStreamEvent {
    pub text: Option<String>,
    pub finish_reason: Option<String>,
}

pub(crate) fn parse_stream_event(data: &str) -> Result<ParsedStreamEvent, ModelError> {
    let response: StreamEnvelope = serde_json::from_str(data).map_err(|_| {
        ModelError::IncompatibleResponse("invalid JSON in streaming response".to_owned())
    })?;
    let choice = response.choices.into_iter().next();
    Ok(ParsedStreamEvent {
        text: choice
            .as_ref()
            .and_then(|choice| choice.delta.content.clone()),
        finish_reason: choice.and_then(|choice| choice.finish_reason),
    })
}

fn string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::String(value) => Some(value),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }))
}
