use std::{collections::BTreeMap, convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use deskaide_assistant_core::{
    ContentBlock, GenerationOptions, MessageRole, ModelCapabilities, ModelMessage, ModelRequest,
};
use futures_util::stream;
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::Mutex};

use super::*;

#[derive(Clone, Debug, Default)]
struct Captured {
    headers: HeaderMap,
    body: Value,
    path: String,
}

#[derive(Clone)]
struct TestState {
    captured: Arc<Mutex<Captured>>,
    response: TestResponse,
}

#[derive(Clone)]
enum TestResponse {
    Json(Value),
    Sse(Vec<&'static [u8]>),
    Error(StatusCode, Value),
    Delay(Duration),
}

async fn handler(State(state): State<TestState>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    let headers = request.headers().clone();
    let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    *state.captured.lock().await = Captured {
        headers,
        body,
        path,
    };
    match state.response {
        TestResponse::Json(value) => Json(value).into_response(),
        TestResponse::Error(status, value) => (status, Json(value)).into_response(),
        TestResponse::Delay(duration) => {
            tokio::time::sleep(duration).await;
            Json(json!({"choices": []})).into_response()
        }
        TestResponse::Sse(chunks) => {
            let body = Body::from_stream(stream::iter(chunks.into_iter().map(Ok::<_, Infallible>)));
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(body)
                .unwrap()
        }
    }
}

async fn server(response: TestResponse) -> (String, Arc<Mutex<Captured>>) {
    let captured = Arc::new(Mutex::new(Captured::default()));
    let state = TestState {
        captured: Arc::clone(&captured),
        response,
    };
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .route("/v1/models/test-model", get(handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), captured)
}

#[tokio::test]
async fn connection_test_uses_the_model_detail_endpoint() {
    let (base_url, captured) = server(TestResponse::Json(json!({"id":"test-model"}))).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
    provider.test_connection().await.unwrap();
    let captured = captured.lock().await;
    assert_eq!(captured.path, "/v1/models/test-model");
    assert_eq!(captured.headers["authorization"], "Bearer super-secret-key");
}

fn config(base_url: String, streaming: bool) -> OpenAiCompatibleConfig {
    OpenAiCompatibleConfig {
        prefer_fast_response: true,
        profile_id: "profile-1".to_owned(),
        base_url,
        model_id: "test-model".to_owned(),
        api_key: SecretString::new("super-secret-key"),
        capabilities: ModelCapabilities {
            supports_tools: false,
            supports_text: true,
            supports_images: false,
            supports_streaming: streaming,
            supports_system_message: true,
            max_images: Some(0),
            context_window: Some(16_384),
        },
        max_output_tokens: Some(700),
        timeout_seconds: 5,
        custom_headers: BTreeMap::from([("x-app".to_owned(), "DeskAide".to_owned())]),
    }
}

#[test]
fn gemini_fast_response_is_scoped_and_can_be_disabled() {
    let mut c = config(
        "https://generativelanguage.googleapis.com/v1beta/openai".into(),
        true,
    );
    c.model_id = "gemini-3.5-flash".into();
    let mut input = request();
    input.generation_options.temperature = Some(0.7);
    let body =
        super::request::ChatCompletionRequest::from_model_request(&c, input.clone()).unwrap();
    assert_eq!(body["reasoning_effort"], "minimal");
    assert_eq!(body["stream"], true);
    assert!(body.get("temperature").is_none());
    c.prefer_fast_response = false;
    let body =
        super::request::ChatCompletionRequest::from_model_request(&c, input.clone()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    c.prefer_fast_response = true;
    c.base_url = "https://example.com/v1".into();
    let body =
        super::request::ChatCompletionRequest::from_model_request(&c, input.clone()).unwrap();
    assert!(body.get("reasoning_effort").is_none());
    assert_eq!(body["temperature"], 0.7);
    c.base_url = "https://generativelanguage.googleapis.com/v1beta/openai".into();
    c.model_id = "gemini-3.1-pro".into();
    let body = super::request::ChatCompletionRequest::from_model_request(&c, input).unwrap();
    assert!(body.get("reasoning_effort").is_none());
}

fn request() -> ModelRequest {
    ModelRequest {
        request_id: "request-1".to_owned(),
        model_profile_id: "profile-1".to_owned(),
        conversation_id: "conversation-1".to_owned(),
        system_prompt: Some("Be helpful".to_owned()),
        messages: vec![
            message(MessageRole::User, "first"),
            message(MessageRole::Assistant, "answer"),
            message(MessageRole::User, "follow-up"),
        ],
        tools: Vec::new(),
        generation_options: GenerationOptions {
            max_output_tokens: None,
            temperature: Some(0.4),
        },
    }
}

fn message(role: MessageRole, text: &str) -> ModelMessage {
    ModelMessage {
        tool_calls: vec![],
        tool_call_id: None,
        role,
        content: vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
    }
}

#[tokio::test]
async fn sends_expected_url_headers_and_multiturn_body() {
    let response = json!({
        "choices": [{"message": {"content": "done"}, "finish_reason": "stop"}]
    });
    let (base_url, captured) = server(TestResponse::Json(response)).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);

    provider.complete(request(), sender).await.unwrap();

    let captured = captured.lock().await;
    assert_eq!(captured.path, "/v1/chat/completions");
    assert_eq!(captured.headers["authorization"], "Bearer super-secret-key");
    assert_eq!(captured.headers["x-app"], "DeskAide");
    assert_eq!(captured.body["model"], "test-model");
    assert_eq!(captured.body["stream"], false);
    assert_eq!(captured.body["temperature"], 0.4);
    assert_eq!(captured.body["max_tokens"], 700);
    assert_eq!(
        captured.body["messages"][0],
        json!({"role":"system","content":"Be helpful"})
    );
    assert_eq!(
        captured.body["messages"][1],
        json!({"role":"user","content":"first"})
    );
    assert_eq!(
        captured.body["messages"][2],
        json!({"role":"assistant","content":"answer"})
    );
    assert_eq!(
        captured.body["messages"][3],
        json!({"role":"user","content":"follow-up"})
    );
}

#[tokio::test]
async fn parses_sse_split_across_transport_chunks_and_done() {
    let chunks = vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel".as_slice(),
        b"lo\"},\"finish_reason\":null}]}\n\n".as_slice(),
        b"data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\ndata: [DO".as_slice(),
        b"NE]\n\n".as_slice(),
    ];
    let (base_url, _) = server(TestResponse::Sse(chunks)).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, true)).unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);

    let response = provider.complete(request(), sender).await.unwrap();
    assert_eq!(response.content, "Hello world");
    let mut delta = String::new();
    while let Some(event) = receiver.recv().await {
        if let ProviderEvent::TextDelta(text) = event {
            delta.push_str(&text);
        }
    }
    assert_eq!(delta, "Hello world");
}

