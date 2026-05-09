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

### Added
- Workspace scaffold: eight crates wired through `[workspace.dependencies]` with centralised pinning, lints, release profile, rustfmt/clippy/cargo-deny/dependabot.
- CI: fmt, clippy, test, doc, and cargo-deny on stable Rust across ubuntu-latest and macos-latest.
- ADR index: ADR-0001 (Git Status Backend) accepted 2026-05-06 after day-1 spike.
- `install.sh`: one-shot build + install + zsh rc wire-up.
