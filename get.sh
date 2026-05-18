#!/usr/bin/env bash
#
# get.sh — one-line bootstrap installer for p10k-rs.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/get.sh | bash
#
# Behaviour:
#   1. Resolve the latest semver-shaped release tag (v[0-9]+.[0-9]+.[0-9]+*)
#      from the remote and check that out. The `main` branch is *not* the
#      default — it bypasses every release-time signing/attestation gate
#      (see security sprint T0.8). Override with P10K_RS_REF if you really
#      want unreleased code.
#   2. Clone (or update) the repo at ${P10K_RS_DIR:-~/.local/share/powerlevel10k-rs}
#      pinned to that ref.
#   3. Hand off to install.sh inside that checkout, which builds the binary,
#      wires `eval "$(p10k-rs init zsh)"` into ~/.zshrc, and symlinks
#      gitstatusd next to the binary if a canonical install is on PATH.
#
# Idempotent: re-piping the same command upgrades an existing checkout
# instead of failing on a non-empty directory.
#
# Environment overrides:
#   P10K_RS_REPO  — clone URL (default: the public canonical URL)
#   P10K_RS_DIR   — destination (default: ~/.local/share/powerlevel10k-rs)
#   P10K_RS_REF   — git ref to check out. Defaults to the latest semver tag.
#                   Set to `main` to track the development branch (NOT
#                   recommended — skips release verification), or to an
#                   explicit tag like `v0.1.5` to pin.
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

# Resolve the ref to install. Honour an explicit override; otherwise ask the
# remote for its latest semver-shaped tag. Failing closed if no tag is found
# is intentional — silently falling back to `main` would defeat the whole
# point of this change.
REF="${P10K_RS_REF:-}"
if [ -z "$REF" ]; then
  REF=$(git ls-remote --tags --refs "$REPO_URL" \
    | awk '{print $2}' \
    | sed 's|refs/tags/||' \
    | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+' \
    | sort -V \
    | tail -1)
  if [ -z "$REF" ]; then
    echo "[get.sh] error: no semver tag (v[0-9]+.[0-9]+.[0-9]+*) found at $REPO_URL" >&2
    echo "[get.sh]        set P10K_RS_REF=main to install from the development branch (not recommended)." >&2
    exit 1
  fi
fi

echo "[get.sh] using ref: $REF"

if [ -d "$TARGET_DIR/.git" ]; then
  echo "[get.sh] updating existing checkout at $TARGET_DIR"
  git -C "$TARGET_DIR" fetch --quiet --tags origin
  # `-c advice.detachedHead=false` keeps the bootstrap output tidy when REF
  # is a tag (the common case). For branch refs, this is still a detached
  # checkout of the remote tip, which is what we want for an installer.
  git -C "$TARGET_DIR" -c advice.detachedHead=false checkout --quiet "$REF"
else
  echo "[get.sh] cloning $REPO_URL → $TARGET_DIR"
  mkdir -p "$(dirname "$TARGET_DIR")"
  git clone --quiet "$REPO_URL" "$TARGET_DIR"
  git -C "$TARGET_DIR" -c advice.detachedHead=false checkout --quiet "$REF"
fi

exec "$TARGET_DIR/install.sh" "$@"