#[tokio::test]
async fn rejects_an_incomplete_stream() {
    let chunks = vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n"
            .as_slice(),
    ];
    let (base_url, _) = server(TestResponse::Sse(chunks)).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, true)).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);
    assert!(matches!(
        provider.complete(request(), sender).await,
        Err(ModelError::StreamInterrupted)
    ));
}

#[tokio::test]
async fn maps_provider_status_codes_and_does_not_leak_the_key() {
    for (status, expected_code) in [
        (StatusCode::UNAUTHORIZED, "authentication_error"),
        (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        (StatusCode::INTERNAL_SERVER_ERROR, "provider_server_error"),
    ] {
        let body =
            json!({"error":{"message":"rejected super-secret-key","type":"provider","code":"bad"}});
        let (base_url, _) = server(TestResponse::Error(status, body)).await;
        let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(256);
        let error = provider.complete(request(), sender).await.unwrap_err();
        assert_eq!(error.code(), expected_code);
        assert!(!error.to_string().contains("super-secret-key"));
        assert!(!format!("{provider:?}").contains("super-secret-key"));
    }
}

#[tokio::test]
async fn recognizes_model_not_found_from_a_400_provider_error() {
    let body = json!({"error":{"message":"model not found","code":"model_not_found"}});
    let (base_url, _) = server(TestResponse::Error(StatusCode::BAD_REQUEST, body)).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);
    assert_eq!(
        provider
            .complete(request(), sender)
            .await
            .unwrap_err()
            .code(),
        "model_not_found"
    );
}

