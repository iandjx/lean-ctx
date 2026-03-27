# Phase 1: Data Structures + JSON I/O

## What was built

A new `graph` module (`rust/src/graph/`) that defines Rust types matching the Codex-CLI-Compact dual-graph JSON format, and can load/save all four `.dual-graph/*.json` files.

## Files created

| File | Purpose |
|------|---------|
| `rust/src/graph/mod.rs` | Module root. `GraphState` struct that holds all graph data in memory. Load/save methods for each JSON file. |
| `rust/src/graph/types.rs` | Serde structs for all four JSON files: `InfoGraph`, `GraphNode`, `GraphEdge`, `SymbolEntry`, `MemoryEntry`, `ActionGraph`, `ActionNode`, `ActionEdge`, `ActionFileEntry`, `ActionEntry`. |

## Files modified

| File | Change |
|------|--------|
| `rust/src/main.rs` | Added `mod graph;` declaration |

## Data structures

### info_graph.json → `InfoGraph`
The semantic graph of the codebase. Contains:
- **File nodes**: path, extension, size, keywords, content (for small files), summary, hash
- **Symbol nodes**: function/class/type definitions with line ranges, body hash, confidence
- **Edges**: relationships between nodes (imports, exports, references, calls)

### symbol_index.json → `SymbolIndex` (HashMap)
Fast lookup from `"file::symbol"` notation to line ranges. Used by `graph_read` to read only specific functions instead of full files.

### context-store.json → `Vec<MemoryEntry>`
Persistent memory across sessions. Stores decisions, tasks, facts, blockers — each max 15 words, tagged with relevant files.

### chat_action_graph.json → `ActionGraph`
Session-level tracking of what was queried, read, and edited. Contains nodes (queries and files), edges (retrieved/read relationships), cached file content, and an action log.

## Design decisions

1. **`#[serde(default)]` on all fields** — Makes deserialization resilient to missing fields. If graperoot adds or removes fields, our structs won't break.

2. **`Option<serde_json::Value>` for flexible fields** — Fields like `evidence` and `meta` have inconsistent shapes across entries. Using `Value` avoids over-specifying the schema.

3. **`GraphState::load()` never panics** — Missing or malformed JSON files are silently skipped, returning an empty state. This lets the MCP server start even if no project has been scanned.

4. **Load helper as a free function** — `load_json<T>(path)` is a simple generic that reads a file and deserializes. Used for all four JSON files.

## Tests (10 total)

| Test | What it verifies |
|------|-----------------|
| `deserialize_info_graph` | Real info_graph.json loads with correct node/edge counts (117/201) |
| `deserialize_symbol_index` | Real symbol_index.json loads; `pick_instances.py::files_touched` has line_start=9 |
| `deserialize_context_store` | Real context-store.json loads with populated entries |
| `deserialize_action_graph` | Real chat_action_graph.json loads with correct node types |
| `info_graph_round_trip` | Serialize → deserialize preserves all fields |
| `empty_json_deserializes_to_defaults` | `{}`, `[]` produce valid empty structs (no panics) |
| `load_real_dual_graph` | `GraphState::load()` on real .dual-graph/ populates all fields |
| `load_nonexistent_dir_returns_empty` | Missing directory returns empty state, no panic |
| `save_and_reload_context_store` | Write → read round-trip for context store |
| `save_and_reload_action_graph` | Write → read round-trip for action graph |

## How to run

```bash
cd rust
cargo test graph -- --nocapture    # just graph tests (10 tests)
cargo test                          # all tests (82 total)
```

## What's next (Phase 2)

Implement 5 graph MCP tools that operate on this data: `graph_read`, `graph_neighbors`, `graph_add_memory`, `graph_register_edit`, `graph_action_summary`. The key integration happens in `graph_read`, which will pipe file reads through LeanCTX's compression pipeline.
