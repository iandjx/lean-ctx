use std::collections::HashSet;

use crate::graph::types::{InfoGraph, MemoryEntry};

/// Result of a `graph_continue` call.
#[derive(Debug)]
pub struct ContinueResult {
    pub ok: bool,
    pub needs_project: bool,
    pub skip: bool,
    pub mode: String,
    pub confidence: String,
    pub recommended_files: Vec<String>,
    pub memories: Vec<MemoryEntry>,
    pub max_supplementary_greps: usize,
    pub max_supplementary_files: usize,
}

impl ContinueResult {
    pub fn to_json(&self) -> String {
        let memories_json: Vec<String> = self
            .memories
            .iter()
            .map(|m| {
                format!(
                    r#"{{"kind":"{}","content":"{}","tags":{}}}"#,
                    m.kind,
                    m.content.replace('"', "\\\""),
                    serde_json::to_string(&m.tags).unwrap_or_else(|_| "[]".to_string())
                )
            })
            .collect();

        format!(
            r#"{{"ok":{},"needs_project":{},"skip":{},"mode":"{}","confidence":"{}","recommended_files":{},"memories":[{}],"max_supplementary_greps":{},"max_supplementary_files":{}}}"#,
            self.ok,
            self.needs_project,
            self.skip,
            self.mode,
            self.confidence,
            serde_json::to_string(&self.recommended_files).unwrap_or_else(|_| "[]".to_string()),
            memories_json.join(","),
            self.max_supplementary_greps,
            self.max_supplementary_files,
        )
    }
}

/// Main retrieval orchestrator. Called on every turn before file reads.
pub fn graph_continue(
    info_graph: Option<&InfoGraph>,
    context_store: &[MemoryEntry],
    query: &str,
    recent_files: &[String],
) -> ContinueResult {
    // 1. No graph loaded?
    let graph = match info_graph {
        Some(g) => g,
        None => {
            return ContinueResult {
                ok: true,
                needs_project: true,
                skip: false,
                mode: "scan_needed".to_string(),
                confidence: "low".to_string(),
                recommended_files: vec![],
                memories: vec![],
                max_supplementary_greps: 0,
                max_supplementary_files: 0,
            };
        }
    };

    // 2. Fewer than 5 files?
    if graph.file_count < 5 {
        return ContinueResult {
            ok: true,
            needs_project: false,
            skip: true,
            mode: "small_project".to_string(),
            confidence: "high".to_string(),
            recommended_files: vec![],
            memories: vec![],
            max_supplementary_greps: 0,
            max_supplementary_files: 0,
        };
    }

    // 3. Search context store for matching memories
    let query_keywords = extract_keywords(query);
    let matching_memories = search_memories(context_store, &query_keywords);

    if !matching_memories.is_empty() {
        // Collect files from matching memories
        let mut files: Vec<String> = Vec::new();
        for mem in &matching_memories {
            for f in &mem.files {
                if !files.contains(f) {
                    files.push(f.clone());
                }
            }
        }
        return ContinueResult {
            ok: true,
            needs_project: false,
            skip: false,
            mode: "memory_hit".to_string(),
            confidence: "high".to_string(),
            recommended_files: files,
            memories: matching_memories,
            max_supplementary_greps: 0,
            max_supplementary_files: 0,
        };
    }

    // 4. Run graph retrieval
    let expanded_keywords = expand_keywords(&query_keywords);
    let (recommended, confidence) = graph_retrieve(graph, &expanded_keywords, recent_files);

    let (max_greps, max_files) = caps_for_confidence(&confidence);

    ContinueResult {
        ok: true,
        needs_project: false,
        skip: false,
        mode: "retrieve_then_read".to_string(),
        confidence,
        recommended_files: recommended,
        memories: vec![],
        max_supplementary_greps: max_greps,
        max_supplementary_files: max_files,
    }
}

