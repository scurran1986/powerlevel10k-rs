//! Render-path safety primitives.
//!
//! Anything that flows from the outside world (git metadata, the working
//! directory, environment values) into the rendered prompt must pass through
//! [`sanitize_for_terminal`] before reaching a segment's output. The prompt is
//! ultimately written to a TTY and assigned to the shell's `PROMPT` variable;
//! both interpret bytes that the producer never intended (terminal escape
//! sequences, zsh `%` prompt expansions). Stripping control bytes at the
//! boundary prevents an attacker who controls a branch name or directory name
//! from steering the terminal or the shell's prompt-expansion engine.
//!
//! Per-shell escapes (zsh's `%` → `%%`) are applied later in
//! [`crate::render_prompt`]'s `wrap_for_shell` pass, after segments have
//! assembled their output.

/// Replace bytes in `s` that are unsafe to emit to a terminal.
///
/// Strips every Unicode control code point (`char::is_control()`) except
/// horizontal tab (`\t`), and also strips DEL (`U+007F`). Tab is preserved
/// because terminals render it as visible whitespace; everything else
/// (`\x1b`, `\r`, `\b`, `\x07`, OSC introducers, the unicode C1 controls in
/// `U+0080..=U+009F`) can drive the terminal's state machine, overwrite
/// already-rendered prompt content, or mask attacker text behind invisible
/// regions.
///
/// The function returns a freshly-allocated `String`; the input is consumed
/// by reference. For inputs that already contain only safe bytes the
/// returned `String` is byte-equal to the input — no normalisation, no
/// case-folding, no encoding changes.
///
/// # Examples
///
/// ```
/// use p10k_rs_core::safety::sanitize_for_terminal;
///
/// assert_eq!(sanitize_for_terminal("main"), "main");
/// assert_eq!(sanitize_for_terminal("foo\tbar"), "foo\tbar");
/// assert_eq!(sanitize_for_terminal("foo\x1b]0;evil\x07bar"), "foo]0;evilbar");
/// assert_eq!(sanitize_for_terminal("a\rb"), "ab");
/// ```
#[must_use]
pub fn sanitize_for_terminal(s: &str) -> String {
    // Keep tab (terminals render it as visible whitespace), strip every
    // other control codepoint and DEL. `is_control()` covers `\x00..=\x1f`,
    // `\x7f`, and the Unicode C1 controls `\u{0080}..=\u{009F}`; the
    // explicit DEL check is belt-and-braces in case a future Rust
    // definition narrows `is_control()`.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\t' || (!c.is_control() && c != '\u{007F}') {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_plain_ascii_through() {
        assert_eq!(sanitize_for_terminal(""), "");
        assert_eq!(sanitize_for_terminal("main"), "main");
        assert_eq!(sanitize_for_terminal("feat/widget-1.2"), "feat/widget-1.2");
    }

    #[test]
    fn preserves_tab() {
        assert_eq!(sanitize_for_terminal("a\tb"), "a\tb");
    }

    #[test]
    fn strips_carriage_return() {
        // CR is the prompt-overwrite vector — anything after \r reprints
        // from column 0 on most terminals.
        assert_eq!(sanitize_for_terminal("normal\rEVIL$ "), "normalEVIL$ ");
    }

    #[test]
    fn strips_backspace() {
        assert_eq!(sanitize_for_terminal("ab\x08c"), "abc");
    }

    #[test]
    fn strips_escape_and_osc_payload() {
        // OSC 0 sets the terminal title. Stripping the ESC and BEL leaves
        // the otherwise-printable payload visible (intentional — the user
        // sees that something weird is in the input).
        assert_eq!(
            sanitize_for_terminal("main\x1b]0;TARS-OWNED\x07"),
            "main]0;TARS-OWNED",
        );
    }

    #[test]
    fn strips_screen_clear() {
        assert_eq!(sanitize_for_terminal("\x1b[2J\x1b[Hgone"), "[2J[Hgone");
    }

    #[test]
    fn strips_del() {
        assert_eq!(sanitize_for_terminal("a\x7Fb"), "ab");
    }

    #[test]
    fn strips_unicode_c1_controls() {
        // U+0085 (NEL) and U+0086 (SSA) are control codes some terminals
        // act on. Strip.
        assert_eq!(sanitize_for_terminal("a\u{0085}b\u{0086}c"), "abc");
    }

    #[test]
    fn preserves_non_control_unicode() {
        // Branch names with emoji, accents, CJK, etc. must round-trip.
        assert_eq!(sanitize_for_terminal("café"), "café");
        assert_eq!(sanitize_for_terminal("分支"), "分支");
        assert_eq!(sanitize_for_terminal("feat/🚀"), "feat/🚀");
    }

    #[test]
    fn does_not_escape_percent() {
        // `%` is a zsh PROMPT-expansion concern handled by the shell-aware
        // wrapping pass; this function must leave `%` alone so non-zsh
        // shells aren't double-processed.
        assert_eq!(sanitize_for_terminal("%n@%m"), "%n@%m");
    }
}
