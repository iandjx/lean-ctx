use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── info_graph.json ─────────────────────────────────────────────────────────

/// Top-level structure of `.dual-graph/info_graph.json`.
/// Contains all file nodes, symbol nodes, and edges for a scanned project.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct InfoGraph {
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub edge_count: usize,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub symbol_count: usize,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// A node in the info-graph. Can be either a "file" or a "symbol".
/// File nodes have path/ext/size/content/summary.
/// Symbol nodes additionally have line_start/line_end/symbol_type/body_hash.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GraphNode {
    #[serde(default)]
    pub id: String,
    /// "file" or "symbol"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub ext: Option<String>,
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub file_hash: Option<String>,
    // Symbol-specific fields (None for file nodes)
    #[serde(default)]
    pub symbol_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub line_start: Option<usize>,
    #[serde(default)]
    pub line_end: Option<usize>,
    #[serde(default)]
    pub body_hash: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub exported: Option<bool>,
    /// Extracted doc comments / docstrings for this node (high-weight retrieval field).
    #[serde(default)]
    pub docs: Option<String>,
}

/// An edge connecting two nodes. `rel` is the relationship type:
/// "references", "imports", "exports", "calls".
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
}

// ── symbol_index.json ───────────────────────────────────────────────────────

/// Maps "file::symbol" keys to line ranges for symbol-level reads.
/// The JSON file is a flat object: `{ "src/auth.ts::handleLogin": { ... } }`.
pub type SymbolIndex = HashMap<String, SymbolEntry>;

/// A single entry in the symbol index.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SymbolEntry {
    pub line_start: usize,
    pub line_end: usize,
    #[serde(default)]
    pub body_hash: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub path: String,
}

// ── context-store.json ──────────────────────────────────────────────────────

/// A persistent memory entry stored in `.dual-graph/context-store.json`.
/// The JSON file is an array of these entries.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MemoryEntry {
    #[serde(default)]
    pub id: String,
    /// "query_association", "decision", "task", "fact", "blocker", "next"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub symbol_hash: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub created_epoch: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub stale: Option<bool>,
    #[serde(default)]
    pub stale_reason: Option<String>,
    #[serde(default)]
    pub observed_queries: Option<Vec<String>>,
    /// Flexible shape — keeps whatever graperoot stored.
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

// ── chat_action_graph.json ──────────────────────────────────────────────────

/// Session-level action graph tracking queries, reads, and edits.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionGraph {
    #[serde(default)]
    pub nodes: Vec<ActionNode>,
    #[serde(default)]
    pub edges: Vec<ActionEdge>,
    /// Maps "file::symbol" → cached content + metadata.
    #[serde(default)]
    pub files: HashMap<String, ActionFileEntry>,
    #[serde(default)]
    pub actions: Vec<ActionEntry>,
}

/// A node in the action graph. Type is "query" or "file".
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionNode {
    #[serde(default)]
    pub id: String,
    /// "query" or "file"
    #[serde(default, rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

/// An edge in the action graph.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub rel: String,
    #[serde(default)]
    pub ts: Option<u64>,
}

/// Cached file content stored in the action graph's `files` map.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionFileEntry {
    #[serde(default)]
    pub query_terms: Vec<String>,
    #[serde(default)]
    pub cached_content: Option<String>,
}