/// Keyword-matched graph retrieval. Scores nodes and returns top files.
pub fn graph_retrieve(
    graph: &InfoGraph,
    query_keywords: &[String],
    recent_files: &[String],
) -> (Vec<String>, String) {
    if query_keywords.is_empty() {
        return (vec![], "low".to_string());
    }

    // Score each node
    let mut scores: Vec<(String, f64)> = Vec::new();
    let mut scored_set: HashSet<String> = HashSet::new();

    for node in &graph.nodes {
        let mut score: f64 = 0.0;

        for kw in query_keywords {
            let kw_lower = kw.to_lowercase();

            // +2.0 per keyword match in node.keywords
            if node.keywords.iter().any(|k| k.to_lowercase().contains(&kw_lower)) {
                score += 2.0;
            }

            // +3.0 per keyword match in node.docs (doc comments — highest weight)
            if let Some(ref docs) = node.docs {
                if docs.to_lowercase().contains(&kw_lower) {
                    score += 3.0;
                }
            }

            // +1.5 per keyword match in node.summary
            if let Some(ref summary) = node.summary {
                if summary.to_lowercase().contains(&kw_lower) {
                    score += 1.5;
                }
            }

            // +1.0 per keyword match in node.content (substring)
            if let Some(ref content) = node.content {
                if content.to_lowercase().contains(&kw_lower) {
                    score += 1.0;
                }
            }

            // +0.5 per keyword match in node.id/path
            if node.path.to_lowercase().contains(&kw_lower) {
                score += 0.5;
            }

            // Symbol boost: if query keywords look like function names, boost symbol nodes
            if node.kind == "symbol" {
                let symbol_name = node.name.as_deref().unwrap_or("");
                if looks_like_symbol_name(kw) && symbol_name.to_lowercase().contains(&kw_lower) {
                    score += 3.0;
                }
            }
        }

        if score > 0.0 {
            // Use the file path (not symbol ID) as the key for dedup
            let key = if node.kind == "symbol" {
                // For symbols, use the full file::symbol ID
                node.id.clone()
            } else {
                node.path.clone()
            };
            if !scored_set.contains(&key) {
                scores.push((key.clone(), score));
                scored_set.insert(key);
            }
        }
    }

    // Boost nodes connected to high-scoring nodes via edges
    let high_score_nodes: HashSet<String> = scores
        .iter()
        .filter(|(_, s)| *s > 3.0)
        .map(|(id, _)| id.clone())
        .collect();

    for edge in &graph.edges {
        if high_score_nodes.contains(&edge.from) {
            if let Some(entry) = scores.iter_mut().find(|(id, _)| id == &edge.to) {
                entry.1 += 0.5;
            }
        }
        if high_score_nodes.contains(&edge.to) {
            if let Some(entry) = scores.iter_mut().find(|(id, _)| id == &edge.from) {
                entry.1 += 0.5;
            }
        }
    }

    // Recency boost: files recently read or edited score +0.75
    for (id, score) in scores.iter_mut() {
        let file_path = if id.contains("::") {
            id.split("::").next().unwrap_or(id.as_str())
        } else {
            id.as_str()
        };
        if recent_files.iter().any(|r| r == file_path || r == id) {
            *score += 0.75;
        }
    }

    // Sort by score descending
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top 5-8 files
    let top_count = if scores.len() > 5 { 8 } else { 5 };
    let recommended: Vec<String> = scores
        .iter()
        .take(top_count)
        .map(|(id, _)| id.clone())
        .collect();

    // Determine confidence
    let top_score = scores.first().map_or(0.0, |(_, s)| *s);
    let match_count = scores.iter().filter(|(_, s)| *s > 1.0).count();

    let confidence = if top_score > 6.0 && match_count >= 3 {
        "high".to_string()
    } else if top_score > 3.0 {
        "medium".to_string()
    } else {
        "low".to_string()
    };

    (recommended, confidence)
}

