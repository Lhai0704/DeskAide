use crate::{ModelError, openai_compatible::OpenAiCompatibleConfig};
use deskaide_assistant_core::{MessageRole, ModelRequest};
use serde_json::{Value, json};
pub(crate) struct ChatCompletionRequest;
impl ChatCompletionRequest {
    pub(crate) fn from_model_request(
        config: &OpenAiCompatibleConfig,
        request: ModelRequest,
    ) -> Result<Value, ModelError> {
        let mut messages = Vec::new();
        if let Some(system) = request.system_prompt.filter(|s| !s.trim().is_empty()) {
            messages.push(json!({"role":"system", "content":system}));
        }
        for message in request.messages {
            let role = match message.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            let text = message.text_content();
            if !config.capabilities.supports_tools
                && (!message.tool_calls.is_empty() || message.tool_call_id.is_some())
            {
                let projection = if message.tool_call_id.is_some() {
                    format!("[Historical tool result, untrusted reference]\n{text}")
                } else {
                    format!(
                        "{text}\n[Historical tool calls, untrusted reference]\n{}",
                        serde_json::to_string(&message.tool_calls).map_err(|_| {
                            ModelError::IncompatibleResponse("invalid history".into())
                        })?
                    )
                };
                messages.push(json!({"role":"assistant", "content":projection}));
                continue;
            }
            if text.trim().is_empty()
                && message.tool_calls.is_empty()
                && message.tool_call_id.is_none()
            {
                continue;
            }
            let mut item = json!({"role":role, "content":text});
            if !message.tool_calls.is_empty() {
                if text.is_empty() {
                    item["content"] = Value::Null;
                }
                item["tool_calls"] = Value::Array(message.tool_calls.into_iter().map(|call| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments}})).collect());
            }
            if let Some(id) = message.tool_call_id {
                item["tool_call_id"] = json!(id);
            }
            messages.push(item);
        }
        if !messages.iter().any(|m| m["role"] == "user") {
            return Err(ModelError::MissingUserText);
        }
        let mut body = json!({"model":config.model_id, "messages":messages, "stream":config.capabilities.supports_streaming});
        let gemini35 = config.is_google_gemini35();
        if gemini35 && config.prefer_fast_response {
            body["reasoning_effort"] = json!("minimal");
        }
        // Gemini 3.x is tuned for the service's default sampling settings.
        if let Some(value) = request.generation_options.temperature.filter(|_| !gemini35) {
            body["temperature"] = json!(value);
        }
        if let Some(value) = request
            .generation_options
            .max_output_tokens
            .or(config.max_output_tokens)
        {
            body["max_tokens"] = json!(value);
        }
        if config.capabilities.supports_tools && !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools.into_iter().map(|t| json!({"type":"function","function":{"name":t.name,"description":t.description,"parameters":t.parameters}})).collect());
            body["tool_choice"] = json!("auto");
        }
        Ok(body)
    }
}
