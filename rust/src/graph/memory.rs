use crate::graph::types::MemoryEntry;

const MAX_ENTRIES: usize = 50;

/// Handle a `graph_add_memory` call. Adds a new memory entry to the context store,
/// prunes old entries if over the cap, and returns confirmation.
pub fn handle(
    context_store: &mut Vec<MemoryEntry>,
    kind: &str,
    content: &str,
    tags: Vec<String>,
    files: Vec<String>,
) -> String {
    // Validate kind
    let valid_kinds = ["decision", "task", "next", "fact", "blocker"];
    if !valid_kinds.contains(&kind) {
        return format!(
            "Invalid kind '{kind}'. Must be one of: {}",
            valid_kinds.join(", ")
        );
    }

    // Validate content length (max 15 words)
    let word_count = content.split_whitespace().count();
    if word_count > 15 {
        return format!("Content too long ({word_count} words). Max 15 words.");
    }

    // Generate a unique ID based on epoch milliseconds
    let epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let now = chrono::Utc::now().to_rfc3339();

    let entry = MemoryEntry {
        id: format!("mem:{epoch_ms}"),
        kind: kind.to_string(),
        content: content.to_string(),
        tags,
        files,
        created_at: now.clone(),
        created_epoch: Some(epoch_ms as u64),
        updated_at: Some(now),
        stale: Some(false),
        ..Default::default()
    };

    context_store.push(entry);

    // Prune if over the cap — remove oldest non-decision entries first
    let pruned = prune(context_store);

    let mut result = format!("Memory added: [{kind}] {content}");
    if pruned > 0 {
        result.push_str(&format!(" ({pruned} old entries pruned)"));
    }
    result
}

/// Prune context store to MAX_ENTRIES. Removes oldest non-decision entries first.
/// Returns number of entries removed.
fn prune(store: &mut Vec<MemoryEntry>) -> usize {
    if store.len() <= MAX_ENTRIES {
        return 0;
    }

    let to_remove = store.len() - MAX_ENTRIES;
    let mut removed = 0;

    // First pass: remove oldest non-decision, non-task entries
    let mut i = 0;
    while removed < to_remove && i < store.len() {
        if store[i].kind != "decision" && store[i].kind != "task" {
            store.remove(i);
            removed += 1;
        } else {
            i += 1;
        }
    }

    // Second pass: if still over cap, remove oldest decisions
    while store.len() > MAX_ENTRIES {
        store.remove(0);
        removed += 1;
    }

    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_memory_basic() {
        let mut store = Vec::new();
        let result = handle(
            &mut store,
            "fact",
            "auth uses JWT tokens",
            vec!["auth".to_string()],
            vec!["src/auth.rs".to_string()],
        );
        assert!(result.contains("Memory added"));
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].kind, "fact");
        assert_eq!(store[0].content, "auth uses JWT tokens");
        assert_eq!(store[0].tags, vec!["auth"]);
        assert_eq!(store[0].files, vec!["src/auth.rs"]);
        assert!(store[0].id.starts_with("mem:"));
    }

    #[test]
    fn rejects_invalid_kind() {
        let mut store = Vec::new();
        let result = handle(&mut store, "invalid", "test", vec![], vec![]);
        assert!(result.contains("Invalid kind"));
        assert!(store.is_empty());
    }

    #[test]
    fn rejects_long_content() {
        let mut store = Vec::new();
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen";
        let result = handle(&mut store, "fact", long, vec![], vec![]);
        assert!(result.contains("too long"));
        assert!(store.is_empty());
    }

    #[test]
    fn prunes_at_max_entries() {
        let mut store: Vec<MemoryEntry> = (0..55)
            .map(|i| MemoryEntry {
                id: format!("mem:{i}"),
                kind: if i % 10 == 0 {
                    "decision".to_string()
                } else {
                    "fact".to_string()
                },
                content: format!("entry {i}"),
                created_epoch: Some(i),
                ..Default::default()
            })
            .collect();

        let removed = prune(&mut store);
        assert_eq!(store.len(), MAX_ENTRIES);
        assert!(removed > 0);

        // Decisions should be preserved preferentially
        let decision_count = store.iter().filter(|e| e.kind == "decision").count();
        assert!(decision_count > 0, "decisions should be preserved");
    }
}
