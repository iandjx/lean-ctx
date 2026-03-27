pub mod edit;
pub mod memory;
pub mod neighbors;
pub mod read;
pub mod retrieval;
pub mod scanner;
pub mod summary;
pub mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use types::{ActionGraph, InfoGraph, MemoryEntry, SymbolIndex};

/// Holds all dual-graph data in memory. Loaded from `.dual-graph/` JSON files.
///
/// This is the central struct that all graph tools operate on.
/// It is stored behind `Arc<RwLock<GraphState>>` in `LeanCtxServer`.
#[derive(Clone, Debug, Default)]
pub struct GraphState {
    /// Absolute path to the `.dual-graph/` directory, if known.
    pub dual_graph_dir: Option<PathBuf>,
    /// The project root (from info_graph.root or the parent of .dual-graph/).
    pub project_root: Option<String>,
    /// The semantic graph of the codebase (files, symbols, edges).
    pub info_graph: Option<InfoGraph>,
    /// Fast lookup: "file::symbol" → line range.
    pub symbol_index: SymbolIndex,
    /// Persistent memory entries (decisions, tasks, facts, blockers).
    pub context_store: Vec<MemoryEntry>,
    /// Session-level action tracking (queries, reads, edits).
    pub action_graph: ActionGraph,
}

impl GraphState {
    /// Try to load all `.dual-graph/*.json` files from the given directory.
    /// Returns a populated `GraphState` on success, or a default (empty) state
    /// if the directory doesn't exist or files are missing.
    ///
    /// This never panics — missing or malformed files are silently skipped.
    pub fn load(dual_graph_dir: &Path) -> Self {
        let mut state = GraphState {
            dual_graph_dir: Some(dual_graph_dir.to_path_buf()),
            ..Default::default()
        };

        // info_graph.json
        if let Some(graph) = load_json::<InfoGraph>(&dual_graph_dir.join("info_graph.json")) {
            state.project_root = Some(graph.root.clone());
            state.info_graph = Some(graph);
        }

        // symbol_index.json
        if let Some(index) =
            load_json::<HashMap<String, types::SymbolEntry>>(&dual_graph_dir.join("symbol_index.json"))
        {
            state.symbol_index = index;
        }

        // context-store.json
        if let Some(entries) =
            load_json::<Vec<MemoryEntry>>(&dual_graph_dir.join("context-store.json"))
        {
            state.context_store = entries;
        }

        // chat_action_graph.json
        if let Some(action) =
            load_json::<ActionGraph>(&dual_graph_dir.join("chat_action_graph.json"))
        {
            state.action_graph = action;
        }

        state
    }

    /// Try to load from the `.dual-graph/` subdirectory of the current working directory.
    /// Returns an empty state if the directory doesn't exist.
    pub fn load_from_cwd() -> Self {
        match std::env::current_dir() {
            Ok(cwd) => {
                let dir = cwd.join(".dual-graph");
                if dir.exists() {
                    Self::load(&dir)
                } else {
                    Self::default()
                }
            }
            Err(_) => Self::default(),
        }
    }

    /// Save the context store back to `.dual-graph/context-store.json`.
    pub fn save_context_store(&self) -> anyhow::Result<()> {
        let dir = self
            .dual_graph_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no dual-graph directory set"))?;
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string_pretty(&self.context_store)?;
        std::fs::write(dir.join("context-store.json"), json)?;
        Ok(())
    }

    /// Save the action graph back to `.dual-graph/chat_action_graph.json`.
    pub fn save_action_graph(&self) -> anyhow::Result<()> {
        let dir = self
            .dual_graph_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no dual-graph directory set"))?;
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string_pretty(&self.action_graph)?;
        std::fs::write(dir.join("chat_action_graph.json"), json)?;
        Ok(())
    }

    /// Save the info graph and symbol index to disk.
    pub fn save_info_graph(&self) -> anyhow::Result<()> {
        let dir = self
            .dual_graph_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no dual-graph directory set"))?;
        std::fs::create_dir_all(dir)?;
        if let Some(ref graph) = self.info_graph {
            let json = serde_json::to_string_pretty(graph)?;
            std::fs::write(dir.join("info_graph.json"), json)?;
        }
        let index_json = serde_json::to_string_pretty(&self.symbol_index)?;
        std::fs::write(dir.join("symbol_index.json"), index_json)?;
        Ok(())
    }

    /// Returns true if a project has been scanned (info_graph loaded).
    pub fn has_project(&self) -> bool {
        self.info_graph.is_some()
    }

    /// Returns the number of file nodes in the info graph, or 0 if not loaded.
    pub fn file_count(&self) -> usize {
        self.info_graph
            .as_ref()
            .map_or(0, |g| g.file_count)
    }

    /// Record a file read in the action graph (called by both ctx_read and graph_read).
    pub fn record_read(&mut self, file: &str, query_context: Option<&str>) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Ensure file node exists
        let file_exists = self.action_graph.nodes.iter().any(|n| n.id == file);
        if !file_exists {
            self.action_graph.nodes.push(types::ActionNode {
                id: file.to_string(),
                node_type: "file".to_string(),
                meta: None,
            });
        }

