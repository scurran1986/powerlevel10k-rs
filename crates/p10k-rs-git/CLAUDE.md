# CLAUDE.md — p10k-rs-git

The only crate in the workspace that has an `unsafe` budget, and the
production hot path for the whole prompt. Read `docs/adr/0001-git-backend.md`
before changing anything here.

## What lives here

- `Backend` trait — every git-state producer implements it. `status(&Path) ->
  Option<GitState>`. `None` means "not a repo" *or* "couldn't talk to the
  daemon" — the caller doesn't distinguish; it falls back.
- `ShellOut` — slow path. Spawns `git status --porcelain=v1 --branch` per
  prompt. Only fills `branch` + `dirty`; richer fields stay at default.
- `Gitstatusd` — fast path. Client of a long-lived daemon spawned by the
  shell init script over two FIFOs.
- `locate_gitstatusd()` — probes `$P10K_RS_GITSTATUSD_BIN` then `$PATH`.

`GitState` itself lives in `p10k-rs-core` so `RenderCtx` can hold an
`Option<&'_ GitState>` without a dependency cycle. This crate produces;
segments consume.

## The wire protocol (gitstatusd backend)

Documented in `.planning/powerlevel10k-rs/07-gitstatus.md` § 1. Summary:

- Request: `id\x1F<dir>\x1E`. `id` is opaque; we send `"p10k-rs-prompt"`.
- Response: 17 `\x1F`-separated fields terminated by `\x1E`. Field 1 is
  `"1"` for "in repo" or `"0"` for "not a repo". The 17-field minimum is
  enforced — short records → `None` (unrecognised wire format, bail).
- Field-offset table (in repo, 0-based) is in `parse_response` and matches
  07-gitstatus.md § 1.3. If a field index needs to change, update both the
  parser comment and the planning doc — they are intentionally redundant.

## Render-path safety (load-bearing)

Every byte string that arrives off the daemon wire or out of `git`'s
stdout is **attacker-controlled input**. Branch names and commit OIDs
flow into the prompt; the prompt is assigned to zsh's `PROMPT` and
written to a TTY.

- Use `SafeText::from_untrusted_bytes` (wire) or `SafeText::from_untrusted`
  (already-UTF-8 string) at the boundary. Do not stuff raw `&str` /
  `Vec<u8>` into `GitState`. The type system enforces this — there is no
  `SafeText::assume_safe`.
- `from_utf8_lossy` substitutes `U+FFFD` for invalid bytes; do not switch
  to `from_utf8(...).unwrap_or("")` to "simplify" — that silently empties
  non-UTF-8 branch names, which is why the test
  `parses_non_utf8_branch_lossily_rather_than_dropping` exists.

## FIFO security checks (slice 9, non-negotiable)

`is_fifo()` is the gate before opening either FIFO:

1. `symlink_metadata` (lstat), not `metadata` (stat) — refuses to follow a
   symlink. Defends against an attacker swapping our FIFO path for a
   symlink to their own pipe.
2. File type must be FIFO (`MetadataExt::file_type().is_fifo()`).
3. Owner UID must equal our effective UID. Defends against a co-tenant
   pre-planting a FIFO in a path we'd otherwise trust.

If you add new IPC paths, mirror this check. Don't reach for `metadata` to
"simplify" — that follows symlinks.

## The 2-second poll deadline

`Gitstatusd::status` uses `poll(2)` with a deadline (default
`DEFAULT_TIMEOUT = 2s`). On timeout, EOF-before-delimiter, or hangup with
no data, we return `None` and the binary falls back to `ShellOut`. A
wedged daemon must not stall the shell forever.

- `i32` ms clamping: `i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX)`
  bounds the cast at ~24 days, well past any reasonable timeout.
- `revents.contains(PollFlags::HUP) && !revents.contains(PollFlags::IN)`
  is the "EOF with no pending data" check. Both flags set means "data
  available, then EOF" — read first.

## The `unsafe` budget

There is no `unsafe` block in this crate **today**. The lint posture
(workspace `unsafe_code = "warn"`) makes the addition deliberate. Before
adding one:

1. State why the safe alternative is unfit.
2. State the invariants the call site upholds.
3. State what would have to change to make the block unsound.

`rustix` is the FFI shim we already pull in; reach for it (or `nix`) before
hand-rolling `unsafe libc::…` calls.

## What is *not* here yet

- `gix-status` correctness fallback. Listed as a follow-up in
  ADR-0001 § Follow-ups; nothing to wire up today. `gix` is removed
  from `Cargo.toml` accordingly.
- Daemon-respawn / health-check cache. The current model is "shell spawns
  one daemon at startup, we open the FIFO per prompt". A wedge falls back
  to `ShellOut`; we do not currently try to restart the daemon ourselves.

## Tests

Parser tests cover: clean repo, dirty repo, conflicts, ahead/behind,
detached HEAD, unborn branch, control chars in branch names, non-UTF-8
branch names, short records, truncated records. The control-char and
non-UTF-8 tests are regression markers — don't delete them when refactoring
the parser, they encode security and UX invariants respectively.

`#[allow(clippy::unwrap_used)]` is scoped to the `#[cfg(test)]` module
only. Production code never unwraps off the wire.