#[tokio::test]
async fn caller_can_cancel_an_in_flight_http_request() {
    let (base_url, _) = server(TestResponse::Delay(Duration::from_secs(5))).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);
    let task = tokio::spawn(async move { provider.complete(request(), sender).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn parses_crlf_sse_boundaries() {
    let chunks = vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\r\n\r\ndata: [DONE]\r\n\r\n".as_slice(),
    ];
    let (base_url, _) = server(TestResponse::Sse(chunks)).await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, true)).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);
    assert_eq!(
        provider.complete(request(), sender).await.unwrap().content,
        "ok"
    );
}

#[tokio::test]
async fn applies_the_configured_timeout() {
    let (base_url, _) = server(TestResponse::Delay(Duration::from_secs(2))).await;
    let mut provider_config = config(base_url, false);
    provider_config.timeout_seconds = 1;
    let provider = OpenAiCompatibleProvider::new(provider_config).unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(256);
    let error = provider.complete(request(), sender).await.unwrap_err();
    assert_eq!(error.code(), "timeout");
}

#[test]
fn normalizes_base_urls_with_and_without_v1() {
    let first = config("https://example.com/openai".to_owned(), false);
    assert_eq!(
        first.chat_completions_url().unwrap().as_str(),
        "https://example.com/openai/v1/chat/completions"
    );
    let second = config("https://example.com/api/v1/".to_owned(), false);
    assert_eq!(
        second.chat_completions_url().unwrap().as_str(),
        "https://example.com/api/v1/chat/completions"
    );
}

#[test]
fn preserves_google_ai_studio_openai_compatible_base_url() {
    let provider_config = config(
        "https://generativelanguage.googleapis.com/v1beta/openai/".to_owned(),
        false,
    );
    assert_eq!(
        provider_config.chat_completions_url().unwrap().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
    );
    assert_eq!(
        provider_config.model_url().unwrap().as_str(),
        "https://generativelanguage.googleapis.com/v1beta/openai/models/test-model"
    );
}

#[test]
fn rejects_sensitive_custom_headers() {
    let mut provider_config = config("https://example.com".to_owned(), false);
    provider_config.custom_headers =
        BTreeMap::from([("Authorization".to_owned(), "secret".to_owned())]);
    assert_eq!(
        provider_config.validate().unwrap_err().code(),
        "invalid_header"
    );
}

