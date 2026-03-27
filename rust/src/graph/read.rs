use std::path::Path;

use crate::core::cache::SessionCache;
use crate::core::tokens::count_tokens;
use crate::graph::types::SymbolIndex;
use crate::tools::CrpMode;

/// Handle a `graph_read` call. Reads a file (or symbol within a file),
/// applies LeanCTX compression, caches the result, and returns compressed output.
///
/// The `file` parameter supports `file::symbol` notation:
/// - `"src/auth.ts"` → reads the full file
/// - `"src/auth.ts::handleLogin"` → reads only the lines for that symbol
pub fn handle(
    cache: &mut SessionCache,
    symbol_index: &SymbolIndex,
    file: &str,
    project_root: Option<&str>,
    crp_mode: CrpMode,
) -> String {
    let (file_path, symbol_name) = parse_file_symbol(file);

    // Resolve to absolute path if project_root is available
    let abs_path = resolve_path(&file_path, project_root);

    // Determine what line range to read
    let line_range = if let Some(sym) = &symbol_name {
        let key = format!("{}::{}", file_path, sym);
        symbol_index.get(&key).map(|e| (e.line_start, e.line_end))
    } else {
        None
    };

    if let Some((start, end)) = line_range {
        // Symbol-level read: read only the specific lines, then compress
        read_symbol_lines(cache, &abs_path, file, start, end, crp_mode)
    } else {
        // Full file read: delegate to ctx_read with auto-selected mode
        let mode = read_mode_from_env();
        crate::tools::ctx_read::handle(cache, &abs_path, &mode, crp_mode)
    }
}

