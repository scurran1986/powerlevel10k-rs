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

# T1.20: require git >= 2.35.2 (CVE-2022-24765 mitigation).
#
# `p10k-rs`'s vcs segment shells out to `git status` on every prompt against
# whatever cwd the user is sitting in. Pre-2.35.2 git will happily operate on
# a `.git` directory owned by a different uid, which lets a malicious repo
# planted in a shared dir (`/tmp`, an extracted tarball, a Docker bind mount,
# …) execute arbitrary code via `core.fsmonitor` the moment the user `cd`s in.
# See https://github.blog/2022-04-12-git-security-vulnerability-announced/ .
#
# Policy: loud warn + non-zero exit. Users on stale distros (RHEL 8, Ubuntu
# 20.04 LTS) can opt in deliberately by setting `P10K_RS_SKIP_GIT_VERSION_CHECK=1`.
# Hard refuse felt too paternal — the user already has to run `bash install.sh`
# by hand, so consent is established; the override gives them a way through
# without forking the script.
check_git_version() {
  local raw major minor patch req_major=2 req_minor=35 req_patch=2
  if ! command -v git >/dev/null 2>&1; then
    echo "[warn] git not on PATH. vcs segment will be silent until git is installed." >&2
    return 0
  fi
  # `git --version` → "git version 2.43.0" / "git version 2.39.5 (Apple Git-154)" / etc.
  raw="$(git --version 2>/dev/null | awk '{print $3}')"
  if ! parse_semver "$raw"; then
    echo "[warn] couldn't parse 'git --version' output ('$raw'); skipping version check." >&2
    return 0
  fi
  major=$PARSED_MAJOR; minor=$PARSED_MINOR; patch=$PARSED_PATCH
  if [ "$major" -gt "$req_major" ] \
     || { [ "$major" -eq "$req_major" ] && [ "$minor" -gt "$req_minor" ]; } \
     || { [ "$major" -eq "$req_major" ] && [ "$minor" -eq "$req_minor" ] && [ "$patch" -ge "$req_patch" ]; }; then
    return 0
  fi
  echo "" >&2
  echo "[error] git $major.$minor.$patch is older than the required 2.35.2." >&2
  echo "        p10k-rs runs 'git status' on every prompt; pre-2.35.2 git is" >&2
  echo "        vulnerable to CVE-2022-24765 (core.fsmonitor RCE in repos" >&2
  echo "        owned by another uid). A malicious .git in /tmp, an extracted" >&2
  echo "        tarball, or a Docker bind mount becomes RCE the moment you cd in." >&2
  echo "        See https://github.blog/2022-04-12-git-security-vulnerability-announced/" >&2
  echo "" >&2
  echo "        Fix: upgrade git (apt/dnf/brew). On stale distros, override with:" >&2
  echo "          P10K_RS_SKIP_GIT_VERSION_CHECK=1 $0 $*" >&2
  echo "" >&2
  return 1
}

# Parse `MAJOR.MINOR.PATCH[...]` into PARSED_MAJOR / PARSED_MINOR / PARSED_PATCH.
# Returns 0 on success, 1 if any of the three aren't pure integers. Trailing
# segments ("rc1", "(Apple Git-154)", "-dev") are ignored.
parse_semver() {
  local v="${1:-}"
  # Strip everything from the first non-version character (space, dash, paren, etc.).
  v="${v%%[!0-9.]*}"
  local IFS=.
  # shellcheck disable=SC2206  # intentional word-split on '.'
  local parts=( $v )
  if [ "${#parts[@]}" -lt 3 ]; then
    return 1
  fi
  case "${parts[0]}${parts[1]}${parts[2]}" in
    *[!0-9]*|"") return 1 ;;
  esac
  PARSED_MAJOR="${parts[0]}"
  PARSED_MINOR="${parts[1]}"
  PARSED_PATCH="${parts[2]}"
  return 0
}

if [ "${P10K_RS_SKIP_GIT_VERSION_CHECK:-0}" != "1" ]; then
  if ! check_git_version; then
    exit 1
  fi
else
  echo "[warn] P10K_RS_SKIP_GIT_VERSION_CHECK=1 — bypassing git >= 2.35.2 check (CVE-2022-24765)." >&2
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

# ---------- gitstatusd discovery -------------------------------------------
#
# `p10k-rs init zsh` substitutes the gitstatusd binary path it found at init
# time. After slice 9 (which dropped a dev-machine fallback for security
# reasons), the binary only probes `$P10K_RS_GITSTATUSD_BIN` and `$PATH`.
# If a known canonical install isn't already on PATH, symlink it next to
# the p10k-rs binary so the daemon path stays intact for new shells.
GITSTATUSD_CANDIDATES=(
  "/opt/homebrew/bin/gitstatusd"
  "/usr/local/bin/gitstatusd"
)
if ! command -v gitstatusd >/dev/null 2>&1; then
  for cand in "${GITSTATUSD_CANDIDATES[@]}"; do
    if [ -x "$cand" ]; then
      ln -sfn "$cand" "$HOME/.cargo/bin/gitstatusd"
      echo "[gitstatusd] symlinked $cand -> ~/.cargo/bin/gitstatusd"
      break
    fi
  done
  if ! command -v gitstatusd >/dev/null 2>&1; then
    echo "[warn] gitstatusd not found — vcs segment will use the slow shell-out fallback." >&2
    echo "       Install one of: \`brew install gitstatusd\`, \`apt install zsh-gitstatus\`," >&2
    echo "       or set \$P10K_RS_GITSTATUSD_BIN before sourcing the eval." >&2
  fi
fi

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
