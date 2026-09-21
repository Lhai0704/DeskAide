use crate::{ContextError, ContextProvider, ContextRequest};
use deskaide_assistant_core::*;
use std::sync::Arc;
const MAX_CONTEXT_CHARS: usize = 64_000;
const DEFAULT_CONTEXT_CHARS: usize = 32_000;
pub struct DesktopContextCollector {
    pub selected_text_provider: Arc<dyn ContextProvider>,
    pub active_window_text_provider: Arc<dyn ContextProvider>,
}
#[async_trait::async_trait]
pub trait TurnContextProvider: Send + Sync {
    async fn collect(
        &self,
        selection: ContextSelection,
        supports_text: bool,
        window: Option<u64>,
    ) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>);
}
#[async_trait::async_trait]
impl TurnContextProvider for DesktopContextCollector {
    async fn collect(
        &self,
        selection: ContextSelection,
        supports_text: bool,
        window: Option<u64>,
    ) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
        collect_context(
            self,
            selection.target.as_ref(),
            &selection.sources,
            &selection.drafts,
            supports_text,
            context_char_budget(window),
        )
        .await
    }
}
pub async fn collect_context(
    state: &DesktopContextCollector,
    target: Option<&TargetWindow>,
    requested: &[ContextSourceType],
    context_drafts: &[TextContextDraft],
    supports_text: bool,
    mut remaining_chars: usize,
) -> (Vec<ContextPayload>, Vec<ContextCollectionResult>) {
    let mut payloads = Vec::new();
    let mut results = Vec::new();
    let mut ordered_sources = Vec::new();
    for source in [
        ContextSourceType::SelectedText,
        ContextSourceType::ActiveWindowText,
    ] {
        if requested.contains(&source) {
            ordered_sources.push(source);
        }
    }
    for source in requested {
        if !ordered_sources.contains(source) {
            ordered_sources.push(*source);
        }
    }

    for source in ordered_sources {
        if matches!(
            source,
            ContextSourceType::SelectedText | ContextSourceType::ActiveWindowText
        ) && !supports_text
        {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "当前模型不支持文字上下文",
            ));
            continue;
        }
        let Some(target) = target else {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "未记录到本次激活前的外部窗口",
            ));
            continue;
        };
        let provider = match source {
            ContextSourceType::SelectedText => Some(Arc::clone(&state.selected_text_provider)),
            ContextSourceType::ActiveWindowText => {
                Some(Arc::clone(&state.active_window_text_provider))
            }
            _ => None,
        };
        let Some(provider) = provider else {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                "该上下文类型尚未实现",
            ));
            continue;
        };
        if remaining_chars == 0 {
            results.push(context_result(
                source,
                ContextCollectionStatus::Unavailable,
                0,
                true,
                "本次请求的上下文预算已用完",
            ));
            continue;
        }

        let request = ContextRequest {
            target: target.clone(),
            sources: vec![source],
        };
        match provider.collect(&request).await {
            Ok(mut payload) => {
                let text = match source {
                    ContextSourceType::SelectedText => payload.selected_text.take(),
                    ContextSourceType::ActiveWindowText => payload.main_text.take(),
                    _ => None,
                };
                let Some(text) = text else {
                    results.push(context_result(
                        source,
                        ContextCollectionStatus::Unavailable,
                        0,
                        false,
                        "未获取到可用文字",
                    ));
                    continue;
                };
                let (text, truncated) = truncate_chars(&text, remaining_chars);
                let character_count = text.chars().count();
                remaining_chars = remaining_chars.saturating_sub(character_count);
                if truncated {
                    payload
                        .warnings
                        .push("文字已按当前模型的上下文预算截断".to_owned());
                }
                match source {
                    ContextSourceType::SelectedText => payload.selected_text = Some(text),
                    ContextSourceType::ActiveWindowText => payload.main_text = Some(text),
                    _ => {}
                }
                results.push(context_result(
                    source,
                    ContextCollectionStatus::Added,
                    character_count,
                    truncated,
                    if truncated {
                        "已添加，内容已按模型预算截断"
                    } else {
                        "已添加到本次提问"
                    },
                ));
                payloads.push(payload);
            }
            Err(error) => {
                let (status, message) = context_error_result(&error);
                results.push(context_result(source, status, 0, false, message));
            }
        }
    }

    if supports_text {
        for draft in context_drafts {
            let source = draft.source;
            if !matches!(
                source,
                ContextSourceType::SelectedText
                    | ContextSourceType::Clipboard
                    | ContextSourceType::ActiveWindowText
            ) {
                results.push(context_result(
                    source,
                    ContextCollectionStatus::Unavailable,
                    0,
                    false,
                    "该文字草稿类型尚未实现",
                ));
                continue;
            }
            let content = draft.content.trim();
            if content.is_empty() {
                results.push(context_result(
                    source,
                    ContextCollectionStatus::Unavailable,
                    0,
                    false,
                    text_context_result_message(draft, "内容为空，已跳过"),
                ));
                continue;
            }
            if remaining_chars == 0 {
                results.push(context_result(
                    source,
                    ContextCollectionStatus::Unavailable,
                    0,
                    true,
                    text_context_result_message(draft, "上下文预算已用完"),
                ));
                continue;
            }
            let (text, truncated) = truncate_chars(content, remaining_chars);
            let character_count = text.chars().count();
            remaining_chars = remaining_chars.saturating_sub(character_count);
            let mut warnings = Vec::new();
            if truncated {
                warnings.push("文字已按当前模型的上下文预算截断".to_owned());
            }
            let (selected_text, main_text) = match source {
                ContextSourceType::SelectedText => (Some(text), None),
                _ => (None, Some(text)),
            };
            payloads.push(ContextPayload {
                source_type: source,
                application_name: draft
                    .target
                    .as_ref()
                    .and_then(|target| target.application_name.clone()),
                process_name: draft
                    .target
                    .as_ref()
                    .and_then(|target| target.process_name.clone()),
                window_title: draft
                    .target
                    .as_ref()
                    .and_then(|target| target.title.clone()),
                url: None,
                selected_text,
                main_text,
                metadata: serde_json::json!({ "draftId": draft.id, "edited": true }),
                images: Vec::new(),
                warnings,
            });
            results.push(context_result(
                source,
                ContextCollectionStatus::Added,
                character_count,
                truncated,
                if truncated {
                    text_context_result_message(draft, "已添加，内容已截断")
                } else {
                    text_context_result_message(draft, "已添加")
                },
            ));
        }
    } else {
        for draft in context_drafts {
            results.push(context_result(
                draft.source,
                ContextCollectionStatus::Unavailable,
                0,
                false,
                text_context_result_message(draft, "当前模型不支持文字上下文"),
            ));
        }
    }

    (payloads, results)
}

