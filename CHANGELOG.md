# Changelog

All notable changes to `p10k-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 minor bumps may be breaking; breakage is documented when it occurs.

## [Unreleased]

### Slice 1: Minimum runnable prompt (acdfb4a)
- Wired `dir` and `prompt_char` segments end-to-end; rendered prompt via `p10k-rs prompt` after `eval "$(p10k-rs init zsh)"`.
- `RenderCtx` no longer `#[non_exhaustive]`; init script embeds absolute binary path to avoid PATH pollution.

### Slice 2: ANSI colors with zsh-aware %{…%} (f075385)
- Segments now emit ANSI SGR escapes (blue cwd, green chevron); post-processed for zsh via `%{…%}` wrapping so prompt width tracking works.
- Hand-rolled UTF-8 state machine scan with char-boundary awareness for escape bracketing.

### Slice 3: Red prompt_char on non-zero $? (6c7b501)
- Added `--last-status N` arg; zsh precmd captures exit code and forwards it; chevron turns red on failure, green on success.
- State tag logic laid groundwork for per-state config overrides in later slices.

### Slice 4: VCS segment via git shell-out (17a921e)
- New `vcs` segment shows branch name + dirty marker; `git status --porcelain=v1 --branch` backend.
- `p10k-rs-git` introduces `Backend` trait; `ShellOut` impl handles six observed branch-header forms.
- Default layout: `[dir, vcs, prompt_char]`.

### Slice 5: command_execution_time segment (00fd3f1)
- Slow commands (≥ 3s) now display duration in cyan between vcs and chevron (e.g., `main 7s ❯`).
- zsh `preexec`/`precmd` hooks record and forward elapsed time via `--last-duration-ms`.
- Default layout: `[dir, vcs, command_execution_time, prompt_char]`.

### Slice 6: gitstatusd long-lived daemon (16ad060)
- Swapped vcs backend from slow shell-out to long-lived gitstatusd subprocess per ADR-0001.
- Daemon spawned once per shell session; `p10k-rs-git::Gitstatusd` client marshals wire protocol (request/response with `\x1F`/`\x1E` delimiters).
- vcs segment marked non-fast (slow shell-out only); flips back to fast on daemon upgrade in slice 7.

### Slice 7: Harden gitstatusd + rich vcs render (a96c7a1)
- 2-second poll(2) timeout on daemon response FIFO; silent fallback to ShellOut on wedge.
- Auto-respawn logic: zsh precmd kills -0 the daemon PID; respawns if dead to recover from crash.
- Rich vcs render: GitState now tracks ahead/behind/staged/unstaged/untracked/conflicts/commit; vcs segment displays `branch [+N] [-N] [marker]`.
- State tags: `clean`, `dirty`, `conflict`, `diverged`.

### Slice 8: Instant prompt — sub-millisecond first shell (bffb9ae)
- New `--dump <PATH>` arg writes rendered PROMPT to `${XDG_CACHE_HOME:-$HOME/.cache}/p10k-rs/dump-$USER.zsh` after every render.
- zsh init script sources the dump at the top (before daemon spawn, before hooks) so first shell renders PROMPT immediately.
- First real precmd overwrites with fresh render; masks ~2s cold gitstatusd cache hit on kernel-sized repos.

### Slice 9: Triage — close ADR-0001 follow-ups, doc refresh, FIFO security (de0072c)
- Removed `crates/spike-gitstatus`; stripped the `gix`, `bincode`, `humantime`, `tempfile` workspace deps it pulled in.
- Removed phantom `p10k-rs-git` dep from `p10k-rs-segments` and unused `tracing` from `p10k-rs-core`.
- FIFO security hardening: `mktemp -d` for unpredictable run dirs, `chmod 0700` parent + `mkfifo -m 0600` under `umask 077`, `_p10k_rs_stop_daemon` validates the dir template before `rm -rf`, `is_fifo` uses `symlink_metadata` + UID ownership check.
- `cmd_init` shell-literal escape now rejects every byte < 0x20, byte 0x7F (DEL), and single quote — was previously single-quote only.
- `init_tracing` early-returns when `RUST_LOG` is unset (saves ~100-300 µs per silent-path prompt invocation).
- `locate_binary` no longer probes the dev-machine `/home/seaburdz/...` path; install.sh symlinks a found `gitstatusd` next to the binary.
- README, CHANGELOG, ADR-0001 follow-ups refreshed; new `THIRD-PARTY-LICENSES.md` documents the GPL-3.0 `gitstatusd` bundle per ADR-0001 § Operational.

### Slice 10: status segment — exit code shown red on non-zero $? (d99a514)
- New `Status` segment renders `✘<code>` in red between `command_execution_time` and `prompt_char` when `$?` is non-zero. Hidden on success.
- Default layout: `[dir, vcs, command_execution_time, status, prompt_char]`.
- Pure additive — last-status plumbing was already in place from slice 3.

### Slice 11: harden render path against %-expansion + ANSI injection (e657779)
- Closes two CRITICAL findings from the prior audit cycle (`.review/20260509T130000Z/02-render-injection.md`). Both reproducible in 30 s against the pre-fix binary; both neutralised here.
- **C1 (zsh PROMPT %-expansion via untrusted branch / cwd):** `wrap_for_shell` now doubles every literal `%` to `%%` in the zsh case, alongside its existing `%{ }` SGR wrapping. SGR escape bodies emitted by segments contain no literal `%`, so the doubling pass only fires on text content. Bash and fish are pass-through unchanged.
- **C2 (terminal-escape injection via untrusted cwd):** new `p10k-rs-core::safety::sanitize_for_terminal` strips every Unicode control codepoint (`is_control()`) except `\t`, plus `\x7F`. Applied at three boundaries: `gitstatusd::parse_response` (branch + commit fields), `git::parse_branch_header` (porcelain backend), `segments::dir::Dir::render` (cwd display).
- `gitstatusd::parse_response` switches from `from_utf8` to `from_utf8_lossy`, closing M2 (silent-empty on non-UTF-8 branch names).
- H1 (instant-prompt dump persists C1 across shell restarts) closes automatically: the dump file writes whatever `wrap_for_shell` produced, so the doubled `%%` survives across shell-restart sourcing.
- 53 tests pass workspace-wide (was 33). End-to-end reproducer rerun against the rebuilt binary confirms `%n@%m` → `%%n@%%m`, OSC `\x1b]…\x07` stripped, CR overwrite stripped.

### Slice 12-a: doc / changelog hygiene (this commit)
- Backfilled CHANGELOG entries for slices 9, 10, 11.
- README rewritten with single-command quickstart and current 11-slice list (separate commit `cab03c8`).
- CONTRIBUTING.md MSRV: 1.84 → 1.88 (matches `rust-toolchain.toml`).
- `init.zsh`: removed the "Slice 2 escapes them" comment that slice 11 just satisfied; rewritten as a description of what the surrounding glue actually does.
- Stripped 21 stale slice-number comments from source — readability + documentation lanes flagged forecasts and history that rot. Concrete present-tense descriptions replace them.

### Added
- Workspace scaffold: eight crates wired through `[workspace.dependencies]` with centralised pinning, lints, release profile, rustfmt/clippy/cargo-deny/dependabot.
- CI: fmt, clippy, test, doc, and cargo-deny on stable Rust across ubuntu-latest and macos-latest.
- ADR index: ADR-0001 (Git Status Backend) accepted 2026-05-06 after day-1 spike.
- `install.sh`: one-shot build + install + zsh rc wire-up.
