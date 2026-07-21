//! Windows platform integration boundary.
//!
//! Phase one intentionally exposes unsupported errors until explicit context
//! capture is implemented in a later phase.

use async_trait::async_trait;
use deskaide_assistant_core::TargetWindow;
use deskaide_context_core::{CapturedImage, DisplayTarget, PlatformError, PlatformIntegration};

#[derive(Debug, Default)]
pub struct WindowsPlatformIntegration;

impl WindowsPlatformIntegration {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformIntegration for WindowsPlatformIntegration {
    async fn get_last_active_window(&self) -> Result<TargetWindow, PlatformError> {
        unsupported("get_last_active_window")
    }

    async fn get_selected_text(
        &self,
        _target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError> {
        unsupported("get_selected_text")
    }

    async fn get_accessible_text(
        &self,
        _target: &TargetWindow,
    ) -> Result<Option<String>, PlatformError> {
        unsupported("get_accessible_text")
    }

    async fn capture_window(&self, _target: &TargetWindow) -> Result<CapturedImage, PlatformError> {
        unsupported("capture_window")
    }

    async fn capture_screen(
        &self,
        _display: DisplayTarget,
    ) -> Result<CapturedImage, PlatformError> {
        unsupported("capture_screen")
    }

    async fn select_region(&self) -> Result<CapturedImage, PlatformError> {
        unsupported("select_region")
    }
}

fn unsupported<T>(capability: &'static str) -> Result<T, PlatformError> {
    Err(PlatformError::Unsupported {
        platform: "windows",
        capability,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unimplemented_capabilities_are_explicit() {
        let integration = WindowsPlatformIntegration::new();
        let error = integration.get_last_active_window().await.unwrap_err();
        assert!(matches!(
            error,
            PlatformError::Unsupported {
                platform: "windows",
                capability: "get_last_active_window"
            }
        ));
    }
}