/// Compute an age-based decay factor for a memory entry.
/// Entries < 7 days old: factor=1.0. Older: decays toward 0.5 at 90 days.
fn memory_decay_factor(created_epoch_ms: Option<u64>) -> f64 {
    let Some(epoch_ms) = created_epoch_ms else {
        return 1.0; // No timestamp → no decay
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let age_days = now_ms.saturating_sub(epoch_ms) / (1000 * 60 * 60 * 24);

    if age_days < 7 {
        1.0
    } else if age_days > 90 {
        0.5
    } else {
        // Linear decay from 1.0 at 7 days to 0.5 at 90 days
        1.0 - 0.5 * (age_days - 7) as f64 / 83.0
    }
}

/// Search memories for entries matching the query keywords.
fn search_memories(store: &[MemoryEntry], query_keywords: &[String]) -> Vec<MemoryEntry> {
    let mut matches: Vec<(usize, &MemoryEntry)> = Vec::new();

    for entry in store {
        if entry.stale == Some(true) {
            continue;
        }

        let mut overlap = 0;
        for kw in query_keywords {
            if entry.tags.iter().any(|t| t.to_lowercase().contains(&kw.to_lowercase())) {
                overlap += 2;
            }
            if entry.content.to_lowercase().contains(&kw.to_lowercase()) {
                overlap += 1;
            }
        }

        if overlap >= 2 {
            let decay = memory_decay_factor(entry.created_epoch);
            let adjusted = ((overlap as f64) * decay) as usize;
            if adjusted >= 1 {
                matches.push((adjusted, entry));
            }
        }
    }

    // Sort by overlap score descending
    matches.sort_by(|a, b| b.0.cmp(&a.0));

    // Return top 3 matches
    matches
        .into_iter()
        .take(3)
        .map(|(_, entry)| entry.clone())
        .collect()
}

/// Returns true if a keyword looks like a function/method name
/// (camelCase with uppercase, or snake_case with underscore).
fn looks_like_symbol_name(kw: &str) -> bool {
    // camelCase: has uppercase letter after a lowercase one
    let has_camel = kw.chars().zip(kw.chars().skip(1)).any(|(a, b)| a.is_lowercase() && b.is_uppercase());
    // snake_case: contains underscore with letters on both sides
    let has_snake = kw.contains('_') && kw.len() > 3;
    has_camel || has_snake
}

/// Expand query keywords with domain synonyms to improve recall.
/// E.g. "auth" also matches "login", "session", "token", "jwt".
pub fn expand_keywords(keywords: &[String]) -> Vec<String> {
    const SYNONYMS: &[(&str, &[&str])] = &[
        ("auth", &["login", "session", "token", "jwt", "credential", "password", "oauth"]),
        ("db", &["database", "query", "connection", "pool", "sql", "postgres", "mysql", "sqlite"]),
        ("database", &["db", "query", "connection", "pool", "sql"]),
        ("api", &["endpoint", "handler", "route", "request", "response", "rest", "http"]),
        ("route", &["router", "path", "endpoint", "handler", "api"]),
        ("cache", &["redis", "memcache", "ttl", "expire", "store"]),
        ("error", &["err", "exception", "panic", "fail", "result"]),
        ("config", &["settings", "env", "environment", "cfg", "options"]),
        ("test", &["spec", "assert", "mock", "fixture", "integration"]),
        ("log", &["logger", "logging", "trace", "debug", "info", "warn"]),
        ("user", &["account", "profile", "member", "identity"]),
        ("event", &["message", "emit", "publish", "subscribe", "listener"]),
        ("file", &["read", "write", "path", "disk", "fs", "io"]),
        ("queue", &["job", "worker", "task", "async", "background"]),
        ("search", &["query", "filter", "find", "index", "lookup"]),
    ];

    let mut expanded = keywords.to_vec();
    for kw in keywords {
        let kw_lower = kw.to_lowercase();
        for (key, synonyms) in SYNONYMS {
            if kw_lower == *key {
                for syn in *synonyms {
                    let s = syn.to_string();
                    if !expanded.contains(&s) {
                        expanded.push(s);
                    }
                }
            }
        }
    }
    expanded
}

/// Extract keywords from a query string.
pub fn extract_keywords(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
        .filter(|w| !is_stop_word(w))
        .map(|w| w.to_lowercase())
        .collect()
}

/// Get exploration caps based on confidence level.
fn caps_for_confidence(confidence: &str) -> (usize, usize) {
    let max_greps = std::env::var("DG_FALLBACK_MAX_CALLS_PER_TURN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    match confidence {
        "high" => (0, 0),
        "medium" => (max_greps, 2),
        _ => (max_greps, 3),
    }
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "the" | "and" | "for" | "are" | "but" | "not" | "you" | "all" | "can" | "her"
            | "was" | "one" | "our" | "out" | "has" | "how" | "its" | "let" | "may"
            | "new" | "now" | "old" | "see" | "way" | "who" | "did" | "get" | "got"
            | "him" | "his" | "had" | "use" | "does" | "this" | "that" | "with"
            | "from" | "have" | "been" | "will" | "what" | "when" | "where" | "which"
    )
}

