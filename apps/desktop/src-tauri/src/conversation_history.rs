use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const HISTORY_FILE: &str = "conversation-history.json";
const HISTORY_KEY: &str = "history";
const HISTORY_VERSION: u32 = 1;
const MAX_TITLE_CHARS: usize = 80;
const DEFAULT_TITLE_CHARS: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    pub role: ConversationRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub model_profile_id: String,
    pub messages: Vec<ConversationMessage>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveConversationInput {
    pub id: String,
    pub title: Option<String>,
    pub model_profile_id: String,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model_profile_id: String,
    pub message_count: usize,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl From<&ConversationRecord> for ConversationSummary {
    fn from(record: &ConversationRecord) -> Self {
        Self {
            id: record.id.clone(),
            title: record.title.clone(),
            model_profile_id: record.model_profile_id.clone(),
            message_count: record.messages.len(),
            created_at_ms: record.created_at_ms,
            updated_at_ms: record.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredHistory {
    version: u32,
    conversations: Vec<ConversationRecord>,
}

impl Default for StoredHistory {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION,
            conversations: Vec::new(),
        }
    }
}

pub fn list<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<ConversationSummary>, String> {
    Ok(sorted_summaries(&load(app)?))
}

fn sorted_summaries(history: &StoredHistory) -> Vec<ConversationSummary> {
    let mut summaries = history
        .conversations
        .iter()
        .map(ConversationSummary::from)
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| right.created_at_ms.cmp(&left.created_at_ms))
    });
    summaries
}

pub fn get<R: Runtime>(
    app: &AppHandle<R>,
    conversation_id: &str,
) -> Result<Option<ConversationRecord>, String> {
    Ok(load(app)?
        .conversations
        .into_iter()
        .find(|conversation| conversation.id == conversation_id))
}

pub fn save<R: Runtime>(
    app: &AppHandle<R>,
    input: SaveConversationInput,
) -> Result<ConversationRecord, String> {
    let mut history = load(app)?;
    let record = upsert(&mut history, input, now_ms()?)?;
    persist(app, &history)?;
    Ok(record)
}

fn upsert(
    history: &mut StoredHistory,
    input: SaveConversationInput,
    now: u64,
) -> Result<ConversationRecord, String> {
    validate_save_input(&input)?;
    let existing_index = history
        .conversations
        .iter()
        .position(|conversation| conversation.id == input.id);
    let existing = existing_index.map(|index| &history.conversations[index]);
    let title = match input.title {
        Some(title) => validate_title(&title)?,
        None => existing
            .map(|conversation| conversation.title.clone())
            .unwrap_or_else(|| default_title(&input.messages)),
    };
    let record = ConversationRecord {
        id: input.id,
        title,
        model_profile_id: input.model_profile_id,
        messages: input.messages,
        created_at_ms: existing.map_or(now, |conversation| conversation.created_at_ms),
        updated_at_ms: now,
    };
    if let Some(index) = existing_index {
        history.conversations[index] = record.clone();
    } else {
        history.conversations.push(record.clone());
    }
    Ok(record)
}

pub fn rename<R: Runtime>(
    app: &AppHandle<R>,
    conversation_id: &str,
    title: &str,
) -> Result<ConversationSummary, String> {
    let mut history = load(app)?;
    let summary = rename_in_history(&mut history, conversation_id, title, now_ms()?)?;
    persist(app, &history)?;
    Ok(summary)
}

fn rename_in_history(
    history: &mut StoredHistory,
    conversation_id: &str,
    title: &str,
    now: u64,
) -> Result<ConversationSummary, String> {
    let title = validate_title(title)?;
    let conversation = history
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "历史对话不存在".to_owned())?;
    conversation.title = title;
    conversation.updated_at_ms = now;
    Ok(ConversationSummary::from(&*conversation))
}

pub fn delete<R: Runtime>(app: &AppHandle<R>, conversation_id: &str) -> Result<bool, String> {
    let mut history = load(app)?;
    let deleted = delete_from_history(&mut history, conversation_id);
    if !deleted {
        return Ok(false);
    }
    persist(app, &history)?;
    Ok(true)
}

fn delete_from_history(history: &mut StoredHistory, conversation_id: &str) -> bool {
    let original_len = history.conversations.len();
    history
        .conversations
        .retain(|conversation| conversation.id != conversation_id);
    history.conversations.len() != original_len
}

fn load<R: Runtime>(app: &AppHandle<R>) -> Result<StoredHistory, String> {
    let store = app.store(HISTORY_FILE).map_err(display_error)?;
    let Some(value) = store.get(HISTORY_KEY) else {
        return Ok(StoredHistory::default());
    };
    decode_history(value)
}

fn decode_history(value: Value) -> Result<StoredHistory, String> {
    let history = serde_json::from_value::<StoredHistory>(value)
        .map_err(|error| format!("历史对话数据无法读取：{error}"))?;
    if history.version != HISTORY_VERSION {
        return Err(format!("历史对话数据版本不受支持：{}", history.version));
    }
    Ok(history)
}

fn persist<R: Runtime>(app: &AppHandle<R>, history: &StoredHistory) -> Result<(), String> {
    let value = serde_json::to_value(history).map_err(display_error)?;
    let store = app.store(HISTORY_FILE).map_err(display_error)?;
    store.set(HISTORY_KEY, value);
    store.save().map_err(display_error)
}

