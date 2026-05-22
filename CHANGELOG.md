# Changelog

All notable changes to `p10k-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 minor bumps may be breaking; breakage is documented when it occurs.

## [Unreleased]

Reserved for v0.1.7. Carry-overs:

- **Slice 60 follow-up phases:** 3.5 (per-category counts:
  staged/unstaged/untracked/conflicts), 4 (ahead/behind via gix
  revwalk — hit API discovery friction during v0.1.6), 6
  (cross-check tests + bench against ShellOut).
- **Slice 64** — daemon-respawn / health-check cache (ADR-0001).

## [0.1.6] - 2026-05-22

Theme: **pure-Rust git-status fallback (slice 60).** When neither
the `gitstatusd` daemon nor a system `git` binary is available,
the prompt now falls back to gitoxide-backed status reporting at
ShellOut parity: branch name, dirty indicator, and in-progress
action (merge/rebase/cherry-pick/revert/bisect). The intended
user is anyone running a shell in a stripped container — AI host
images (Claude Code, Cursor), CI runners that drop `git` to save
space — where the prompt would previously silently lose its VCS
indicator.

Slice 60 design split into 6 phases. v0.1.6 ships **phases 1,
2, 3, 5** (the field-coverage equivalents to ShellOut). Phases
3.5 (per-category counts), 4 (ahead/behind), and 6 (cross-check
+ bench) are deferred to v0.1.7 — both because the gix
`revision`-feature API needs more probing than fit a clean v0.1.6
boundary, and because shipping ShellOut-parity is the meaningful
user-facing milestone on its own.

Test count: **544 passing**, 3 ignored (up from 537 at v0.1.5).

### Slice 60 phases (4 commits)

- `0ad2db1` **slice 60 phase 1** `feat(git)`: scaffold
  `GixBackend` in the fallback chain. The pure-stub returns
  `None`; behaviour is byte-identical to pre-slice-60 until
  phases 2-5 populate fields.
- `515d47c` **slice 60 phase 2** `feat(git)`: branch + HEAD
  lookup via `gix::discover` + `repo.head_name()`. Adds the
  `gix = "=0.83.0"` workspace dep with
  `default-features = false, features = ["sha1"]` as the minimal
  hash-backend enable. Empirical research flipped the design
  doc's scoped-vs-umbrella recommendation: scoped subcrates
  would have required a `gix-hash` workaround AND blow past +53
  packages; umbrella matches the count without the workaround
  and gives the high-level API. Branch bytes flow through
  `SafeText::from_untrusted_bytes` →
  `from_untrusted_with_cap(4 KiB)` for render-path sanitisation
  + defensive length cap.
- `b2a720f` **slice 60 phase 3** `feat(git)`: `dirty: bool` via
  gix's working-tree status iterator
  (`repo.status(...).into_index_worktree_iter(...)`). Coverage
  matches ShellOut exactly (any modification, untracked file, or
  conflict collapses to `dirty=true`). Adds `status` feature to
  the gix dep.
- `56a5612` **slice 60 phase 5** `feat(git)`: in-progress action
  probe via `crate::detect_action(repo.git_dir())` — reuses the
  filesystem sentinel scanner from the ShellOut path. Zero new
  deps. Output values are consistent across all three backends.

### Notes

- **Two RUSTSEC advisories on initial gix bring-up.** Pinning
  `gix = "0.66"` (the design doc's reference) triggered
  RUSTSEC-2025-0140 (gix-date `TimeBuf::as_str` non-UTF-8) and
  RUSTSEC-2025-0021 (gix-features SHA-1 collision detection
  missing). Both fixed in gix 0.83+; the workspace pins
  `=0.83.0` exact. `cargo deny check` clean.
- **Workspace package count grew 112 → 240** (+128 packages).
  Higher than the slice-60 design doc's "~12 scoped" estimate;
  reality is the scoped subset is no longer ~12 either (+53
  minimum). The minimum-features umbrella path is the floor.
- **Phases 4 (ahead/behind) and 3.5 (per-category counts)
  deferred** to v0.1.7. Phase 4 hit API discovery friction —
  `head_id.ahead_behind(upstream_id)` from the design doc
  doesn't exist on gix 0.83's `Id`; needs more probing.

## [0.1.5] - 2026-05-21

Theme: **gitstatusd supply-chain pinning (T0.5)** — the last open
Tier-0 security item closes. `install.sh` now defaults to a
pinned download + sha256 verification of the gitstatusd helper,
and a new `p10k-rs verify` subcommand lets users re-run the same
comparison at any time. SECURITY.md documents the verification
recipe downstream packagers can mirror, and a weekly CI workflow
opens a PR when upstream publishes a newer tag with per-triple
binaries.

Slice 60 (gix-status fallback) and slice 64 (daemon respawn) remain
design-doc-only and are targeted for v0.1.6.

Test count: **537 passing**, 3 ignored (up from 527 at v0.1.4).

### Supply-chain hardening (T0.5)

- `356b509` **T0.5** `feat(install)`: install.sh defaults to the
  new `pinned` gitstatusd acquisition mode. Detects host triple,
  downloads `gitstatusd-<triple>.tar.gz` from
  `https://github.com/romkatv/gitstatus/releases/download/<version>/`,
  verifies the tarball sha256 against the committed pin in
  `crates/p10k-rs-git/data/gitstatusd-pins.toml`, and only then
  extracts the binary into the install prefix. New flags:
  `--gitstatusd=pinned` (default), `--gitstatusd=system` (legacy
  symlink path for users who explicitly want their brew/apt copy),
  `--gitstatusd=none`. Every failure path warns and falls back to
  the ShellOut runtime path — install never hard-fails on a
  missing optional perf optimisation. Pin entries cover all four
  release triples (`x86_64-linux-gnu`, `aarch64-linux-gnu`,
  `x86_64-darwin`, `aarch64-darwin`).