        // If there's a query context, create an edge from the latest query to this file
        if let Some(query) = query_context {
            let query_id = format!("query:{}", &query[..query.len().min(40)]);
            let query_exists = self.action_graph.nodes.iter().any(|n| n.id == query_id);
            if !query_exists {
                self.action_graph.nodes.push(types::ActionNode {
                    id: query_id.clone(),
                    node_type: "query".to_string(),
                    meta: Some(serde_json::json!({"text": query})),
                });
            }
            self.action_graph.edges.push(types::ActionEdge {
                from: query_id,
                to: file.to_string(),
                rel: "retrieved".to_string(),
                ts: Some(ts),
            });
        }

        // Log the action
        self.action_graph.actions.push(types::ActionEntry {
            ts,
            kind: "read".to_string(),
            payload: Some(serde_json::json!({"file": file})),
        });
    }
}

/// Helper: deserialize a JSON file, returning None on any error.
fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dual_graph_dir() -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("Codex-CLI-Compact/.dual-graph");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    #[test]
    fn load_real_dual_graph() {
        let dir = match dual_graph_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: Codex-CLI-Compact/.dual-graph not found");
                return;
            }
        };
        let state = GraphState::load(&dir);
        assert!(state.has_project(), "should have loaded info_graph");
        assert!(state.file_count() > 0, "should have file nodes");
        assert!(
            !state.symbol_index.is_empty(),
            "should have loaded symbol index"
        );
        assert!(
            !state.context_store.is_empty(),
            "should have loaded context store"
        );
        assert!(
            !state.action_graph.nodes.is_empty(),
            "should have loaded action graph"
        );
        assert!(
            state.project_root.is_some(),
            "project_root should be set from info_graph.root"
        );
    }

    #[test]
    fn load_nonexistent_dir_returns_empty() {
        let state = GraphState::load(Path::new("/tmp/nonexistent_dual_graph_test_12345"));
        assert!(!state.has_project());
        assert_eq!(state.file_count(), 0);
        assert!(state.symbol_index.is_empty());
        assert!(state.context_store.is_empty());
    }

    #[test]
    fn save_and_reload_context_store() {
        let tmp = std::env::temp_dir().join("lean_ctx_graph_test_ctx");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut state = GraphState {
            dual_graph_dir: Some(tmp.clone()),
            ..Default::default()
        };
        state.context_store.push(types::MemoryEntry {
            id: "mem:123".to_string(),
            kind: "fact".to_string(),
            content: "test entry".to_string(),
            tags: vec!["test".to_string()],
            files: vec![],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            ..Default::default()
        });
        state.save_context_store().unwrap();

        // Reload
        let reloaded = GraphState::load(&tmp);
        assert_eq!(reloaded.context_store.len(), 1);
        assert_eq!(reloaded.context_store[0].id, "mem:123");
        assert_eq!(reloaded.context_store[0].content, "test entry");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_and_reload_action_graph() {
        let tmp = std::env::temp_dir().join("lean_ctx_graph_test_action");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut state = GraphState {
            dual_graph_dir: Some(tmp.clone()),
            ..Default::default()
        };
        state.action_graph.nodes.push(types::ActionNode {
            id: "query:test".to_string(),
            node_type: "query".to_string(),
            meta: None,
        });
        state.action_graph.edges.push(types::ActionEdge {
            from: "query:test".to_string(),
            to: "file.rs".to_string(),
            rel: "retrieved".to_string(),
            ts: Some(1234567890),
        });
        state.save_action_graph().unwrap();

        let reloaded = GraphState::load(&tmp);
        assert_eq!(reloaded.action_graph.nodes.len(), 1);
        assert_eq!(reloaded.action_graph.edges.len(), 1);
        assert_eq!(reloaded.action_graph.nodes[0].node_type, "query");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn record_read_adds_file_node_and_action() {
        let mut state = GraphState::default();
        assert!(state.action_graph.nodes.is_empty());

        state.record_read("src/auth.rs", None);

        assert_eq!(state.action_graph.nodes.len(), 1);
        assert_eq!(state.action_graph.nodes[0].id, "src/auth.rs");
        assert_eq!(state.action_graph.nodes[0].node_type, "file");
        assert_eq!(state.action_graph.actions.len(), 1);
        assert_eq!(state.action_graph.actions[0].kind, "read");
    }

    #[test]
    fn record_read_with_query_creates_edge() {
        let mut state = GraphState::default();

        state.record_read("src/auth.rs", Some("how does auth work"));

        // Should have 2 nodes: query + file
        assert_eq!(state.action_graph.nodes.len(), 2);
        let query_node = state.action_graph.nodes.iter().find(|n| n.node_type == "query");
        assert!(query_node.is_some());

        // Should have 1 edge: query → file
        assert_eq!(state.action_graph.edges.len(), 1);
        assert_eq!(state.action_graph.edges[0].rel, "retrieved");
        assert_eq!(state.action_graph.edges[0].to, "src/auth.rs");
    }

    #[test]
    fn record_read_no_duplicate_file_nodes() {
        let mut state = GraphState::default();

        state.record_read("src/auth.rs", None);
        state.record_read("src/auth.rs", None);

        // Only 1 file node, but 2 actions
        let file_nodes: Vec<_> = state
            .action_graph
            .nodes
            .iter()
            .filter(|n| n.id == "src/auth.rs")
            .collect();
        assert_eq!(file_nodes.len(), 1);
        assert_eq!(state.action_graph.actions.len(), 2);
    }
}
