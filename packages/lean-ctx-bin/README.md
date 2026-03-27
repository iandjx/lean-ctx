# lean-ctx-bin

[lean-ctx](https://github.com/yvgude/lean-ctx) distributed as an npm package — **no Rust required**.

Downloads the correct pre-built binary for your platform on install.

## Install

```bash
npm install -g lean-ctx-bin
```

That's it. The `lean-ctx` binary is now available on your PATH.

## Usage

```bash
lean-ctx --help
lean-ctx --version
```

Or with the `lctx` launcher (Claude Code integration):

```bash
curl -fsSL https://raw.githubusercontent.com/yvgude/lean-ctx/main/install.sh | bash -s -- --download
lctx ~/your-project
```

## Platforms

| OS | Architecture | Support |
|----|---|---|
| macOS | Apple Silicon (arm64) | ✓ |
| macOS | Intel (x86_64) | ✓ |
| Linux | x86_64 | ✓ |
| Linux | aarch64 | ✓ |
| Windows | x86_64 | ✓ |

## Source

https://github.com/yvgude/lean-ctx
