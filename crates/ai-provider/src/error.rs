use thiserror::Error;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderErrorDetails {
    pub provider_type: Option<String>,
    pub provider_code: Option<String>,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("the model request contains no user text")]
    MissingUserText,
    #[error("the response receiver was closed")]
    ResponseReceiverClosed,
    #[error("the base URL is invalid")]
    InvalidBaseUrl,
    #[error("the custom header is invalid: {0}")]
    InvalidHeader(String),
    #[error("API Key is not configured")]
    ApiKeyMissing,
    #[error("authentication failed: {message}")]
    Authentication {
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("permission denied: {message}")]
    Permission {
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("model not found: {message}")]
    ModelNotFound {
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("provider rate limit reached: {message}")]
    RateLimited {
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("provider server error ({status}): {message}")]
    ProviderServer {
        status: u16,
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("provider rejected the request ({status}): {message}")]
    ProviderRequest {
        status: u16,
        message: String,
        details: ProviderErrorDetails,
    },
    #[error("network connection failed: {0}")]
    Network(String),
    #[error("request timed out")]
    Timeout,
    #[error("the provider response format is incompatible: {0}")]
    IncompatibleResponse(String),
    #[error("the streaming response was interrupted")]
    StreamInterrupted,
}

impl ModelError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingUserText => "missing_user_text",
            Self::ResponseReceiverClosed => "response_receiver_closed",
            Self::InvalidBaseUrl => "invalid_base_url",
            Self::InvalidHeader(_) => "invalid_header",
            Self::ApiKeyMissing => "api_key_missing",
            Self::Authentication { .. } => "authentication_error",
            Self::Permission { .. } => "permission_error",
            Self::ModelNotFound { .. } => "model_not_found",
            Self::RateLimited { .. } => "rate_limited",
            Self::ProviderServer { .. } => "provider_server_error",
            Self::ProviderRequest { .. } => "provider_request_error",
            Self::Network(_) => "network_error",
            Self::Timeout => "timeout",
            Self::IncompatibleResponse(_) => "incompatible_response",
            Self::StreamInterrupted => "stream_interrupted",
        }
    }

    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else if error.is_connect() {
            Self::Network("unable to connect to the provider".to_owned())
        } else {
            Self::Network("the provider connection failed".to_owned())
        }
    }

    pub(crate) fn from_stream_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::StreamInterrupted
        }
    }
}
