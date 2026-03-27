#!/usr/bin/env bash
# install.sh — Build lean-ctx and install lctx launcher
#
# Usage:
#   ./install.sh              # build + install
#   ./install.sh --build-only # just build, don't symlink
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$SCRIPT_DIR/rust"
BIN_DIR="$SCRIPT_DIR/bin"
INSTALL_DIR="$HOME/.local/bin"

echo "LeanCTX Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# ── Check prerequisites ─────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo "Error: cargo not found. Install Rust: https://rustup.rs"
  exit 1
fi

if ! command -v claude &>/dev/null; then
  echo "Warning: claude CLI not found. Install it to use lctx launcher."
  echo "  npm install -g @anthropic-ai/claude-code"
fi

# ── Build release binary ────────────────────────────────────────────────────
echo ""
echo "Building lean-ctx (release)..."
(cd "$RUST_DIR" && cargo build --release)

BINARY="$RUST_DIR/target/release/lean-ctx"
if [[ ! -x "$BINARY" ]]; then
  echo "Error: build failed — binary not found at $BINARY"
  exit 1
fi

echo "Built: $BINARY"

if [[ "${1:-}" == "--build-only" ]]; then
  echo ""
  echo "Done (build only). Binary at: $BINARY"
  exit 0
fi

# ── Install to ~/.local/bin ──────────────────────────────────────────────────
echo ""
echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

# Symlink lean-ctx binary
ln -sf "$BINARY" "$INSTALL_DIR/lean-ctx"
echo "  lean-ctx  -> $BINARY"

# Symlink lctx launcher
ln -sf "$BIN_DIR/lctx" "$INSTALL_DIR/lctx"
echo "  lctx      -> $BIN_DIR/lctx"

# ── Check PATH ───────────────────────────────────────────────────────────────
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "Warning: $INSTALL_DIR is not in your PATH."

  SHELL_NAME="$(basename "$SHELL" 2>/dev/null || echo "bash")"
  case "$SHELL_NAME" in
    zsh)  RC_FILE="$HOME/.zshrc" ;;
    fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
    *)    RC_FILE="$HOME/.bashrc" ;;
  esac

  echo "Add it with:"
  if [[ "$SHELL_NAME" == "fish" ]]; then
    echo "  fish_add_path $INSTALL_DIR"
  else
    echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> $RC_FILE"
    echo "  source $RC_FILE"
  fi
fi

echo ""
echo "Done! Usage:"
echo "  cd ~/your-project"
echo "  lctx                          # scan + launch Claude Code"
echo "  lctx /path/to/project         # scan a specific project"
echo "  lctx /path/to/project \"task\"  # start with a prompt"
echo "  lctx --resume ID              # resume session"