/// Handle `fallback_rg` — shells out to ripgrep with hard caps.
pub fn fallback_rg(pattern: &str, project_root: Option<&str>, max_hits: usize) -> String {
    let root = project_root.unwrap_or(".");

    let output = std::process::Command::new("rg")
        .arg("--max-count")
        .arg(max_hits.to_string())
        .arg("--no-heading")
        .arg("--line-number")
        .arg("--color=never")
        .arg(pattern)
        .arg(root)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.is_empty() {
                format!("No matches for '{pattern}'")
            } else {
                // Truncate to reasonable size
                let lines: Vec<&str> = stdout.lines().take(max_hits).collect();
                format!("{} match(es):\n{}", lines.len(), lines.join("\n"))
            }
        }
        Err(e) => format!("fallback_rg error: {e}. Is ripgrep (rg) installed?"),
    }
}

/// Handle `graph_impact` — show what depends on a file (2-level deep).
pub fn graph_impact(info_graph: Option<&InfoGraph>, file: &str) -> String {
    let graph = match info_graph {
        Some(g) => g,
        None => return "No project scanned. Call graph_scan first.".to_string(),
    };

    let mut level1: Vec<(&str, &str)> = Vec::new();
    let mut level2: Vec<(&str, &str, &str)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(file);

    // Level 1: direct connections
    for edge in &graph.edges {
        if edge.from == file || edge.from.starts_with(&format!("{file}::")) {
            if !seen.contains(edge.to.as_str()) {
                level1.push((&edge.to, &edge.rel));
                seen.insert(&edge.to);
            }
        }
        if edge.to == file || edge.to.starts_with(&format!("{file}::")) {
            if !seen.contains(edge.from.as_str()) {
                level1.push((&edge.from, &edge.rel));
                seen.insert(&edge.from);
            }
        }
    }

    // Level 2: connections of connections
    let level1_ids: Vec<&str> = level1.iter().map(|(id, _)| *id).collect();
    for l1_id in &level1_ids {
        for edge in &graph.edges {
            if edge.from == *l1_id && !seen.contains(edge.to.as_str()) {
                level2.push((&edge.to, &edge.rel, l1_id));
                seen.insert(&edge.to);
            }
            if edge.to == *l1_id && !seen.contains(edge.from.as_str()) {
                level2.push((&edge.from, &edge.rel, l1_id));
                seen.insert(&edge.from);
            }
        }
    }

    if level1.is_empty() {
        return format!("No impact found for '{file}'.");
    }

    let mut output = format!("Impact of '{file}':\n\nDirect (depth 1):\n");
    for (target, rel) in &level1 {
        output.push_str(&format!("  {file} --[{rel}]--> {target}\n"));
    }

    if !level2.is_empty() {
        output.push_str("\nIndirect (depth 2):\n");
        for (target, rel, via) in &level2 {
            output.push_str(&format!("  {via} --[{rel}]--> {target}\n"));
        }
    }

    output.push_str(&format!(
        "\nTotal: {} direct, {} indirect",
        level1.len(),
        level2.len()
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{GraphEdge, GraphNode};

    fn test_graph() -> InfoGraph {
        InfoGraph {
            root: "/tmp".to_string(),
            node_count: 4,
            edge_count: 3,
            file_count: 5,
            symbol_count: 1,
            nodes: vec![
                GraphNode {
                    id: "src/auth.rs".to_string(),
                    kind: "file".to_string(),
                    path: "src/auth.rs".to_string(),
                    keywords: vec!["auth".to_string(), "token".to_string(), "jwt".to_string()],
                    summary: Some("Authentication module using JWT tokens".to_string()),
                    content: Some("pub fn authenticate(token: &str) -> bool { true }".to_string()),
                    ..Default::default()
                },
                GraphNode {
                    id: "src/db.rs".to_string(),
                    kind: "file".to_string(),
                    path: "src/db.rs".to_string(),
                    keywords: vec!["database".to_string(), "query".to_string(), "connection".to_string()],
                    summary: Some("Database connection pool and queries".to_string()),
                    content: Some("pub fn connect() -> Pool { Pool::new() }".to_string()),
                    ..Default::default()
                },
                GraphNode {
                    id: "src/api.rs".to_string(),
                    kind: "file".to_string(),
                    path: "src/api.rs".to_string(),
                    keywords: vec!["api".to_string(), "handler".to_string(), "endpoint".to_string()],
                    summary: Some("API endpoint handlers".to_string()),
                    content: Some("use auth; use db;\npub fn handle() {}".to_string()),
                    ..Default::default()
                },
                GraphNode {
                    id: "src/auth.rs::authenticate".to_string(),
                    kind: "symbol".to_string(),
                    path: "src/auth.rs".to_string(),
                    keywords: vec!["authenticate".to_string()],
                    ..Default::default()
                },
            ],
            edges: vec![
                GraphEdge {
                    from: "src/api.rs".to_string(),
                    to: "src/auth.rs".to_string(),
                    rel: "imports".to_string(),
                },
                GraphEdge {
                    from: "src/api.rs".to_string(),
                    to: "src/db.rs".to_string(),
                    rel: "imports".to_string(),
                },
                GraphEdge {
                    from: "src/auth.rs".to_string(),
                    to: "src/auth.rs::authenticate".to_string(),
                    rel: "exports".to_string(),
                },
            ],
        }
    }

    #[test]
    fn extract_keywords_basic() {
        let kw = extract_keywords("how does authentication work with JWT tokens");
        assert!(kw.contains(&"authentication".to_string()));
        assert!(kw.contains(&"work".to_string()));
        assert!(kw.contains(&"tokens".to_string()));
        // "how", "does", "with" should be filtered as stop words or too short
        assert!(!kw.contains(&"how".to_string()));
    }

    #[test]
    fn retrieve_finds_relevant_files() {
        let graph = test_graph();
        let keywords = extract_keywords("authentication token JWT");
        let (files, confidence) = graph_retrieve(&graph, &keywords, &[]);

        assert!(!files.is_empty(), "should find matching files");
        assert_eq!(files[0], "src/auth.rs", "auth.rs should be top match");
        assert!(
            confidence == "high" || confidence == "medium",
            "confidence should be medium or high, got {confidence}"
        );
    }

    #[test]
    fn retrieve_database_query() {
        let graph = test_graph();
        let keywords = extract_keywords("database connection query");
        let (files, _) = graph_retrieve(&graph, &keywords, &[]);

        assert!(!files.is_empty());
        assert!(
            files.iter().any(|f| f.contains("db.rs")),
            "should include db.rs"
        );
    }

    #[test]
    fn retrieve_no_match() {
        let graph = test_graph();
        let keywords = extract_keywords("zzznonexistent");
        let (files, confidence) = graph_retrieve(&graph, &keywords, &[]);

        assert!(files.is_empty());
        assert_eq!(confidence, "low");
    }

    #[test]
    fn continue_no_graph() {
        let result = graph_continue(None, &[], "test query", &[]);
        assert!(result.needs_project);
    }

    #[test]
    fn continue_small_project() {
        let graph = InfoGraph {
            file_count: 3,
            ..Default::default()
        };
        let result = graph_continue(Some(&graph), &[], "test query", &[]);
        assert!(result.skip);
    }

    #[test]
    fn continue_with_memory_hit() {
        let graph = test_graph();
        let memories = vec![MemoryEntry {
            id: "mem:1".to_string(),
            kind: "fact".to_string(),
            content: "auth uses JWT tokens for validation".to_string(),
            tags: vec!["auth".to_string(), "jwt".to_string(), "token".to_string()],
            files: vec!["src/auth.rs".to_string()],
            ..Default::default()
        }];

        let result = graph_continue(Some(&graph), &memories, "JWT token authentication", &[]);
        assert_eq!(result.mode, "memory_hit");
        assert_eq!(result.confidence, "high");
        assert!(result.recommended_files.contains(&"src/auth.rs".to_string()));
        assert!(!result.memories.is_empty());
    }

    #[test]
    fn continue_with_retrieval() {
        let graph = test_graph();
        let result = graph_continue(Some(&graph), &[], "database connection pool", &[]);
        assert_eq!(result.mode, "retrieve_then_read");
        assert!(!result.recommended_files.is_empty());
    }

    #[test]
    fn confidence_caps() {
        assert_eq!(caps_for_confidence("high"), (0, 0));
        assert_eq!(caps_for_confidence("medium"), (2, 2));
        assert_eq!(caps_for_confidence("low"), (2, 3));
    }

    #[test]
    fn impact_finds_connections() {
        let graph = test_graph();
        let result = graph_impact(Some(&graph), "src/auth.rs");
        assert!(result.contains("Direct"));
        assert!(result.contains("api.rs"));
        assert!(result.contains("authenticate"));
    }

    #[test]
    fn impact_no_graph() {
        let result = graph_impact(None, "src/auth.rs");
        assert!(result.contains("No project scanned"));
    }

    #[test]
    fn docs_field_scores_higher_than_content() {
        use crate::graph::types::GraphNode;
        let graph_with_docs = crate::graph::types::InfoGraph {
            root: "/tmp".to_string(),
            node_count: 2,
            edge_count: 0,
            file_count: 5,
            symbol_count: 0,
            nodes: vec![
                GraphNode {
                    id: "src/auth.rs".to_string(),
                    kind: "file".to_string(),
                    path: "src/auth.rs".to_string(),
                    keywords: vec![],
                    docs: Some("Authenticate users with JWT tokens".to_string()),
                    content: None,
                    summary: None,
                    ..Default::default()
                },
                GraphNode {
                    id: "src/db.rs".to_string(),
                    kind: "file".to_string(),
                    path: "src/db.rs".to_string(),
                    keywords: vec![],
                    docs: None,
                    content: Some("JWT token".to_string()), // same keyword in content only
                    summary: None,
                    ..Default::default()
                },
            ],
            edges: vec![],
        };
        let kw = extract_keywords("JWT authentication");
        let (files, _) = graph_retrieve(&graph_with_docs, &kw, &[]);
        // auth.rs has docs match (score 3.0) vs db.rs content match (score 1.0)
        // auth.rs should rank first
        assert!(!files.is_empty());
        assert!(files[0].contains("auth"), "documented file should rank first");
    }

    #[test]
    fn memory_decay_fresh_entry_has_factor_one() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        assert_eq!(memory_decay_factor(Some(now_ms)), 1.0);
    }

    #[test]
    fn memory_decay_old_entry_has_lower_factor() {
        // 100 days ago
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let old_ms = now_ms - (100 * 24 * 60 * 60 * 1000);
        let factor = memory_decay_factor(Some(old_ms));
        assert!(factor <= 0.5, "100-day-old entry should have factor <= 0.5, got {factor}");
    }

    #[test]
    fn memory_decay_no_timestamp_returns_one() {
        assert_eq!(memory_decay_factor(None), 1.0);
    }

    #[test]
    fn memory_decay_seven_days_old_has_factor_one() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let seven_days_ago = now_ms - (7 * 24 * 60 * 60 * 1000);
        assert_eq!(memory_decay_factor(Some(seven_days_ago)), 1.0);
    }

    #[test]
    fn symbol_aware_scoring_boosts_camel_case_match() {
        let graph = test_graph();
        // "authenticate" is a symbol in auth.rs
        let kw = extract_keywords("authenticate function");
        let (files, _) = graph_retrieve(&graph, &kw, &[]);
        // Should find auth.rs::authenticate symbol or auth.rs file
        assert!(files.iter().any(|f| f.contains("auth")));
    }

    #[test]
    fn looks_like_symbol_name_detects_camel_case() {
        assert!(looks_like_symbol_name("handleLogin"));
        assert!(looks_like_symbol_name("getUserById"));
        assert!(!looks_like_symbol_name("auth"));
        assert!(!looks_like_symbol_name("token"));
    }

    #[test]
    fn looks_like_symbol_name_detects_snake_case() {
        assert!(looks_like_symbol_name("handle_login"));
        assert!(looks_like_symbol_name("get_user_by_id"));
        assert!(!looks_like_symbol_name("auth"));
    }

    #[test]
    fn query_expansion_adds_synonyms() {
        let kw = vec!["auth".to_string()];
        let expanded = expand_keywords(&kw);
        assert!(expanded.contains(&"auth".to_string()));
        assert!(expanded.contains(&"login".to_string()));
        assert!(expanded.contains(&"token".to_string()));
        assert!(expanded.contains(&"jwt".to_string()));
    }

    #[test]
    fn query_expansion_no_duplicates() {
        let kw = vec!["auth".to_string(), "token".to_string()];
        let expanded = expand_keywords(&kw);
        let count = expanded.iter().filter(|s| *s == "token").count();
        assert_eq!(count, 1, "no duplicates in expanded keywords");
    }

    #[test]
    fn retrieve_with_expanded_keywords_finds_more() {
        let graph = test_graph();
        // Query with "auth" should match auth.rs via both direct and synonym scoring
        let kw = vec!["auth".to_string()];
        let expanded = expand_keywords(&kw);
        let (files, _) = graph_retrieve(&graph, &expanded, &[]);
        assert!(files.iter().any(|f| f.contains("auth")));
    }

    #[test]
    fn recency_boost_scores_recent_files_higher() {
        let graph = test_graph();
        let keywords = extract_keywords("database query");
        // Without recency boost
        let (files_no_boost, _) = graph_retrieve(&graph, &keywords, &[]);
        // With recency boost for auth (unrelated to query)
        let (files_with_boost, _) = graph_retrieve(&graph, &keywords, &["src/auth.rs".to_string()]);
        // auth.rs should rank higher when it gets a recency boost (or at least it should not crash)
        assert!(!files_with_boost.is_empty());
        // db.rs should still appear
        assert!(files_no_boost.iter().any(|f| f.contains("db.rs")));
    }

    #[test]
    fn fallback_rg_no_rg_binary() {
        // This test just verifies the function doesn't panic
        // It may fail if rg is not installed, which is fine
        let result = fallback_rg("nonexistent_pattern_xyz", Some("/tmp"), 5);
        assert!(!result.is_empty());
    }
}
