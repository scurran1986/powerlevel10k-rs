# CLAUDE.md — p10k-rs-core

The render pipeline and shared types. **I/O-free** by design — that's the
whole point of this crate. Higher-level crates plug into the types defined
here.

## What lives here

- `Segment` trait — every prompt segment implements it. `name()`,
  `render()`, optional `enabled()` and `is_fast()` gates.
- `RenderCtx` — per-prompt input bundle (config, shell, cwd, git, jobs,
  duration, env snapshot). Fields are `pub` and *not* `#[non_exhaustive]`
  on purpose; adding a field is a `SemVer` minor downstream.
- `SegmentOutput` — what `render()` returns. Text is already ANSI-styled;
  the renderer does not re-style.
- `Prompt` — `{ left, right, transient }` triple. Right + transient land
  with their respective features.
- `render_prompt()` — pure function. Walks segments, calls `enabled` then
  `render`, joins with a space, runs `wrap_for_shell`.
- `safety::SafeText` + `safety::sanitize_for_terminal` — the
  attacker-controlled-input chokepoint.
- `style::*` — colour / SGR helpers segments compose.

`Config` is re-exported from `p10k-rs-config` so `RenderCtx` can name it
without dependents pulling in the config crate directly.

## Dependency direction (don't invert it)

`p10k-rs-core` depends on `p10k-rs-config`. Never the other way. The
sanitiser the config crate runs at parse time is an inlined copy of
`safety::sanitize_for_terminal` — keep them in sync if you ever touch one,
but do not import the function from `core` into `config`.

## The I/O-free rule

This crate must compile and run identically on any host with no
environment, no `git`, no filesystem touches. Specifically:

- No `std::env`, no `std::fs`, no `std::process`, no networking.
- No syscalls outside what `Duration`/`SystemTime` do natively.
- `RenderCtx` is the carrier: producers (the binary, `p10k-rs-git`,
  `p10k-rs-ai`) gather state and hand it in pre-computed.

If a feature here needs to call out, it's in the wrong crate.

## `SafeText` invariant (load-bearing)

Branch names and cwd flow into the prompt, which is assigned to `PROMPT`
and written to a TTY. Both interpret bytes the producer never intended:
ANSI escapes, OSC sequences, the unicode C1 controls, zsh's `%`
expansions. `SafeText` encodes "this string has passed
`sanitize_for_terminal`" in the type system.

- No `assume_safe` escape hatch. By design.
- Constructors: `from_untrusted(&str)`, `from_untrusted_bytes(&[u8])`,
  `From<&str>` (sugar for `from_untrusted`).
- `sanitize_for_terminal` strips every `char::is_control()` codepoint
  *except* `\t` (terminals render it as visible whitespace), plus DEL.
  Covers `\x00..=\x1f`, `\x7f`, and unicode C1 (`U+0080..=U+009F`).
- `%` is **not** stripped here. That's a zsh-specific PROMPT-expansion
  concern handled by `wrap_for_shell` in the render pass. Don't merge
  the two — non-zsh shells must not see `%%` in their output.

## Shell wrapping (`wrap_for_shell`)

Two transforms, applied only when `shell == Shell::Zsh`:

1. Each `\x1b[…m` SGR escape gets wrapped in `%{…%}` so zsh's
   prompt-width tracker knows those bytes don't take a column.
2. Every literal `%` is doubled to `%%` so zsh's PROMPT-expansion engine
   sees a literal `%` instead of `%n` / `%/` / etc. SGR bodies we emit
   contain no `%`, so the doubling only fires on text content.

Test invariants you can't drop:
- `wrap_for_zsh_doubles_literal_percent_in_text` — the C1-fix regression
  marker. Drop this and a branch named `%n@%m` becomes a username/host
  leak (or worse with `PROMPT_SUBST`).
- `wrap_for_non_zsh_leaves_percent_alone` — bash/fish don't expand `%`.
- `wrap_for_zsh_does_not_double_brackets_we_emit` — guards against
  doubling the `%{` / `%}` we just added.

## Segment contract

When writing a new `Segment`:

- `name()` returns a `&'static str`, lowercase, `snake_case`. It is the
  TOML config key; changing it is a breaking change for users.
- `render()` does not write to stdout/stderr/the filesystem. Everything
  flows through the returned `SegmentOutput`.
- `enabled()` is the cheap precondition. Use it to gate auto-detected
  segments (`kubecontext` only when `kubectl` is on `PATH`) without
  paying full render cost.
- `is_fast()` defaults to `true`. Return `false` if the render may block
  on disk or network — relevant once the post-MVP daemon ships.
- `plain_len: u16` is **visual columns**, not bytes. Count grapheme
  clusters; ANSI escapes don't count.

## What is *not* here

- Tokio / async anything. MVP is spawn-per-prompt synchronous. v0.2
  conversation.
- Network or filesystem I/O. See the I/O-free rule above.
- `EnvSnapshot` real fields. It's a placeholder today; fields land
  segment-by-segment as need arises. Marked `#[non_exhaustive]` because
  the binary is its sole constructor.

## Tests

`wrap_for_shell` and `safety::sanitize_for_terminal` carry the
load-bearing security invariants. Their tests are regression markers —
adding new behaviour is fine, deleting an existing assertion needs a
matching explanation in the commit message.
