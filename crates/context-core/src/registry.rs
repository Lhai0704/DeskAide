use deskaide_assistant_core::ContextPayload;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextScope {
    CurrentTurn,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStrategy {
    ReplaceSelf,
    AppendSelf,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMutation {
    pub operation: UpdateStrategy,
    pub entries: usize,
    pub bytes: usize,
    pub sequence: u64,
}
/// Turn-owned, never persisted. Observability deliberately excludes source keys and payloads.
#[derive(Default)]
pub struct ContextRegistry {
    active: BTreeMap<String, Vec<ContextPayload>>,
    history: VecDeque<ContextMutation>,
    sequence: u64,
    total_bytes: usize,
}
impl ContextRegistry {
    pub fn update(
        &mut self,
        key: String,
        payload: ContextPayload,
        strategy: UpdateStrategy,
    ) -> Result<(), &'static str> {
        let bytes = payload_bytes(&payload);
        if key.len() > 256 || bytes > 256_000 {
            return Err("context entry exceeds limit");
        }
        if !self.active.contains_key(&key) && self.active.len() >= 128 {
            return Err("context source limit");
        }
        let replaced = if strategy == UpdateStrategy::ReplaceSelf {
            self.active
                .get(&key)
                .map_or(0, |items| items.iter().map(payload_bytes).sum())
        } else {
            0
        };
        let next_bytes = self.total_bytes.saturating_sub(replaced) + bytes;
        if next_bytes > 1024 * 1024 {
            return Err("context snapshot exceeds limit");
        }
        let bucket = self.active.entry(key).or_default();
        if strategy == UpdateStrategy::ReplaceSelf {
            bucket.clear();
        }
        if bucket.len() >= 128 {
            return Err("context entry limit");
        }
        bucket.push(payload);
        self.total_bytes = next_bytes;
        self.sequence += 1;
        if self.history.len() == 128 {
            self.history.pop_front();
        }
        self.history.push_back(ContextMutation {
            operation: strategy,
            entries: bucket.len(),
            bytes,
            sequence: self.sequence,
        });
        Ok(())
    }
    pub fn snapshot(&self) -> Vec<ContextPayload> {
        self.active.values().flatten().cloned().collect()
    }
    pub fn history(&self) -> Vec<ContextMutation> {
        self.history.iter().cloned().collect()
    }
    pub fn reset(&mut self) {
        self.active.clear();
        self.history.clear();
        self.sequence = 0;
        self.total_bytes = 0;
    }
    pub fn clear(&mut self, key: &str) {
        if let Some(items) = self.active.remove(key) {
            self.total_bytes = self
                .total_bytes
                .saturating_sub(items.iter().map(payload_bytes).sum());
        }
    }
}
fn payload_bytes(payload: &ContextPayload) -> usize {
    serde_json::to_vec(payload).map_or(usize::MAX, |bytes| bytes.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    use deskaide_assistant_core::ContextSourceType;
    fn payload(text: &str) -> ContextPayload {
        ContextPayload {
            source_type: ContextSourceType::SelectedText,
            application_name: None,
            process_name: None,
            window_title: None,
            url: None,
            selected_text: Some(text.into()),
            main_text: None,
            metadata: serde_json::Value::Null,
            images: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn replace_append_snapshot_reset_and_bounds() {
        let mut r = ContextRegistry::default();
        r.update("a".into(), payload("first"), UpdateStrategy::AppendSelf)
            .unwrap();
        let mut snapshot = r.snapshot();
        snapshot[0].selected_text = Some("edited".into());
        assert_eq!(r.snapshot()[0].selected_text.as_deref(), Some("first"));
        r.update("a".into(), payload("second"), UpdateStrategy::AppendSelf)
            .unwrap();
        assert_eq!(r.snapshot().len(), 2);
        for _ in 0..200 {
            r.update("a".into(), payload("last"), UpdateStrategy::ReplaceSelf)
                .unwrap();
        }
        assert_eq!(r.snapshot().len(), 1);
        assert_eq!(r.history().len(), 128);
        r.clear("a");
        assert!(r.snapshot().is_empty());
        r.reset();
        assert!(r.history().is_empty());
    }
}
