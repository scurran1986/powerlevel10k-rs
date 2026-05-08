#!/usr/bin/env bash
#
# install.sh — build, install, and wire `p10k-rs` into your shell.
#
# Defaults:
#   1. cargo build --release
#   2. cargo install --path crates/p10k-rs --force   (puts binary in ~/.cargo/bin)
#   3. add `eval "$(p10k-rs init <shell>)"` to your shell rc file (idempotent)
#
# Idempotent: re-running upgrades the binary and leaves the rc untouched if the
# line is already there.
#
# Usage:
#   ./install.sh                         # zsh, full install
#   ./install.sh --shell zsh             # explicit
#   ./install.sh --no-rc                 # build + install binary, leave rc alone
#   ./install.sh --no-build              # skip the cargo build step (use existing)
#   ./install.sh --uninstall             # reverse: remove rc line and the binary
#
# Limitations until later slices:
#   - Only `zsh` is wired. Fish/bash init scripts ship with their respective
#     foundation phases.
#   - Assumes `cargo` is on PATH. If you don't have rust installed yet, get
#     rustup from https://rustup.rs first.

set -euo pipefail
LC_ALL=C

# ---------- args -----------------------------------------------------------

SHELL_NAME="zsh"
DO_BUILD=1
DO_RC=1
UNINSTALL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --shell) SHELL_NAME="$2"; shift 2 ;;
    --no-rc) DO_RC=0; shift ;;
    --no-build) DO_BUILD=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help)
      sed -n '2,/^set -euo/p' "$0" | sed -e 's/^# \?//' -e '/^set -euo/d'
      exit 0
      ;;
    *) echo "[error] unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Resolve repo root from the script's location, not $PWD.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

case "$SHELL_NAME" in
  zsh)  RC_FILE="$HOME/.zshrc" ;;
  fish|bash)
    echo "[error] $SHELL_NAME init script lands in a later slice — only zsh is wired today." >&2
    exit 2
    ;;
  *) echo "[error] unsupported shell: $SHELL_NAME (try zsh)" >&2; exit 2 ;;
esac

EVAL_LINE="eval \"\$(p10k-rs init $SHELL_NAME)\""
RC_MARKER="# p10k-rs ($SHELL_NAME) — managed by install.sh; remove this line + the eval below to uninstall"

# ---------- uninstall path -------------------------------------------------

if [ "$UNINSTALL" -eq 1 ]; then
  if [ -f "$RC_FILE" ] && grep -qF "$RC_MARKER" "$RC_FILE"; then
    # Strip the marker line and the eval line that follows it.
    tmp="$(mktemp)"
    awk -v marker="$RC_MARKER" '
      $0 == marker { skip = 2; next }
      skip > 0     { skip--; next }
      { print }
    ' "$RC_FILE" >"$tmp"
    mv "$tmp" "$RC_FILE"
    echo "[uninstall] removed p10k-rs block from $RC_FILE"
  else
    echo "[uninstall] no p10k-rs block found in $RC_FILE — skipping"
  fi
  if command -v cargo >/dev/null 2>&1; then
    cargo uninstall p10k-rs 2>/dev/null && \
      echo "[uninstall] removed binary via 'cargo uninstall p10k-rs'" || \
      echo "[uninstall] no cargo-installed p10k-rs binary found"
  fi
  echo "[uninstall] done. Open a new shell to drop the prompt."
  exit 0
fi

# ---------- preflight ------------------------------------------------------

if ! command -v cargo >/dev/null 2>&1; then
  echo "[error] cargo not on PATH. Install rust via https://rustup.rs first." >&2
  exit 1
fi

# ---------- build ----------------------------------------------------------

if [ "$DO_BUILD" -eq 1 ]; then
  echo "[build] cargo build --release -p p10k-rs"
  cargo build --release -p p10k-rs
fi

# ---------- install --------------------------------------------------------

echo "[install] cargo install --path crates/p10k-rs --force"
cargo install --path crates/p10k-rs --force

INSTALLED_BIN="$(command -v p10k-rs || true)"
if [ -z "$INSTALLED_BIN" ]; then
  echo "[error] cargo install succeeded but 'p10k-rs' isn't on PATH." >&2
  echo "        Make sure ~/.cargo/bin is on \$PATH (rustup adds this for new shells)." >&2
  exit 1
fi
echo "[install] binary at $INSTALLED_BIN"

# ---------- rc edit --------------------------------------------------------

if [ "$DO_RC" -eq 1 ]; then
  if [ ! -f "$RC_FILE" ]; then
    echo "[rc] $RC_FILE does not exist; creating it."
    : >"$RC_FILE"
  fi
  if grep -qF "$RC_MARKER" "$RC_FILE"; then
    echo "[rc] already wired in $RC_FILE — leaving alone"
  else
    {
      echo ""
      echo "$RC_MARKER"
      echo "$EVAL_LINE"
    } >>"$RC_FILE"
    echo "[rc] appended eval to $RC_FILE"
  fi
fi

# ---------- verification ---------------------------------------------------

# Smoke: the init script for $SHELL_NAME should print non-empty output.
if ! "$INSTALLED_BIN" init "$SHELL_NAME" >/dev/null 2>&1; then
  echo "[error] '$INSTALLED_BIN init $SHELL_NAME' failed. Re-run with -v to debug." >&2
  exit 1
fi
echo "[verify] $INSTALLED_BIN init $SHELL_NAME → ok"

# Friendly tail.
cat <<EOF

[done] p10k-rs is installed.

To use it in this shell session immediately:
  exec $SHELL_NAME

Or just open a new $SHELL_NAME terminal.

To uninstall:
  $SCRIPT_DIR/install.sh --uninstall
EOF
