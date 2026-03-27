# LeanCTX Token Reduction Strategy

## Core Philosophy

LeanCTX treats the LLM context window as a scarce resource. Instead of sending raw, verbose data to the model, it acts as a **Cognitive Filter** — ensuring every token carries maximum signal. The goal is to maximize **information entropy per token**.

> "The winners won't be those who can afford 1M token contexts. They'll be those who achieve the same result with 10K."

---

## Three Delivery Mechanisms

### 1. Shell Hook (Passive, No LLM Cooperation Needed)
Intercepts CLI output before it reaches the LLM. Pattern-matches against **90+ command patterns** (git, cargo, npm, docker, curl, test runners, etc.) and compresses the output in-place. The LLM never sees the raw verbose version.

### 2. MCP Server (21 Tools, Active Cooperation)
Exposes tools that the LLM calls instead of raw file reads. The server controls *what* and *how much* data the model receives per request.

### 3. AI Tool Hooks
One-command integration (`lean-ctx init --agent <tool>`) for Claude Code, Cursor, Gemini CLI, Codex, Windsurf, and Cline. Installs hooks that route tool output through the compression layer automatically.

---

## Six Compression Techniques

### 1. AST-Based Signatures (tree-sitter, 14 languages)
Instead of sending full file contents, sends only the structural skeleton — function signatures, class definitions, type aliases, trait/interface declarations. Supports TypeScript/JavaScript, Rust, Python, Go, Java, C, C++, Ruby, C#, Kotlin, Swift, and PHP.

- **Savings**: ~80-90% of file tokens eliminated
- **When used**: `signatures` read mode — when the LLM needs the API surface, not the implementation

### 2. Delta-Loading (Incremental Reads)
Tracks file state in a session cache. On subsequent reads, only the **diff** (changed lines) is transmitted instead of the full file.

- **Savings**: ~90-99% on re-reads
- **When used**: Any file read after the first — automatic via session cache

### 3. Session Caching with Auto-TTL
Files are cached after first read. Re-reads return a **13-token cache hit confirmation** instead of re-transmitting thousands of tokens. Stale entries are purged automatically.

- **Savings**: 99%+ on repeated reads (30,000 tokens → 13 tokens)
- **When used**: Every file re-read within a session

### 4. Token Dense Dialect (TDD)
Replaces verbose programming grammar with mathematical symbols and short identifiers:

| Symbol | Replaces |
|--------|----------|
| `λ` | function/handler |
| `§` | struct/class/module |
| `∂` | interface/trait |
| `τ` | type alias |
| `ε` | enum |
| `α1, α2...` | long identifiers (>12 chars) |

Example: `λ+handle(⊕,path:s)→s` instead of `fn pub async handle(&self, path: String) -> String`

- **Savings**: 8-25% additional on top of other compression
- **When used**: Default mode for all MCP tool output

### 5. Shannon Entropy Filtering
Analyzes each line's information entropy. Lines that carry no unique information (boilerplate, repetitive patterns, filler) are stripped.

- **Savings**: ~60-80% on noisy files (logs, generated code, config)
- **When used**: `entropy` read mode — best for files with high noise-to-signal ratio

### 6. CLI Output Pattern Matching (90+ Patterns)
Regex-based compression rules for common dev tool output. Each pattern knows what's signal and what's noise for that specific command.

| Command Type | Compression |
|---|---|
| git status/log/diff | **-70%** |
| ls / find | **-80%** |
| cargo/npm build | **-80%** |
| Test runners | **-90%** |
| curl (JSON) | **-89%** |
| docker ps/build | **-80%** |
| grep / rg | **-70%** |

---

## Adaptive Read Modes

LeanCTX provides **6 read modes** so the right fidelity is chosen per task:

| Mode | What It Sends | Use Case |
|------|--------------|----------|
| `full` | Entire file | Editing, debugging |
| `map` | Structure outline | Understanding architecture |
| `signatures` | Function/class signatures only | API surface exploration |
| `entropy` | Entropy-filtered content | Noisy files, logs |
| `delta` | Only changed lines since last read | Iterative editing |
| `cached` | 13-token confirmation | Already-read files |

---

## Context Management Layer

### Session Cache with TTL
Tracks every file read, computes diffs, purges stale entries. Prevents the same file from consuming tokens twice.

### Context Checkpoints (`ctx_compress`)
Creates ultra-compact state summaries when conversations grow long — preserves critical context while freeing token budget.

### Cross-Session Memory (CCP — Context Continuity Protocol)
Persists context across chat sessions and context compactions. Cold start cost drops from ~50,000 tokens to ~400 tokens.

### LITM-Aware Positioning
Places critical information at attention-optimal positions in the context window (beginning and end), based on the "Lost in the Middle" research (Liu et al., 2023). Beginning attention weight: α=0.9, end: γ=0.85.

### Subagent Isolation
`fresh` parameter and `ctx_cache clear` prevent stale cache hits when new agents spawn within the same session.

---

## Additional Features

- **Intent Detection**: Classifies queries as retrieval ("What") vs. reasoning ("How") to inform mode selection
- **Cross-File Deduplication**: Detects and eliminates duplicate content across multiple file reads
- **Dependency Maps**: Understands import/require relationships to suggest relevant files
- **Project Graph**: Builds a structural graph of the codebase for intelligent navigation

---

## Measured Results (Typical Session)

| Operation | Frequency | Standard Tokens | LeanCTX Tokens | Savings |
|---|---|---|---|---|
| File reads (cached) | 15x | 30,000 | 195 | **-99%** |
| File reads (map mode) | 10x | 20,000 | 2,000 | **-90%** |
| ls / find | 8x | 6,400 | 1,280 | **-80%** |
| git status/log/diff | 10x | 8,000 | 2,400 | **-70%** |
| grep / rg | 5x | 8,000 | 2,400 | **-70%** |
| cargo/npm build | 5x | 5,000 | 1,000 | **-80%** |
| Test runners | 4x | 10,000 | 1,000 | **-90%** |
| curl (JSON) | 3x | 1,500 | 165 | **-89%** |
| docker ps/build | 3x | 900 | 180 | **-80%** |
| **Total** | | **~89,800** | **~10,620** | **-88%** |

---

## Summary

LeanCTX achieves token reduction through a layered approach:

1. **Don't send what hasn't changed** — session caching + delta-loading
2. **Don't send what isn't needed** — adaptive read modes (signatures, map, entropy)
3. **Compress what you do send** — TDD symbols, CLI pattern matching, entropy filtering
4. **Remember across sessions** — CCP eliminates cold-start re-reads
5. **Position for attention** — LITM-aware placement maximizes model comprehension of what is sent

The strategies are composable and stack multiplicatively. A cached re-read in TDD mode with entropy filtering can reduce a 2,000-token file to 13 tokens — a 99.4% reduction.
