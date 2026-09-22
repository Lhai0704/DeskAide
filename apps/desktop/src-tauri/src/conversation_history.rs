//! Atomic v2 transcript storage. Never overwrite or delete the legacy v1 file.
use async_trait::async_trait;
use deskaide_assistant_core::*;
use deskaide_assistant_runtime::SessionRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_HISTORY_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct History {
    version: u32,
    conversations: Vec<Session>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            version: 2,
            conversations: vec![],
        }
    }
}
pub struct HistoryRepository {
    directory: PathBuf,
    lock: Mutex<()>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub model_profile_id: String,
    pub messages: Vec<ConversationMessage>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model_profile_id: String,
    pub message_count: usize,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
fn error(code: &str, message: &str) -> ToolError {
    ToolError::new(code, message)
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn projection(session: &Session) -> ConversationRecord {
    ConversationRecord {
        id: session.id.clone(),
        title: session.title.clone(),
        model_profile_id: session.model_profile_id.clone(),
        messages: session
            .transcript
            .iter()
            .filter(|m| matches!(m.message.role, MessageRole::User | MessageRole::Assistant))
            .filter_map(|m| {
                let content = m.message.text_content();
                if content.is_empty() && m.note.is_none() {
                    return None;
                }
                Some(ConversationMessage {
                    id: m.id.clone(),
                    turn_id: Some(m.turn_id.clone()),
                    role: if m.message.role == MessageRole::User {
                        "user"
                    } else {
                        "assistant"
                    }
                    .into(),
                    content,
                    note: m.note.clone(),
                })
            })
            .collect(),
        created_at_ms: session.created_at_ms,
        updated_at_ms: session.updated_at_ms,
        revision: session.revision,
    }
}
fn summary(s: &Session) -> ConversationSummary {
    let p = projection(s);
    ConversationSummary {
        id: p.id,
        title: p.title,
        model_profile_id: p.model_profile_id,
        message_count: p.messages.len(),
        revision: p.revision,
        created_at_ms: p.created_at_ms,
        updated_at_ms: p.updated_at_ms,
    }
}
impl HistoryRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            lock: Mutex::new(()),
        }
    }
    fn read(&self) -> Result<History, ToolError> {
        let path = self.directory.join("conversation-history-v2.json");
        if path.exists() {
            let value = read_json(&path)?;
            let mut history: History = serde_json::from_value(value)
                .map_err(|_| error("invalid_history", "历史文件格式无效；原文件已保留"))?;
            if history.version != 2 {
                return Err(error("history_version", "历史版本不受支持；原文件已保留"));
            }
            // Active turns are only cleared by explicit startup recovery, not during a running save.
            validate_history(&mut history)?;
            return Ok(history);
        }
        let legacy = self.directory.join("conversation-history.json");
        if !legacy.exists() {
            return Ok(History::default());
        }
        let root = read_json(&legacy)?;
        let mut migrated = migrate(root.get("history").cloned().unwrap_or(root))?;
        validate_history(&mut migrated)?;
        self.write(&migrated)?;
        Ok(migrated)
    }
    fn write(&self, history: &History) -> Result<(), ToolError> {
        let bytes =
            serde_json::to_vec(history).map_err(|_| error("history_write", "历史序列化失败"))?;
        if bytes.len() as u64 > MAX_HISTORY_BYTES {
            return Err(error(
                "history_full",
                "历史文件达到 64 MiB 上限；未删除任何已有记录",
            ));
        }
        std::fs::create_dir_all(&self.directory)
            .map_err(|_| error("history_write", "无法创建历史目录"))?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|_| error("history_write", "无法创建历史临时文件"))?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| error("history_write", "无法写入历史文件"))?;
        file.persist(self.directory.join("conversation-history-v2.json"))
            .map_err(|_| error("history_write", "无法原子替换历史文件；原记录保持不变"))?;
        Ok(())
    }
    pub fn recover(&self) -> Result<(), ToolError> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = self.read()?;
        let mut changed = false;
        for s in &mut h.conversations {
            if let Some(turn) = s.active_turn.take() {
                if let Some(user) = s
                    .transcript
                    .iter_mut()
                    .find(|m| m.turn_id == turn && m.message.role == MessageRole::User)
                {
                    user.turn_status = Some(TurnStatus::Cancelled);
                }
                s.revision += 1;
                changed = true;
                if let Some(m) = s.transcript.last_mut() {
                    m.note = Some("上次运行中断；未确认的工具执行不会自动重试".into());
                }
            }
        }
        if changed {
            self.write(&h)?;
        }
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<ConversationSummary>, ToolError> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let h = self.read()?;
        let mut list: Vec<_> = h.conversations.iter().map(summary).collect();
        list.sort_by_key(|s| std::cmp::Reverse((s.updated_at_ms, s.created_at_ms)));
        Ok(list)
    }
    pub async fn get(&self, id: &str) -> Result<Option<ConversationRecord>, ToolError> {
        Ok(self.load(id).await?.as_ref().map(projection))
    }
    pub fn rename(&self, id: &str, title: &str) -> Result<ConversationSummary, ToolError> {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() || title.chars().count() > 80 {
            return Err(error("invalid_title", "标题应为 1–80 字符"));
        }
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = self.read()?;
        let s = h
            .conversations
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| error("missing_conversation", "历史对话不存在"))?;
        if s.active_turn.is_some() {
            return Err(error("conversation_busy", "请先停止当前生成再重命名"));
        }
        s.title = title;
        s.revision += 1;
        s.updated_at_ms = now();
        let result = summary(s);
        self.write(&h)?;
        Ok(result)
    }
    pub fn delete(&self, id: &str) -> Result<bool, ToolError> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = self.read()?;
        if h.conversations
            .iter()
            .any(|s| s.id == id && s.active_turn.is_some())
        {
            return Err(error("conversation_busy", "请先停止当前生成"));
        }
        let len = h.conversations.len();
        h.conversations.retain(|s| s.id != id);
        if len == h.conversations.len() {
            return Ok(false);
        }
        self.write(&h)?;
        Ok(true)
    }
}
#[async_trait]
impl SessionRepository for HistoryRepository {
    async fn load(&self, id: &str) -> Result<Option<Session>, ToolError> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        Ok(self.read()?.conversations.into_iter().find(|s| s.id == id))
    }
    async fn save(&self, session: &Session, expected_revision: u64) -> Result<Session, ToolError> {
        let _lock = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut h = self.read()?;
        let existing = h.conversations.iter().position(|s| s.id == session.id);
        if existing.map_or(0, |i| h.conversations[i].revision) != expected_revision {
            return Err(error(
                "revision_conflict",
                "历史记录已更新，不能覆盖较新版本",
            ));
        }
        let mut saved = session.clone();
        saved.revision += 1;
        saved.updated_at_ms = now();
        if saved.created_at_ms == 0 {
            saved.created_at_ms = saved.updated_at_ms;
        }
        if saved.title.is_empty() {
            saved.title = saved
                .transcript
                .iter()
                .find(|m| m.message.role == MessageRole::User)
                .map(|m| {
                    let text = m
                        .message
                        .text_content()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut chars = text.chars();
                    let mut title: String = chars.by_ref().take(40).collect();
                    if chars.next().is_some() {
                        title.push('…');
                    }
                    title
                })
                .unwrap_or_else(|| "新对话".into());
        }
        if let Some(i) = existing {
            h.conversations[i] = saved.clone();
        } else {
            h.conversations.push(saved.clone());
        }
        validate_history(&mut h)?;
        self.write(&h)?;
        Ok(saved)
    }
}
fn read_json(path: &std::path::Path) -> Result<Value, ToolError> {
    if std::fs::metadata(path)
        .map_err(|_| error("history_read", "无法读取历史文件"))?
        .len()
        > MAX_HISTORY_BYTES
    {
        return Err(error("history_full", "历史文件超过大小限制；文件已保留"));
    }
    serde_json::from_slice(
        &std::fs::read(path).map_err(|_| error("history_read", "无法读取历史文件"))?,
    )
    .map_err(|_| error("invalid_history", "历史 JSON 损坏；文件已保留"))
}
fn validate_history(h: &mut History) -> Result<(), ToolError> {
    let mut ids = std::collections::HashSet::new();
    for s in &h.conversations {
        if s.id.is_empty() || !ids.insert(&s.id) {
            return Err(error("invalid_history", "重复或无效的会话 ID"));
        }
        let mut messages = std::collections::HashSet::new();
        let mut pending = std::collections::HashSet::new();
        for entry in &s.transcript {
            if entry.id.is_empty() || !messages.insert(&entry.id) {
                return Err(error("invalid_history", "重复或无效的消息 ID"));
            }
            let message = &entry.message;
            if message.role == MessageRole::Tool {
                if !message.tool_calls.is_empty()
                    || !message
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| pending.remove(id))
                {
                    return Err(error("invalid_history", "工具结果缺少匹配调用"));
                }
            } else {
                if !pending.is_empty() || message.tool_call_id.is_some() {
                    return Err(error("invalid_history", "工具调用和结果不完整"));
                }
                if !message.tool_calls.is_empty() && message.role != MessageRole::Assistant {
                    return Err(error("invalid_history", "无效的工具调用角色"));
                }
                for call in &message.tool_calls {
                    if call.id.is_empty() || !pending.insert(call.id.clone()) {
                        return Err(error("invalid_history", "重复或无效的工具调用 ID"));
                    }
                }
            }
        }
        if !pending.is_empty() {
            return Err(error("invalid_history", "工具调用缺少结果"));
        }
    }
    Ok(())
}
fn migrate(value: Value) -> Result<History, ToolError> {
    if value["version"] != 1 {
        return Err(error("history_version", "旧历史版本不受支持；文件已保留"));
    }
    let records: Vec<ConversationRecord> = serde_json::from_value(
        value
            .get("conversations")
            .cloned()
            .ok_or_else(|| error("invalid_history", "缺少历史列表"))?,
    )
    .map_err(|_| error("invalid_history", "旧历史记录无法读取"))?;
    let mut h = History::default();
    for record in records {
        let mut session = Session::new(record.id, record.model_profile_id);
        session.title = record.title;
        session.created_at_ms = record.created_at_ms;
        session.updated_at_ms = record.updated_at_ms;
        let mut turn = String::new();
        for message in record.messages {
            let role = match message.role.as_str() {
                "user" => {
                    turn = message.id.clone();
                    MessageRole::User
                }
                "assistant" => MessageRole::Assistant,
                _ => return Err(error("invalid_history", "旧历史包含无效角色")),
            };
            session.transcript.push(TranscriptMessage {
                id: message.id,
                turn_id: turn.clone(),
                message: ModelMessage::text(role, message.content),
                note: message.note,
                omitted: false,
                turn_status: None,
                tool_provenance: vec![],
            });
        }
        h.conversations.push(session);
    }
    Ok(h)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn migrates_without_touching_v1_and_does_not_resurrect_deleted_records() {
        let dir = tempfile::tempdir().unwrap();
        let source=serde_json::json!({"history":{"version":1,"conversations":[{"id":"old","title":"标题","modelProfileId":"mock","messages":[{"id":"m","role":"user","content":"hello"}],"createdAtMs":1,"updatedAtMs":2}]}}).to_string();
        std::fs::write(dir.path().join("conversation-history.json"), &source).unwrap();
        let r = HistoryRepository::new(dir.path().into());
        assert_eq!(
            r.get("old").await.unwrap().unwrap().messages[0].content,
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("conversation-history.json")).unwrap(),
            source
        );
        assert!(r.delete("old").unwrap());
        assert!(r.list().unwrap().is_empty());
    }
    #[tokio::test]
    async fn compare_and_swap_and_corrupt_history_are_safe() {
        let dir = tempfile::tempdir().unwrap();
        let r = HistoryRepository::new(dir.path().into());
        let s = Session::new("s".into(), "mock".into());
        let saved = r.save(&s, 0).await.unwrap();
        assert_eq!(saved.revision, 1);
        assert!(r.save(&s, 0).await.is_err());
        std::fs::write(dir.path().join("conversation-history-v2.json"), "broken").unwrap();
        assert!(r.load("s").await.is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("conversation-history-v2.json")).unwrap(),
            "broken"
        );
    }

    #[tokio::test]
    async fn interrupted_tools_recover_with_pairing_and_invalid_history_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let r = HistoryRepository::new(dir.path().into());
        let mut session = Session::new("s".into(), "mock".into());
        session.active_turn = Some("t".into());
        let mut assistant = ModelMessage::text(MessageRole::Assistant, "");
        assistant.tool_calls = vec![ToolCall {
            id: "call".into(),
            name: "echo".into(),
            arguments: "{}".into(),
        }];
        let mut result = ModelMessage::text(
            MessageRole::Tool,
            ToolResult::failure("interrupted_unknown", "unknown").model_text(),
        );
        result.tool_call_id = Some("call".into());
        session.transcript = vec![
            TranscriptMessage {
                id: "a".into(),
                turn_id: "t".into(),
                message: assistant,
                note: None,
                omitted: false,
                turn_status: None,
                tool_provenance: vec![],
            },
            TranscriptMessage {
                id: "r".into(),
                turn_id: "t".into(),
                message: result,
                note: None,
                omitted: false,
                turn_status: None,
                tool_provenance: vec![],
            },
        ];
        let saved = r.save(&session, 0).await.unwrap();
        assert!(r.rename("s", "busy").is_err());
        assert!(r.delete("s").is_err());
        r.recover().unwrap();
        let recovered = r.load("s").await.unwrap().unwrap();
        assert_eq!(recovered.revision, saved.revision + 1);
        assert!(recovered.active_turn.is_none());
        assert!(
            recovered.transcript[1]
                .message
                .text_content()
                .contains("interrupted_unknown")
        );
        let original = std::fs::read(dir.path().join("conversation-history-v2.json")).unwrap();
        let mut invalid = recovered.clone();
        invalid.transcript.pop();
        assert!(r.save(&invalid, recovered.revision).await.is_err());
        assert_eq!(
            std::fs::read(dir.path().join("conversation-history-v2.json")).unwrap(),
            original
        );
        r.rename("s", "  新  标题  ").unwrap();
        assert_eq!(r.get("s").await.unwrap().unwrap().title, "新 标题");
    }
    #[test]
    fn unknown_legacy_version_and_write_failure_preserve_original() {
        let dir = tempfile::tempdir().unwrap();
        let original = "{\"version\":99,\"conversations\":[]}";
        let path = dir.path().join("conversation-history.json");
        std::fs::write(&path, original).unwrap();
        let r = HistoryRepository::new(dir.path().into());
        assert!(r.list().is_err());
        assert!(!dir.path().join("conversation-history-v2.json").exists());
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "keep").unwrap();
        let r = HistoryRepository::new(blocked.clone());
        assert!(r.write(&History::default()).is_err());
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "keep");
    }
    #[tokio::test]
    async fn title_ordering_wire_names_and_creation_time_remain_compatible() {
        let dir = tempfile::tempdir().unwrap();
        let r = HistoryRepository::new(dir.path().into());
        let mut session = Session::new("s".into(), "mock".into());
        session.transcript.push(TranscriptMessage {
            id: "u".into(),
            turn_id: "t".into(),
            message: ModelMessage::text(
                MessageRole::User,
                format!("  {}  尾部", "助手".repeat(21)),
            ),
            note: None,
            omitted: false,
            turn_status: None,
            tool_provenance: vec![],
        });
        let first = r.save(&session, 0).await.unwrap();
        assert_eq!(first.title.chars().count(), 41);
        assert!(first.title.ends_with('…'));
        assert!(r.rename("s", "  \n").is_err());
        assert!(r.rename("s", &"对".repeat(81)).is_err());
        assert!(r.rename("missing", "标题").is_err());
        assert!(!r.delete("missing").unwrap());
        r.rename("s", "手动标题").unwrap();
        let mut current = r.load("s").await.unwrap().unwrap();
        current.model_profile_id = "remote".into();
        let saved = r.save(&current, current.revision).await.unwrap();
        assert_eq!(saved.created_at_ms, first.created_at_ms);
        assert_eq!(saved.title, "手动标题");
        assert_eq!(saved.model_profile_id, "remote");
        let wire = serde_json::to_value(r.get("s").await.unwrap().unwrap()).unwrap();
        assert_eq!(wire["modelProfileId"], "remote");
        assert!(wire.get("created_at_ms").is_none());
        let mut older = saved.clone();
        older.id = "older".into();
        older.updated_at_ms = 0;
        r.write(&History {
            version: 2,
            conversations: vec![older, saved],
        })
        .unwrap();
        assert_eq!(r.list().unwrap()[0].id, "s");
    }
}
