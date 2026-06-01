#!/usr/bin/env bash
# Local gate sweep mirroring the v1.0-cycle main-push CI matrix.
#
# Why this script exists: the v1.0 swarm surfaced four pre-existing
# defects (rustdoc broken link, cargo-machete unused dep, miri toolchain
# pin, cargo-fuzz hex-Visitor panic) that local gate sweeps had been
# missing because the local sweep didn't match what CI ran. This script
# is the single source of truth; CLAUDE.md points here.
#
# Every gate captures its own exit code directly — no `cargo X | tail`
# patterns. That trap is gotcha #1 in STATE.md and bit the v1.0 swarm
# despite being documented in memory.
#
# Default: fast gates (build / test / clippy / fmt / doc / deny /
# machete). Runs in ~2 minutes on a warm cache.
#
# --slow / --full: additionally runs miri + cargo-semver-checks.
# Adds ~5–10 minutes; mirrors the supplemental CI workflows.

set -euo pipefail

green()  { printf "\033[32m%s\033[0m" "$1"; }
red()    { printf "\033[31m%s\033[0m" "$1"; }
yellow() { printf "\033[33m%s\033[0m" "$1"; }

LOGDIR="${TMPDIR:-/tmp}/p10k-gates-$$"
mkdir -p "$LOGDIR"

PASS=0
FAIL=0
SKIP=0
FAILED_GATES=()

gate() {
    local name="$1"; shift
    local log="$LOGDIR/$(echo "$name" | tr ' /' '__').log"
    printf "  → %-30s ... " "$name"
    if "$@" >"$log" 2>&1; then
        green "ok"; echo
        PASS=$((PASS + 1))
    else
        red "FAIL"; echo
        FAIL=$((FAIL + 1))
        FAILED_GATES+=("$name")
        echo "    log: $log"
        sed 's/^/    /' < <(tail -8 "$log")
    fi
}

skip() {
    local name="$1"; local reason="$2"
    printf "  → %-30s ... " "$name"
    yellow "SKIP"; echo " ($reason)"
    SKIP=$((SKIP + 1))
}

HEAD_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo '?')"
echo "p10k-rs gate sweep against ${HEAD_SHA}"
echo "──"

# Fast gates — match the CI main-push matrix.
gate "fmt"        cargo fmt --all -- --check
gate "build"      cargo build --workspace --locked
gate "clippy"     cargo clippy --workspace --all-targets --locked -- -D warnings
gate "test"       cargo test --workspace --locked
gate "doc"        env RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked

if command -v cargo-deny >/dev/null 2>&1; then
    gate "deny"   cargo deny check
else
    skip "deny"   "cargo-deny not installed"
fi

if command -v cargo-machete >/dev/null 2>&1; then
    gate "machete" cargo machete
else
    skip "machete" "cargo-machete not installed"
fi

# Slow gates — opt-in via --slow / --full.
if [[ "${1:-}" == "--slow" || "${1:-}" == "--full" ]]; then
    echo
    echo "Slow gates (--slow):"
    if command -v cargo-semver-checks >/dev/null 2>&1; then
        gate "semver-checks" cargo semver-checks
    else
        skip "semver-checks"  "cargo-semver-checks not installed"
    fi
    if rustup toolchain list 2>/dev/null | grep -q nightly; then
        # rust-toolchain.toml pins 1.88.0; +nightly overrides so cargo-miri
        # (only on nightly) is what actually runs. Same workaround as the
        # CI miri workflow.
        gate "miri (p10k-rs-config)" env MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test -p p10k-rs-config
    else
        skip "miri" "nightly toolchain not installed"
    fi
fi

echo
echo "──"
if [[ $FAIL -eq 0 ]]; then
    msg="all gates green ($PASS pass"
    [[ $SKIP -gt 0 ]] && msg="$msg, $SKIP skipped"
    msg="$msg)"
    green "$msg"; echo
    rm -rf "$LOGDIR"
    exit 0
else
    red "$FAIL gate(s) failed: ${FAILED_GATES[*]}"; echo
    echo "logs preserved at: $LOGDIR"
    exit 1
fi
