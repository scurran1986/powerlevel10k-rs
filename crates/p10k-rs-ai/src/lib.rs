//! AI host detection, OSC emission, and `--host` statusline rendering.
//!
//! Three responsibilities:
//!
//! 1. Detect the AI host the shell is running inside (Claude Code, Cursor,
//!    Aider, ...) from environment heuristics. The result rides in
//!    [`p10k_rs_core::HostKind`] inside `RenderCtx::host` so segments can
//!    react to it.
//! 2. Emit the OSC sequences that semantically demarcate prompt boundaries
//!    and signal cwd to host terminals (OSC 7, OSC 133).
//! 3. Render the `p10k-rs statusline --host claude-code` payload that AI
//!    hosts shell out to.
//!
//! Only (1) is wired today. (2) and (3) ship in later slices.
//!
//! See `ARCHITECTURE.md` § 2.6 and `10-ai-integration.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use p10k_rs_core::HostKind;

/// Detect which AI host (if any) is wrapping the current shell by probing
/// process environment variables.
///
/// This is the production entry point — it reads `std::env`. For unit
/// tests and any caller that wants a deterministic input, use
/// [`detect_from_env`] with a closure that returns the values you want
/// the detector to see.
///
/// Detection precedence matches the order callers usually care about:
///
/// 1. `$CLAUDECODE` set (any value) → [`HostKind::ClaudeCode`].
/// 2. `$GOOSE_TERMINAL=1` or `$AGENT=goose` (case-insensitive) →
///    [`HostKind::Goose`].
/// 3. Any `$AIDER_*` env var set → [`HostKind::Aider`].
/// 4. Any `$CURSOR_*` env var set → [`HostKind::Cursor`].
/// 5. `$AI_AGENT` or `$AGENT` non-empty → [`HostKind::Generic`] carrying
///    the (trimmed, lowercased) value.
/// 6. Otherwise → [`HostKind::None`].
///
/// Hosts beyond Claude Code are best-effort stubs for this slice — the
/// goal is the wiring, not perfect detection. Future slices can refine
/// fingerprints and add structured per-host data.
#[must_use]
pub fn detect_host_kind() -> HostKind {
    detect_from_env(|name| std::env::var(name).ok())
}

/// Pure variant of [`detect_host_kind`] that probes via the supplied
/// callback instead of `std::env`. The callback receives an env-var
/// name and returns the value, or `None` if unset.
///
/// Factored out so unit tests can drive the detector with a synthetic
/// environment without mutating process-global state — relevant because
/// `std::env::set_var` is racy across the test threadpool.
///
/// The probe list is intentionally short — one canonical "this host is
/// running" fingerprint per supported host. Adding more probes per host
/// is fine, but keep the precedence stable (Claude Code → Goose → Aider
/// → Cursor → Generic → None) so a single shell wrapped by multiple
/// integrations reports the outermost / most-specific one. The generic
/// `AGENT` / `AI_AGENT` probe runs *after* per-tool exact matches so an
/// agent that exports both `CLAUDECODE=1` and `AGENT=claude-code` still
/// reports as `ClaudeCode`, not as a stringly-typed `Generic`.
#[must_use]
pub fn detect_from_env<F: Fn(&str) -> Option<String>>(env: F) -> HostKind {
    // Claude Code exports `$CLAUDECODE` on every spawn — that's the
    // canonical fingerprint and the only one we treat as authoritative
    // for this slice.
    if env("CLAUDECODE").is_some() {
        return HostKind::ClaudeCode;
    }

    // Goose (Block's open-source agent) exports `$GOOSE_TERMINAL=1` from
    // its built-in shell session and also adopts the `$AGENT=goose`
    // convention. Either is a strong signal. Comparison on `AGENT` is
    // case-insensitive because the convention isn't formal and downstream
    // tooling varies on capitalisation.
    if env("GOOSE_TERMINAL").is_some_and(|v| v.trim() == "1")
        || env("AGENT").is_some_and(|v| v.trim().eq_ignore_ascii_case("goose"))
    {
        return HostKind::Goose;
    }

    // Aider doesn't (today) export a single canonical variable. The
    // most stable signals are `$AIDER_API_KEY` (set when the user
    // configured a key) and `$AIDER_MODEL` (set on every invocation
    // when the model is pinned). Either one is good-enough for the
    // MVP — false positives here only mis-paint a prompt badge.
    for key in ["AIDER_API_KEY", "AIDER_MODEL", "AIDER_AUTO_COMMITS"] {
        if env(key).is_some() {
            return HostKind::Aider;
        }
    }

    // Cursor exports `$CURSOR_TRACE_ID` for its built-in terminal and
    // `$CURSOR_SESSION_ID` for some integrations. Either signals we're
    // running inside a Cursor shell.
    for key in ["CURSOR_TRACE_ID", "CURSOR_SESSION_ID"] {
        if env(key).is_some() {
            return HostKind::Cursor;
        }
    }

    // Generic agent probe: covers every future agent that adopts the
    // informal `AGENT=` / `AI_AGENT=` convention without us shipping a
    // per-tool variant. Runs last so explicit fingerprints above always
    // win — the generic path is the catch-all, not a competitor.
    //
    // `AI_AGENT` is preferred over `AGENT` because the latter collides
    // with a couple of generic env names in the wild (some CI systems,
    // older Mac launchctl plists). If both are set, take `AI_AGENT`.
    // Empty / whitespace-only values are ignored — an agent that exports
    // `AGENT=""` is not declaring anything useful.
    for key in ["AI_AGENT", "AGENT"] {
        if let Some(raw) = env(key) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return HostKind::Generic(trimmed.to_ascii_lowercase());
            }
        }
    }

    HostKind::None
}