- `1ef1d68` **T0.5** `feat(cli)`: `p10k-rs verify` subcommand.
  Reads the embedded pin file, locates gitstatusd via the same
  `$P10K_RS_GITSTATUSD_BIN` → `$PATH` probe the prompt path uses,
  hashes the on-disk binary, and prints a stable wire line with
  distinct exit codes: `OK <triple> <version> <sha-prefix>`
  (exit 0), `MISMATCH expected=<hex> got=<hex>` (exit 2),
  `NOT_FOUND <reason>` (exit 3), `UNSUPPORTED_ARCH <triple>`
  (exit 4). New `p10k_rs_git::pins` module is the typed view over
  the pin TOML; shared between install.sh (via `awk`) and the
  verify command (via `include_str!`). Adds the `sha2` workspace
  dep (RustCrypto, small transitive surface, MSRV-clean at 1.88).
  10 new tests.
- `15171f7` **T0.5** `ci(pins)`: weekly upstream pin-probe
  workflow at `.github/workflows/pin-gitstatusd.yml`. Mondays at
  14:00 UTC: probes `romkatv/gitstatus` for new releases, checks
  per-triple binary attachment (releases that ship signatures
  only — as upstream v1.5.5 did — are detected via `curl -fsI` and
  skipped), and opens a PR updating `gitstatusd-pins.toml` when a
  usable newer tag is available. Plain curl + sha256sum + gh; no
  third-party actions beyond `actions/checkout`.
- `57ad775` **T0.5** `docs(security)`: SECURITY.md "Verifying the
  gitstatusd helper" section. Documents the `p10k-rs verify` wire
  format, a reproducible bash recipe downstream packagers
  (homebrew / AUR / nixpkgs / distro maintainers) can mirror, the
  weekly bump cadence, and the warn-and-fallback failure
  semantics.

### Notes

