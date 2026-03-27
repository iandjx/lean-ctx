use std::collections::HashMap;
use std::path::Path;

use walkdir::WalkDir;

use crate::core::deps;
use crate::core::signatures;
use crate::graph::types::{GraphEdge, GraphNode, InfoGraph, SymbolEntry, SymbolIndex};
use serde_json;

/// Supported source file extensions for scanning.
const SOURCE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "h", "cpp", "cc", "cxx", "hpp",
    "rb", "cs", "kt", "kts", "swift", "php", "md", "json", "toml", "yaml", "yml",
];

/// Max file size to include content in the graph (8 KB).
const MAX_CONTENT_SIZE: u64 = 8192;
/// Max file size to scan at all (512 KB).
const MAX_SCAN_SIZE: u64 = 524_288;

/// Directories to always skip.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".dual-graph",
    "node_modules",
    "target",
    "__pycache__",
    ".next",
    "dist",
    "build",
    ".venv",
    "venv",
    ".tox",
    "vendor",
];

/// Load existing graph data from .dual-graph/ for incremental scanning.
/// Returns (file_nodes_by_path, symbol_nodes_by_id, edges_by_from_file, symbol_index).
fn load_existing_for_incremental(
    project_root: &str,
) -> (
    HashMap<String, GraphNode>,
    HashMap<String, GraphNode>,
    HashMap<String, Vec<GraphEdge>>,
    SymbolIndex,
) {
    let dg_dir = Path::new(project_root).join(".dual-graph");

    let graph: Option<InfoGraph> = std::fs::read_to_string(dg_dir.join("info_graph.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let (file_nodes, symbol_nodes, edges_by_from) = match graph {
        Some(g) => {
            let file_nodes: HashMap<String, GraphNode> = g
                .nodes
                .iter()
                .filter(|n| n.kind == "file" && n.file_hash.is_some())
                .map(|n| (n.path.clone(), n.clone()))
                .collect();
            let symbol_nodes: HashMap<String, GraphNode> = g
                .nodes
                .into_iter()
                .filter(|n| n.kind == "symbol")
                .map(|n| (n.id.clone(), n))
                .collect();
            let mut edges_by_from: HashMap<String, Vec<GraphEdge>> = HashMap::new();
            for edge in g.edges {
                edges_by_from.entry(edge.from.clone()).or_default().push(edge);
            }
            (file_nodes, symbol_nodes, edges_by_from)
        }
        None => (HashMap::new(), HashMap::new(), HashMap::new()),
    };

    let sym_index: SymbolIndex =
        std::fs::read_to_string(dg_dir.join("symbol_index.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

    (file_nodes, symbol_nodes, edges_by_from, sym_index)
}

/// Scan a project directory and build an `InfoGraph` + `SymbolIndex`.
/// Uses LeanCTX's existing tree-sitter + regex signature extraction.
pub fn scan(project_root: &str) -> (InfoGraph, SymbolIndex) {
    let root = Path::new(project_root);
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut symbol_index: SymbolIndex = HashMap::new();
    let mut file_count: usize = 0;
    let mut symbol_count: usize = 0;

    // Load existing data for incremental mode (skip re-parsing unchanged files)
    let (existing_file_nodes, existing_symbol_nodes, existing_edges_by_from, existing_sym_index) =
        load_existing_for_incremental(project_root);

    // Walk the directory tree
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            // Skip hidden dirs and known non-source dirs
            if e.file_type().is_dir() {
                return !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_ref());
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if !SOURCE_EXTS.contains(&ext) {
            continue;
        }

        // Skip files that are too large
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size > MAX_SCAN_SIZE {
            continue;
        }

        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary/unreadable files
        };

        // Compute hash first for incremental check
        let hash = compute_hash(&content);

        // Incremental: if file unchanged, reuse previous scan results
        if let Some(existing_node) = existing_file_nodes.get(&rel_path) {
            if existing_node.file_hash.as_deref() == Some(hash.as_str()) {
                nodes.push(existing_node.clone());
                file_count += 1;
                for sym_node in existing_symbol_nodes.values().filter(|n| n.path == rel_path) {
                    nodes.push(sym_node.clone());
                    symbol_count += 1;
                }
                for (id, entry) in &existing_sym_index {
                    if entry.path == rel_path {
                        symbol_index.insert(id.clone(), entry.clone());
                    }
                }
                if let Some(file_edges) = existing_edges_by_from.get(&rel_path) {
                    edges.extend_from_slice(file_edges);
                }
                continue;
            }
        }

        // Build file node (new or changed file)
        let keywords = extract_keywords(&rel_path, &content);
        let summary = extract_summary(&content);

        let file_node = GraphNode {
            id: rel_path.clone(),
            kind: "file".to_string(),
            path: rel_path.clone(),
            ext: Some(format!(".{ext}")),
            size: Some(size as usize),
            keywords: keywords.clone(),
            content: if size <= MAX_CONTENT_SIZE {
                Some(content.clone())
            } else {
                None
            },
            summary: Some(summary),
            file_hash: Some(hash),
            ..Default::default()
        };
        nodes.push(file_node);
        file_count += 1;

        // Extract symbols (functions, classes, types) using tree-sitter / regex
        let sigs = signatures::extract_signatures(&content, ext);
        let lines: Vec<&str> = content.lines().collect();

        for sig in &sigs {
            let symbol_id = format!("{}::{}", rel_path, sig.name);

            // Find the line range for this symbol
            let (line_start, line_end) = find_symbol_lines(&lines, &sig.name);

            let symbol_node = GraphNode {
                id: symbol_id.clone(),
                kind: "symbol".to_string(),
                path: rel_path.clone(),
                symbol_type: Some(sig.kind.to_string()),
                name: Some(sig.name.clone()),
                line_start: Some(line_start),
                line_end: Some(line_end),
                body_hash: Some(compute_hash(
                    &lines[line_start.saturating_sub(1)..line_end.min(lines.len())]
                        .join("\n"),
                )),
                confidence: Some("high".to_string()),
                exported: Some(sig.is_exported),
                keywords: vec![sig.name.clone()],
                ..Default::default()
            };
            nodes.push(symbol_node);
            symbol_count += 1;

            // Add to symbol index
            symbol_index.insert(
                symbol_id,
                SymbolEntry {
                    line_start,
                    line_end,
                    body_hash: "".to_string(),
                    confidence: "high".to_string(),
                    path: rel_path.clone(),
                },
            );
        }

        // Extract deps and build edges
        let dep_info = deps::extract_deps(&content, ext);

        for import in &dep_info.imports {
            edges.push(GraphEdge {
                from: rel_path.clone(),
                to: resolve_import(import, &rel_path),
                rel: "imports".to_string(),
            });
        }

        for export in &dep_info.exports {
            edges.push(GraphEdge {
                from: rel_path.clone(),
                to: format!("{}::{}", rel_path, export),
                rel: "exports".to_string(),
            });
        }
    }

    let info_graph = InfoGraph {
        root: project_root.to_string(),
        node_count: nodes.len(),
        edge_count: edges.len(),
        file_count,
        symbol_count,
        nodes,
        edges,
    };

    (info_graph, symbol_index)
}

/// Extract keywords from filename and content.
/// Keywords are used for retrieval matching.
fn extract_keywords(path: &str, content: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    // Add path components as keywords
    for part in path.split('/') {
        let stem = part
            .rsplit_once('.')
            .map_or(part, |(name, _)| name);
        if stem.len() >= 3 {
            keywords.push(stem.to_lowercase());
        }
    }

    // Extract significant words from content (first ~2000 bytes, snapped to a char boundary)
    let end = content.len().min(2000);
    let end = content.floor_char_boundary(end);
    let sample = &content[..end];
    for word in sample.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let w = word.to_lowercase();
        if w.len() >= 4 && !is_stop_word(&w) && !keywords.contains(&w) {
            keywords.push(w);
            if keywords.len() >= 20 {
                break;
            }
        }
    }

    keywords
}

