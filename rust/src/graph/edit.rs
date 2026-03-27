use crate::graph::types::{ActionEdge, ActionEntry, ActionGraph, ActionNode};

/// Handle a `graph_register_edit` call. Records which files were edited
/// in the action graph so future retrievals can prioritize them.
pub fn handle(action_graph: &mut ActionGraph, files: &[String]) -> String {
    if files.is_empty() {
        return "No files specified.".to_string();
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let edit_node_id = format!("edit:{ts}");

    // Add an edit action node
    action_graph.nodes.push(ActionNode {
        id: edit_node_id.clone(),
        node_type: "edit".to_string(),
        meta: Some(serde_json::json!({
            "files": files,
            "ts": ts,
        })),
    });

    // Add edges from edit node to each file
    for file in files {
        // Ensure the file node exists
        let file_exists = action_graph.nodes.iter().any(|n| n.id == *file);
        if !file_exists {
            action_graph.nodes.push(ActionNode {
                id: file.clone(),
                node_type: "file".to_string(),
                meta: None,
            });
        }

        action_graph.edges.push(ActionEdge {
            from: edit_node_id.clone(),
            to: file.clone(),
            rel: "edited".to_string(),
            ts: Some(ts),
        });
    }

    // Log the action
    action_graph.actions.push(ActionEntry {
        ts,
        kind: "edit".to_string(),
        payload: Some(serde_json::json!({
            "kind": "edit",
            "files": files,
        })),
    });

    format!(
        "Registered edit: {} file(s) [{}]",
        files.len(),
        files.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_single_edit() {
        let mut graph = ActionGraph::default();
        let result = handle(&mut graph, &["src/auth.rs".to_string()]);

        assert!(result.contains("1 file(s)"));
        assert!(result.contains("src/auth.rs"));

        // Should have 2 nodes: edit node + file node
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].node_type, "edit");
        assert_eq!(graph.nodes[1].node_type, "file");
        assert_eq!(graph.nodes[1].id, "src/auth.rs");

        // Should have 1 edge
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].rel, "edited");
        assert_eq!(graph.edges[0].to, "src/auth.rs");

        // Should have 1 action
        assert_eq!(graph.actions.len(), 1);
        assert_eq!(graph.actions[0].kind, "edit");
    }

    #[test]
    fn register_multiple_edits() {
        let mut graph = ActionGraph::default();
        let files = vec![
            "src/auth.rs".to_string(),
            "src/auth.rs::handleLogin".to_string(),
        ];
        let result = handle(&mut graph, &files);

        assert!(result.contains("2 file(s)"));
        // 1 edit node + 2 file nodes = 3
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn register_edit_no_duplicate_file_node() {
        let mut graph = ActionGraph::default();
        // Pre-add a file node
        graph.nodes.push(ActionNode {
            id: "src/auth.rs".to_string(),
            node_type: "file".to_string(),
            meta: None,
        });

        handle(&mut graph, &["src/auth.rs".to_string()]);

        // Should not duplicate the file node
        let file_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.id == "src/auth.rs")
            .collect();
        assert_eq!(file_nodes.len(), 1);
    }

    #[test]
    fn register_edit_empty_files() {
        let mut graph = ActionGraph::default();
        let result = handle(&mut graph, &[]);
        assert!(result.contains("No files specified"));
        assert!(graph.nodes.is_empty());
    }
}