- **Pinned version is v1.5.4, not v1.5.5.** Upstream's v1.5.5
  GitHub release ships only `.asc` signature files; the per-triple
  binaries we need still point at v1.5.4 (matches what brew / apt /
  nix all ship as of 2026-05-21). The pin-probe workflow will
  surface a v1.5.6 (or a late-uploaded v1.5.5 binary set) the
  moment upstream publishes one.

## [0.1.4] - 2026-05-20

Post-v0.1.3 slice ledger. Ships 4 of 5 sprint-memo v0.2-deferred
items as code (T1.8, T1.10, T1.11, T1.24) plus two ROADMAP
suggested-next slices (58, 63), the slice 61 follow-up (AI
statusline render now wired), and an mdBook docs audit. The
remaining T0.5 (gitstatusd sha256-pin) and slice 60 (gix-status
correctness fallback) shipped as **design docs** in
`~/.planning/powerlevel10k-rs/research/`; T0.5 implementation
lands in v0.1.5. Notes at `.github/release-notes/v0.1.4.md`.

Test count: **527 passing**, 3 ignored (up from 368 at v0.1.3).

### Render-layer Unicode safety (T1.10, T1.11)

- `75b7c92` **T1.10** `feat(safety)`: `SafeText` now strips
  Unicode-class hazards (BiDi controls, ZWJ/ZWNJ, format
  characters, deprecated tags), normalises to NFC, and truncates
  on grapheme-cluster boundaries via `from_untrusted_with_cap`.
  Closes the BiDi/ZWJ class of render-path injection the
  forward-research threat model flagged.
- `d266a5e` **T1.11** `refactor(segments)`: ten segment helpers
  that read user-controlled env vars now return `Option<SafeText>`
  instead of `Option<String>` — `anaconda`, `aws`, `docker_context`,
  `fnm`, `kubecontext`, `nodenv`, `pixi`, `pyenv`, `terraform`,
  `virtualenv`. Behaviour unchanged (`SafeText: Display`); the
  type-system invariant "this value is sanitised" is now
  load-bearing at the helper boundary, not by convention.

### Transient prompt modes wired (T1.8)

- `86fcd6d` **T1.8** `feat(transient)`: the four-mode
  `TransientPromptMode` enum (`Off` / `Always` / `SameDir` /
  `UniqueDir`) is now honestly differentiated at the wire. New
  exit-code-2 protocol between the binary and the zsh init lets
  `same-dir` / `unique-dir` keep the full ribbon in scrollback
  when the cwd-compare fails. `Off` behaviour byte-identical to
  pre-T1.8. `UniqueDir` aliased to `SameDir` until cross-prompt
  history lands. 8 new unit tests cover the truth table.

### Truecolor hex literals (T1.24)

- `aa323f6` **T1.24** `feat(config)`: TOML colour values now accept
  `#rgb` (shorthand) and `#rrggbb` (full) hex literals alongside
  the existing name, integer-index, and `[r, g, b]`-array forms.
  Shorthand expands per CSS convention (`#f60` ≡ `#ff6600`). The
  truecolor SGR emission path was already wired since slice 25-era;
  T1.24 closes the user-facing ergonomic gap with a custom serde
  `Visitor` and 10 new tests. End-to-end smoke-tested:
  `foreground = "#ff6600"` emits `\x1b[38;2;255;102;0m` exactly.

  Schema discipline: any `#`-prefixed string MUST be a valid 3- or
  6-digit hex literal — `"#xyz"` is a parse error, not a silent
  fallback to `Color::Named("#xyz")`.

### Per-segment + per-host config

- `4392a6a` **slice 59** `feat(vcs)`:
  `[segment.<name>].marker_foreground` opt-in field paints the
  `* ! + ~ ? ≡` index markers independently from the branch
  text. Closes the last hardcoded colour in the prompt.
- `274f3ba` **slice 62** `feat(jj)`: jj `divergent` and
  `conflicts` parsed from the `jj log -T` template (slice 52
  had left them at default). Render side was already wired in
  `p10k-rs-segments/src/jj.rs`. 3 parser tests.
