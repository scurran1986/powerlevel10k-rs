# Security model

The prompt sits at a privileged spot: it runs on every keystroke, it
prints to a terminal that interprets escape sequences, and most of its
inputs are attacker-controlled (branch names from `git`, cwd from
filesystem, segment icons from user-edited TOML). Three primitives in
`p10k-rs-core::safety` guard the boundary.

## `sanitize_for_terminal`

Strips every Unicode control codepoint except horizontal tab, plus
DEL (`U+007F`). The OSC injection vector `a\x1b]0;EVIL\x07b` becomes
`a]0;EVILb` — the escape and the BEL terminator are gone, the literal
glyphs survive. Run on every prompt-bound string field at TOML parse
time (`Layout::separators`, `frame.glyph`, `frame.bottom_glyph`,
`ruler.glyph`, every `segment.<name>.icon`, every state icon) so the
renderer never sees a control byte.

## `SafeText`

The newtype wrapper that says "this string passed sanitisation". Branch
names and cwd both pass through `SafeText` before they hit the prompt.
`SafeText::from_bytes` handles non-UTF-8 input (branch names from `git`
can be arbitrary bytes) by lossy-decoding then sanitising. The
construction path is the only way to get one — there is no
`SafeText::new_unchecked`.

## `wrap_for_shell`

The final escape that adapts to each shell's prompt-width-tracking
syntax (zsh `%{...%}`, bash `\[...\]`, fish raw). Run once at the end
of `render_prompt` — segments themselves emit raw SGR escapes; the
wrap step turns them into shell-specific zero-width markers.

## Load-bearing tests

The invariants above are pinned by `safety::tests` in
`crates/p10k-rs-core/src/safety.rs`:

- `passes_plain_ascii_through`
- `preserves_tab`
- `strips_carriage_return`
- `strips_backspace`
- `strips_escape_and_osc_payload`
- `strips_screen_clear`
- `strips_del`
- `strips_unicode_c1_controls`
- `preserves_non_control_unicode`
- `does_not_escape_percent`
- `safe_text_strips_controls_at_construction`
- `safe_text_from_bytes_handles_non_utf8`
- `safe_text_from_bytes_strips_controls`
- `safe_text_default_is_empty`
- `safe_text_displays_as_inner_string`
- `safe_text_from_str_via_into_strips_controls`

A duplicate of `sanitize_for_terminal` lives in `p10k-rs-config` to
avoid a `core → config → core` cycle; both copies are pinned to the
same invariants. Drift would show up as a test mismatch in either
crate.

## FIFO security (gitstatusd)

The daemon FIFOs in `p10k-rs-ipc` are pre-opened, mode-checked, and
owner-checked before any read or write. The hardening landed in slice 9
under [ADR-0001](https://github.com/scurran1986/powerlevel10k-rs/blob/main/docs/adr/0001-git-backend.md)
§ Operational.

## What is _not_ in scope

- Sandboxing the user's own shell config. p10k-rs is a prompt, not an
  EDR.
- Validating that a TOML colour name resolves to a real palette entry
  — the renderer accepts any string and falls through to a default.
- Defending against a malicious `$P10K_RS_GITSTATUSD_BIN` (the user
  controls their own `$PATH`).
