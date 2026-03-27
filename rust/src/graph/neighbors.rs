use crate::graph::types::InfoGraph;

/// Handle a `graph_neighbors` call. Returns all files/symbols connected
/// to the given file via edges in the info-graph.
pub fn handle(info_graph: Option<&InfoGraph>, file: &str) -> String {
    let graph = match info_graph {
        Some(g) => g,
        None => return "No project scanned. Call graph_scan first.".to_string(),
    };

    let mut outgoing: Vec<(&str, &str)> = Vec::new(); // (target, rel)
    let mut incoming: Vec<(&str, &str)> = Vec::new(); // (source, rel)

    for edge in &graph.edges {
        if edge.from == file || edge.from.starts_with(&format!("{file}::")) {
            outgoing.push((&edge.to, &edge.rel));
        }
        if edge.to == file || edge.to.starts_with(&format!("{file}::")) {
            incoming.push((&edge.from, &edge.rel));
        }
    }

    if outgoing.is_empty() && incoming.is_empty() {
        return format!("No neighbors found for '{file}'.");
    }

    let mut output = format!("Neighbors of '{file}':\n");

    if !outgoing.is_empty() {
        output.push_str("\n→ Outgoing:\n");
        for (target, rel) in &outgoing {
            output.push_str(&format!("  {file} --[{rel}]--> {target}\n"));
        }
    }

    if !incoming.is_empty() {
        output.push_str("\n← Incoming:\n");
        for (source, rel) in &incoming {
            output.push_str(&format!("  {source} --[{rel}]--> {file}\n"));
        }
    }

    output.push_str(&format!(
        "\nTotal: {} outgoing, {} incoming",
        outgoing.len(),
        incoming.len()
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{GraphEdge, GraphNode, InfoGraph};

    fn test_graph() -> InfoGraph {
        InfoGraph {
            root: "/tmp".to_string(),
            node_count: 3,
            edge_count: 3,
            file_count: 3,
            symbol_count: 0,
            nodes: vec![
                GraphNode {
                    id: "a.rs".to_string(),
                    kind: "file".to_string(),
                    path: "a.rs".to_string(),
                    ..Default::default()
                },
                GraphNode {
                    id: "b.rs".to_string(),
                    kind: "file".to_string(),
                    path: "b.rs".to_string(),
                    ..Default::default()
                },
                GraphNode {
                    id: "c.rs".to_string(),
                    kind: "file".to_string(),
                    path: "c.rs".to_string(),
                    ..Default::default()
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "a.rs".to_string(),
                    to: "b.rs".to_string(),
                    rel: "imports".to_string(),
                },
                GraphEdge {
                    from: "a.rs".to_string(),
                    to: "c.rs".to_string(),
                    rel: "calls".to_string(),
                },
                GraphEdge {
                    from: "c.rs".to_string(),
                    to: "a.rs".to_string(),
                    rel: "references".to_string(),
                },
            ],
        }
    }

    #[test]
    fn neighbors_with_both_directions() {
        let graph = test_graph();
        let result = handle(Some(&graph), "a.rs");
        assert!(result.contains("Outgoing:"));
        assert!(result.contains("a.rs --[imports]--> b.rs"));
        assert!(result.contains("a.rs --[calls]--> c.rs"));
        assert!(result.contains("Incoming:"));
        assert!(result.contains("c.rs --[references]--> a.rs"));
        assert!(result.contains("2 outgoing, 1 incoming"));
    }

    #[test]
    fn neighbors_no_matches() {
        let graph = test_graph();
        let result = handle(Some(&graph), "unknown.rs");
        assert!(result.contains("No neighbors found"));
    }

    #[test]
    fn neighbors_no_graph() {
        let result = handle(None, "a.rs");
        assert!(result.contains("No project scanned"));
    }
}