/// Parse a `--host` CLI value into a [`HostKind`].
///
/// Inverse of [`HostKind`]'s `Display` impl: accepts `"none"`,
/// `"claude-code"`, `"goose"`, `"aider"`, `"cursor"`, and any other
/// non-empty kebab-case-ish string (folded to lowercase, trimmed),
/// which becomes [`HostKind::Generic`]. An empty input maps to
/// [`HostKind::None`] to match the "no host declared" semantic.
///
/// Pure, infallible: there are no invalid host names — unknown labels
/// become [`HostKind::Generic`], which is the catch-all in the schema.
#[must_use]
pub fn parse_host_kind(s: &str) -> HostKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "none" => HostKind::None,
        "claude-code" => HostKind::ClaudeCode,
        "goose" => HostKind::Goose,
        "aider" => HostKind::Aider,
        "cursor" => HostKind::Cursor,
        other => HostKind::Generic(other.to_owned()),
    }
}

/// Subset of the Claude Code statusline JSON contract we actually
/// render. Every field is `Option`-wrapped because the canonical
/// schema (see
/// `~/.planning/powerlevel10k-rs/research/claude-code-statusline-contract.md`)
/// documents that most fields may be absent before the first API
/// response, after `/compact`, or when the model doesn't support the
/// feature. Unknown fields are silently ignored for
/// forward-compatibility — `serde_json` defaults to that posture.
#[derive(Debug, Default, serde::Deserialize)]
struct ClaudeCodeStatuslineInput {
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    model: Option<ClaudeCodeModel>,
    #[serde(default)]
    context_window: Option<ClaudeCodeContextWindow>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ClaudeCodeModel {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ClaudeCodeContextWindow {
    #[serde(default)]
    context_window_size: Option<u32>,
    #[serde(default)]
    used_percentage: Option<f64>,
}

/// Cap on the visible width of any single text token we paste into the
/// statusline. Statuslines live in a single terminal row that may be
/// narrow (split panes, tmux, IDE side-bars), so we cap each field
/// rather than try to fit a wide model name + cwd into 80 columns.
///
/// Applied via [`SafeText::from_untrusted_with_cap`] so the limit is
/// grapheme-aware and survives non-ASCII model names like
/// `"Sonnet 4.6 (中文)"`.
const STATUSLINE_TOKEN_CAP: usize = 32;

/// Render the statusline payload for `host`, given the host's input
/// JSON on stdin and the user's `[ai]` config block.
///
/// Wire contract per host:
///
/// - [`HostKind::ClaudeCode`] — parses the Claude Code statusline JSON
///   schema (see
///   `~/.planning/powerlevel10k-rs/research/claude-code-statusline-contract.md`)
///   and returns a single-line summary:
///   `"<model> | <used%> / <ctx-k>k | <cwd-basename>"`. User-supplied
///   overrides in `[ai].model` and `[ai].context_tokens` win over the
///   live JSON values so the user can override the host's reported
///   labels (useful when running a model fine-tune that reports a
///   different `display_name` than the user thinks of it as).
/// - [`HostKind::Cursor`] — returns an empty string **by design**.
///   Cursor publishes no statusline contract (no stdin schema, no IPC,
///   no documented env-var payload beyond the opaque `CURSOR_TRACE_ID`
///   / `CURSOR_SESSION_ID` fingerprints we already key detection off).
///   The `ai_host` prompt segment carries Cursor visibility instead.
///   Full rationale in
///   `~/.planning/powerlevel10k-rs/research/cursor-statusline-contract.md`
///   — read it before "wiring up" Cursor; the absence is the contract.
/// - Every other [`HostKind`] — returns an empty string. The per-host
///   protocol for Aider / Goose / generic agents is not yet
///   documented; rendering anything would be a guess. Empty stdout is
///   the contract for "we don't know what to render," and the host
///   will just see no statusline.
///
/// Crash-safe: malformed JSON returns an empty string rather than
/// panicking. The host kills the process on every update anyway
/// (in-flight cancellation per the Claude Code contract), so any
/// non-trivial error handling beyond "best-effort render or empty" is
/// wasted code.
///
/// [`SafeText`] boundary: `model.display_name` and `cwd` flow from
/// the host's JSON, which originates from a session that may have an
/// attacker-controlled `display_name` (a maliciously named local
/// fine-tune) or `cwd` (a hostile repository path). Both pass through
/// [`SafeText::from_untrusted_with_cap`] before landing in the output.
///
/// [`SafeText`]: p10k_rs_core::safety::SafeText
/// [`SafeText::from_untrusted_with_cap`]: p10k_rs_core::safety::SafeText::from_untrusted_with_cap
#[must_use]
pub fn render_statusline(host: &HostKind, json_in: &[u8], ai: &p10k_rs_config::AiConfig) -> String {
    match host {
        HostKind::ClaudeCode => render_claude_code_statusline(json_in, ai),
        // Every other host returns empty. Cursor's emptiness is
        // *documented choice* — see the doc comment above and
        // `~/.planning/powerlevel10k-rs/research/cursor-statusline-contract.md`
        // for the full "no public contract exists" research trail.
        // Aider / Goose / Generic are "empty until protocol is known".
        // The dedicated test `render_statusline_cursor_is_intentionally_empty`
        // pins the Cursor decision so a future "add Cursor support"
        // patch has to consciously delete the assertion. Clippy
        // flags a per-variant `HostKind::Cursor => ""` arm as
        // identical-to-wildcard, so the discrimination lives in the
        // doc comment + the dedicated test rather than the match.
        _ => String::new(),
    }
}

fn render_claude_code_statusline(json_in: &[u8], ai: &p10k_rs_config::AiConfig) -> String {
    use p10k_rs_core::safety::SafeText;

    // Malformed JSON → fall back to defaults across the board so the
    // user still gets *something* useful (the [ai].model override if
    // set) instead of a panic.
    let parsed: ClaudeCodeStatuslineInput = serde_json::from_slice(json_in).unwrap_or_default();

    // User override wins. Falls back to host JSON, then to "?".
    let model_raw = ai
        .model
        .as_deref()
        .or_else(|| {
            parsed
                .model
                .as_ref()
                .and_then(|m| m.display_name.as_deref())
        })
        .unwrap_or("?");
    let model = SafeText::from_untrusted_with_cap(model_raw, STATUSLINE_TOKEN_CAP);

    let ctx_size = ai.context_tokens.or_else(|| {
        parsed
            .context_window
            .as_ref()
            .and_then(|c| c.context_window_size)
    });

    let used_pct = parsed
        .context_window
        .as_ref()
        .and_then(|c| c.used_percentage);

    let cwd_basename_raw = parsed
        .cwd
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|os| os.to_str())
        .unwrap_or("?");
    let cwd = SafeText::from_untrusted_with_cap(cwd_basename_raw, STATUSLINE_TOKEN_CAP);

    // Three shapes depending on which fields the host gave us; pick
    // the most-informative one available without inventing missing
    // data.
    match (used_pct, ctx_size) {
        (Some(pct), Some(size_tokens)) => {
            // Clamp to 0..=100 because a misbehaving host could emit
            // outside-range values; format precision `:.0` rounds to
            // integer percent without an `as u32` cast that would
            // truncate / lose sign.
            let pct_clamped = pct.clamp(0.0, 100.0);
            let ctx_k = size_tokens / 1000;
            format!("{model} | {pct_clamped:.0}% / {ctx_k}k | {cwd}")
        }
        (None, Some(size_tokens)) => {
            // Pre-first-API-call: no usage yet, but we know the budget.
            let ctx_k = size_tokens / 1000;
            format!("{model} | -- / {ctx_k}k | {cwd}")
        }
        _ => {
            // No context-window data at all. Just model + cwd.
            format!("{model} | {cwd}")
        }
    }
}

/// Emit an OSC 7 sequence reporting `cwd` to the host terminal.
///
/// Re-exported from [`p10k_rs_core::osc7_emit`] — the canonical
/// implementation lives in `p10k-rs-core` so the render pipeline can
/// call it directly without an `ai → core` re-entry. This symbol is
/// kept here as the stable public entry point for external callers and
/// the `statusline --host` path.
///
/// See [`p10k_rs_core::osc7_emit`] for the full encoding contract.
pub use p10k_rs_core::osc7_emit;

/// OSC 133 prompt-start marker (`A` — semantic prompt boundary).
///
/// Building block for [`osc133_command_start`]. Hosts that parse only
/// the `A` marker (some older `VSCode` shell-integration probes) can use
/// this directly.
#[must_use]
pub fn osc133_prompt_start() -> &'static str {
    "\x1b]133;A\x07"
}

