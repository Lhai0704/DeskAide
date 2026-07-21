//! Platform-neutral context acquisition contracts.

use async_trait::async_trait;
use deskaide_assistant_core::{ContextPayload, ContextSourceType, TargetWindow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextRequest {
    pub target: TargetWindow,
    pub sources: Vec<ContextSourceType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapturedImage {
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DisplayTarget {
    Current,
    Primary,
    Named(String),
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("context capability '{capability}' is unsupported")]
    Unsupported { capability: &'static str },
    #[error("context collection failed: {0}")]
    Collection(String),
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("platform capability '{capability}' is unsupported on {platform}")]
    Unsupported {
        platform: &'static str,
        capability: &'static str,
    },
    #[error("platform integration failed: {0}")]
    Integration(String),
}

#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn is_available(&self, target: &TargetWindow) -> Result<bool, ContextError>;

    async fn collect(&self, request: &ContextRequest) -> Result<ContextPayload, ContextError>;
}

#[async_trait]
pub trait PlatformIntegration: Send + Sync {
    async fn get_last_active_window(&self) -> Result<TargetWindow, PlatformError>;

    async fn get_selected_text(
        &self,
        target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError>;

    async fn get_accessible_text(
        &self,
        target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError>;

    async fn capture_window(&self, target: &TargetWindow) -> Result<CapturedImage, PlatformError>;

    async fn capture_screen(&self, display: DisplayTarget) -> Result<CapturedImage, PlatformError>;

    async fn select_region(&self) -> Result<CapturedImage, PlatformError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_object_safe(_: &dyn PlatformIntegration, _: &dyn ContextProvider) {}

    #[allow(dead_code)]
    fn traits_are_object_safe(platform: &dyn PlatformIntegration, context: &dyn ContextProvider) {
        assert_object_safe(platform, context);
    }
}