- `e288972` **slice 61** `feat(ai)`: schema-only addition of
  `[ai].model: Option<String>` and `[ai].context_tokens:
  Option<u32>`. Both opt-in. Render path remains a stub —
  `render_statusline()` returns empty until the per-host
  metadata story lands.

### AI statusline render wired (slice 61 follow-up)

- `b5862dd` `feat(ai)`: `render_statusline()` is no longer a
  stub. Implements the Claude Code statusline contract documented
  at `~/.planning/powerlevel10k-rs/research/claude-code-statusline-contract.md`:
  reads JSON on stdin, prints a single line on stdout. Three
  render shapes — full (`<model> | <pct>% / <ctxk>k | <cwd>`),
  budget-known-but-no-usage-yet (`<model> | -- / <ctxk>k | <cwd>`),
  and minimal (`<model> | <cwd>`). User `[ai].model` and
  `[ai].context_tokens` overrides win over the host JSON. Hosts
  other than `ClaudeCode` return empty (their protocols aren't
  yet documented). New `pub fn parse_host_kind(s: &str) -> HostKind`
  routes the `--host` CLI flag. The binary's `Statusline`
  subcommand now actually does something — previously it bailed
  with "AI integration phase." End-to-end smoke verified:
  `echo '<JSON>' | p10k-rs statusline --host claude-code` →
  `"Opus 4.7 | 18% / 200k | work"`.
  Closes STATE.md gotcha #12. 10 new tests on the parse / render
  matrix. New deps in `p10k-rs-ai`: `serde`, `serde_json`,
  `p10k-rs-config` — all already in `[workspace.dependencies]`,
  no workspace-level change.

### Prompt-loop refinements (slice 58, slice 63)

- `d579217` **slice 58** `feat(zsh)`: true upcoming-command via
  ZLE `line-pre-redraw`. A new `_p10k_rs_zle_line_pre_redraw`
  widget assigns `$BUFFER` to `_P10K_RS_UPCOMING_CMD` on every
  redraw; `zle reset-prompt` is gated on first-word change via
  the new `_P10K_RS_PREV_UPCOMING_FIRST_WORD` cache, keeping the
  redraw rate at one-per-verb-change rather than one-per-keystroke.
  Preserves the legacy `preexec` assignment for any consumer that
  wanted the "last-ran" semantic. Closes STATE.md gotcha #3 (the
  slice-44 honesty-gap). 6 new pinning tests in `p10k-rs-shell`.

- `5716ab3` **slice 63** `feat(instant-prompt)`: the dump file
  name now embeds a sanitised `$TERM` token. Switching terminals
  (e.g. `xterm-256color` → `tmux-256color`) produces a different
  path so the previous session's dump is never sourced into a
  shell with mismatched capabilities. Sanitisation strips to
  `[a-zA-Z0-9_-]` with a `dumb` fallback. Cache-write side (the
  binary) needs no change — the shell-derived path arrives via
  `--dump`. 5 new path-derivation tests in `p10k-rs` + 1 pinning
  test in `p10k-rs-shell`.

### mdBook docs backfill

- `156183f` `docs(mdbook)`: 5 pages updated to reflect
  post-v0.1.3 features — `config/index.md` (hex literals),
  `reference/schema.md` (Color shapes, marker_foreground,
  TransientPromptMode table, AiConfig rows), `arch/security.md`
  (Unicode-class hardening section), `segments/index.md` (jj
  divergent/conflicts), `theming.md` (marker_foreground).
  62 lines added, 12 replaced.

### Test-harness env-race hardening

Three `std::env::set_var`-based test races found and serialised
under module-level `Mutex<()>`. New invariant documented in
STATE.md — any test that mutates env vars must acquire the
module lock.

- `cc4141b` `fix(tests)`: `p10k-rs-config::tests` (P10K_RS_CONFIG
  collision between missing-file + parse-error tests) and
  `p10k-rs-git::gitstatusd::tests` (PATH="" test breaking the
  `mkfifo` shell-out in concurrent FIFO tests).
