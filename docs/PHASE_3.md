# Phase 3: Scanner + Retrieval Engine

## What was built

A pure Rust project scanner and retrieval engine — the system is now fully self-contained with zero Python dependency. 5 new MCP tools bring the total to 31.

## Files created

| File | Purpose |
|------|---------|
| `rust/src/graph/scanner.rs` | Pure Rust project scanner using tree-sitter (14 languages) + regex deps extraction. Walks the directory, builds `InfoGraph` + `SymbolIndex`. |
| `rust/src/graph/retrieval.rs` | Retrieval engine: `graph_continue` (orchestrator), `graph_retrieve` (keyword scoring), `fallback_rg` (controlled grep), `graph_impact` (dependency analysis). |

## Files modified

| File | Change |
|------|--------|
| `rust/src/graph/mod.rs` | Added `pub mod retrieval; pub mod scanner;` |
| `rust/src/server.rs` | Added 5 tool definitions + 5 match arms: `graph_continue`, `graph_retrieve`, `graph_scan`, `graph_impact`, `fallback_rg` |

## How the tools work

### `graph_scan` — Pure Rust scanner
Walks the project directory using `walkdir` and for each source file:
1. Creates a **file node** with path, size, keywords (extracted from filename + content), content (if <8KB), summary (first meaningful line), and MD5 hash
2. Parses with **tree-sitter** (14 languages) via existing `signatures::extract_signatures()` → creates **symbol nodes** with line ranges
3. Extracts imports/exports via existing `deps::extract_deps()` → creates **edges**
4. Builds the `symbol_index` for `file::symbol` → line range lookups
5. Writes `info_graph.json` + `symbol_index.json` to `.dual-graph/`

Skips: `.git`, `node_modules`, `target`, `__pycache__`, `dist`, `build`, `.venv`, `vendor`, and files >512KB.

### `graph_continue` — Retrieval orchestrator
Called first on every turn:
1. No graph? → `{needs_project: true}` (caller should run `graph_scan`)
2. <5 files? → `{skip: true}` (too small for graph overhead)
3. Search **action-memory** (context-store.json) for matching entries by tag/content overlap
4. If memory hit → return files from memory entries, `confidence: "high"`
5. Else → run `graph_retrieve` for keyword-matched scoring

### `graph_retrieve` — Keyword scoring
Scores every node in the info-graph:
- +2.0 per keyword match in `node.keywords`
- +1.5 per keyword match in `node.summary`
- +1.0 per keyword match in `node.content`
- +0.5 per keyword match in `node.path`
- +0.5 boost for nodes connected to high-scoring nodes via edges

Confidence thresholds:
| Top Score | Matches | Confidence | Extra Greps | Extra Files |
|-----------|---------|------------|-------------|-------------|
| >6.0 | ≥3 | high | 0 | 0 |
| >3.0 | any | medium | 2 | 2 |
| else | any | low | 2 | 3 |

### `graph_impact` — Dependency analysis
Shows what depends on a file, up to 2 levels deep:
- Level 1: direct edges (imports, exports, calls, references)
- Level 2: edges of edges

### `fallback_rg` — Controlled grep
Shells out to `rg` with `--max-count` and `--no-heading`. Returns formatted file:line:text results. Hard cap enforced by the confidence system.

## Design decisions

1. **Reuses existing tree-sitter + deps extraction** — `scanner.rs` calls `signatures::extract_signatures()` and `deps::extract_deps()`, which are already battle-tested for 14 languages. No duplicate parsing code.

2. **Keyword extraction is simple** — splits on non-alphanumeric chars, filters stop words, lowercases. No stemming or fuzzy matching. Simple is fast and predictable.

3. **Confidence drives exploration budget** — hard numerical caps prevent the LLM from spiraling into excessive reads. This is the core insight from the graph approach.

4. **`graph_continue` returns JSON** — structured response so the LLM can parse confidence, caps, and recommended files programmatically.

5. **Scanner skips known noise directories** — node_modules, target, dist, etc. Max file size of 512KB prevents scanning generated files.

## Tests (20 new, 121 total)

| Module | Tests | What they verify |
|--------|-------|-----------------|
| `scanner` | 7 | Keyword extraction, summary extraction, symbol line finding, full scan, skip dirs, import resolution |
| `retrieval` | 13 | Keyword extraction, retrieve matching, no-match, continue (no graph/small/memory/retrieval), confidence caps, impact, fallback_rg |

## How to run

```bash
cd rust
cargo test graph::scanner -- --nocapture    # scanner tests
cargo test graph::retrieval -- --nocapture  # retrieval tests
cargo test                                   # all 121 tests
```

## What's next (Phase 4)

Wire everything together: unified session tracking (ctx_read ↔ action graph), `lean-ctx init --with-graph` command, and updated MCP instructions telling the LLM to call `graph_continue` first.
