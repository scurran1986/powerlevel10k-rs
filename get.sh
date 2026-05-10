#!/usr/bin/env bash
#
# get.sh — one-line bootstrap installer for p10k-rs.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/get.sh | bash
#
# Behaviour:
#   1. Clone (or update) the repo at ${P10K_RS_DIR:-~/.local/share/powerlevel10k-rs}.
#   2. Hand off to install.sh inside that checkout, which builds the binary,
#      wires `eval "$(p10k-rs init zsh)"` into ~/.zshrc, and symlinks
#      gitstatusd next to the binary if a canonical install is on PATH.
#
# Idempotent: re-piping the same command upgrades an existing checkout
# instead of failing on a non-empty directory.
#
# Environment overrides:
#   P10K_RS_REPO  — clone URL (default: the public canonical URL)
#   P10K_RS_DIR   — destination (default: ~/.local/share/powerlevel10k-rs)
#
# Any extra args (`bash -s -- --no-rc`, etc.) flow through to install.sh.
#
# Requirements: git, cargo (https://rustup.rs), zsh.

set -euo pipefail
LC_ALL=C

REPO_URL="${P10K_RS_REPO:-https://github.com/scurran1986/powerlevel10k-rs.git}"
TARGET_DIR="${P10K_RS_DIR:-$HOME/.local/share/powerlevel10k-rs}"

if ! command -v git >/dev/null 2>&1; then
  echo "[get.sh] error: git is not on PATH. Install git first." >&2
  exit 1
fi

if [ -d "$TARGET_DIR/.git" ]; then
  echo "[get.sh] updating existing checkout at $TARGET_DIR"
  git -C "$TARGET_DIR" pull --ff-only --quiet
else
  echo "[get.sh] cloning $REPO_URL → $TARGET_DIR"
  mkdir -p "$(dirname "$TARGET_DIR")"
  git clone --quiet "$REPO_URL" "$TARGET_DIR"
fi

exec "$TARGET_DIR/install.sh" "$@"