/// OSC 133 command-line-start marker (`B` — end of PS1, start of the
/// editable command line).
#[must_use]
pub fn osc133_prompt_end() -> &'static str {
    "\x1b]133;B\x07"
}

/// Concatenated OSC 133 `A` + `B` for the prompt boundary.
///
/// Emitted by the binary at the head of the rendered prompt: `A` marks
/// where PS1 begins, `B` is appended at the tail of PS1 by the renderer
/// in a separate call to [`osc133_prompt_end`]. This helper exists for
/// the common "emit both at once" case the render path doesn't use, plus
/// future statusline paths that don't have a split point.
#[must_use]
pub fn osc133_command_start() -> String {
    let mut s = String::with_capacity(16);
    s.push_str(osc133_prompt_start());
    s.push_str(osc133_prompt_end());
    s
}

/// OSC 133 command-end marker carrying the exit code (`D;<exit>`).
///
/// Emitted by the shell's `precmd` hook with `$?` — the prompt itself
/// never sees the exit code at the right point to emit this, so the
/// pure-function shape here is for the init script to embed and unit
/// tests to pin the format.
#[must_use]
pub fn osc133_command_end(exit: i32) -> String {
    format!("\x1b]133;D;{exit}\x07")
}

#[cfg(test)]
mod tests {
    use super::{
        detect_from_env, osc133_command_end, osc133_command_start, osc7_emit, parse_host_kind,
        render_statusline, HostKind,
    };
    use p10k_rs_config::AiConfig;
    use std::path::Path;

