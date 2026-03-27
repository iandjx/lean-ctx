#!/usr/bin/env bash
# install.sh — Install lean-ctx (download pre-built binary or build from source)
#
# Usage:
#   ./install.sh              # build from source + install  (requires Rust)
#   ./install.sh --download   # download pre-built binary    (no Rust needed)
#   ./install.sh --build-only # build only, don't symlink    (requires Rust)
#
# One-liner (no Rust required):
#   curl -fsSL https://raw.githubusercontent.com/yvgude/lean-ctx/main/install.sh | bash -s -- --download
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-install.sh}")" 2>/dev/null && pwd || pwd)"
RUST_DIR="$SCRIPT_DIR/rust"
BIN_DIR="$SCRIPT_DIR/bin"
INSTALL_DIR="$HOME/.local/bin"
REPO="yvgude/lean-ctx"

echo "LeanCTX Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# ── Shared: PATH check + usage printout ──────────────────────────────────────
_finish() {
  if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "Warning: $INSTALL_DIR is not in your PATH."
    SHELL_NAME="$(basename "${SHELL:-bash}" 2>/dev/null || echo bash)"
    case "$SHELL_NAME" in
      zsh)  RC="$HOME/.zshrc" ;;
      fish) RC="$HOME/.config/fish/config.fish" ;;
      *)    RC="$HOME/.bashrc" ;;
    esac
    echo "Add it with:"
    if [[ "$SHELL_NAME" == "fish" ]]; then
      echo "  fish_add_path $INSTALL_DIR"
    else
      echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> $RC && source $RC"
    fi
  fi
  echo ""
  echo "Done! Usage:"
  echo "  cd ~/your-project"
  echo "  lctx                          # scan + launch Claude Code"
  echo "  lctx /path/to/project         # scan a specific project"
  echo "  lctx /path/to/project \"task\"  # start with a prompt"
  echo "  lctx --resume ID              # resume session"
}

# ── Download pre-built binary (--download) ───────────────────────────────────
if [[ "${1:-}" == "--download" ]]; then
  echo "Mode: download pre-built binary (no Rust required)"
  echo ""

  # Detect OS + arch
  OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64)        ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *)
      echo "Error: unsupported architecture '$ARCH'."
      echo "Please build from source: ./install.sh"
      exit 1 ;;
  esac

  case "$OS" in
    linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
    darwin) TARGET="${ARCH}-apple-darwin" ;;
    *)
      echo "Error: unsupported OS '$OS'."
      echo "Windows users: download the .zip from https://github.com/${REPO}/releases/latest"
      exit 1 ;;
  esac

  echo "Platform detected: $TARGET"

  # Resolve latest release tag via GitHub API
  echo "Fetching latest release tag..."
  LATEST="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)"

  if [[ -z "$LATEST" ]]; then
    echo "Error: could not determine latest release."
    echo "Check: https://github.com/${REPO}/releases"
    exit 1
  fi

  echo "Latest: $LATEST"

  URL="https://github.com/${REPO}/releases/download/${LATEST}/lean-ctx-${TARGET}.tar.gz"
  echo "Downloading: $URL"

  mkdir -p "$INSTALL_DIR"
  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  if ! curl -fsSL "$URL" -o "$TMPDIR/lean-ctx.tar.gz"; then
    echo ""
    echo "Error: download failed."
    echo "Check releases: https://github.com/${REPO}/releases"
    exit 1
  fi

  tar -xzf "$TMPDIR/lean-ctx.tar.gz" -C "$TMPDIR"
  install -m755 "$TMPDIR/lean-ctx" "$INSTALL_DIR/lean-ctx"
  echo "  lean-ctx  -> $INSTALL_DIR/lean-ctx"

  # Install lctx launcher (local copy or download from repo)
  if [[ -f "$BIN_DIR/lctx" ]]; then
    ln -sf "$BIN_DIR/lctx" "$INSTALL_DIR/lctx"
    echo "  lctx      -> $BIN_DIR/lctx"
  else
    curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/bin/lctx" \
      -o "$INSTALL_DIR/lctx"
    chmod +x "$INSTALL_DIR/lctx"
    echo "  lctx      -> $INSTALL_DIR/lctx (downloaded)"
  fi

  _finish
  exit 0
fi

# ── Prerequisites check (build-from-source mode) ─────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo "Error: cargo not found."
  echo ""
  echo "Options:"
  echo "  Install Rust (https://rustup.rs)  then re-run: ./install.sh"
  echo "  Or download pre-built binary:               ./install.sh --download"
  exit 1
fi

if ! command -v claude &>/dev/null; then
  echo "Warning: claude CLI not found. Install it to use lctx launcher."
  echo "  npm install -g @anthropic-ai/claude-code"
fi

# ── Build release binary ──────────────────────────────────────────────────────
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

# ── Install to ~/.local/bin ───────────────────────────────────────────────────
echo ""
echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

ln -sf "$BINARY" "$INSTALL_DIR/lean-ctx"
echo "  lean-ctx  -> $BINARY"

ln -sf "$BIN_DIR/lctx" "$INSTALL_DIR/lctx"
echo "  lctx      -> $BIN_DIR/lctx"

_finish