/// Read specific lines from a file (symbol-level read).
/// Stores the full file in cache for future delta/cache hits,
/// but returns only the requested line range with compression.
fn read_symbol_lines(
    cache: &mut SessionCache,
    abs_path: &str,
    display_name: &str,
    start: usize,
    end: usize,
    crp_mode: CrpMode,
) -> String {
    let content = match std::fs::read_to_string(abs_path) {
        Ok(c) => c,
        Err(e) => return format!("ERROR reading {display_name}: {e}"),
    };

    // Cache the full file for future reads
    cache.store(abs_path, content.clone());

    // Extract the requested lines (1-indexed)
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start_idx = start.saturating_sub(1).min(total);
    let end_idx = end.min(total);

    if start_idx >= total {
        return format!("{display_name}: line range {start}-{end} out of bounds (file has {total} lines)");
    }

    let selected: Vec<String> = lines[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}| {}", start + i, line))
        .collect();
    let extracted = selected.join("\n");

    let original_tokens = count_tokens(&content);
    let sent_tokens = count_tokens(&extracted);

    // Apply TDD symbol shorthand if enabled
    let output = if crp_mode.is_tdd() {
        use crate::core::symbol_map::{self, SymbolMap};
        let ext = Path::new(abs_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let mut sym = SymbolMap::new();
        let idents = symbol_map::extract_identifiers(&extracted, ext);
        for ident in &idents {
            sym.register(ident);
        }
        let compressed = sym.apply(&extracted);
        let sym_table = sym.format_table();
        format!(
            "{display_name} [L{start}-{end} of {total}L]\n{compressed}{sym_table}"
        )
    } else {
        format!("{display_name} [L{start}-{end} of {total}L]\n{extracted}")
    };

    let savings_pct = if original_tokens > 0 {
        ((original_tokens - sent_tokens) as f64 / original_tokens as f64 * 100.0) as u32
    } else {
        0
    };
    format!(
        "{output}\n[graph_read: {original_tokens}→{sent_tokens} tok, -{savings_pct}%]"
    )
}

/// Parse `"file::symbol"` notation into `(file, Some(symbol))` or `(file, None)`.
fn parse_file_symbol(input: &str) -> (String, Option<String>) {
    if let Some(pos) = input.find("::") {
        let file = input[..pos].to_string();
        let symbol = input[pos + 2..].to_string();
        if symbol.is_empty() {
            (file, None)
        } else {
            (file, Some(symbol))
        }
    } else {
        (input.to_string(), None)
    }
}

/// Resolve a relative path to absolute using the project root.
fn resolve_path(file_path: &str, project_root: Option<&str>) -> String {
    let path = Path::new(file_path);
    if path.is_absolute() {
        return file_path.to_string();
    }
    if let Some(root) = project_root {
        let abs = Path::new(root).join(file_path);
        if abs.exists() {
            return abs.to_string_lossy().to_string();
        }
    }
    // Try relative to cwd
    if let Ok(cwd) = std::env::current_dir() {
        let abs = cwd.join(file_path);
        if abs.exists() {
            return abs.to_string_lossy().to_string();
        }
    }
    file_path.to_string()
}

/// Get the default read mode for graph_read from env, defaulting to "map".
fn read_mode_from_env() -> String {
    std::env::var("DG_DEFAULT_READ_MODE").unwrap_or_else(|_| "map".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parse_file_symbol_with_symbol() {
        let (file, sym) = parse_file_symbol("src/auth.ts::handleLogin");
        assert_eq!(file, "src/auth.ts");
        assert_eq!(sym, Some("handleLogin".to_string()));
    }

    #[test]
    fn parse_file_symbol_without_symbol() {
        let (file, sym) = parse_file_symbol("src/auth.ts");
        assert_eq!(file, "src/auth.ts");
        assert_eq!(sym, None);
    }

    #[test]
    fn parse_file_symbol_empty_symbol() {
        let (file, sym) = parse_file_symbol("src/auth.ts::");
        assert_eq!(file, "src/auth.ts");
        assert_eq!(sym, None);
    }

    #[test]
    fn read_mode_default_is_map() {
        std::env::remove_var("DG_DEFAULT_READ_MODE");
        assert_eq!(read_mode_from_env(), "map");
    }

    #[test]
    fn graph_read_with_symbol_on_real_file() {
        // Create a temp file with known content
        let tmp = std::env::temp_dir().join("lean_ctx_graph_read_test.py");
        std::fs::write(
            &tmp,
            "# line 1\ndef foo():\n    pass\n\ndef bar():\n    return 42\n\ndef baz():\n    return 0\n",
        )
        .unwrap();

        let mut cache = SessionCache::new();
        let mut index = HashMap::new();
        // Symbol "bar" is on lines 5-6
        index.insert(
            format!("{}::bar", tmp.to_string_lossy()),
            crate::graph::types::SymbolEntry {
                line_start: 5,
                line_end: 6,
                body_hash: "abc".to_string(),
                confidence: "high".to_string(),
                path: tmp.to_string_lossy().to_string(),
            },
        );

        let result = handle(
            &mut cache,
            &index,
            &format!("{}::bar", tmp.to_string_lossy()),
            None,
            CrpMode::Off,
        );

        // Should contain only lines 5-6
        assert!(result.contains("return 42"), "should contain bar's body");
        assert!(!result.contains("def foo"), "should NOT contain foo");
        assert!(!result.contains("def baz"), "should NOT contain baz");
        assert!(result.contains("graph_read:"), "should have savings line");

        // File should be cached for future reads
        assert!(
            cache.get(&tmp.to_string_lossy()).is_some(),
            "full file should be cached"
        );

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn graph_read_full_file_uses_ctx_read() {
        let tmp = std::env::temp_dir().join("lean_ctx_graph_read_full_test.rs");
        std::fs::write(&tmp, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let mut cache = SessionCache::new();
        let index = HashMap::new();

        let result = handle(
            &mut cache,
            &index,
            &tmp.to_string_lossy(),
            None,
            CrpMode::Off,
        );

        // Should have content (delegated to ctx_read in map mode)
        assert!(!result.is_empty());
        // File should be cached
        assert!(cache.get(&tmp.to_string_lossy()).is_some());

        let _ = std::fs::remove_file(&tmp);
    }
}
