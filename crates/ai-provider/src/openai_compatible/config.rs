use std::{collections::BTreeMap, fmt};

use deskaide_assistant_core::ModelCapabilities;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::ModelError;

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleConfig {
    pub profile_id: String,
    pub base_url: String,
    pub model_id: String,
    pub api_key: SecretString,
    pub capabilities: ModelCapabilities,
    pub max_output_tokens: Option<u32>,
    pub timeout_seconds: u64,
    pub custom_headers: BTreeMap<String, String>,
}

impl OpenAiCompatibleConfig {
    pub fn validate(&self) -> Result<(), ModelError> {
        self.base_url()?;
        if self.model_id.trim().is_empty() {
            return Err(ModelError::IncompatibleResponse(
                "model ID is empty".to_owned(),
            ));
        }
        if self.api_key.expose().trim().is_empty() {
            return Err(ModelError::ApiKeyMissing);
        }
        if self.timeout_seconds == 0 {
            return Err(ModelError::Timeout);
        }
        self.header_map()?;
        Ok(())
    }

    pub(crate) fn chat_completions_url(&self) -> Result<Url, ModelError> {
        self.endpoint_url("chat/completions")
    }

    pub(crate) fn model_url(&self) -> Result<Url, ModelError> {
        self.endpoint_url(&format!("models/{}", self.model_id))
    }

    pub(crate) fn header_map(&self) -> Result<HeaderMap, ModelError> {
        let mut headers = HeaderMap::new();
        for (name, value) in &self.custom_headers {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ModelError::InvalidHeader(name.clone()))?;
            if is_sensitive_header(&header_name) {
                return Err(ModelError::InvalidHeader(format!(
                    "{name} is managed by DeskAide or may contain credentials"
                )));
            }
            let header_value = HeaderValue::from_str(value)
                .map_err(|_| ModelError::InvalidHeader(name.clone()))?;
            headers.insert(header_name, header_value);
        }
        Ok(headers)
    }

    fn endpoint_url(&self, endpoint: &str) -> Result<Url, ModelError> {
        let mut base = self.base_url()?;
        let mut path = base.path().trim_end_matches('/').to_owned();
        let has_versioned_openai_path = path.ends_with("/v1") || path.ends_with("/v1beta/openai");
        if !has_versioned_openai_path {
            path.push_str("/v1");
        }
        path.push('/');
        path.push_str(endpoint);
        base.set_path(&path);
        base.set_query(None);
        base.set_fragment(None);
        Ok(base)
    }

    fn base_url(&self) -> Result<Url, ModelError> {
        let url = Url::parse(self.base_url.trim()).map_err(|_| ModelError::InvalidBaseUrl)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ModelError::InvalidBaseUrl);
        }
        Ok(url)
    }
}

fn is_sensitive_header(name: &HeaderName) -> bool {
    let name = name.as_str();
    name.contains("authorization")
        || name.contains("api-key")
        || name.contains("apikey")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("cookie")
}
