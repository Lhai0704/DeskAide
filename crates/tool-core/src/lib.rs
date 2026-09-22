//! Provider-neutral tool registration, validation and execution.
use async_trait::async_trait;
use deskaide_assistant_core::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolResult;
}
struct RegisteredTool {
    definition: ToolDefinition,
    executor: Arc<dyn ToolExecutor>,
    validator: jsonschema::Validator,
}
pub struct ToolRegistry {
    entries: RwLock<BTreeMap<String, Arc<RegisteredTool>>>,
    changes: tokio::sync::watch::Sender<u64>,
}
impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            changes: tokio::sync::watch::channel(0).0,
        }
    }
}
pub struct ToolPermissionPolicy;
impl ToolPermissionPolicy {
    pub fn requires_approval(definition: &ToolDefinition) -> bool {
        !matches!(
            (&definition.source, &definition.risk),
            (ToolSource::Internal, ToolRisk::ReadOnly)
        )
    }
}
fn safe_schema(value: &Value, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Object(o) => o.iter().all(|(k, v)| {
            if matches!(k.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef")
                && v.as_str().is_none_or(|s| !s.starts_with('#'))
            {
                return false;
            }
            // No file/network resource bases either.
            if k == "$id" {
                return false;
            }
            safe_schema(v, depth + 1)
        }),
        Value::Array(a) => a.iter().all(|v| safe_schema(v, depth + 1)),
        _ => true,
    }
}
impl ToolRegistry {
    pub fn subscribe_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }
    fn changed(&self) {
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
    pub fn register(
        &self,
        definition: ToolDefinition,
        executor: Arc<dyn ToolExecutor>,
    ) -> Result<(), ToolError> {
        if definition.id.is_empty()
            || definition.name.is_empty()
            || definition.name.len() > 64
            || !definition
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || definition.description.len() > 8192
            || definition.parameters.to_string().len() > MAX_TOOL_ARGUMENT_BYTES
            || !safe_schema(&definition.parameters, 0)
        {
            return Err(ToolError::new(
                "invalid_tool_definition",
                "工具定义无效或超过大小限制",
            ));
        }
        let validator = jsonschema::validator_for(&definition.parameters)
            .map_err(|_| ToolError::new("invalid_schema", "工具参数 Schema 不受支持"))?;
        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= 256 {
            return Err(ToolError::new("tool_limit", "工具数量超过限制"));
        }
        if entries.contains_key(&definition.name)
            || entries.values().any(|e| e.definition.id == definition.id)
        {
            return Err(ToolError::new("duplicate_tool", "工具已经注册"));
        }
        entries.insert(
            definition.name.clone(),
            Arc::new(RegisteredTool {
                definition,
                executor,
                validator,
            }),
        );
        self.changed();
        Ok(())
    }
    pub fn unregister(&self, id: &str) {
        self.entries
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, e| e.definition.id != id);
        self.changed();
    }
    pub fn unregister_owner(&self, server_id: &str) {
        self.entries.write().unwrap_or_else(|e|e.into_inner()).retain(|_,e|!matches!(&e.definition.source,ToolSource::Mcp {server_id:id,..} if id==server_id));
        self.changed();
    }
    pub fn list(&self) -> Vec<ToolDefinition> {
        self.entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|e| e.definition.clone())
            .collect()
    }
    pub fn lookup(&self, name: &str) -> Option<ToolDefinition> {
        self.entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .map(|e| e.definition.clone())
    }
    pub fn validate(&self, call: &ToolCall) -> Result<Value, ToolError> {
        if call.arguments.len() > MAX_TOOL_ARGUMENT_BYTES {
            return Err(ToolError::new("arguments_too_large", "工具参数过大"));
        }
        let value: Value = serde_json::from_str(&call.arguments)
            .map_err(|_| ToolError::new("invalid_arguments", "工具参数不是有效 JSON"))?;
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let entry = entries
            .get(&call.name)
            .ok_or_else(|| ToolError::new("unknown_tool", "工具不存在或已失效"))?;
        if !value.is_object() || !entry.validator.is_valid(&value) {
            return Err(ToolError::new("invalid_arguments", "工具参数不符合 Schema"));
        }
        Ok(value)
    }
    pub async fn execute(
        &self,
        call: &ToolCall,
        expected: &ToolDefinition,
        cancel: CancellationToken,
    ) -> ToolResult {
        let value = match self.validate(call) {
            Ok(v) => v,
            Err(e) => return ToolResult::failure(&e.code, &e.message),
        };
        let entry = self
            .entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&call.name)
            .cloned();
        let Some(entry) = entry.filter(|e| e.definition == *expected) else {
            return ToolResult::failure("tool_changed", "工具定义已更改，请重新发起调用");
        };
        if cancel.is_cancelled() {
            return ToolResult::failure("cancelled", "调用已取消");
        }
        let result = entry.executor.execute(value, cancel).await;
        if result.model_text().len() > MAX_TOOL_RESULT_BYTES {
            ToolResult::failure("result_too_large", "工具结果超过大小限制")
        } else {
            result
        }
    }
}

