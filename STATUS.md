# Status

Snapshot of what `p10k-rs` ships today, what's daily-driver-grade,
and what's still a stub.

## Release

**v0.1.5** is the current tag. Daily-driver-grade for the
maintainer. Multi-arch release builds fire on every tag push.
Full slice ledger: [CHANGELOG.md](CHANGELOG.md).

| | |
|---|---|
| Current tag | `v0.1.5` (2026-05-22) |
| Tests | 537 passing, 3 ignored |
| MSRV | Stable Rust **1.88** (pinned in `rust-toolchain.toml`) |
| MSRV policy | stable − 2 |
| License | MIT / Apache-2.0 (dual) |

## Quality gates

All seven gates run clean on Ubuntu + macOS in CI:

| Gate | Command |
|---|---|
| Build | `cargo build --workspace --locked` |
| Tests | `cargo test --workspace --locked` |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Format | `cargo fmt --all -- --check` |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked` |
| Deny policy | `cargo deny check` |
| Unused deps | `cargo machete` |

Per [STATE.md gotcha #1](https://github.com/scurran1986/powerlevel10k-rs):
never pipe a gate command through `tail` — the shell swallows the
exit code. Run each un-piped (or with `set -o pipefail`).

## Supported platforms

Multi-arch binary distribution on every tag:

| Triple | Binary | Tarball signing |
|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ | sigstore keyless + SLSA provenance |
| `aarch64-unknown-linux-gnu` | ✅ | sigstore keyless + SLSA provenance |
| `x86_64-apple-darwin` | ✅ | sigstore keyless + SLSA provenance |
| `aarch64-apple-darwin` | ✅ | sigstore keyless + SLSA provenance |

Verification recipe in [SECURITY.md](SECURITY.md).

## Feature matrix

| Feature | State |
|---|---|
| All 21 MVP segments + 10 modern extras (`jj`, `ai_host`, `mise`, `fnm`, `pixi`, `docker_context`, `os_icon`, `node_version`, `python_version`, `rust_version`) | **31 segments wired** |
| Per-segment styling via TOML (`foreground` / `background` / per-state overrides) | ✅ |
| Truecolor hex literals (`"#ff6600"`, `"#f60"`) and `[r, g, b]` arrays | ✅ |
| Four colour modes — `Ansi8` / `Ansi256` / `TrueColor` / `FollowTerminal` (OSC 4 palette probe) | ✅ |
| Powerline ribbon, multi-line frame, ruler, right prompt (`RPROMPT`) | ✅ |
| Transient prompt — four modes (`off` / `always` / `same-dir` / `unique-dir`) | ✅ (unique-dir aliased to same-dir) |
| Instant prompt (sub-ms first shell via cached `dump.zsh`; per-`$TERM` cache key) | ✅ |
| `gitstatusd` long-lived daemon over FIFOs (ADR-0001 hot path) | ✅ |
| `gitstatusd` sha256-pinning + `p10k-rs verify` (T0.5) | ✅ |
| `git` shell-out fallback with 2-second timeout, hardened env | ✅ |
| `gix-status` pure-Rust fallback (no `git` on PATH) | ⏳ design-doc only; queued for v0.1.6 |
| Branch / cwd render-path sanitization (`SafeText`: BiDi, ZWJ, control bytes, NFC, grapheme-safe truncation) | ✅ |
| `p10k-rs import ~/.p10k.zsh` (Powerlevel9k importer) | ✅ |
| `show_on_command` segment gating (live `$BUFFER` via ZLE) | ✅ zsh |
| `show_in_dir` / `disabled_dir_pattern` segment gating | ✅ |
| AI-host detection + OSC 7/133 emission | ✅ |
| Claude Code statusline render path | ✅ |
| Cursor / Aider / Goose statusline contracts | ⏳ stub; needs documented protocols |
| `p10k-rs configure` wizard (TUI) | ⏳ stub |
| `bash` init script (no RPROMPT, no preexec timing, no gitstatusd, no transient) | ⚠️ best-effort |
| `fish` init script | ⏳ stub |
| mdBook documentation site (source: [docs/src/](docs/src/SUMMARY.md)) | ⚠️ source ready; publishing blocked on a one-click GitHub Pages source flip |

## Workspace layout

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import (data only)
  p10k-rs-segments   # segment implementations (31 segments)
  p10k-rs-git        # gitstatusd client + git shell-out fallback + sha-pins
  p10k-rs-jj         # Jujutsu VCS detection + state producer
  p10k-rs-shell      # per-shell init scripts (zsh end-to-end; bash best-effort; fish stub)
  p10k-rs-wizard     # `configure` TUI (stub)
  p10k-rs-ai         # AI-host detection + OSC 7/133 emission + statusline
  p10k-rs-ipc        # FIFO plumbing
```

Per-crate detail in the [architecture chapter](docs/src/arch/crates.md)
and [docs/adr/](docs/adr/).

## What's next

- **v0.1.6 candidate slate:** slice 60 (gix-status fallback), slice 64 (daemon respawn), real `UniqueDir` history, per-host statusline contracts. See [`~/.planning/powerlevel10k-rs/NEXT-STEPS.md`](https://github.com/scurran1986/powerlevel10k-rs) (project-local; not in the repo).
- **v1.0 stability commitment:** see the [`docs/adr/`](docs/adr/) index and the ROADMAP doc in the planning tree.
