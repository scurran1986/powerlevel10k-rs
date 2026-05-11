# CLAUDE.md — powerlevel10k-rs

Project-specific guidance for Claude Code working in this repo. Per-user
preferences live in `~/CLAUDE.md`; this file only documents what's true
about *this* codebase.

## What this is

Rust port and spiritual successor to [Powerlevel10k][p10k]. Single
static binary, declarative TOML config, multi-shell prompt. The
load-bearing performance claim is `gitstatusd`-class git latency.

[p10k]: https://github.com/romkatv/powerlevel10k

Status: early-alpha, daily-driver-grade for the maintainer. Currently
on slice 13 (TOML config loader wired into render path). 53+ tests
green on stable Rust **1.88**.

## Build / test / lint

```bash
cargo build --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --no-deps --workspace --locked
```

CI runs the same on Ubuntu + macOS. Don't ship a slice without all five
clean.

- **Toolchain is pinned** to `1.88.0` in `rust-toolchain.toml`. Don't bump
  casually — the floor is set by transitive deps (clap_derive 4.6 needs
  edition2024 → 1.85; `home` 0.5.12 needs 1.88).
- **MSRV policy:** stable − 2, currently 1.88. Don't reach for unstable.
- Rustfmt is stable-only (`max_width = 100`, no `unstable_features`).

## Workspace layout

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import (data only)
  p10k-rs-segments   # segment implementations
  p10k-rs-git        # gitstatusd client + git shell-out fallback (the only `unsafe` budget)
  p10k-rs-shell      # per-shell init scripts (zsh today, bash/fish stubs)
  p10k-rs-wizard     # `configure` TUI (stub)
  p10k-rs-ai         # AI-host detection / OSC emission (stub)
  p10k-rs-ipc        # FIFO plumbing
```

Architectural rationale lives in `docs/adr/`. Read **ADR-0001** before
touching anything git-related — the hot path is a long-lived
`gitstatusd` daemon over FIFOs, *not* a pure-Rust scanner. That decision
is load-bearing.

Hot paths (touch with care):
- `crates/p10k-rs-git/src/gitstatusd.rs` — daemon client, FIFO IPC
- `crates/p10k-rs-shell/shells/zsh/init.zsh` — shell-side hooks
- `install.sh` — bootstrap, gitstatusd symlink, uninstall

## Code standards (non-negotiable)

These are enforced as warnings/errors via `[workspace.lints]` in the
root `Cargo.toml`. Don't `#[allow]` them away — fix the code.

1. **Conservative deps.** Adding a crate is a long-term commitment.
   If it has <3 reverse-deps on crates.io and you can write the
   equivalent in 50 lines, write it. New deps need rationale in the
   PR.
2. **No `unsafe` without justification.** Only `p10k-rs-git` has the
   `unsafe` budget. Every `unsafe` block needs a safety comment that
   states why the safe alternative is unfit, what invariants the call
   site upholds, and what would have to change to make the block
   unsound.
3. **Doc comments on every `pub` item.** `///` everywhere. One example
   for any non-obvious API. `missing_docs = "warn"` at workspace level.
4. **Typed errors in libraries.** `thiserror` on every error enum.
   `anyhow` *only* in the binary's `main` and binary-side glue.
   Libraries never panic.
5. **`unwrap_used`, `expect_used`, `panic`, `todo`, `dbg_macro` are
   warns.** Treat them as bugs.
6. **No `tokio` in MVP.** Architecture is spawn-per-prompt synchronous.
   Async is a v0.2 conversation.
7. **Boring code wins.** No clever macros, premature traits, or
   abstractions for their own sake.
8. **Render-path sanitization is load-bearing.** Branch names and cwd
   pass through `SafeText` (see slice 11–12). Don't bypass it. ANSI
   escapes and `%`-expansion are attacker-controlled input.

## Commits

- One logical change per commit. Message explains the *why*.
- Conventional Commits encouraged (`feat:`, `fix:`, `refactor:`, etc.)
  but not enforced.
- Don't squash unrelated changes; rebase before merge.
- Slices land as `slice N: <one-line goal>` — see git log for the cadence.

## Planning artifacts

Project-level planning lives outside the repo at
`~/.planning/powerlevel10k-rs/` (per user `CLAUDE.md` convention).
Key docs the maintainer references:
- `MVP-SPEC.md` § 1.2 — segment inventory (canonical list of what's
  shipped vs. stubbed)
- `ARCHITECTURE.md` § 1 — crate layout rationale
- `09-rust-ecosystem.md` — dep selection notes

`segment_names()` in `crates/p10k-rs-segments/src/lib.rs` is the
runtime-authoritative version of the segment list.

## Things that will bite you

- **`default_layout()` is the canonical 5-segment order today**, not the
  TOML config. Slice 13 wired the loader; full TOML-driven layout is
  still landing.
- **gitstatusd is GPL-3.0**, bundled as a separate static binary
  (not statically linked into our MIT/Apache-2.0 binary). See
  `THIRD-PARTY-LICENSES.md` and ADR-0001 § Operational. Don't link it in.
- **Stale slice comments** were stripped in slice 12-a. Don't add
  `// slice N: ...` markers to new code — they rot.
- **FIFO security** (slice 9) — pre-opened, mode-checked, owner-checked
  before use. Don't shortcut this when adding new IPC.
- The `crates/spike-gitstatus` directory does **not** exist — the day-1
  latency spike was removed in slice 9 per ADR-0001 § Follow-ups. Don't
  recreate it.