- `09293c9` `fix(tests)`: `p10k-rs-core::term_caps::tests`
  (XDG_RUNTIME_DIR / P10K_RS_SESSION_ID / COLORTERM / TERM
  collisions across four tests; closes the
  `write_cache_uses_0600_perms_on_unix` flake the slice-59
  agent flagged in passing).

### Minor

- `5bfb2fd` `fix(core)`: drop a private intra-doc link to
  `CAPS_CACHE` in `term_caps` that broke `cargo doc -D warnings`.

## [0.1.3] - 2026-05-18

First **signed release** — every tarball ships with a sigstore
keyless signature plus SLSA build-provenance attestation. Combined
supply-chain (Tier 0) + shell-integration / observability (Tier 1)
release: 30 slices across both waves.

See `.github/release-notes/v0.1.3.md` for the full user-facing
write-up. Highlights:

### Tier 0 — supply chain

- Sigstore-signed release artifacts (`*.cosign.bundle`) via
  keyless OIDC; identity bound to the workflow file + tag and
  recorded in Rekor.
- SLSA build-provenance attestation per artifact, queryable via
  `gh attestation verify`.
- Third-party GitHub Actions pinned to commit SHAs.
- Dependabot weekly with grouped patch+minor PRs.
- Tag pattern tightened to `v[0-9]+.[0-9]+.[0-9]+*` to refuse
  accidental non-semver releases.
- `concurrency: release-${ref}` with `cancel-in-progress: false`
  so partial publishes finish cleanly.
- `SECURITY.md` + release-verification recipe documented
  (T0.9, T0.10).

### Tier 1 — shell integration + UX

- OSC 133 A/B/C/D semantic-prompt boundaries emit by default on
  modern terminals (Ghostty, WezTerm, iTerm2, Kitty, VS Code,
  Windows Terminal, and any host exporting `$TERM_PROGRAM` /
  `$WT_SESSION` / `$GHOSTTY_RESOURCES_DIR` / `$KITTY_WINDOW_ID`
  / an AI-agent env var). Suppressed on Warp.
- DECSET 2026 synchronized-output wrap with a `Drop` guard.
- `p10k-rs config check` subcommand for TOML validation.
- Generic `AGENT` / `AI_AGENT` env-var probes + Goose detection.
- Bracketed-paste re-arm on every precmd.
- Atomic dump writes at mode `0600`.
- Foreign-owned gitstatusd binary refused at locate-time.
- Daily tracing-appender log at `$XDG_STATE_HOME/p10k-rs`.
- Defensive env (`GIT_CEILING_DIRECTORIES` etc.) on `ShellOut`
  spawn.
- Install script requires `git ≥ 2.35.2` (CVE-2022-24765).
- Binary stderr routed to the diagnostics log too.

### Deferred to v0.2 (at tag time)

T0.5 (sha256-pin gitstatusd binary at install — needs its own
release cycle), T1.8 (transient prompt modes), T1.10 (Unicode
hardening), T1.11 (E2E SafeText for 10 segments), T1.24 (24-bit
truecolor + hex schema).

**Status update**: T1.8 / T1.10 / T1.11 / T1.24 shipped post-tag
and are on `main`; see `[Unreleased]` above. Only T0.5 remains
deferred — it needs a different install flow (today's install.sh
symlinks a `brew` / `apt`-installed gitstatusd; pinning requires
a download + verify model).

Test count: 368 passing at tag time.

## [0.1.2] - 2026-05-15

Review-swarm-driven hardening release. The v0.1.1 → v0.1.2 cycle
closed 4 of 5 actionable HIGHs and ~25 MEDIUMs from the
20260514T023753Z review swarm, plus three trailing v0.1.1 fixes.

CI had been silently failing the `rustdoc` and `cargo-deny` jobs
since the v0.1.1 release-notes commit (`552a83d`); slice A
restored every gate to honest-green. From this tag forward all six
CI jobs (ubuntu clippy/test, macOS clippy/test, rustdoc, rustfmt,
cargo-deny, cargo-machete) plus `cargo machete` locally must stay
green on every push.