/// Extract a summary from the first meaningful line of the file.
fn extract_summary(content: &str) -> String {
    for line in content.lines().take(10) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip shebangs, comment markers alone
        if trimmed == "#!" || trimmed == "//" || trimmed == "/*" || trimmed == "*/" {
            continue;
        }
        // Clean up comment prefixes
        let clean = trimmed
            .trim_start_matches("// ")
            .trim_start_matches("/// ")
            .trim_start_matches("# ")
            .trim_start_matches("/* ")
            .trim_start_matches("* ")
            .trim_start_matches("//! ");
        if !clean.is_empty() {
            // Truncate to ~80 chars
            return if clean.len() > 80 {
                format!("{}...", &clean[..clean.floor_char_boundary(77)])
            } else {
                clean.to_string()
            };
        }
    }
    String::new()
}

/// Compute a short hash for change detection.
fn compute_hash(content: &str) -> String {
    use md5::{Digest, Md5};
    let hash = Md5::digest(content.as_bytes());
    format!("{:x}", hash)[..8].to_string()
}

/// Find the line range (1-indexed) for a symbol by name.
/// Scans for the definition line and extends to the next definition or end of file.
fn find_symbol_lines(lines: &[&str], name: &str) -> (usize, usize) {
    let mut start = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.contains(name)
            && (line.contains("fn ") || line.contains("def ") || line.contains("class ")
                || line.contains("function ") || line.contains("struct ")
                || line.contains("trait ") || line.contains("enum ")
                || line.contains("interface ") || line.contains("type ")
                || line.contains("impl ") || line.contains("const "))
        {
            start = i + 1; // 1-indexed
            break;
        }
    }

    if start == 0 {
        return (1, lines.len().min(10));
    }

    // Find the end: next definition at same or lower indent, or end of file
    let start_indent = lines[start - 1]
        .len()
        .saturating_sub(lines[start - 1].trim_start().len());

    let mut end = start;
    for i in start..lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            end = i + 1;
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        // If we hit a new definition at same/lower indent, stop
        if indent <= start_indent
            && i > start
            && (trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("async def ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("pub struct ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("pub trait ")
                || trimmed.starts_with("impl "))
        {
            break;
        }
        end = i + 1;
    }

    (start, end)
}