/// Keeps discovered processes alive while a turn waits for approval or executes tools.
pub struct ToolLease {
    pub definitions: Vec<ToolDefinition>,
    pub warnings: Vec<ToolError>,
    pub guard: Option<Box<dyn Send + Sync>>,
}
#[async_trait]
pub trait ToolDiscovery: Send + Sync {
    async fn acquire(&self, cancel: CancellationToken) -> ToolLease;
}
pub struct LocalTools(pub Arc<ToolRegistry>);
#[async_trait]
impl ToolDiscovery for LocalTools {
    async fn acquire(&self, _: CancellationToken) -> ToolLease {
        ToolLease {
            definitions: self.0.list(),
            warnings: vec![],
            guard: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Echo;
    #[async_trait]
    impl ToolExecutor for Echo {
        async fn execute(&self, v: Value, _: CancellationToken) -> ToolResult {
            ToolResult::success(v)
        }
    }
    fn definition() -> ToolDefinition {
        ToolDefinition {
            display_name: None,
            id: "echo".into(),
            name: "echo".into(),
            description: "echo".into(),
            parameters: serde_json::json!({"type":"object","required":["text"],"properties":{"text":{"type":"string"}},"additionalProperties":false}),
            source: ToolSource::Internal,
            risk: ToolRisk::ReadOnly,
            revision: 1,
        }
    }
    #[tokio::test]
    async fn registry_validation_execution_and_lifetime() {
        let r = ToolRegistry::default();
        let d = definition();
        r.register(d.clone(), Arc::new(Echo)).unwrap();
        assert!(r.register(d.clone(), Arc::new(Echo)).is_err());
        assert!(!ToolPermissionPolicy::requires_approval(&d));
        let c = ToolCall {
            id: "1".into(),
            name: "echo".into(),
            arguments: r#"{"text":"hello"}"#.into(),
        };
        assert_eq!(
            r.execute(&c, &d, CancellationToken::new()).await.content["text"],
            "hello"
        );
        assert!(
            r.validate(&ToolCall {
                arguments: "{}".into(),
                ..c.clone()
            })
            .is_err()
        );
        let mut changed = d.clone();
        changed.revision = 2;
        assert!(
            r.execute(&c, &changed, CancellationToken::new())
                .await
                .error
                .is_some()
        );
        r.unregister("echo");
        assert!(r.lookup("echo").is_none());
        assert!(r.validate(&c).is_err());
    }
    #[test]
    fn external_metadata_cannot_grant_permission_or_fetch_schema() {
        let mut d = definition();
        d.source = ToolSource::Mcp {
            server_id: "s".into(),
            server_name: "s".into(),
        };
        assert!(ToolPermissionPolicy::requires_approval(&d));
        d.parameters = serde_json::json!({"$ref":"file:///secret"});
        assert!(ToolRegistry::default().register(d, Arc::new(Echo)).is_err());
    }
}
