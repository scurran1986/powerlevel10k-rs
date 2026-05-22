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
# T0.5: gitstatusd acquisition mode.
#   pinned  — download + sha256-verify (default, v0.1.5)
#   system  — legacy: symlink an already-installed brew/apt copy
#   none    — skip entirely; the binary will use the ShellOut fallback
GITSTATUSD_MODE="pinned"

while [ $# -gt 0 ]; do
  case "$1" in
    --shell) SHELL_NAME="$2"; shift 2 ;;
    --no-rc) DO_RC=0; shift ;;
    --no-build) DO_BUILD=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --gitstatusd=*) GITSTATUSD_MODE="${1#--gitstatusd=}"; shift ;;
    --gitstatusd) GITSTATUSD_MODE="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,/^set -euo/p' "$0" | sed -e 's/^# \?//' -e '/^set -euo/d'
      exit 0
      ;;
    *) echo "[error] unknown arg: $1" >&2; exit 2 ;;
  esac
done

case "$GITSTATUSD_MODE" in
  pinned|system|none) ;;
  *) echo "[error] --gitstatusd=$GITSTATUSD_MODE: expected pinned, system, or none" >&2; exit 2 ;;
esac

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

# ---------- gitstatusd acquisition -----------------------------------------
#
# T0.5: pinned download + sha256 verification is the default in v0.1.5.
# `--gitstatusd=system` keeps the legacy symlink path for users who
# explicitly want their brew/apt-installed binary. `--gitstatusd=none`
# skips the work entirely (the binary falls back to its slow ShellOut
# git path at runtime).
#
# Important contract (per design doc § "Fallback if download fails"):
# this section must never `exit` non-zero on failure. A missing
# optional perf optimisation is not a reason to break the install —
# the prompt still renders via ShellOut, just slower. Every failure
# path drops to "warn loudly, continue".

# Sha256 helper — handles shasum (POSIX, Darwin default) and sha256sum
# (Linux coreutils). Echoes the digest hex to stdout, returns 1 if no
# hasher is available. Quiet on success; the caller compares.
sha256_of() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    return 1
  fi
}

verify_sha256() {
  local file="$1" expected="$2" actual
  actual="$(sha256_of "$file")" || return 1
  [ "$actual" = "$expected" ]
}

# Map (uname -s, uname -m) → our canonical triple key (matches
# crates/p10k-rs-git/data/gitstatusd-pins.toml).
# Echoes the triple on stdout; non-zero on unsupported host.
detect_host_triple() {
  local s m
  s="$(uname -s 2>/dev/null | tr '[:upper:]' '[:lower:]')"
  m="$(uname -m 2>/dev/null)"
  case "$s/$m" in
    linux/x86_64|linux/amd64)   echo "x86_64-linux-gnu" ;;
    linux/aarch64|linux/arm64)  echo "aarch64-linux-gnu" ;;
    darwin/x86_64)              echo "x86_64-darwin" ;;
    darwin/arm64|darwin/aarch64) echo "aarch64-darwin" ;;
    *) return 1 ;;
  esac
}