    /// Build an empty `AiConfig` for tests that don't exercise overrides.
    fn empty_ai_config() -> AiConfig {
        AiConfig::default()
    }

    #[test]
    fn render_statusline_non_claude_hosts_return_empty() {
        // Only `ClaudeCode` has a documented wire protocol; every other
        // host returns the empty contract until each gets its own spec.
        let ai = empty_ai_config();
        assert_eq!(render_statusline(&HostKind::None, b"", &ai), "");
        assert_eq!(render_statusline(&HostKind::Goose, b"{}", &ai), "");
        assert_eq!(render_statusline(&HostKind::Aider, b"\x00\xff", &ai), "");
        assert_eq!(render_statusline(&HostKind::Cursor, b"{}", &ai), "");
        assert_eq!(
            render_statusline(&HostKind::Generic("custom".into()), b"{}", &ai),
            ""
        );
    }

    #[test]
    fn render_statusline_cursor_is_intentionally_empty() {
        // Cursor has no public statusline contract (no stdin JSON
        // schema, no IPC, no documented payload). See
        // `~/.planning/powerlevel10k-rs/research/cursor-statusline-contract.md`
        // for the full research trail. This test pins the "render
        // empty by design" decision so a future patch that wires a
        // real Cursor renderer has to delete this assertion
        // deliberately — not by accident.
        //
        // Distinct from `render_statusline_non_claude_hosts_return_empty`
        // because Cursor is the only "empty by documented choice"
        // host (the others are "empty until protocol is known"). When
        // Cursor ships a contract, this test deletes; the catch-all
        // test keeps living for Aider / Goose / Generic.
        let ai = empty_ai_config();
        // Empty input.
        assert_eq!(render_statusline(&HostKind::Cursor, b"", &ai), "");
        // Minimal JSON object.
        assert_eq!(render_statusline(&HostKind::Cursor, b"{}", &ai), "");
        // Claude-Code-shaped JSON: even if a confused host pipes us a
        // Claude payload while declaring `--host cursor`, we still emit
        // nothing. The host kind, not the payload shape, decides.
        let claude_shape = br#"{
            "cwd": "/tmp",
            "model": { "display_name": "Opus 4.7" },
            "context_window": { "context_window_size": 200000, "used_percentage": 5.0 }
        }"#;
        assert_eq!(render_statusline(&HostKind::Cursor, claude_shape, &ai), "");
        // User-provided `[ai].model` override must not leak into the
        // Cursor render path either — the contract is "empty," not
        // "empty unless the user set [ai].model".
        let mut ai_with_override = empty_ai_config();
        ai_with_override.model = Some("my-model".to_owned());
        ai_with_override.context_tokens = Some(123_456);
        assert_eq!(
            render_statusline(&HostKind::Cursor, b"{}", &ai_with_override),
            ""
        );
    }

    #[test]
    fn render_statusline_claude_code_crash_safe_on_garbage() {
        // Malformed JSON, non-UTF-8 bytes — must NOT panic. Returns a
        // best-effort string built from defaults + any `[ai]` overrides.
        let ai = empty_ai_config();
        // Empty bytes: serde_json errors → defaults across the board.
        let out = render_statusline(&HostKind::ClaudeCode, b"", &ai);
        assert!(
            out.contains('?'),
            "expected fallback '?' tokens, got {out:?}"
        );
        // Random binary noise: same path.
        let out2 = render_statusline(&HostKind::ClaudeCode, b"\xff\xfe\x00garbage", &ai);
        assert!(
            !out2.is_empty(),
            "garbage input should still produce a best-effort line"
        );
    }

    #[test]
    fn render_statusline_claude_code_renders_full_schema() {
        let ai = empty_ai_config();
        let json = br#"{
            "cwd": "/home/u/work/powerlevel10k-rs",
            "model": { "display_name": "Opus 4.7" },
            "context_window": {
                "context_window_size": 200000,
                "used_percentage": 18.3
            }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        // Format: `<model> | <pct>% / <ctxk>k | <cwd-basename>`.
        assert!(out.contains("Opus 4.7"), "model name missing: {out:?}");
        // 18.3 rounds to 18.
        assert!(out.contains("18%"), "rounded percentage missing: {out:?}");
        // 200000 / 1000 = 200.
        assert!(
            out.contains("200k"),
            "context-budget k-form missing: {out:?}"
        );
        // Basename only — full path not in output.
        assert!(
            out.contains("powerlevel10k-rs"),
            "cwd basename missing: {out:?}"
        );
        assert!(
            !out.contains("/home/u"),
            "full cwd leaked into statusline: {out:?}"
        );
    }

    #[test]
    fn render_statusline_claude_code_user_model_override_wins() {
        // `[ai].model` set by the user → must be preferred over the
        // host's `model.display_name`. Use case: user runs a local
        // fine-tune that reports a name they don't recognise.
        let mut ai = empty_ai_config();
        ai.model = Some("my-fork-of-opus".to_owned());
        let json = br#"{"model": {"display_name": "Opus 4.7"}}"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        assert!(
            out.contains("my-fork-of-opus"),
            "user override missing: {out:?}"
        );
        assert!(
            !out.contains("Opus 4.7"),
            "user override should replace host display_name: {out:?}"
        );
    }

    #[test]
    fn render_statusline_claude_code_user_ctx_size_override_wins() {
        let mut ai = empty_ai_config();
        ai.context_tokens = Some(1_000_000);
        let json = br#"{
            "context_window": { "context_window_size": 200000, "used_percentage": 5.0 }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        // 1_000_000 / 1000 = 1000. Override beats the 200k from JSON.
        assert!(out.contains("1000k"), "user ctx override missing: {out:?}");
        assert!(
            !out.contains("200k"),
            "host ctx leaked despite override: {out:?}"
        );
    }

    #[test]
    fn render_statusline_claude_code_handles_missing_context_window() {
        // Pre-first-API-call case: model known, no context-window data.
        let ai = empty_ai_config();
        let json = br#"{
            "cwd": "/tmp/foo",
            "model": { "display_name": "Sonnet 4.6" }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        // Two-token shape `<model> | <cwd>` (no "% / k").
        assert!(out.contains("Sonnet 4.6"));
        assert!(out.contains("foo"));
        assert!(
            !out.contains('%'),
            "no percentage shape allowed without data: {out:?}"
        );
        assert!(
            !out.contains('k'),
            "no k-form shape allowed without data: {out:?}"
        );
    }

    #[test]
    fn render_statusline_claude_code_handles_pre_first_api_call() {
        // We know the budget but used_percentage hasn't populated yet.
        // Should render `<model> | -- / <ctxk>k | <cwd>`.
        let ai = empty_ai_config();
        let json = br#"{
            "cwd": "/tmp/foo",
            "model": { "display_name": "Opus 4.7" },
            "context_window": { "context_window_size": 200000 }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        assert!(out.contains("--"), "expected '--' placeholder: {out:?}");
        assert!(out.contains("200k"), "ctx budget still rendered: {out:?}");
    }

    #[test]
    fn render_statusline_claude_code_sanitises_hostile_model_name() {
        // A maliciously named local model could embed an ANSI escape
        // or BiDi control in `display_name`. SafeText must strip it
        // before the value lands in the statusline output.
        let ai = empty_ai_config();
        let json = br#"{
            "cwd": "/tmp",
            "model": { "display_name": "Opus\r[31mEVIL" }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        // CR and ESC stripped. The literal `[31m` remains as harmless
        // text — that's the documented post-sanitisation behaviour
        // (SafeText strips control bytes, not their textual fallout).
        assert!(!out.contains('\r'), "CR leaked: {out:?}");
        assert!(!out.contains('\u{001b}'), "ESC leaked: {out:?}");
    }

    #[test]
    fn render_statusline_claude_code_clamps_out_of_range_percent() {
        // Defensive: a misbehaving host emitting > 100% must not produce
        // ridiculous output. Clamped to 0..=100.
        let ai = empty_ai_config();
        let json = br#"{
            "model": { "display_name": "M" },
            "context_window": { "context_window_size": 100000, "used_percentage": 9999.0 }
        }"#;
        let out = render_statusline(&HostKind::ClaudeCode, json, &ai);
        assert!(out.contains("100%"), "expected 100% clamp: {out:?}");
        assert!(
            !out.contains("9999"),
            "raw value leaked despite clamp: {out:?}"
        );
    }

    #[test]
    fn parse_host_kind_maps_known_names() {
        assert_eq!(parse_host_kind("claude-code"), HostKind::ClaudeCode);
        assert_eq!(parse_host_kind("CLAUDE-CODE"), HostKind::ClaudeCode);
        assert_eq!(parse_host_kind("  goose  "), HostKind::Goose);
        assert_eq!(parse_host_kind("aider"), HostKind::Aider);
        assert_eq!(parse_host_kind("cursor"), HostKind::Cursor);
        assert_eq!(parse_host_kind("none"), HostKind::None);
        assert_eq!(parse_host_kind(""), HostKind::None);
    }

    #[test]
    fn parse_host_kind_unknown_becomes_generic() {
        // Forward-compat: an agent that adopts a new label still routes
        // through the catch-all rather than erroring.
        assert_eq!(
            parse_host_kind("future-agent"),
            HostKind::Generic("future-agent".to_owned())
        );
    }

    /// Build a fake env lookup from a list of key/value pairs. Anything
    /// not listed returns `None`.
    fn env_with<'a>(
        pairs: &'a [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn detect_none_when_env_is_empty() {
        let kind = detect_from_env(env_with(&[]));
        assert_eq!(kind, HostKind::None);
    }

    #[test]
    fn detect_claude_code_from_claudecode_env() {
        // `$CLAUDECODE` is the canonical fingerprint. Any non-empty
        // value flips us into the Claude-Code branch — Anthropic sets
        // it to `"1"` today but we don't pin the value.
        let kind = detect_from_env(env_with(&[("CLAUDECODE", "1")]));
        assert_eq!(kind, HostKind::ClaudeCode);

        let kind = detect_from_env(env_with(&[("CLAUDECODE", "anything")]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn detect_aider_from_aider_api_key() {
        let kind = detect_from_env(env_with(&[("AIDER_API_KEY", "sk-…")]));
        assert_eq!(kind, HostKind::Aider);

        let kind = detect_from_env(env_with(&[("AIDER_MODEL", "gpt-4o")]));
        assert_eq!(kind, HostKind::Aider);
    }

    #[test]
    fn detect_cursor_from_cursor_trace_id() {
        let kind = detect_from_env(env_with(&[("CURSOR_TRACE_ID", "abc123")]));
        assert_eq!(kind, HostKind::Cursor);

        let kind = detect_from_env(env_with(&[("CURSOR_SESSION_ID", "abc123")]));
        assert_eq!(kind, HostKind::Cursor);
    }

    #[test]
    fn claudecode_wins_over_other_hosts() {
        // If a user happens to have Aider/Cursor vars exported AND
        // they're inside Claude Code, the outermost host wins — Claude
        // Code is checked first.
        let kind = detect_from_env(env_with(&[
            ("CLAUDECODE", "1"),
            ("AIDER_API_KEY", "sk-…"),
            ("CURSOR_TRACE_ID", "abc"),
        ]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn aider_wins_over_cursor() {
        let kind = detect_from_env(env_with(&[
            ("AIDER_MODEL", "gpt-4o"),
            ("CURSOR_TRACE_ID", "abc"),
        ]));
        assert_eq!(kind, HostKind::Aider);
    }

    #[test]
    fn detect_generic_agent_via_ai_agent() {
        // The dedicated `AI_AGENT` slot is the preferred carrier — the
        // informal convention is that agents which avoid the `AGENT` name
        // collision use this one. Trim + lowercase on the way through.
        let kind = detect_from_env(env_with(&[("AI_AGENT", "  CoolBot  ")]));
        assert_eq!(kind, HostKind::Generic("coolbot".to_owned()));
    }

    #[test]
    fn detect_generic_agent_via_agent_var() {
        // Fall back to the bare `AGENT` slot when `AI_AGENT` is unset.
        // Same trim + lowercase pass — the prompt badge stays consistent
        // regardless of how the upstream tool capitalises its self-id.
        let kind = detect_from_env(env_with(&[("AGENT", "MyAgent")]));
        assert_eq!(kind, HostKind::Generic("myagent".to_owned()));
    }

    #[test]
    fn generic_agent_ignores_empty_and_whitespace_values() {
        // An agent that sets `AGENT=""` (or all-whitespace) hasn't
        // declared anything meaningful — fall through to `None` instead
        // of painting a blank badge.
        assert_eq!(
            detect_from_env(env_with(&[("AI_AGENT", "")])),
            HostKind::None
        );
        assert_eq!(
            detect_from_env(env_with(&[("AGENT", "   ")])),
            HostKind::None
        );
    }

    #[test]
    fn ai_agent_wins_over_plain_agent_for_generic() {
        // When both are set, prefer `AI_AGENT` — it's the namespaced
        // slot and less likely to be a stale value from launchctl or
        // an unrelated CI tool.
        let kind = detect_from_env(env_with(&[("AI_AGENT", "primary"), ("AGENT", "stale")]));
        assert_eq!(kind, HostKind::Generic("primary".to_owned()));
    }

    #[test]
    fn detect_goose_via_terminal_var() {
        // `$GOOSE_TERMINAL=1` is Goose's documented session marker.
        let kind = detect_from_env(env_with(&[("GOOSE_TERMINAL", "1")]));
        assert_eq!(kind, HostKind::Goose);
    }

    #[test]
    fn detect_goose_via_agent_var() {
        // `AGENT=goose` is the informal community convention. Comparison
        // is case-insensitive so `Goose`, `GOOSE`, and `goose` all match.
        let kind = detect_from_env(env_with(&[("AGENT", "goose")]));
        assert_eq!(kind, HostKind::Goose);

        let kind = detect_from_env(env_with(&[("AGENT", "Goose")]));
        assert_eq!(kind, HostKind::Goose);

        let kind = detect_from_env(env_with(&[("AGENT", "GOOSE")]));
        assert_eq!(kind, HostKind::Goose);
    }

    #[test]
    fn goose_takes_precedence_over_aider() {
        // Goose slots between Claude Code and Aider in the precedence
        // chain — if a Goose-wrapped shell also has Aider env vars
        // exported, Goose wins because it's the outer agent.
        let kind = detect_from_env(env_with(&[
            ("GOOSE_TERMINAL", "1"),
            ("AIDER_MODEL", "gpt-4o"),
        ]));
        assert_eq!(kind, HostKind::Goose);
    }

    #[test]
    fn claudecode_wins_over_goose() {
        // Claude Code stays at the top of the precedence chain. A shell
        // running Goose-inside-Claude reports the outermost host so the
        // OSC contract stays anchored on Claude's parser.
        let kind = detect_from_env(env_with(&[("CLAUDECODE", "1"), ("GOOSE_TERMINAL", "1")]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn explicit_probes_take_precedence_over_generic() {
        // The `AGENT` / `AI_AGENT` generic probe runs last. If a tool
        // exports both its canonical fingerprint AND a generic name,
        // the per-tool variant must win — `Generic("foobar")` for a
        // Claude Code shell would lose us routing data downstream.
        let kind = detect_from_env(env_with(&[("CLAUDECODE", "1"), ("AGENT", "foobar")]));
        assert_eq!(kind, HostKind::ClaudeCode);

        // Same for the dedicated `AI_AGENT` slot — Claude still wins.
        let kind = detect_from_env(env_with(&[("CLAUDECODE", "1"), ("AI_AGENT", "foobar")]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn host_kind_display_uses_short_labels() {
        // The `Display` impl is the segment-layer / status-line carrier.
        // Pin the short labels so a future rename doesn't silently
        // re-shape prompt text.
        assert_eq!(HostKind::None.to_string(), "none");
        assert_eq!(HostKind::ClaudeCode.to_string(), "claude-code");
        assert_eq!(HostKind::Goose.to_string(), "goose");
        assert_eq!(HostKind::Aider.to_string(), "aider");
        assert_eq!(HostKind::Cursor.to_string(), "cursor");
        assert_eq!(
            HostKind::Generic("coolbot".to_owned()).to_string(),
            "coolbot"
        );
    }

    #[test]
    fn osc7_encodes_simple_path() {
        // ASCII unreserved + `/`: pass-through, wrapped in the OSC
        // envelope with an empty host (Claude Code accepts `file:///…`).
        let s = osc7_emit(Path::new("/home/seaburdz"));
        assert_eq!(s, "\x1b]7;file:///home/seaburdz\x1b\\");
    }

    #[test]
    fn osc7_percent_encodes_spaces_and_special_chars() {
        // Spaces become `%20`. Capital hex per RFC 3986.
        let s = osc7_emit(Path::new("/tmp/foo bar"));
        assert_eq!(s, "\x1b]7;file:///tmp/foo%20bar\x1b\\");
    }

    #[test]
    fn osc7_preserves_unreserved_set() {
        // The unreserved set (`A-Z a-z 0-9 - _ . ~`) must not be
        // encoded — a `~` in a path stays a `~`, not `%7E`.
        let s = osc7_emit(Path::new("/home/~user/a.b-c_d"));
        assert_eq!(s, "\x1b]7;file:///home/~user/a.b-c_d\x1b\\");
    }

    #[test]
    fn osc133_command_start_emits_a_then_b() {
        // The "prompt" boundary the binary emits at the head of PS1 is
        // the A+B pair so a host that snapshots only one of them still
        // gets a useful signal.
        assert_eq!(osc133_command_start(), "\x1b]133;A\x07\x1b]133;B\x07");
    }

    #[test]
    fn osc133_command_end_includes_exit_code() {
        assert_eq!(osc133_command_end(0), "\x1b]133;D;0\x07");
        assert_eq!(osc133_command_end(130), "\x1b]133;D;130\x07");
        // Negative codes: zsh stores `$?` as unsigned, but the function
        // is `i32` for ergonomic interop with `RenderCtx::last_status`.
        // Round-trip the literal so hosts that parse signed get what
        // they expect.
        assert_eq!(osc133_command_end(-1), "\x1b]133;D;-1\x07");
    }
}