fn text_context_label(draft: &TextContextDraft) -> &str {
    match draft.source {
        ContextSourceType::SelectedText => "选中文字",
        ContextSourceType::Clipboard => "剪贴板内容",
        ContextSourceType::ActiveWindowText => draft
            .target
            .as_ref()
            .and_then(|target| {
                target
                    .title
                    .as_deref()
                    .or(target.application_name.as_deref())
                    .or(target.process_name.as_deref())
            })
            .unwrap_or("窗口内容"),
        _ => "文字上下文",
    }
}

fn text_context_result_message(draft: &TextContextDraft, message: &str) -> String {
    if draft.source == ContextSourceType::ActiveWindowText {
        format!("{}：{message}", text_context_label(draft))
    } else {
        message.to_owned()
    }
}

fn context_char_budget(context_window: Option<u64>) -> usize {
    context_window
        .map(|tokens| usize::try_from(tokens / 2).unwrap_or(MAX_CONTEXT_CHARS))
        .unwrap_or(DEFAULT_CONTEXT_CHARS)
        .clamp(1, MAX_CONTEXT_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    let mut chars = text.chars();
    let result: String = chars.by_ref().take(max_chars).collect();
    let truncated = chars.next().is_some();
    (result, truncated)
}

fn context_result(
    source: ContextSourceType,
    status: ContextCollectionStatus,
    character_count: usize,
    truncated: bool,
    message: impl Into<String>,
) -> ContextCollectionResult {
    ContextCollectionResult {
        source,
        status,
        character_count,
        truncated,
        message: message.into(),
    }
}

fn context_error_result(error: &ContextError) -> (ContextCollectionStatus, String) {
    match error {
        ContextError::Unsupported { .. } => (
            ContextCollectionStatus::Unavailable,
            "该上下文类型在当前平台不可用".to_owned(),
        ),
        ContextError::Unavailable { reason, .. } => {
            (ContextCollectionStatus::Unavailable, reason.clone())
        }
        ContextError::Timeout { .. } => (
            ContextCollectionStatus::Failed,
            "采集超过 3 秒，已跳过该上下文".to_owned(),
        ),
        ContextError::Collection(message) => (
            ContextCollectionStatus::Failed,
            format!("采集失败：{message}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NeverCollect;
    #[async_trait::async_trait]
    impl ContextProvider for NeverCollect {
        fn id(&self) -> &'static str {
            "never"
        }
        async fn is_available(&self, _: &TargetWindow) -> Result<bool, ContextError> {
            Ok(true)
        }
        async fn collect(&self, _: &ContextRequest) -> Result<ContextPayload, ContextError> {
            panic!("drafts must never recollect the desktop")
        }
    }
    #[tokio::test]
    async fn edited_drafts_share_budget_without_platform_recollection() {
        let collector = DesktopContextCollector {
            selected_text_provider: Arc::new(NeverCollect),
            active_window_text_provider: Arc::new(NeverCollect),
        };
        let drafts = vec![
            TextContextDraft {
                id: "a".into(),
                source: ContextSourceType::SelectedText,
                target: None,
                content: "修改后文字".into(),
            },
            TextContextDraft {
                id: "b".into(),
                source: ContextSourceType::Clipboard,
                target: None,
                content: "剪贴板".into(),
            },
        ];
        let (payloads, results) = collect_context(&collector, None, &[], &drafts, true, 6).await;
        assert_eq!(payloads[0].selected_text.as_deref(), Some("修改后文字"));
        assert_eq!(payloads[1].main_text.as_deref(), Some("剪"));
        assert!(results[1].truncated);
        let (payloads, results) = collect_context(&collector, None, &[], &drafts, false, 100).await;
        assert!(payloads.is_empty());
        assert!(
            results
                .iter()
                .all(|r| r.status == ContextCollectionStatus::Unavailable)
        );
        assert_eq!(context_char_budget(Some(u64::MAX)), MAX_CONTEXT_CHARS);
        assert_eq!(truncate_chars("🙂文字", 1), ("🙂".into(), true));
    }
    #[test]
    fn context_budget_uses_half_the_model_window_with_a_hard_cap() {
        assert_eq!(context_char_budget(Some(4_096)), 2_048);
        assert_eq!(context_char_budget(None), DEFAULT_CONTEXT_CHARS);
        assert_eq!(context_char_budget(Some(1_000_000)), MAX_CONTEXT_CHARS);
        assert_eq!(context_char_budget(Some(1)), 1);
    }

    #[test]
    fn context_truncation_preserves_unicode_boundaries() {
        assert_eq!(truncate_chars("助手ABC", 3), ("助手A".to_owned(), true));
        assert_eq!(truncate_chars("助手", 3), ("助手".to_owned(), false));
    }

    #[test]
    fn context_results_use_the_frontend_wire_format() {
        let result = context_result(
            ContextSourceType::SelectedText,
            ContextCollectionStatus::Added,
            12,
            false,
            "added",
        );
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["source"], "selectedText");
        assert_eq!(value["characterCount"], 12);
        assert_eq!(value["status"], "added");
    }

    #[test]
    fn context_errors_map_to_non_blocking_user_statuses() {
        let unavailable = ContextError::Unavailable {
            capability: "selected_text",
            reason: "没有选中文字".to_owned(),
        };
        let timeout = ContextError::Timeout {
            capability: "active_window_text",
        };
        assert_eq!(
            context_error_result(&unavailable),
            (
                ContextCollectionStatus::Unavailable,
                "没有选中文字".to_owned()
            )
        );
        assert_eq!(
            context_error_result(&timeout).0,
            ContextCollectionStatus::Failed
        );
    }
}
