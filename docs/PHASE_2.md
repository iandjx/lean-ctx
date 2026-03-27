# Phase 2: Core Graph Tools

## What was built

5 new MCP tools that operate on the dual-graph data structures from Phase 1. These are now registered in the LeanCTX MCP server alongside the existing 21 tools (26 tools total).

The key integration happens in `graph_read`, which pipes file reads through LeanCTX's compression pipeline — this is where the two strategies (selection + compression) merge.

## Files created

| File | Purpose |
|------|---------|
| `rust/src/graph/read.rs` | `graph_read` handler — reads files/symbols with LeanCTX compression |
| `rust/src/graph/neighbors.rs` | `graph_neighbors` handler — finds connected files via graph edges |
| `rust/src/graph/memory.rs` | `graph_add_memory` handler — persistent cross-session memory |
| `rust/src/graph/edit.rs` | `graph_register_edit` handler — records file edits in action graph |
| `rust/src/graph/summary.rs` | `graph_action_summary` handler — session action summary |

## Files modified

| File | Change |
|------|--------|
| `rust/src/graph/mod.rs` | Added `pub mod` declarations for 5 new submodules |
| `rust/src/tools/mod.rs` | Added `graph: Arc<RwLock<GraphState>>` field to `LeanCtxServer`, initialized from cwd |
| `rust/src/server.rs` | Added 5 tool definitions in `list_tools()` + 5 match arms in `call_tool()` |

## How the tools work

### `graph_read` (the integration point)
This is where graph-backed selection meets LeanCTX compression:

1. **Parse** the `file` parameter — supports `file::symbol` notation (e.g. `src/auth.ts::handleLogin`)
2. **Symbol lookup** — if `::symbol` present, look up line range in `symbol_index`
3. **Read** — read from disk (or LeanCTX session cache if already cached)
4. **Extract** — for symbol reads, extract only the relevant lines
5. **Compress** — apply LeanCTX compression:
   - Symbol reads: TDD shorthand on extracted lines
   - Full file reads: delegate to `ctx_read` in `map` mode (configurable via `DG_DEFAULT_READ_MODE`)
6. **Cache** — store full file in `SessionCache` for future delta/cache hits
7. **Track** — record token savings via `record_call()`

### `graph_neighbors`
Finds all edges where `from` or `to` matches the given file. Returns outgoing and incoming connections with relationship types (imports, exports, calls, references).

### `graph_add_memory`
Creates a new `MemoryEntry` with:
- Auto-generated ID (`mem:{epoch_ms}`)
- Validated kind (decision/task/next/fact/blocker)
- Content limited to 15 words
- Tags and files for future retrieval
- Auto-prunes at 50 entries (preserves decisions/tasks preferentially)
- Saves to disk immediately

### `graph_register_edit`
Records file edits in the action graph:
- Creates an edit node + edges to each file
- Ensures file nodes exist (no duplicates)
- Logs the action with timestamp
- Saves to disk immediately

### `graph_action_summary`
Returns a compact summary: query count, files accessed, edits, retrievals, total actions. Lists query text and edited file names.

## Design decisions

1. **`graph_read` delegates to `ctx_read` for full files** — no duplicate compression code. Symbol reads do their own line extraction + TDD shorthand.

2. **`GraphState` loaded from cwd on server startup** — if `.dual-graph/` exists in the current directory, it's loaded automatically. No explicit init needed for existing projects.

3. **Graph tools save to disk immediately** — `graph_add_memory` and `graph_register_edit` persist changes right away. This matches the original Codex-CLI-Compact behavior.

4. **Graph tools skip auto-checkpoint** — they're lightweight metadata operations, not file reads. No need to trigger the checkpoint system.

## Tests (19 new, 101 total)

| Module | Tests | What they verify |
|--------|-------|-----------------|
| `graph::read` | 6 | Symbol parsing, symbol-level reads, full file delegation, caching |
| `graph::neighbors` | 3 | Both directions, no matches, no graph |
| `graph::memory` | 4 | Basic add, invalid kind, long content, pruning |
| `graph::edit` | 4 | Single edit, multiple edits, no duplicates, empty files |
| `graph::summary` | 2 | Empty graph, graph with data |

## How to run

```bash
cd rust
cargo test graph -- --nocapture    # just graph tests (29 tests)
cargo test                          # all tests (101 total)
```

## What's next (Phase 3)

Implement the retrieval engine (`graph_continue`, `graph_retrieve`, `fallback_rg`) and the pure Rust scanner (`graph_scan`) using LeanCTX's tree-sitter integration. This will make the system fully self-contained — no Python dependency.
