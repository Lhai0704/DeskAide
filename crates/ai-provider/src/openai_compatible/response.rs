use crate::ModelError;
use deskaide_assistant_core::{
    MAX_RESPONSE_BYTES, MAX_TOOL_ARGUMENT_BYTES, MAX_TOOLS_PER_STEP, ModelResponse, TokenUsage,
    ToolCall,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
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

fn invalid(message: &str) -> ModelError {
    ModelError::IncompatibleResponse(message.into())
}
fn usage(value: &Value) -> Option<TokenUsage> {
    let u = value.get("usage")?.as_object()?;
    Some(TokenUsage {
        input_tokens: u.get("prompt_tokens").and_then(Value::as_u64),
        output_tokens: u.get("completion_tokens").and_then(Value::as_u64),
        total_tokens: u.get("total_tokens").and_then(Value::as_u64),
    })
}
fn check_calls(calls: &[ToolCall]) -> Result<(), ModelError> {
    let mut ids = HashSet::new();
    if calls.len() > MAX_TOOLS_PER_STEP {
        return Err(invalid("too many tool calls"));
    }
    for c in calls {
        if c.id.is_empty()
            || c.id.len() > 256
            || c.name.is_empty()
            || c.name.len() > 64
            || !ids.insert(&c.id)
            || c.arguments.len() > MAX_TOOL_ARGUMENT_BYTES
        {
            return Err(invalid("invalid tool call metadata or size"));
        }
        // Invalid JSON is a tool validation error, not a reason to execute partial arguments.
    }
    Ok(())
}
pub(crate) fn parse_response(value: Value) -> Result<ModelResponse, ModelError> {
    let choice = value["choices"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or_else(|| invalid("response contains no choices"))?;
    let m = &choice["message"];
    if !m["content"].is_null() && !m["content"].is_string() {
        return Err(invalid("invalid assistant content"));
    }
    let content = m["content"].as_str().unwrap_or_default().to_owned();
    let mut calls = vec![];
    if let Some(items) = m.get("tool_calls").filter(|v| !v.is_null()) {
        for c in items
            .as_array()
            .ok_or_else(|| invalid("tool_calls must be an array"))?
        {
            if c["type"] != "function" {
                return Err(invalid("unsupported tool call type"));
            }
            calls.push(ToolCall {
                id: c["id"]
                    .as_str()
                    .ok_or_else(|| invalid("missing call ID"))?
                    .into(),
                name: c["function"]["name"]
                    .as_str()
                    .ok_or_else(|| invalid("missing tool name"))?
                    .into(),
                arguments: c["function"]["arguments"]
                    .as_str()
                    .ok_or_else(|| invalid("missing arguments"))?
                    .into(),
            });
        }
    }
    check_calls(&calls)?;
    if content.len() > MAX_RESPONSE_BYTES || (m["content"].as_str().is_none() && calls.is_empty()) {
        return Err(invalid("response contains no assistant text or tool calls"));
    }
    let finish = choice["finish_reason"].as_str().unwrap_or("stop");
    if !calls.is_empty() && !matches!(finish, "tool_calls" | "stop") {
        return Err(invalid("incomplete tool call response"));
    }
    if finish == "tool_calls" && calls.is_empty() {
        return Err(invalid("missing tool calls"));
    }
    Ok(ModelResponse {
        content,
        finish_reason: finish.into(),
        tool_calls: calls,
        usage: usage(&value),
    })
}
#[derive(Default)]
pub(crate) struct StreamAccumulator {
    pub content: String,
    calls: BTreeMap<usize, ToolCall>,
    finish: Option<String>,
    usage: Option<TokenUsage>,
    done: bool,
    bytes: usize,
}
impl StreamAccumulator {
    pub fn push(&mut self, data: &str) -> Result<(Option<String>, Option<String>), ModelError> {
        self.bytes = self.bytes.saturating_add(data.len());
        if self.bytes > 8 * MAX_RESPONSE_BYTES {
            return Err(invalid("stream exceeds size limit"));
        }
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok((None, None));
        }
        let v: Value = serde_json::from_str(data)
            .map_err(|_| invalid("invalid JSON in streaming response"))?;
        if let Some(u) = usage(&v) {
            self.usage = Some(u);
        }
        if self.done {
            return Ok((None, None));
        }
        let choices = v["choices"]
            .as_array()
            .ok_or_else(|| invalid("missing stream choices"))?;
        if choices
            .iter()
            .any(|c| c.get("index").is_some_and(|index| index.as_u64().is_none()))
        {
            return Err(invalid("invalid choice index"));
        }
        let Some(c) = choices
            .iter()
            .find(|c| c["index"].as_u64().unwrap_or(0) == 0)
        else {
            return Ok((None, None));
        };
        let d = &c["delta"];
        let text = d["content"].as_str().map(str::to_owned);
        let reasoning = d["reasoning_content"]
            .as_str()
            .or_else(|| d["reasoning"].as_str())
            .map(str::to_owned);
        if let Some(t) = &text {
            self.content.push_str(t);
        }
        if self.content.len() > MAX_RESPONSE_BYTES {
            return Err(invalid("response exceeds size limit"));
        }
        if let Some(items) = d.get("tool_calls").filter(|v| !v.is_null()) {
            for item in items
                .as_array()
                .ok_or_else(|| invalid("invalid tool delta"))?
            {
                let index = item["index"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing tool index"))?
                    as usize;
                if index >= MAX_TOOLS_PER_STEP {
                    return Err(invalid("tool index out of range"));
                }
                if item
                    .get("type")
                    .is_some_and(|t| !t.is_null() && t != "function")
                {
                    return Err(invalid("unsupported tool type"));
                }
                let call = self.calls.entry(index).or_insert_with(|| ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
                if let Some(id) = item["id"].as_str() {
                    call.id.push_str(id);
                }
                if let Some(name) = item["function"]["name"].as_str() {
                    call.name.push_str(name);
                }
                if let Some(args) = item["function"]["arguments"].as_str() {
                    call.arguments.push_str(args);
                }
                if call.arguments.len() > MAX_TOOL_ARGUMENT_BYTES
                    || call.id.len() > 256
                    || call.name.len() > 64
                {
                    return Err(invalid("tool call exceeds size limit"));
                }
            }
        }
        if let Some(reason) = c["finish_reason"].as_str() {
            self.finish = Some(reason.into());
        }
        Ok((text, reasoning))
    }
    pub fn finish(self) -> Result<ModelResponse, ModelError> {
        if !self.done && self.finish.is_none() {
            return Err(ModelError::StreamInterrupted);
        }
        let calls: Vec<_> = self.calls.into_values().collect();
        check_calls(&calls)?;
        if !calls.is_empty()
            && self.finish.as_deref() != Some("tool_calls")
            && self.finish.as_deref() != Some("stop")
        {
            return Err(invalid("incomplete tool call stream"));
        }
        if self.finish.as_deref() == Some("tool_calls") && calls.is_empty() {
            return Err(invalid("missing tool calls"));
        }
        Ok(ModelResponse {
            content: self.content,
            tool_calls: calls,
            finish_reason: self.finish.unwrap_or_else(|| "stop".into()),
            usage: self.usage,
        })
    }
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
