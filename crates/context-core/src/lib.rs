//! Platform-neutral context acquisition contracts.

use std::sync::Arc;

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
    #[error("context capability '{capability}' is unavailable: {reason}")]
    Unavailable {
        capability: &'static str,
        reason: String,
    },
    #[error("context capability '{capability}' timed out")]
    Timeout { capability: &'static str },
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
    #[error("platform capability '{capability}' is unavailable on {platform}: {reason}")]
    Unavailable {
        platform: &'static str,
        capability: &'static str,
        reason: String,
    },
    #[error("platform capability '{capability}' timed out on {platform}")]
    Timeout {
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

pub struct SelectedTextContextProvider {
    platform: Arc<dyn PlatformIntegration>,
}

impl SelectedTextContextProvider {
    pub fn new(platform: Arc<dyn PlatformIntegration>) -> Self {
        Self { platform }
    }
}

#[async_trait]
impl ContextProvider for SelectedTextContextProvider {
    fn id(&self) -> &'static str {
        "selected_text"
    }

    async fn is_available(&self, target: &TargetWindow) -> Result<bool, ContextError> {
        Ok(!target.id.is_empty())
    }

    async fn collect(&self, request: &ContextRequest) -> Result<ContextPayload, ContextError> {
        ensure_source(request, ContextSourceType::SelectedText, self.id())?;
        let text = self
            .platform
            .get_selected_text(&request.target)
            .await
            .map_err(map_platform_error)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ContextError::Unavailable {
                capability: self.id(),
                reason: "目标控件未公开选中文字".to_owned(),
            })?;
        Ok(text_payload(
            &request.target,
            ContextSourceType::SelectedText,
            Some(text),
            None,
        ))
    }
}

pub struct ActiveWindowTextContextProvider {
    platform: Arc<dyn PlatformIntegration>,
}

impl ActiveWindowTextContextProvider {
    pub fn new(platform: Arc<dyn PlatformIntegration>) -> Self {
        Self { platform }
    }
}

#[async_trait]
impl ContextProvider for ActiveWindowTextContextProvider {
    fn id(&self) -> &'static str {
        "active_window_text"
    }

    async fn is_available(&self, target: &TargetWindow) -> Result<bool, ContextError> {
        Ok(!target.id.is_empty())
    }

    async fn collect(&self, request: &ContextRequest) -> Result<ContextPayload, ContextError> {
        ensure_source(request, ContextSourceType::ActiveWindowText, self.id())?;
        let text = self
            .platform
            .get_accessible_text(&request.target)
            .await
            .map_err(map_platform_error)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ContextError::Unavailable {
                capability: self.id(),
                reason: "目标窗口未公开可访问文字".to_owned(),
            })?;
        Ok(text_payload(
            &request.target,
            ContextSourceType::ActiveWindowText,
            None,
            Some(text),
        ))
    }
}

fn ensure_source(
    request: &ContextRequest,
    source: ContextSourceType,
    capability: &'static str,
) -> Result<(), ContextError> {
    if request.sources.contains(&source) {
        Ok(())
    } else {
        Err(ContextError::Unsupported { capability })
    }
}

fn text_payload(
    target: &TargetWindow,
    source_type: ContextSourceType,
    selected_text: Option<String>,
    main_text: Option<String>,
) -> ContextPayload {
    ContextPayload {
        source_type,
        application_name: target.application_name.clone(),
        process_name: target.process_name.clone(),
        window_title: target.title.clone(),
        url: None,
        selected_text,
        main_text,
        metadata: serde_json::Value::Null,
        images: Vec::new(),
        warnings: Vec::new(),
    }
}

fn map_platform_error(error: PlatformError) -> ContextError {
    match error {
        PlatformError::Unsupported { capability, .. } => ContextError::Unsupported { capability },
        PlatformError::Unavailable {
            capability, reason, ..
        } => ContextError::Unavailable { capability, reason },
        PlatformError::Timeout { capability, .. } => ContextError::Timeout { capability },
        PlatformError::Integration(message) => ContextError::Collection(message),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn assert_object_safe(_: &dyn PlatformIntegration, _: &dyn ContextProvider) {}

    #[allow(dead_code)]
    fn traits_are_object_safe(platform: &dyn PlatformIntegration, context: &dyn ContextProvider) {
        assert_object_safe(platform, context);
    }

    #[derive(Default)]
    struct FakePlatform;

    #[async_trait]
    impl PlatformIntegration for FakePlatform {
        async fn get_last_active_window(&self) -> Result<TargetWindow, PlatformError> {
            unreachable!()
        }

        async fn get_selected_text(
            &self,
            _target: &TargetWindow,
        ) -> Result<Option<String>, PlatformError> {
            Ok(Some("selected".to_owned()))
        }

        async fn get_accessible_text(
            &self,
            _target: &TargetWindow,
        ) -> Result<Option<String>, PlatformError> {
            Ok(Some("document".to_owned()))
        }

        async fn capture_window(
            &self,
            _target: &TargetWindow,
        ) -> Result<CapturedImage, PlatformError> {
            unreachable!()
        }

        async fn capture_screen(
            &self,
            _display: DisplayTarget,
        ) -> Result<CapturedImage, PlatformError> {
            unreachable!()
        }

        async fn select_region(&self) -> Result<CapturedImage, PlatformError> {
            unreachable!()
        }
    }

    fn target() -> TargetWindow {
        TargetWindow {
            id: "window-1".to_owned(),
            application_name: Some("Editor".to_owned()),
            process_name: Some("editor.exe".to_owned()),
            title: Some("Document".to_owned()),
        }
    }

    #[tokio::test]
    async fn text_providers_create_source_specific_payloads() {
        let platform: Arc<dyn PlatformIntegration> = Arc::new(FakePlatform);
        let selected = SelectedTextContextProvider::new(Arc::clone(&platform));
        let window = ActiveWindowTextContextProvider::new(platform);

        let selected_payload = selected
            .collect(&ContextRequest {
                target: target(),
                sources: vec![ContextSourceType::SelectedText],
            })
            .await
            .unwrap();
        let window_payload = window
            .collect(&ContextRequest {
                target: target(),
                sources: vec![ContextSourceType::ActiveWindowText],
            })
            .await
            .unwrap();

        assert_eq!(selected_payload.selected_text.as_deref(), Some("selected"));
        assert_eq!(selected_payload.main_text, None);
        assert_eq!(window_payload.main_text.as_deref(), Some("document"));
        assert_eq!(window_payload.selected_text, None);
        assert_eq!(window_payload.application_name.as_deref(), Some("Editor"));
    }
}
