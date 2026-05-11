# Changelog

All notable changes to `p10k-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 minor bumps may be breaking; breakage is documented when it occurs.

## [0.1.0] - 2026-05-11

First tagged release. Feature-complete for the MVP segment surface and
the ROADMAP Phase 6 release gates: 21 configurable segments, three
colour modes with Powerlevel9k-compat names, Powerlevel10k importer,
zsh + bash + fish init scripts, configure wizard, multi-arch binaries
on tag push. See `.github/release-notes/v0.1.0.md` for the user-facing
write-up; the section below documents every slice that landed.

### Slice 21: Multi-arch release workflow (315354b)
- New `.github/workflows/release.yml` runs on semver tag push (`v*`). Matrix
  builds release binaries for `x86_64-unknown-linux-gnu` (native),
  `aarch64-unknown-linux-gnu` (cross via `gcc-aarch64-linux-gnu`),
  `x86_64-apple-darwin` (macos-13), and `aarch64-apple-darwin` (macos-14).
- Each binary is stripped, tarred with `LICENSE-*`, `README.md`, and
  `THIRD-PARTY-LICENSES.md`, sha256summed, and uploaded as a release
  asset via `softprops/action-gh-release@v2`.

### Slice 20: Configure wizard (c66cd0d)
- New `p10k-rs-wizard` crate; `p10k-rs configure` now produces a real
  three-question Q&A flow (style preset / glyph mode / colour palette)
  instead of bailing.
- Driver core takes `BufRead + Write` so the flow is fully unit-testable
  without a real terminal — 7 tests cover defaults, every preset path,
  invalid-input retries, EOF→Cancelled, and `to_toml`/`from_toml` round-trip.
- Output is TOML on stdout for the `>~/.config/p10k-rs/config.toml`
  redirect pattern; prompts on stderr.
- Dropped `crossterm` and `tracing` from wizard deps for the MVP — the
  30-screen raw-mode TUI from `06-wizard-and-presets.md` lands when it lands.

### Slice 19: bash and fish init scripts (fa8541c)
- `p10k-rs init bash` emits a `PROMPT_COMMAND`-driven hook that calls the
  binary with `$?`. No timing (bash lacks a clean preexec; `trap DEBUG`
  interacts badly with completion), no gitstatusd FIFO orchestration
  (zsh-specific), idempotent via `_P10K_RS_INSTALLED` sentinel.
- `p10k-rs init fish` emits `fish_prompt` + an `--on-event fish_preexec`
  handler. Has timing via `date +%s%3N` (falls back to second resolution
  on systems without GNU coreutils).
- `p10k_rs_shell::init_script` becomes infallible; drops the
  `InitScriptUnimplemented` error type.

### Slice 18: Powerlevel9k importer (932ddbb)
- New `p10k_rs_config::import` module. `p10k-rs import ~/.p10k.zsh` reads
  a P9k zsh config and emits equivalent TOML to stdout. The importer
  never executes the input — pure textual translation.
- Coverage: layout arrays (`LEFT_PROMPT_ELEMENTS` / `RIGHT_PROMPT_ELEMENTS`,
  filters `=newline` pseudo-elements), `POWERLEVEL9K_MODE`,
  `POWERLEVEL9K_INSTANT_PROMPT`, per-segment and per-state foreground /
  background. Colour values: indexed (0–255), named, or `#rrggbb` hex.
- Longest-prefix match against a snapshot of `segment_names()`
  disambiguates `vcs_clean` (state) from `command_execution_time`
  (multi-word segment) cleanly. Cross-crate test
  `importer_known_segment_names_match` keeps the lists in sync.
- New `Config::to_toml(&self)` serialiser used by both `import` and
  `configure`.
- Unrecognised variables go to stderr with the original key; stdout is
  pure TOML.

