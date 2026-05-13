# Changelog

All notable changes to `p10k-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 minor bumps may be breaking; breakage is documented when it occurs.

## [0.1.1] - 2026-05-12

Powerline ribbon + multi-line frame + 31 segments + dual-sided layout.
The prompt now visibly matches upstream Powerlevel10k out of the box.
See `.github/release-notes/v0.1.1.md` for the user-facing write-up.

### Slices 22–25 (carried from prev. changelog)
- `e136d9f` slice 22: per-segment `padding` + `separators.left`.
- `93fc850` slice 23: default Nerd Font icons (5 segments) + `icon`
  override.
- `e249584` slice 24: per-state `icon` override + `style::resolve_icon`.
- `6c4086e` slice 25: `[layout.frame]` + `[layout.ruler]`.

### Slice 26 (1c1bffc inclusive) — 8 more default icons
- `python_version`, `node_version`, `rust_version`, `terraform`,
  `pyenv`, `nodenv`, `anaconda`, `virtualenv`. Slice 31 closed the
  remaining 6, completing the 21-segment icon sweep.

### Slice 27 (5157245) — `[segment.<name>].disabled` gate
- Silent skip inside `assemble_segments` (no warning — the user
  opted in). `assemble_segments` signature gained `&Config`.

### Slice 28 (1c1bffc) — powerline ribbon + multi-line frame
- **The big visual slice.** `SegmentOutput` gained
  `background: Option<style::Color>` and `#[derive(Default)]`.
- `render_prompt` emits powerline `\u{e0b0}` arrows between adjacent
  bg-bearing segments and a closing arrow into terminal default.
  Multi-line `prompt_char` on line 2 behind a `╰─` corner when
  `frame.glyph` is set.
- Per-segment palette across 21 segments via four parallel agents.

### Slice 29 (65df220) — `ioctl(TIOCGWINSZ)` for terminal width
- Replaces `$COLUMNS` env-var probe. Probe order: stdout → stderr →
  stdin → `$COLUMNS` → 80. `rustix::termios::tcgetwinsize`, no raw
  ioctl. Second I/O exception in `p10k-rs-core` (first was env read).

### Slice 30 (8b4878f) — vcs richer markers
- Surfaces ahead/behind/staged/unstaged/untracked/conflicts indicators:
  `<branch> ⇡<N> ⇣<N> *<dirty> !<conflicts> +<staged> ~<unstaged>
  ?<untracked>`. Bare `*` suppressed when any count is present.

### Slice 31 (840ea8a) — last 6 default icons
- `command_execution_time`, `status`, `vi_mode`, `background_jobs`,
  `root_indicator`, `context`. Icon coverage hits 21/21.

### Slice 32 (a8a2570) — `show_in_dir` + `disabled_dir_pattern`
- Cwd-driven gates inside `assemble_segments`. `globset` 0.4 added
  (binary crate only). `Glob` type stays a transparent newtype.

### Slice 33 (707f552) — `layout.right` / RPROMPT
- `render_prompt` signature gained both left and right segment lists;
  new `render_right` helper emits left-pointing `\u{e0b2}` arrows
  with mirrored fg/bg colouring.
- Binary's `--render-side <left|right>` flag (slice 35 added `transient`).
- zsh init invokes the binary twice per prompt — once for `PROMPT`,
  once for `RPROMPT`.

### Slice 34 (7ed03c8) — status + vi_mode state-aware
- `status` sets `state: Some("error")` so per-state TOML overrides
  fire.
- `vi_mode` palette by current mode: `command` blue/white, `insert`
  green/black, `visual` yellow/black, `replace` red/white.

### Slice 35 (68ef454) — transient_prompt
- zsh `zle-line-finish` widget swaps PROMPT for a lone `❯` after
  every accepted command. `--render-side transient` value added.
  `Off` mode suppresses to empty.

### Slice 36 (dc6ad36) — dir truncation `to_last` + `middle`
- New `SegmentConfig.truncate: DirTruncate` field with strategy +
  length. `DirTruncate` and `DirTruncateStrategy` are
  `#[non_exhaustive]`.

### Slice 37 (bb657ae) — `context` user@host
- Renders `<user>@<host>` with upstream visibility rules: hidden
  when `$P10K_RS_DEFAULT_USER == username` AND local; always shown
  for root or remote. States: `root` red/white, `remote` yellow/black,
  `local` yellow/black.

### Slices 38 + 40 (093e76b) — factory default + `frame.bottom_glyph`
- `factory_default_config()` ships the P10K-classic two-sided layout.
- `FrameStyle` gained `bottom_glyph: Option<String>` (default `╰─`).
  Sanitised by `Config::sanitize_in_place`.

### Slice 39 (1e1034d) — dir `not_writable` state
- Probes `rustix::fs::access(cwd, Access::WRITE_OK)`. Any errno
  collapses to `not_writable`. State drives bg=yellow / fg=black
  with the `\u{f023}` padlock icon.

### Slice 41 (7e0aa2b) — bash init script
- Promoted from stub to PROMPT_COMMAND-based PS1 build. Bash 3.x–5.x
  compatible. Documented gaps: RPROMPT, command_execution_time,
  gitstatusd, transient, instant-prompt.

### Slice 42 (1332168) — `os_icon` distro detection
- WSL (via `/proc/version`) → Linux `/etc/os-release` ID lookup →
  macOS Apple → *BSDs daemon → generic. Cached in `OnceLock`.