See `.github/release-notes/v0.1.2.md` for the user-facing write-up.

### Fixes (HEAD-trailing the v0.1.1 tag)

- `d7d4528` `fix(ai)`: `render_statusline` returned `unimplemented!()`,
  which killed the binary the first time anything sourced the AI
  statusline path. Replaced with an empty-string stub until the
  per-host metadata story lands.
- `0c73a8e` `fix(core)`: `wrap_for_shell`'s CSI scanner only accepted
  the `m` (SGR) final byte, so any non-SGR control sequence (cursor
  moves, scrolling regions, etc.) emitted by a segment was left
  un-bracketed in zsh mode — breaking the prompt-width tracker.
  The scanner now accepts any ECMA-48 final byte (`0x40..=0x7E`).
- `0bb21d4` `perf(core)`: avoid a heap alloc per render on the
  ruler/frame fg default path (`unwrap_or_else` on `&Color`).

### Slice E (`dbaca60`) — IPC + dump security hardening

Closes 3 MEDIUM findings from the 20260514T023753Z review swarm.

- `gitstatusd` FIFO open: closes the lstat→open TOCTOU window via
  `open_fifo_safely`. Opens with `O_NOFOLLOW`, then `fstat`s the
  held fd to re-verify file-type=FIFO and owner=euid. A mid-flight
  symlink swap now returns `ELOOP` instead of redirecting IPC.
- `gitstatusd` read buffer: 1 MiB cap on `read_until_with_deadline`
  (previously unbounded; a misbehaving daemon could grow heap until
  OOM before any delimiter byte).
- Instant-prompt dump: tempfile open is now
  `O_CREAT|O_EXCL|O_NOFOLLOW` at mode `0o600`, with `fsync(2)`
  before rename. Defeats pre-planted-symlink attacks on the
  `.tmp` path and survives power loss between rename and
  writeback.
- 6 new tests cover the IPC and dump paths (real FIFO accept /
  symlink-to-FIFO reject / regular-file reject / 1 MiB cap / dump
  mode `0o600` / pre-planted-symlink target preserved).

### Slice A (`bfcc2f2`) — deterministic gate hygiene

- `cargo deny`: resolved the `lazy_static` ban (transitively via
  `tracing-subscriber → sharded-slab`) with a documented
  `wrappers = ["sharded-slab"]` allowance.
- `cargo doc -D warnings`: fixed 7 pub-API doc-link errors pointing
  at private items (`sanitize_for_terminal`, `PER_QUERY_TIMEOUT`,
  `TOTAL_BUDGET`, `MAX_WALKUP`) and dropped two redundant explicit
  link targets.
- `cargo machete`: trimmed 16 unused `dep.workspace = true`
  declarations across 7 crates. Retired the vestigial `serde`
  feature on `p10k-rs-core` (no enabler).
- CI: new `machete` job; `RUSTDOCFLAGS="-D warnings"` and
  `cargo deny check` already in CI now actually pass.

### Slice B (`70c84ce`) — doc-bundle reset

- README: feature table refreshed (31 segments, multi-arch
  distribution, mdBook docs). Test count corrected (53 → 368).
  Workspace-layout block adds `p10k-rs-jj` and `p10k-rs-ipc`.
  Hacking section lists the two newly-CI'd gates.
- `docs/src/segments/index.md` adds the `jj` segment row (shipped
  since slice 52; chapter just never picked it up).
- `.github/pull_request_template.md`: new — CHANGELOG-update
  checkbox enforces the discipline that three review cycles had
  flagged as a recurring drift.

### Slice C — hot-path allocation reduction

Closes the allocator theme that agent 01 (rust principles) and
agent 03 (perf) raised. Six commits:

