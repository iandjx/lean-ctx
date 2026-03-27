# Phase 4: Unified Session + Init Command + Polish

## What was built

The two systems are now fully wired together. Every file read — whether via `ctx_read` or `graph_read` — is tracked in both the LeanCTX session and the dual-graph action graph. A new `lean-ctx init --with-graph` command sets up everything in one step.

## Files modified

| File | Change |
|------|--------|
| `rust/src/core/session.rs` | Added `graph_reads` and `graph_memory_entries` fields to `SessionStats` |
| `rust/src/server.rs` | `ctx_read` now records in action graph; `graph_read` now updates session stats; `graph_add_memory` tracks in session; updated `build_instructions()` with graph-first workflow |
| `rust/src/cli.rs` | Added `--with-graph` flag to `init` command; new `init_graph()` function |
| `rust/src/graph/mod.rs` | Added `record_read()` method to `GraphState` + 3 new tests |

## How the unified tracking works

### `ctx_read` → action graph
When the LLM calls `ctx_read(path)`, the handler now also calls `graph.record_read(path, None)`. This means files discovered via traditional `ctx_read` are visible to the graph's retrieval engine on subsequent turns.

### `graph_read` → session cache
When the LLM calls `graph_read(file)`, the handler:
1. Stores the full file in LeanCTX's `SessionCache` (so future `ctx_read` calls get cache hits)
2. Increments `session.stats.graph_reads`
3. Records in the action graph via `graph.record_read()`

### `graph_add_memory` → session stats
Tracks `session.stats.graph_memory_entries` so session summaries show graph usage.

### `record_read()` method
A unified helper on `GraphState` that:
- Ensures the file node exists in the action graph (no duplicates)
- If a query context is provided, creates a query node + "retrieved" edge
- Logs a "read" action entry with timestamp

## `lean-ctx init --with-graph`

One command to set up the full system:

```bash
lean-ctx init --with-graph [project_root]
```

What it does:
1. Creates `.dual-graph/` directory
2. Runs `graph_scan` using tree-sitter (14 languages) — builds `info_graph.json` + `symbol_index.json`
3. Initializes empty `context-store.json` + `chat_action_graph.json`
4. Adds `.dual-graph/` to `.gitignore` (if not already present)
5. Creates `CLAUDE.md` with the dual-graph context policy (if it doesn't exist)

If `project_root` is omitted, uses the current working directory.

## Updated MCP instructions

The server's `build_instructions()` now includes the graph tools section, telling the LLM to:
- Call `graph_continue` first on every turn
- Use `graph_read` for recommended files
- Respect confidence caps
- Call `graph_register_edit` after making changes

## Tests (3 new, 124 total)

| Test | What it verifies |
|------|-----------------|
| `record_read_adds_file_node_and_action` | Recording a read creates a file node and action entry |
| `record_read_with_query_creates_edge` | Query context creates query node + retrieved edge |
| `record_read_no_duplicate_file_nodes` | Repeated reads of same file don't create duplicate nodes |

## How to run

```bash
cd rust
cargo test -- --nocapture           # all 124 tests
cargo build --release               # release binary

# Manual test:
./target/release/lean-ctx init --with-graph /path/to/project
```

## Final architecture

The system is now a **single Rust binary** with 31 MCP tools:

```
User Request
     │
     ▼
graph_continue (action-memory → info-graph retrieval)
     │
     ├── memories found? → return files, confidence=high, skip reads
     │
     ▼
graph_read (symbol-level + LeanCTX compression)
  ├── file::symbol → extract lines → TDD shorthand
  ├── full file → ctx_read in map mode → AST signatures + deps
  └── cache in SessionCache for future delta/cache hits
     │
     ▼
confidence caps → hard limits on further exploration
     │
     ▼
Compact, compressed context → LLM
```

**Selection** (graph picks 3 right files, not 40) × **Compression** (LeanCTX shrinks them 80-90%) = multiplicative savings.

## Summary of all phases

| Phase | What | Files Created | Files Modified | Tests Added |
|-------|------|---------------|----------------|-------------|
| 1 | Data structures + JSON I/O | 2 | 1 | 10 |
| 2 | Core graph tools (read, neighbors, memory, edit, summary) | 5 | 3 | 19 |
| 3 | Scanner + retrieval engine | 2 | 2 | 20 |
| 4 | Unified session + init command | 0 | 4 | 3 |
| **Total** | | **9 new files** | **7 modified** | **52 new tests** |

Total test count: **124** (118 unit + 6 integration), zero regressions from the original 72.