### Slice 17: Finish the MVP segment set (aa08061)
- Implements the last 8 segments from `MVP-SPEC.md` § 1.2: `time` (local
  with UTC fallback via the `time` crate's `local-offset` feature),
  `context` (user@host with root/ssh/normal state tags), `vi_mode`
  (reads `$P10K_RS_VI_MODE` for now — zsh `zle-keymap-select` plumbing
  lands later), `kubecontext` (hand-parses `current-context:` from
  `~/.kube/config` or `$KUBECONFIG`), `terraform` (walks for
  `.terraform/environment` or reads `$TF_WORKSPACE`), `node_version`,
  `python_version`, `rust_version` (each spawns a subprocess when its
  cwd-marker file exists in any parent dir up to depth 64).
- Workspace deps: `time = "0.3"` with `formatting` / `local-offset` /
  `macros`; `rustix` gains the `system` feature for `uname()`.
- All 21 segments resolve via `p10k_rs_segments::build()`.

### Slice 16: env-driven segments (f9cc420)
- 4 new segments mirroring the `virtualenv` template: `aws` (probes
  `AWS_VAULT` / `AWS_PROFILE` / `AWS_DEFAULT_PROFILE` in order),
  `pyenv` (reads `PYENV_VERSION`), `nodenv` (reads `NODENV_VERSION`),
  `anaconda` (reads `CONDA_DEFAULT_ENV`; takes basename when activated
  via `-p <path>`).
- Each factors the env read into a private helper; the pure
  sanitisation helpers are unit-tested without env mutation.

### Slice 15: four new segments (79334cd)
- New segments: `background_jobs` (`ctx.jobs > 0` → `⚙N` in cyan),
  `root_indicator` (`geteuid() == 0` → `⚡` in red),
  `virtualenv` (`$VIRTUAL_ENV` set → basename in yellow),
  `os_icon` (Nerd Font codepoints per `target_os`, `?` fallback).
- `rustix` added to segments crate deps for the EUID query.

### Slice 14: per-segment styling threads through render (c875ea2)
- `p10k_rs_core::style` grows from a stub into the styling chokepoint:
  `render_fg` / `render_bg` resolve `[segment.<name>].states.<state>`
  → `[segment.<name>]` → default and emit the SGR escape under the
  active `ColorMode`.
- 16 P9k-compatible named colours; `Color::Indexed(0..=255)` and
  `Color::Rgb([r,g,b])` lower correctly under each `ColorMode`
  (truecolor passthrough, Ansi256 cube quantisation, Ansi8 3-bit cube).
- All 5 existing segments refactored to call `style::render_fg` /
  `style::reset_fg` instead of writing raw escapes. Marker colour in
  `vcs` (the `*` / `!`) stays hardcoded red — single-fg-per-state config
  can't distinguish branch from marker.
- End-to-end integration test
  `segment_foreground_override_reaches_render` proves a TOML override
  actually flips the emitted SGR.

### Slice 13.5: delete default_layout() (9fc45c0)
- `p10k_rs_segments::default_layout()` had zero callers after slice 13
  — `cmd_prompt` already assembled segments from `cfg.layout.left`.
  Function removed; docstrings + README + workspace CLAUDE.md updated.
- `factory_default_config()` in `main.rs` remains as the no-config-file
  fallback.

### Slice 13: TOML config loader (0a74ee6)
- `cmd_prompt` reads `Config::load_default()` and assembles segments
  via `p10k_rs_segments::build()` from the parsed `layout.left`.
- Discovery order: `$P10K_RS_CONFIG`, `$XDG_CONFIG_HOME/p10k-rs/config.toml`,
  `~/.config/p10k-rs/config.toml`. Missing or broken file falls back
  silently to the factory-default TOML; the binary's no-config behaviour
  is byte-identical to the pre-loader prompt.
- `Config::sanitize_in_place` runs at parse time over every prompt-bound
  string (separators, icons, frame glyphs) so the renderer can hand
  imported values straight through `SafeText`.

### Slice 12-b: SafeText newtype (1c8f80b)
- New `p10k_rs_core::safety::SafeText`: a `String` wrapper whose
  constructors run `sanitize_for_terminal`. Producers can't bypass it
  (no `assume_safe` escape hatch); consumers see an always-safe `&str`.
- `GitState::branch` and `GitState::commit` are now `SafeText` —
  producers in `p10k-rs-git` allocate via `SafeText::from_untrusted_bytes`
  at the wire-format boundary. Encodes the slice-11 invariant in the
  type system.

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
