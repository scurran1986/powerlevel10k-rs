#!/usr/bin/env bash
# fetch_fixtures.sh — clone benchmark fixtures at pinned commits.
#
# Usage:
#   ./bench/fetch_fixtures.sh                    # ripgrep only (sanity)
#   ./bench/fetch_fixtures.sh --with-linux       # +linux (~5 GB)
#   ./bench/fetch_fixtures.sh --with-chromium    # +chromium (~25 GB, slow)
#   ./bench/fetch_fixtures.sh --with-linux --with-chromium
#
# Honours BENCH_FIXTURES_DIR (default: <script>/fixtures/repos).
# Idempotent: re-running on an already-checked-out fixture at the right
# commit is a no-op.
#
# Pinned commits are part of the spike contract. Do not "update to latest" —
# benchmark numbers are only comparable across runs if the workdir bytes are
# identical. If you genuinely need to bump a pin, treat it as a spec change.

set -euo pipefail
LC_ALL=C
export LC_ALL

# ---- Locate self ----------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="${BENCH_FIXTURES_DIR:-${SCRIPT_DIR}/fixtures/repos}"

# ---- Args -----------------------------------------------------------------
WITH_LINUX=0
WITH_CHROMIUM=0
for arg in "$@"; do
  case "$arg" in
    --with-linux)    WITH_LINUX=1 ;;
    --with-chromium) WITH_CHROMIUM=1 ;;
    -h|--help)
      sed -n '2,15p' "$0"
      exit 0
      ;;
    *)
      printf 'fetch_fixtures.sh: unknown argument: %s\n' "$arg" >&2
      exit 64
      ;;
  esac
done

# ---- Pre-flight ------------------------------------------------------------
if ! command -v git >/dev/null 2>&1; then
  printf 'fetch_fixtures.sh: git is required but not on PATH\n' >&2
  exit 127
fi

mkdir -p "$FIXTURES_DIR"

# ---- Helpers --------------------------------------------------------------
# clone_pinned <name> <url> <commit> <approx_size>
#
# Performs:
#   - First-time: clone (no --depth — we need full history for index ops),
#     then checkout the pinned commit in detached-HEAD state.
#   - Subsequent: verify HEAD == pinned commit; if so, skip; if not, fetch
#     and reset to the pinned commit.
clone_pinned() {
  name="$1"
  url="$2"
  commit="$3"
  approx="$4"
  dest="${FIXTURES_DIR}/${name}"

  if [ -d "${dest}/.git" ]; then
    current="$(git -C "$dest" rev-parse HEAD 2>/dev/null || printf '')"
    if [ "$current" = "$commit" ]; then
      printf '[skip] %-9s already at %s\n' "$name" "$commit"
      return 0
    fi
    printf '[move] %-9s %s -> %s\n' "$name" "${current:-<unknown>}" "$commit"
    git -C "$dest" fetch --tags origin "$commit"
    git -C "$dest" -c advice.detachedHead=false checkout --quiet "$commit"
    return 0
  fi

  printf '[fetch] %-9s %s (~%s)\n' "$name" "$url" "$approx"
  git clone --quiet "$url" "$dest"
  git -C "$dest" -c advice.detachedHead=false checkout --quiet "$commit"
  printf '[done]  %-9s now at %s\n' "$name" "$commit"
}

# ---- ripgrep — always -----------------------------------------------------
# Tag: 14.1.1 — small repo, validates the harness end-to-end.
# This is the commit SHA, not the tag-object SHA — pinning the tag SHA
# (0e8390a66fbcf6eeac1aeb0541b367663a597c79) breaks `clone_pinned`'s
# idempotency check because `git rev-parse HEAD` reports the commit, not the
# tag. The original pin had a hallucinated trailing fragment (4649aa9700c388c5...
# vs the real 4649aa9700619f94...) — leading 8 hex were right.
clone_pinned ripgrep \
  https://github.com/BurntSushi/ripgrep.git \
  4649aa9700619f94cf9c66876e9549d83420e16c \
  30MiB

# ---- linux — opt-in -------------------------------------------------------
if [ "$WITH_LINUX" -eq 1 ]; then
  printf '[warn]  linux fixture is ~5 GB; first clone takes several minutes\n' >&2
  # v6.6 LTS — a stable, popular kernel tag with realistic file count (~80k).
  clone_pinned linux \
    https://github.com/torvalds/linux.git \
    2d1bcbc6cd703e64caf8df314e3669b4f2e1e16a \
    5GiB
fi

# ---- chromium — opt-in ----------------------------------------------------
if [ "$WITH_CHROMIUM" -eq 1 ]; then
  printf '[warn]  chromium fixture is ~25 GB; first clone is slow (>30 min)\n' >&2
  printf '[warn]  expect the workdir checkout alone to take several minutes\n' >&2
  # 120.0.6099.71 stable — referenced by gitstatusd README's headline number.
  clone_pinned chromium \
    https://github.com/chromium/chromium.git \
    e6c2c6c59e4d9ecf26c07b8ab2c61f0e57f8b2fa \
    25GiB
fi

printf '\nFixtures present in %s:\n' "$FIXTURES_DIR"
for d in "$FIXTURES_DIR"/*/; do
  [ -d "$d" ] || continue
  name="$(basename "$d")"
  rev="$(git -C "$d" rev-parse --short HEAD 2>/dev/null || printf '?')"
  printf '  - %-12s @ %s\n' "$name" "$rev"
done