/// Resolve an import path to a file path in the project.
fn resolve_import(import: &str, from_file: &str) -> String {
    // For relative imports, resolve relative to the importing file
    if import.starts_with('.') {
        if let Some(dir) = Path::new(from_file).parent() {
            let clean_import = import.trim_start_matches("./");
            let resolved = dir.join(clean_import);
            return resolved
                .to_string_lossy()
                .replace('\\', "/")
                .to_string();
        }
    }
    import.to_string()
}

/// Check if a word is a common stop word (not useful as a keyword).
fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "this" | "that" | "with" | "from" | "have" | "been" | "will" | "would" | "could"
            | "should" | "their" | "there" | "then" | "than" | "when" | "what" | "which"
            | "where" | "while" | "true" | "false" | "none" | "null" | "undefined"
            | "return" | "const" | "string" | "number" | "boolean" | "function"
            | "import" | "export" | "default" | "async" | "await" | "self" | "super"
            | "some" | "option" | "result" | "error" | "impl" | "struct" | "enum"
            | "trait" | "type" | "void" | "class" | "interface" | "public" | "private"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_keywords_from_path_and_content() {
        let keywords = extract_keywords(
            "src/auth/handler.rs",
            "use crate::db;\n\npub fn authenticate(token: &str) -> bool {\n    true\n}",
        );
        assert!(keywords.contains(&"auth".to_string()));
        assert!(keywords.contains(&"handler".to_string()));
        assert!(keywords.contains(&"authenticate".to_string()));
    }

    #[test]
    fn extract_summary_from_comment() {
        assert_eq!(
            extract_summary("// This is a module for auth handling\nfn main() {}"),
            "This is a module for auth handling"
        );
    }

    #[test]
    fn extract_summary_from_code() {
        assert_eq!(
            extract_summary("pub fn handle_request(req: Request) -> Response {"),
            "pub fn handle_request(req: Request) -> Response {"
        );
    }

    #[test]
    fn find_symbol_lines_basic() {
        let content = "use std::io;\n\npub fn foo() {\n    println!(\"hello\");\n}\n\npub fn bar() {\n    println!(\"world\");\n}\n";
        let lines: Vec<&str> = content.lines().collect();
        let (start, end) = find_symbol_lines(&lines, "foo");
        assert_eq!(start, 3); // "pub fn foo()" is line 3
        assert!(end >= 5);    // Should include the closing brace
        assert!(end <= 6);    // Should not include bar
    }

    #[test]
    fn scan_creates_valid_graph() {
        // Create a temp directory with a few source files
        let tmp = std::env::temp_dir().join("lean_ctx_scanner_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::fs::write(
            tmp.join("main.rs"),
            "use crate::helper;\n\npub fn main() {\n    helper::greet();\n}\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join("helper.rs"),
            "pub fn greet() {\n    println!(\"hello\");\n}\n\npub fn farewell() {\n    println!(\"bye\");\n}\n",
        )
        .unwrap();

        let (graph, index) = scan(&tmp.to_string_lossy());

        // Should have file nodes
        assert!(graph.file_count >= 2, "should have at least 2 files, got {}", graph.file_count);
        assert_eq!(graph.node_count, graph.nodes.len());
        assert_eq!(graph.edge_count, graph.edges.len());

        // Should have symbol nodes
        assert!(graph.symbol_count > 0, "should have extracted symbols");

        // Symbol index should have entries
        assert!(!index.is_empty(), "symbol index should not be empty");

        // Should have import edges
        let import_edges: Vec<_> = graph.edges.iter().filter(|e| e.rel == "imports").collect();
        assert!(!import_edges.is_empty(), "should have import edges");

        // File nodes should have keywords
        let file_nodes: Vec<_> = graph.nodes.iter().filter(|n| n.kind == "file").collect();
        for node in &file_nodes {
            assert!(!node.keywords.is_empty(), "file {} should have keywords", node.path);
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_skips_node_modules() {
        let tmp = std::env::temp_dir().join("lean_ctx_scanner_skip_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(tmp.join("src")).unwrap();

        std::fs::write(tmp.join("src/app.ts"), "export function app() {}").unwrap();
        std::fs::write(tmp.join("node_modules/pkg/index.js"), "module.exports = {}").unwrap();

        let (graph, _) = scan(&tmp.to_string_lossy());

        let paths: Vec<&str> = graph.nodes.iter().map(|n| n.path.as_str()).collect();
        assert!(
            !paths.iter().any(|p| p.contains("node_modules")),
            "should skip node_modules"
        );
        assert!(
            paths.iter().any(|p| p.contains("app.ts")),
            "should include src files"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_relative_import() {
        assert_eq!(
            resolve_import("./helper", "src/auth/handler.rs"),
            "src/auth/helper"
        );
    }

    #[test]
    fn resolve_absolute_import() {
        assert_eq!(
            resolve_import("lodash", "src/app.ts"),
            "lodash"
        );
    }

    #[test]
    fn scan_incremental_reuses_unchanged_files() {
        let tmp = std::env::temp_dir().join("lean_ctx_incremental_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        std::fs::write(
            tmp.join("lib.rs"),
            "pub fn greet() -> &'static str { \"hello\" }\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join("main.rs"),
            "fn main() { println!(\"world\"); }\n",
        )
        .unwrap();

        // First scan
        let (graph1, index1) = scan(&tmp.to_string_lossy());
        assert_eq!(graph1.file_count, 2);

        // Save to .dual-graph/
        let dg = tmp.join(".dual-graph");
        std::fs::create_dir_all(&dg).unwrap();
        let json = serde_json::to_string(&graph1).unwrap();
        std::fs::write(dg.join("info_graph.json"), &json).unwrap();
        let idx_json = serde_json::to_string(&index1).unwrap();
        std::fs::write(dg.join("symbol_index.json"), &idx_json).unwrap();

        // Second scan — lib.rs is unchanged, main.rs is changed
        std::fs::write(tmp.join("main.rs"), "fn main() { println!(\"updated\"); }\n").unwrap();
        let (graph2, _) = scan(&tmp.to_string_lossy());

        assert_eq!(graph2.file_count, 2);

        // lib.rs node should have identical hash across both scans
        let lib1 = graph1.nodes.iter().find(|n| n.kind == "file" && n.path.contains("lib.rs")).unwrap();
        let lib2 = graph2.nodes.iter().find(|n| n.kind == "file" && n.path.contains("lib.rs")).unwrap();
        assert_eq!(lib1.file_hash, lib2.file_hash, "unchanged file should have same hash");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