/// A logged action (retrieve, read, edit, etc.).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionEntry {
    #[serde(default)]
    pub ts: u64,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: path to the Codex-CLI-Compact .dual-graph directory.
    /// Tests skip gracefully if the directory doesn't exist (e.g. in CI).
    fn dual_graph_dir() -> Option<std::path::PathBuf> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
    fn deserialize_info_graph() {
        let dir = match dual_graph_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: Codex-CLI-Compact/.dual-graph not found");
                return;
            }
        };
        let json = std::fs::read_to_string(dir.join("info_graph.json")).unwrap();
        let graph: InfoGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(graph.node_count, 117, "expected 117 nodes");
        assert_eq!(graph.edge_count, 201, "expected 201 edges");
        assert_eq!(graph.nodes.len(), graph.node_count);
        assert_eq!(graph.edges.len(), graph.edge_count);

        // Verify file nodes have paths
        let file_nodes: Vec<_> = graph.nodes.iter().filter(|n| n.kind == "file").collect();
        assert!(!file_nodes.is_empty(), "should have file nodes");
        for node in &file_nodes {
            assert!(!node.path.is_empty(), "file node should have a path");
        }

        // Verify symbol nodes have line ranges
        let symbol_nodes: Vec<_> = graph.nodes.iter().filter(|n| n.kind == "symbol").collect();
        assert!(!symbol_nodes.is_empty(), "should have symbol nodes");
        for node in &symbol_nodes {
            assert!(node.line_start.is_some(), "symbol should have line_start");
            assert!(node.line_end.is_some(), "symbol should have line_end");
        }
    }

    #[test]
    fn deserialize_symbol_index() {
        let dir = match dual_graph_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: Codex-CLI-Compact/.dual-graph not found");
                return;
            }
        };
        let json = std::fs::read_to_string(dir.join("symbol_index.json")).unwrap();
        let index: SymbolIndex = serde_json::from_str(&json).unwrap();
        assert!(!index.is_empty(), "symbol index should not be empty");

        // Check a known entry
        let entry = index
            .get("pick_instances.py::files_touched")
            .expect("should have pick_instances.py::files_touched");
        assert_eq!(entry.line_start, 9);
        assert_eq!(entry.line_end, 10);
        assert_eq!(entry.confidence, "high");
    }

    #[test]
    fn deserialize_context_store() {
        let dir = match dual_graph_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: Codex-CLI-Compact/.dual-graph not found");
                return;
            }
        };
        let json = std::fs::read_to_string(dir.join("context-store.json")).unwrap();
        let entries: Vec<MemoryEntry> = serde_json::from_str(&json).unwrap();
        assert!(!entries.is_empty(), "context store should have entries");

        // Verify structure
        for entry in &entries {
            assert!(!entry.id.is_empty(), "entry should have id");
            assert!(!entry.kind.is_empty(), "entry should have kind");
            assert!(!entry.tags.is_empty(), "entry should have tags");
        }
    }

    #[test]
    fn deserialize_action_graph() {
        let dir = match dual_graph_dir() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: Codex-CLI-Compact/.dual-graph not found");
                return;
            }
        };
        let json = std::fs::read_to_string(dir.join("chat_action_graph.json")).unwrap();
        let graph: ActionGraph = serde_json::from_str(&json).unwrap();
        assert!(!graph.nodes.is_empty(), "action graph should have nodes");
        assert!(!graph.edges.is_empty(), "action graph should have edges");

        // Verify node types
        for node in &graph.nodes {
            assert!(
                node.node_type == "query" || node.node_type == "file",
                "unexpected node type: {}",
                node.node_type
            );
        }
    }

    #[test]
    fn info_graph_round_trip() {
        let graph = InfoGraph {
            root: "/tmp/test".to_string(),
            node_count: 1,
            edge_count: 1,
            file_count: 1,
            symbol_count: 0,
            nodes: vec![GraphNode {
                id: "test.rs".to_string(),
                kind: "file".to_string(),
                path: "test.rs".to_string(),
                ..Default::default()
            }],
            edges: vec![GraphEdge {
                from: "a.rs".to_string(),
                to: "b.rs".to_string(),
                rel: "imports".to_string(),
            }],
        };
        let json = serde_json::to_string(&graph).unwrap();
        let parsed: InfoGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.root, graph.root);
        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.edges.len(), 1);
        assert_eq!(parsed.edges[0].rel, "imports");
    }

    #[test]
    fn empty_json_deserializes_to_defaults() {
        // InfoGraph from empty object
        let graph: InfoGraph = serde_json::from_str("{}").unwrap();
        assert_eq!(graph.node_count, 0);
        assert!(graph.nodes.is_empty());

        // ActionGraph from empty object
        let action: ActionGraph = serde_json::from_str("{}").unwrap();
        assert!(action.nodes.is_empty());

        // SymbolIndex from empty object
        let index: SymbolIndex = serde_json::from_str("{}").unwrap();
        assert!(index.is_empty());

        // MemoryEntry list from empty array
        let entries: Vec<MemoryEntry> = serde_json::from_str("[]").unwrap();
        assert!(entries.is_empty());
    }
}
