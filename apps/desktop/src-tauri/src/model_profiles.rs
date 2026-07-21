use std::collections::BTreeMap;

use deskaide_assistant_core::ModelCapabilities;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const MOCK_PROFILE_ID: &str = "mock-local";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Mock,
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfile {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    pub base_url: String,
    pub model_id: String,
    pub capabilities: ModelCapabilities,
    pub max_output_tokens: Option<u32>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub custom_headers: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub provider_type: ProviderType,
    pub base_url: String,
    pub model_id: String,
    pub capabilities: ModelCapabilities,
    pub max_output_tokens: Option<u32>,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub custom_headers: BTreeMap<String, String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileView {
    #[serde(flatten)]
    pub profile: ModelProfile,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantBootstrap {
    pub active_model_profile_id: String,
    pub model_profiles: Vec<ModelProfileView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedProfiles {
    pub profiles: Vec<ModelProfile>,
    pub active_model_profile_id: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("model profile not found")]
    NotFound,
    #[error("the built-in Mock Profile cannot be changed")]
    MockIsReadOnly,
    #[error("the active model profile cannot be deleted")]
    ActiveProfileCannotBeDeleted,
    #[error("profile name is required")]
    NameRequired,
    #[error("model ID is required")]
    ModelIdRequired,
    #[error("base URL must be a valid HTTP or HTTPS URL")]
    InvalidBaseUrl,
    #[error("request timeout must be between 1 and 600 seconds")]
    InvalidTimeout,
    #[error("context window must be greater than zero")]
    InvalidContextWindow,
    #[error("maximum output tokens must be greater than zero")]
    InvalidMaxOutputTokens,
    #[error("custom header is invalid or sensitive: {0}")]
    InvalidHeader(String),
    #[error("another profile already uses this name")]
    DuplicateName,
}

#[derive(Debug, Clone)]
pub struct ProfileCollection {
    profiles: Vec<ModelProfile>,
    active_model_profile_id: String,
}

impl Default for ProfileCollection {
    fn default() -> Self {
        Self {
            profiles: vec![mock_profile()],
            active_model_profile_id: MOCK_PROFILE_ID.to_owned(),
        }
    }
}

impl ProfileCollection {
    pub fn from_saved(saved: SavedProfiles) -> Self {
        let mut collection = Self::default();
        for profile in saved.profiles {
            if profile.provider_type != ProviderType::Mock && validate_profile(&profile).is_ok() {
                collection.profiles.push(profile);
            }
        }
        if collection
            .profiles
            .iter()
            .any(|profile| profile.id == saved.active_model_profile_id)
        {
            collection.active_model_profile_id = saved.active_model_profile_id;
        }
        collection
    }

    pub fn saved(&self) -> SavedProfiles {
        SavedProfiles {
            profiles: self.profiles.clone(),
            active_model_profile_id: self.active_model_profile_id.clone(),
        }
    }

    pub fn profiles(&self) -> &[ModelProfile] {
        &self.profiles
    }

    pub fn active_id(&self) -> &str {
        &self.active_model_profile_id
    }

    pub fn active(&self) -> &ModelProfile {
        self.get(&self.active_model_profile_id)
            .expect("active profile must exist")
    }

    pub fn get(&self, id: &str) -> Result<&ModelProfile, ProfileError> {
        self.profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or(ProfileError::NotFound)
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), ProfileError> {
        self.get(id)?;
        self.active_model_profile_id = id.to_owned();
        Ok(())
    }

    pub fn save(&mut self, input: ModelProfileInput) -> Result<ModelProfile, ProfileError> {
        let profile = ModelProfile {
            id: input
                .id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            name: input.name.trim().to_owned(),
            provider_type: input.provider_type,
            base_url: input.base_url.trim().trim_end_matches('/').to_owned(),
            model_id: input.model_id.trim().to_owned(),
            capabilities: input.capabilities,
            max_output_tokens: input.max_output_tokens,
            timeout_seconds: input.timeout_seconds,
            custom_headers: input.custom_headers,
        };
        if profile.id == MOCK_PROFILE_ID || profile.provider_type == ProviderType::Mock {
            return Err(ProfileError::MockIsReadOnly);
        }
        validate_profile(&profile)?;
        if self
            .profiles
            .iter()
            .any(|other| other.id != profile.id && other.name.eq_ignore_ascii_case(&profile.name))
        {
            return Err(ProfileError::DuplicateName);
        }
        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|existing| existing.id == profile.id)
        {
            *existing = profile.clone();
        } else {
            self.profiles.push(profile.clone());
        }
        Ok(profile)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ProfileError> {
        if id == MOCK_PROFILE_ID {
            return Err(ProfileError::MockIsReadOnly);
        }
        if id == self.active_model_profile_id {
            return Err(ProfileError::ActiveProfileCannotBeDeleted);
        }
        let original_len = self.profiles.len();
        self.profiles.retain(|profile| profile.id != id);
        if self.profiles.len() == original_len {
            return Err(ProfileError::NotFound);
        }
        Ok(())
    }
}

pub fn mock_profile() -> ModelProfile {
    ModelProfile {
        id: MOCK_PROFILE_ID.to_owned(),
        name: "Mock Local".to_owned(),
        provider_type: ProviderType::Mock,
        base_url: String::new(),
        model_id: "mock-local".to_owned(),
        capabilities: ModelCapabilities {
            supports_text: true,
            supports_images: false,
            supports_streaming: true,
            supports_system_message: true,
            max_images: Some(0),
            context_window: Some(4_096),
        },
        max_output_tokens: Some(512),
        timeout_seconds: 30,
        custom_headers: BTreeMap::new(),
    }
}

pub fn validate_profile(profile: &ModelProfile) -> Result<(), ProfileError> {
    if profile.name.trim().is_empty() {
        return Err(ProfileError::NameRequired);
    }
    if profile.model_id.trim().is_empty() {
        return Err(ProfileError::ModelIdRequired);
    }
    let base_url = Url::parse(profile.base_url.trim()).map_err(|_| ProfileError::InvalidBaseUrl)?;
    if !matches!(base_url.scheme(), "http" | "https")
        || base_url.host_str().is_none()
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || base_url.query().is_some()
        || base_url.fragment().is_some()
    {
        return Err(ProfileError::InvalidBaseUrl);
    }
    if !(1..=600).contains(&profile.timeout_seconds) {
        return Err(ProfileError::InvalidTimeout);
    }
    if profile.capabilities.context_window == Some(0) {
        return Err(ProfileError::InvalidContextWindow);
    }
    if profile.max_output_tokens == Some(0) {
        return Err(ProfileError::InvalidMaxOutputTokens);
    }
    for (name, value) in &profile.custom_headers {
        let normalized_name = name.to_ascii_lowercase();
        if name.trim().is_empty()
            || !name.bytes().all(is_header_name_byte)
            || value.contains(['\r', '\n'])
            || value
                .chars()
                .any(|character| character.is_control() && character != '\t')
            || is_sensitive_header(&normalized_name)
        {
            return Err(ProfileError::InvalidHeader(name.clone()));
        }
    }
    Ok(())
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn is_sensitive_header(name: &str) -> bool {
    name.contains("authorization")
        || name.contains("api-key")
        || name.contains("apikey")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("cookie")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: Option<&str>, name: &str) -> ModelProfileInput {
        ModelProfileInput {
            id: id.map(str::to_owned),
            name: name.to_owned(),
            provider_type: ProviderType::OpenAiCompatible,
            base_url: "https://example.com/v1".to_owned(),
            model_id: "model".to_owned(),
            capabilities: mock_profile().capabilities,
            max_output_tokens: Some(100),
            timeout_seconds: 30,
            custom_headers: BTreeMap::new(),
            api_key: None,
        }
    }

    #[test]
    fn creates_updates_selects_and_deletes_profiles() {
        let mut profiles = ProfileCollection::default();
        let created = profiles.save(input(None, "Remote")).unwrap();
        assert_eq!(profiles.profiles().len(), 2);
        profiles.set_active(&created.id).unwrap();
        assert_eq!(profiles.active_id(), created.id);

        let updated = profiles.save(input(Some(&created.id), "Updated")).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(profiles.profiles().len(), 2);

        assert_eq!(
            profiles.delete(&created.id),
            Err(ProfileError::ActiveProfileCannotBeDeleted)
        );
        profiles.set_active(MOCK_PROFILE_ID).unwrap();
        profiles.delete(&created.id).unwrap();
        assert_eq!(profiles.profiles().len(), 1);
    }

    #[test]
    fn persisted_profiles_always_restore_the_mock_profile() {
        let saved = SavedProfiles {
            profiles: vec![],
            active_model_profile_id: "missing".to_owned(),
        };
        let profiles = ProfileCollection::from_saved(saved);
        assert_eq!(profiles.active_id(), MOCK_PROFILE_ID);
        assert_eq!(profiles.profiles(), &[mock_profile()]);
    }

    #[test]
    fn provider_type_uses_the_frontend_wire_name() {
        assert_eq!(
            serde_json::to_string(&ProviderType::OpenAiCompatible).unwrap(),
            "\"openai_compatible\""
        );
    }

    #[test]
    fn rejects_sensitive_custom_headers() {
        let mut request = input(None, "Remote");
        request
            .custom_headers
            .insert("Authorization".to_owned(), "secret".to_owned());
        assert!(matches!(
            ProfileCollection::default().save(request),
            Err(ProfileError::InvalidHeader(_))
        ));
    }

    #[test]
    fn saved_profile_data_has_no_api_key_field() {
        let mut profiles = ProfileCollection::default();
        profiles.save(input(None, "Remote")).unwrap();
        let json = serde_json::to_string(&profiles.saved()).unwrap();
        assert!(!json.to_ascii_lowercase().contains("apikey"));
        assert!(!json.contains("api_key"));
    }
}