fn validate_save_input(input: &SaveConversationInput) -> Result<(), String> {
    if input.id.trim().is_empty() {
        return Err("对话 ID 不能为空".to_owned());
    }
    if input.model_profile_id.trim().is_empty() {
        return Err("模型 Profile ID 不能为空".to_owned());
    }
    let has_user_message = input.messages.iter().any(|message| {
        message.role == ConversationRole::User && !message.content.trim().is_empty()
    });
    if !has_user_message {
        return Err("空白对话不会保存到历史记录".to_owned());
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<String, String> {
    let normalized = normalize_whitespace(title);
    if normalized.is_empty() {
        return Err("对话标题不能为空".to_owned());
    }
    if normalized.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("对话标题不能超过 {MAX_TITLE_CHARS} 个字符"));
    }
    Ok(normalized)
}

fn default_title(messages: &[ConversationMessage]) -> String {
    let source = messages
        .iter()
        .find(|message| {
            message.role == ConversationRole::User && !message.content.trim().is_empty()
        })
        .map(|message| normalize_whitespace(&message.content))
        .unwrap_or_else(|| "新对话".to_owned());
    let mut chars = source.chars();
    let prefix = chars.by_ref().take(DEFAULT_TITLE_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(display_error)?;
    u64::try_from(duration.as_millis()).map_err(display_error)
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, role: ConversationRole, content: &str) -> ConversationMessage {
        ConversationMessage {
            id: id.to_owned(),
            role,
            content: content.to_owned(),
            note: None,
        }
    }

    fn record(id: &str, created_at_ms: u64, updated_at_ms: u64) -> ConversationRecord {
        ConversationRecord {
            id: id.to_owned(),
            title: id.to_owned(),
            model_profile_id: "mock".to_owned(),
            messages: vec![message("message", ConversationRole::User, id)],
            created_at_ms,
            updated_at_ms,
        }
    }

    #[test]
    fn default_title_normalizes_and_truncates_unicode_by_character() {
        let source = format!("  {}  尾部", "助手".repeat(21));
        let title = default_title(&[message("1", ConversationRole::User, &source)]);
        assert_eq!(title.chars().count(), DEFAULT_TITLE_CHARS + 1);
        assert!(title.ends_with('…'));
        assert!(!title.contains("  "));
    }

    #[test]
    fn title_validation_rejects_empty_and_overlong_titles() {
        assert_eq!(validate_title(" \n ").unwrap_err(), "对话标题不能为空");
        assert!(validate_title(&"对".repeat(MAX_TITLE_CHARS + 1)).is_err());
        assert_eq!(validate_title("  一段  标题 ").unwrap(), "一段 标题");
    }

    #[test]
    fn summaries_sort_by_most_recent_update() {
        let history = StoredHistory {
            version: HISTORY_VERSION,
            conversations: vec![record("older", 1, 3), record("newer", 2, 9)],
        };
        let summaries = sorted_summaries(&history);
        assert_eq!(summaries[0].id, "newer");
    }

    #[test]
    fn upsert_preserves_creation_time_and_manual_title() {
        let mut history = StoredHistory::default();
        let initial = upsert(
            &mut history,
            SaveConversationInput {
                id: "conversation".to_owned(),
                title: None,
                model_profile_id: "mock".to_owned(),
                messages: vec![message("1", ConversationRole::User, "第一条提问")],
            },
            10,
        )
        .unwrap();
        assert_eq!(initial.title, "第一条提问");

        rename_in_history(&mut history, "conversation", "手动标题", 15).unwrap();
        let updated = upsert(
            &mut history,
            SaveConversationInput {
                id: "conversation".to_owned(),
                title: None,
                model_profile_id: "remote".to_owned(),
                messages: vec![
                    message("1", ConversationRole::User, "第一条提问"),
                    message("2", ConversationRole::Assistant, "回答"),
                ],
            },
            20,
        )
        .unwrap();
        assert_eq!(updated.title, "手动标题");
        assert_eq!(updated.created_at_ms, 10);
        assert_eq!(updated.updated_at_ms, 20);
        assert_eq!(updated.model_profile_id, "remote");
    }

    #[test]
    fn rename_and_delete_report_missing_conversations() {
        let mut history = StoredHistory {
            version: HISTORY_VERSION,
            conversations: vec![record("one", 1, 1)],
        };
        let renamed = rename_in_history(&mut history, "one", "新标题", 2).unwrap();
        assert_eq!(renamed.title, "新标题");
        assert!(rename_in_history(&mut history, "missing", "标题", 3).is_err());
        assert!(!delete_from_history(&mut history, "missing"));
        assert!(delete_from_history(&mut history, "one"));
        assert!(history.conversations.is_empty());
    }

    #[test]
    fn malformed_or_unknown_history_versions_are_rejected() {
        assert!(decode_history(serde_json::json!({ "version": 1 })).is_err());
        assert!(decode_history(serde_json::json!({ "version": 99, "conversations": [] })).is_err());
    }

    #[test]
    fn records_and_summaries_use_camel_case_wire_names() {
        let record = record("conversation", 10, 20);
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["modelProfileId"], "mock");
        assert_eq!(value["createdAtMs"], 10);
        assert!(value.get("created_at_ms").is_none());

        let summary = serde_json::to_value(ConversationSummary::from(&record)).unwrap();
        assert_eq!(summary["messageCount"], 1);
    }

    #[test]
    fn save_validation_requires_a_real_user_message() {
        let input = SaveConversationInput {
            id: "conversation".to_owned(),
            title: None,
            model_profile_id: "mock".to_owned(),
            messages: vec![message("1", ConversationRole::Assistant, "answer")],
        };
        assert!(validate_save_input(&input).is_err());
    }
}
