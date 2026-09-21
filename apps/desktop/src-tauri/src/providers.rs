use crate::credentials::CredentialStore;
use crate::model_profiles::{ModelProfile, ProviderType};
use deskaide_ai_provider::{
    MockProvider, ModelError, ModelProvider, OpenAiCompatibleProvider,
    openai_compatible::{OpenAiCompatibleConfig, SecretString},
};
use std::sync::Arc;
pub(super) fn provider_for_profile(
    profile: &ModelProfile,
    credentials: &dyn CredentialStore,
) -> Result<Arc<dyn ModelProvider>, ModelError> {
    match profile.provider_type {
        ProviderType::Mock => Ok(Arc::new(MockProvider::new())),
        ProviderType::OpenAiCompatible => {
            let api_key = credentials
                .get(&profile.id)
                .map_err(|_| ModelError::ApiKeyMissing)?
                .ok_or(ModelError::ApiKeyMissing)?;
            Ok(Arc::new(openai_provider(profile, api_key)?))
        }
    }
}

pub(super) fn openai_provider(
    profile: &ModelProfile,
    api_key: String,
) -> Result<OpenAiCompatibleProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        prefer_fast_response: profile.prefer_fast_response,
        profile_id: profile.id.clone(),
        base_url: profile.base_url.clone(),
        model_id: profile.model_id.clone(),
        api_key: SecretString::new(api_key),
        capabilities: profile.capabilities,
        max_output_tokens: profile.max_output_tokens,
        timeout_seconds: profile.timeout_seconds,
        custom_headers: profile.custom_headers.clone(),
    })
}