### Slice 43 (e132e1a) — dir `truncate_to_unique`
- Filesystem-aware sibling-prefix shortening via `std::fs::read_dir`,
  capped at 200 entries per parent. IO failure falls back to
  first-char. Opt-in (cost vs `to_last` / `middle`).

### Slices 44 + 45 (9edfc5b) — `show_on_command` + vcs stash + action
- `RenderCtx.upcoming_command: &'a str`. Binary takes
  `--upcoming-command <STRING>`. zsh init captures `$1` in preexec
  and passes the LAST command — documented honest-gap vs upstream's
  "upcoming" semantic.
- vcs `GitState` gained `stash: u32` + `action: SafeText`. Daemon
  parser reads wire fields 16 (stash) and 8 (action). `p10k-rs-git`
  gained `detect_action(git_dir)` for the shellout backend.
- vcs render: appends `<ACTION>` (red, uppercase) and `≡<stash>`
  after the slice 30 indicators.

### Slices 46 + 47 + HEAD unbreak (4089a01) — `ai_host` + vcs detached
- `HostKind` expanded to `{ None, ClaudeCode, Aider, Cursor }`,
  moved from `p10k-rs-ai` to `p10k-rs-core` so `RenderCtx` can
  carry it without a cycle.
- `p10k-rs-ai::detect_host_kind()` reads `$CLAUDECODE`, `$AIDER_*`,
  `$CURSOR_*`.
- New `ai_host` segment renders the host label
  (`claude-code`/`aider`/`cursor`) in white-on-magenta.
- vcs detached-HEAD: emit short-SHA when `branch == "HEAD"` or
  empty. Tag display: ` @ <tag>` when non-empty.
- HEAD unbreak: dropped a `#[non_exhaustive]` struct-expression in
  the slice 43 test helper; `render_prompt` extracted helpers;
  `render_transient` return type cleaned.

### Slices 48–50 (355e6fe) — `mise`/`rtx`, `fnm`, `pixi`, `docker_context`
- Four new segments closing upstream `#2212`, `#713`, `#2798`, `#1485`.
- `mise` gated on `$MISE_DATA_DIR`; `rtx` accepted as alias.
- `fnm` reads `$FNM_NODE_VERSION`.
- `pixi` reads `$PIXI_PROJECT_NAME`.
- `docker_context` precedence: `$DOCKER_CONTEXT` → `$DOCKER_HOST` →
  `~/.docker/config.json`. Hides on `default`.

### Slices 52 + 55 (20854bf) — `jj` VCS + OSC 7 / OSC 133 emission
- **New sibling crate `p10k-rs-jj`.** `detect_jj(cwd)` walks up to
  64 levels for `.jj/`, shells out to `jj log -T '<pipe-template>'`
  + `jj status`. All bytes via `SafeText::from_untrusted`.
- New `JjState` in `p10k-rs-core`; `RenderCtx` gains
  `jj: Option<&'a JjState>`.
- New `jj` segment renders bookmark (or 8-char change_id) on
  green/black; dirty `*` painted red.
- OSC 7 (cwd reporting) + OSC 133 A/B/C/D (shell-integration
  command boundaries) emit when `HostKind != None`. `wrap_for_shell`
  extended to bracket OSC sequences in zsh mode.

### Slice 54 (4433eff) — mdBook docs + GH Pages workflow
- `docs/book.toml` + 11-chapter `SUMMARY.md`.
- `.github/workflows/docs.yml` builds + publishes via
  `actions/deploy-pages@v4`. One manual step: enable Pages
  "GitHub Actions" source.

### Slice 56 (ef84470) — `layout.separators.right` + `.subsegment`
- `render_right` now reads `separators.right` (falling back to
  `.left` then `" "`).
- `vcs` is the first real `separators.subsegment` consumer.

### Slices 51 + 53 (23754af) — subprocess timeout + OSC 4 follow-terminal
- New `p10k-rs-core::proc::output_with_deadline` (500 ms budget for
  version segments — addresses upstream `#2860` freeze class).
- New `ColorMode::FollowTerminal` + `p10k-rs-core::term_query` —
  OSC 4 probe with `OnceLock` cache + 800 ms total budget.

### Slice 57 (90dcc01) — `layout.left_top_only` / `right_top_only`
- Promoted from `bool` placeholder to `Vec<SegmentRef>`. Segments
  listed render on line 1; the rest fall to line 2 behind the
  bottom-corner frame. `right_top_only` reserved (RPROMPT is
  single-line today).
- `render_prompt` partitions `enabled` segments into (line1, line2)
  via a new `append_ribbon` helper.

### fix (6cdc0dc) — vcs tag field off-by-one
- Slice 47's daemon parser read tag at 0-indexed slot 18; per
  `07-gitstatus.md` § 1.3 it's wire field 18 ONE-INDEXED → slot 17.
  Bogus `@ 0` was rendering because we were reading
  `num_unstaged_deleted` and labelling it "tag".

### chore (40abc0e) — clippy cleanup
- Workspace `[lints.clippy]` excludes `similar_names` — `bg`/`fg`
  is the natural pair across every prompt segment.
- `vcs::render` dropped from 135 to under 100 lines via
  `paint_alarm_spans` + `append_index_indicators` helper extractions.
- Test modules across 7 segment files gained the conventional
  `#[allow(clippy::expect_used, unwrap_used, panic)]` scope.
- `context.rs` collapsed a `match` to `matches!`; `dir.rs::
  truncate_path` dropped a redundant explicit `None` arm.

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
