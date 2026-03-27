use crate::graph::types::ActionGraph;

/// Handle a `graph_action_summary` call. Returns a compact summary of
/// what has happened in the current session's action graph.
pub fn handle(action_graph: &ActionGraph) -> String {
    if action_graph.nodes.is_empty() && action_graph.actions.is_empty() {
        return "No actions recorded in this session.".to_string();
    }

    let query_nodes: Vec<_> = action_graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "query")
        .collect();
    let file_nodes: Vec<_> = action_graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "file")
        .collect();
    let edit_nodes: Vec<_> = action_graph
        .nodes
        .iter()
        .filter(|n| n.node_type == "edit")
        .collect();

    let retrieve_edges = action_graph
        .edges
        .iter()
        .filter(|e| e.rel == "retrieved")
        .count();
    let edited_edges = action_graph
        .edges
        .iter()
        .filter(|e| e.rel == "edited")
        .count();

    let mut output = String::from("Session Action Summary:\n");
    output.push_str(&format!("  Queries: {}\n", query_nodes.len()));
    output.push_str(&format!("  Files accessed: {}\n", file_nodes.len()));
    output.push_str(&format!("  Edits: {}\n", edit_nodes.len()));
    output.push_str(&format!("  Retrievals: {retrieve_edges}\n"));
    output.push_str(&format!("  Edit operations: {edited_edges}\n"));
    output.push_str(&format!("  Total actions: {}\n", action_graph.actions.len()));

    // List queries
    if !query_nodes.is_empty() {
        output.push_str("\nQueries:\n");
        for node in &query_nodes {
            let text = node
                .meta
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("(no text)");
            output.push_str(&format!("  - {text}\n"));
        }
    }

    // List edited files
    if !edit_nodes.is_empty() {
        let edited_files: Vec<&str> = action_graph
            .edges
            .iter()
            .filter(|e| e.rel == "edited")
            .map(|e| e.to.as_str())
            .collect();
        if !edited_files.is_empty() {
            output.push_str("\nEdited files:\n");
            for file in &edited_files {
                output.push_str(&format!("  - {file}\n"));
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{ActionEdge, ActionEntry, ActionNode};

    #[test]
    fn summary_empty() {
        let graph = ActionGraph::default();
        let result = handle(&graph);
        assert!(result.contains("No actions recorded"));
    }

    #[test]
    fn summary_with_data() {
        let graph = ActionGraph {
            nodes: vec![
                ActionNode {
                    id: "query:test".to_string(),
                    node_type: "query".to_string(),
                    meta: Some(serde_json::json!({"text": "how does auth work"})),
                },
                ActionNode {
                    id: "src/auth.rs".to_string(),
                    node_type: "file".to_string(),
                    meta: None,
                },
                ActionNode {
                    id: "edit:123".to_string(),
                    node_type: "edit".to_string(),
                    meta: None,
                },
            ],
            edges: vec![
                ActionEdge {
                    from: "query:test".to_string(),
                    to: "src/auth.rs".to_string(),
                    rel: "retrieved".to_string(),
                    ts: Some(123),
                },
                ActionEdge {
                    from: "edit:123".to_string(),
                    to: "src/auth.rs".to_string(),
                    rel: "edited".to_string(),
                    ts: Some(124),
                },
            ],
            actions: vec![ActionEntry {
                ts: 123,
                kind: "retrieve".to_string(),
                payload: None,
            }],
            ..Default::default()
        };

        let result = handle(&graph);
        assert!(result.contains("Queries: 1"));
        assert!(result.contains("Files accessed: 1"));
        assert!(result.contains("Edits: 1"));
        assert!(result.contains("how does auth work"));
        assert!(result.contains("src/auth.rs"));
    }
}