# Read a value from the simple `key = "value"` pin file. We avoid pulling
# in a TOML parser — the pin file is intentionally flat (no nesting beyond
# the per-triple table) and the patterns we care about are all
# `key = "..."` lines under a `[pins.<triple>]` header.
#
# Args: <pin-file> <triple> <field>   (field = upstream_file | tarball_sha256 | binary_sha256)
read_pin_field() {
  local file="$1" triple="$2" field="$3"
  awk -v want="[pins.$triple]" -v key="$field" '
    $0 == want { in_section = 1; next }
    /^\[/      { in_section = 0; next }
    in_section && $1 == key {
      # match key = "value" — strip surrounding quotes from the last field
      v = $0
      sub(/^[^=]*=[ \t]*/, "", v)
      sub(/^"/, "", v)
      sub(/"[ \t]*$/, "", v)
      print v
      exit
    }
  ' "$file"
}

install_pinned_gitstatusd() {
  local pin_file="$SCRIPT_DIR/crates/p10k-rs-git/data/gitstatusd-pins.toml"
  if [ ! -f "$pin_file" ]; then
    echo "[warn] gitstatusd pin file not found at $pin_file; skipping pinned install." >&2
    echo "       The vcs segment will use the slow ShellOut fallback." >&2
    return 0
  fi
  local triple
  if ! triple="$(detect_host_triple)"; then
    echo "[warn] unsupported host ($(uname -s)/$(uname -m)) — no gitstatusd pin available." >&2
    echo "       The vcs segment will use the slow ShellOut fallback." >&2
    return 0
  fi
  local version file expected_sha
  version="$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$pin_file")"
  file="$(read_pin_field "$pin_file" "$triple" "upstream_file")"
  expected_sha="$(read_pin_field "$pin_file" "$triple" "tarball_sha256")"
  if [ -z "$version" ] || [ -z "$file" ] || [ -z "$expected_sha" ]; then
    echo "[warn] pin file missing version / $triple entry; skipping pinned install." >&2
    return 0
  fi
  if ! command -v curl >/dev/null 2>&1; then
    echo "[warn] curl not on PATH — cannot fetch pinned gitstatusd. ShellOut fallback in use." >&2
    return 0
  fi
  if ! command -v tar >/dev/null 2>&1; then
    echo "[warn] tar not on PATH — cannot extract pinned gitstatusd. ShellOut fallback in use." >&2
    return 0
  fi
  if ! command -v sha256_of >/dev/null 2>&1; then : ; fi
  if ! sha256_of /dev/null >/dev/null 2>&1; then
    echo "[warn] neither shasum nor sha256sum on PATH; cannot verify gitstatusd pin. Skipping." >&2
    return 0
  fi

  local target_dir target_path
  target_dir="$HOME/.cargo/bin"
  target_path="$target_dir/gitstatusd"
  mkdir -p "$target_dir"

  # Idempotency: if the on-disk binary already matches the pinned binary
  # sha256, skip the download.
  #
  # Hardening: even when the sha matches, the runtime opens the gitstatusd
  # path with `O_NOFOLLOW` (see `locate_binary_checked` in `p10k-rs-git`)
  # so the daemon-spawn path can't be hijacked by a symlink swap. A
  # legacy symlink at `$target_path` matches by sha (bash follows symlinks
  # transparently) but is refused by the runtime safe-open. Detect that
  # case and replace the symlink with a real copy of the verified bytes
  # so install.sh and the runtime agree on what's installable.
  local binary_sha
  binary_sha="$(read_pin_field "$pin_file" "$triple" "binary_sha256")"
  if [ -n "$binary_sha" ] && [ -x "$target_path" ] && verify_sha256 "$target_path" "$binary_sha"; then
    if [ -L "$target_path" ]; then
      local resolved
      resolved="$(readlink -f "$target_path" 2>/dev/null)"
      if [ -n "$resolved" ] && [ -f "$resolved" ]; then
        rm "$target_path"
        cp "$resolved" "$target_path"
        chmod +x "$target_path"
        echo "[gitstatusd] $target_path was a symlink to $resolved — replaced in place with the verified bytes ($version, ${binary_sha:0:12}…). The runtime's O_NOFOLLOW open will now accept it."
        return 0
      fi
      echo "[warn] $target_path is a broken symlink — falling through to fresh download." >&2
      # fall through to the download path below
    else
      echo "[gitstatusd] $target_path already at pinned sha256 ($version, ${binary_sha:0:12}…) — skipping download."
      return 0
    fi
  fi

  local url="https://github.com/romkatv/gitstatus/releases/download/${version}/${file}.tar.gz"
  local tmp_tar tmp_dir
  tmp_tar="$(mktemp "$target_dir/.gitstatusd.tar.gz.XXXXXXXX")"
  tmp_dir="$(mktemp -d "$target_dir/.gitstatusd.d.XXXXXXXX")"
  # Best-effort cleanup on any exit from this function.
  trap 'rm -f "$tmp_tar"; rm -rf "$tmp_dir"' RETURN

  echo "[gitstatusd] downloading $file.tar.gz from $url"
  if ! curl --fail --location --silent --show-error \
        --proto '=https' --tlsv1.2 \
        --max-time 60 \
        -o "$tmp_tar" "$url"; then
    echo "[warn] gitstatusd download failed from $url — falling back to ShellOut." >&2
    return 0
  fi

  if ! verify_sha256 "$tmp_tar" "$expected_sha"; then
    local actual
    actual="$(sha256_of "$tmp_tar" 2>/dev/null || echo '<sha-failed>')"
    echo "[warn] gitstatusd tarball sha256 mismatch for $triple:" >&2
    echo "         expected $expected_sha" >&2
    echo "         got      $actual" >&2
    echo "       Refusing to install — falling back to ShellOut." >&2
    return 0
  fi

  if ! tar -xzf "$tmp_tar" -C "$tmp_dir"; then
    echo "[warn] failed to extract $tmp_tar — falling back to ShellOut." >&2
    return 0
  fi
  local staged="$tmp_dir/$file"
  if [ ! -f "$staged" ]; then
    echo "[warn] extracted tarball did not contain expected file '$file' — falling back to ShellOut." >&2
    return 0
  fi
  # Belt-and-braces: re-verify the inner binary against the pinned
  # binary_sha256 even though the tarball already passed.
  if [ -n "$binary_sha" ] && ! verify_sha256 "$staged" "$binary_sha"; then
    local actual_bin
    actual_bin="$(sha256_of "$staged" 2>/dev/null || echo '<sha-failed>')"
    echo "[warn] extracted gitstatusd binary sha256 mismatch:" >&2
    echo "         expected $binary_sha" >&2
    echo "         got      $actual_bin" >&2
    echo "       Refusing to install — falling back to ShellOut." >&2
    return 0
  fi
  chmod 0755 "$staged"
  # Atomic install: mv onto the final path. On the same filesystem the
  # rename(2) is atomic; an old binary stays usable until the moment
  # the swap completes.
  if ! mv "$staged" "$target_path"; then
    echo "[warn] failed to install gitstatusd at $target_path — falling back to ShellOut." >&2
    return 0
  fi
  echo "[gitstatusd] installed pinned $version ($triple) at $target_path"
  echo "[gitstatusd] verify with: p10k-rs verify"
}

install_system_gitstatusd() {
  # Legacy symlink path. Identical to the pre-T0.5 behaviour; kept for
  # users who explicitly opt out of the pinned download.
  local cand
  if command -v gitstatusd >/dev/null 2>&1; then
    echo "[gitstatusd] system gitstatusd already on PATH — leaving alone."
    return 0
  fi
  for cand in "/opt/homebrew/bin/gitstatusd" "/usr/local/bin/gitstatusd"; do
    if [ -x "$cand" ]; then
      ln -sfn "$cand" "$HOME/.cargo/bin/gitstatusd"
      echo "[gitstatusd] symlinked $cand -> ~/.cargo/bin/gitstatusd"
      return 0
    fi
  done
  echo "[warn] gitstatusd not found — vcs segment will use the slow shell-out fallback." >&2
  echo "       Install one of: \`brew install gitstatusd\`, \`apt install zsh-gitstatus\`," >&2
  echo "       run \`./install.sh --gitstatusd=pinned\`, or set \$P10K_RS_GITSTATUSD_BIN." >&2
}

case "$GITSTATUSD_MODE" in
  pinned) install_pinned_gitstatusd ;;
  system) install_system_gitstatusd ;;
  none)   echo "[gitstatusd] --gitstatusd=none — skipping; vcs segment will use ShellOut fallback." ;;
esac

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