#[test]
fn rejects_base_urls_that_embed_credentials_or_query_parameters() {
    let credential_url = config("https://user:secret@example.com/v1".to_owned(), false);
    assert_eq!(
        credential_url.validate().unwrap_err().code(),
        "invalid_base_url"
    );
    let query_url = config("https://example.com/v1?api_key=secret".to_owned(), false);
    assert_eq!(query_url.validate().unwrap_err().code(), "invalid_base_url");
}
#[tokio::test]
async fn nonstreaming_tools_mixed_content_and_tool_result_roundtrip() {
    use deskaide_assistant_core::{ToolCall, ToolDefinition, ToolRisk, ToolSource};
    let body = json!({"choices":[{"message":{"content":"checking","tool_calls":[{"id":"call-a","type":"function","function":{"name":"echo","arguments":"{\"text\":\"hi\"}"}},{"id":"call-b","type":"function","function":{"name":"echo","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}});
    let (base_url, captured) = server(TestResponse::Json(body)).await;
    let mut config = config(base_url, false);
    config.capabilities.supports_tools = true;
    let provider = OpenAiCompatibleProvider::new(config).unwrap();
    let mut request = request();
    request.tools = vec![ToolDefinition {
        display_name: None,
        id: "echo".into(),
        name: "echo".into(),
        description: "echo".into(),
        parameters: json!({"type":"object"}),
        source: ToolSource::Internal,
        risk: ToolRisk::ReadOnly,
        revision: 1,
    }];
    let mut assistant = ModelMessage::text(MessageRole::Assistant, "");
    assistant.tool_calls = vec![ToolCall {
        id: "old".into(),
        name: "echo".into(),
        arguments: "{}".into(),
    }];
    request.messages.push(assistant);
    let mut result = ModelMessage::text(MessageRole::Tool, "result");
    result.tool_call_id = Some("old".into());
    request.messages.push(result);
    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    let result = provider.complete(request, tx).await.unwrap();
    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.usage.unwrap().total_tokens, Some(15));
    let captured = captured.lock().await;
    assert_eq!(captured.body["tool_choice"], "auto");
    assert_eq!(captured.body["messages"][4]["content"], Value::Null);
    assert_eq!(captured.body["messages"][5]["tool_call_id"], "old");
}

#[test]
fn reconstructs_fragmented_interleaved_calls_and_usage_only_frames() {
    let mut stream = response::StreamAccumulator::default();
    stream.push(&json!({"choices":[{"index":0,"delta":{"content":"checking","tool_calls":[{"index":0,"id":"call-a","type":"function","function":{"name":"ec","arguments":"{\"te"}},{"index":1,"id":"call-b","type":"function","function":{"name":"echo","arguments":"{"}}]}}]}).to_string()).unwrap();
    stream.push(&json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":"}"}},{"index":0,"function":{"name":"ho","arguments":"xt\":\"hi\"}"}}]},"finish_reason":"tool_calls"}]}).to_string()).unwrap();
    stream
        .push(&json!({"choices":[],"usage":{"total_tokens":99}}).to_string())
        .unwrap();
    stream.push("[DONE]").unwrap();
    let result = stream.finish().unwrap();
    assert_eq!(result.content, "checking");
    assert_eq!(result.tool_calls[0].name, "echo");
    assert_eq!(result.tool_calls[0].arguments, "{\"text\":\"hi\"}");
    assert_eq!(result.tool_calls[1].arguments, "{}");
    assert_eq!(result.usage.unwrap().total_tokens, Some(99));
}

#[test]
fn rejects_malformed_tool_responses_and_accepts_null_text_tool_only() {
    let valid = json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"a","type":"function","function":{"name":"echo","arguments":"{}"}}]},"finish_reason":"tool_calls"}]});
    assert_eq!(
        response::parse_response(valid.clone())
            .unwrap()
            .tool_calls
            .len(),
        1
    );
    let mut duplicate = valid.clone();
    duplicate["choices"][0]["message"]["tool_calls"] = json!([
        valid["choices"][0]["message"]["tool_calls"][0],
        valid["choices"][0]["message"]["tool_calls"][0]
    ]);
    assert!(response::parse_response(duplicate).is_err());
    let mut stream = response::StreamAccumulator::default();
    assert!(stream.push("not json").is_err());
    assert!(
        stream
            .push(&json!({"choices":[{"delta":{"tool_calls":[{"index":999}]}}]}).to_string())
            .is_err()
    );
    assert!(response::parse_response(json!({"choices":[{"message":{"content":null}}]})).is_err());
}

#[tokio::test]
async fn tools_disabled_omits_definitions_and_projects_old_tool_messages() {
    let (base_url, captured) = server(TestResponse::Json(
        json!({"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}),
    ))
    .await;
    let provider = OpenAiCompatibleProvider::new(config(base_url, false)).unwrap();
    let mut request = request();
    let mut result = ModelMessage::text(MessageRole::Tool, "previous result");
    result.tool_call_id = Some("call".into());
    request.messages.push(result);
    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    provider.complete(request, tx).await.unwrap();
    let captured = captured.lock().await;
    assert!(captured.body.get("tools").is_none());
    assert!(captured.body.get("tool_choice").is_none());
    assert_eq!(captured.body["messages"][4]["role"], "assistant");
}