- `142dec8` C.1: `Color::Named(String)` → `Color::Named(Cow<'static, str>)`.
  Every `Color::Named("blue".into())` call site is now zero-alloc
  via the `&'static str → Cow::Borrowed` `From` impl. `render_prompt`
  line partition consumes the `Vec` via `into_iter` /
  `split_off` instead of deep-cloning every `SegmentOutput`.
  `EnvSnapshot.home` populated once at construction so `Dir::render`
  stops `getenv`ing `HOME` per prompt.
- `69b2c23` C.2: `sgr_fg` / `sgr_bg` return `Cow<'static, str>`
  with a 10-entry static lookup table for the Ansi8 fast path —
  zero allocation on the common-colour render path.
- `1b9b3d1` C.3: thread-local cache for compiled `globset::Glob`
  matchers; each unique `show_in_dir` / `disabled_dir_pattern`
  pattern compiles exactly once per process. Bad patterns cache
  as `None` so the `tracing::warn!` fires once per pattern, not
  once per call.
- `7c4c0c8` C.4: deduplicated `osc7_for_cwd` (was implemented in
  both core and the AI crate). Single canonical impl in
  `p10k-rs-core::osc7_emit`; `p10k-rs-ai` re-exports.
  Verified the two implementations were byte-identical before merge.
- `651f700` C.5: `sanitize_for_terminal` returns `Cow<'_, str>`
  with a borrow-on-clean-input fast path. Mirror copy in
  `p10k-rs-config` updated in lockstep (the doc promise is now
  honest).

Bench delta (hyperfine, 50-warmup × 500-runs, stripped release,
WSL2): warm-path prompt 1.5 ms ± 0.1 ms → 1.4 ms ± 0.1 ms; trend
consistent across three back-to-back runs but the per-commit
deltas sit inside hyperfine's variance band on the spawn-once
workload.

### Slice D (`6c51c23`) — finish SafeText migration

Closes the architecture-review MEDIUM "SafeText migration is half
done" — a third-consecutive-cycle carry-forward. `RenderCtx`
gains a `cwd_display: SafeText` field, produced once by the binary
at prompt-construction time via
`SafeText::from_untrusted(&cwd.display().to_string())`. `Dir::render`
consumes `ctx.cwd_display.as_str()` directly and the type system
now enforces "control bytes already stripped at producer boundary."
23 `RenderCtx` construction sites updated to provide the new field.

### Slice F (`69c103f` + `4141132`) — readability

Closes agent 04's HIGH "lying comment": the doc comment for
`render_transient` lived 170 lines from the function it described
and rustdoc attached it to `append_ruler_and_frame_top`. Reattached
to the real function. Added cross-reference doc between
`append_ribbon` (left) and `render_right` (right) documenting why
they remain two purpose-named functions rather than one parametric
helper (four structural differences listed).

### Slice G (`af42415`) — fork+exec ceiling, first pass

- `lto = "fat"` on the release + bench profiles. Stripped release
  binary 3,217,704 → 3,049,752 bytes (-167,952, -5.2 %). The ELF
  page-mapping + dynamic-linker work that dominates the 605 µs
  fork+exec floor scales linearly with size; smaller binary is the
  cheapest available lever.
- Trade-off: link time grows ~3-5×. Acceptable for a once-per-
  release artifact. CI's cache keeps iteration reasonable.

A v0.1.3-or-v0.2 conversation: the full close of the fork+exec
floor needs the daemon-mode architecture (one long-lived process
answers many prompt requests over a socket; spawn-per-prompt
collapses to a few-µs IPC ping).

### CI portability fixups

Six commits flagged by the macOS leg of CI during the slice E/A/B
cycle: `mkfifo` shellout for rustix-feature portability,
`expect_used` → `unwrap_used` (workspace lint policy),
`u32::from(st_mode)` for `cast_lossless`,
`#[allow(clippy::useless_conversion)]` for the Linux/macOS lint
clash, and `scratch_dir` canonicalisation for the macOS
`/var/folders/…/T/` → `/private/var/folders/…/T/` symlink chain
that broke `*_dir_pattern` glob tests.

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
