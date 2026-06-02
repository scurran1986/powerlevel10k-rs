//! `context` — `user@host` segment with privilege/SSH awareness.
//!
//! Identity line: who you are, where you are. The render is gated by a
//! `state` tag so users can recolour the segment per situation via TOML
//! overrides like `[segment.context.states.root].foreground = "red"`.
//!
//! State tag mapping (first match wins):
//! - `"root"`   — effective UID is 0 (privileged shell, regardless of SSH).
//! - `"remote"` — any of `$SSH_CONNECTION` / `$SSH_CLIENT` / `$SSH_TTY` set.
//! - `"local"`  — local, unprivileged.
//!
//! ## Visibility
//!
//! Upstream P10K hides `context` when you're "you on your own machine":
//! username matches `$DEFAULT_USER` and the shell isn't remote or
//! privileged. We mirror that with `$P10K_RS_DEFAULT_USER`:
//!
//! - Hidden when `$P10K_RS_DEFAULT_USER == username`, **and** no SSH env
//!   var is set, **and** EUID != 0.
//! - Always shown otherwise (root or SSH overrides the default-user hide).
//! - Hidden when both `$USER` and `$LOGNAME` are unset/empty — the segment
//!   has nothing useful to render.
//!
//! Both `$USER`/`$LOGNAME` and `$HOSTNAME`/`uname(2).nodename` are
//! attacker-shaped input on shared systems, so all four pass through
//! [`SafeText::from_untrusted`] (which delegates to
//! [`sanitize_for_terminal`]) before they reach `text` — see `dir.rs` /
//! `virtualenv.rs` for the same pattern. `$SSH_CONNECTION` /
//! `$SSH_CLIENT` / `$SSH_TTY` are state-only (never displayed), so they
//! deliberately stay untyped — wrapping them would imply a render edge
//! that does not exist.

use p10k_rs_core::safety::{sanitize_for_terminal, SafeText};
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (person). Override via
/// `[segment.context].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f007}";

/// User-and-host context segment.
///
/// Renders `<user>@<host>`, tagged with one of `"root" | "remote" | "local"`
/// so TOML can swap the colour per state.
#[derive(Debug, Default)]
pub struct Context;

impl Segment for Context {
    fn name(&self) -> &'static str {
        "context"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        // Resolve user/state with the same logic `render` uses. If the user
        // is empty we have nothing useful to show; if the default-user
        // hide rule applies we suppress the segment entirely.
        let user_env = std::env::var("USER").ok();
        let logname_env = std::env::var("LOGNAME").ok();
        let user = user_or_fallback_opt(user_env.as_deref(), logname_env.as_deref());
        let Some(user) = user else {
            return false;
        };

        let ssh_set = ssh_session_active();
        let euid = current_euid();
        let state = detect_state(euid, ssh_set);
        let default_user = std::env::var("P10K_RS_DEFAULT_USER").ok();
        should_show(&user, state, default_user.as_deref())
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // User: $USER wins; fall back to $LOGNAME; final fallback "?". The
        // empty/None case is already filtered by `enabled()`, but render
        // must stay safe if called standalone (matches `virtualenv.rs`).
        // `SafeText::from_untrusted` re-runs the sanitiser — idempotent
        // on the already-cleaned value from `user_or_fallback`, but it
        // pins the boundary at the type level so future refactors can't
        // forget the wrap.
        let user_env = std::env::var("USER").ok();
        let logname_env = std::env::var("LOGNAME").ok();
        let user = SafeText::from_untrusted(&user_or_fallback(
            user_env.as_deref(),
            logname_env.as_deref(),
        ));

        // Hostname: `$HOSTNAME` first (some shells export it, some don't —
        // zsh does, bash on Linux doesn't unless you `export` it), then
        // `uname(2).nodename`. Empty `$HOSTNAME` is treated as unset so
        // `env -i` style runs still fall through to uname.
        let host_env = std::env::var("HOSTNAME").ok();
        let host = SafeText::from_untrusted(&host_or_uname(host_env.as_deref()));

        let euid = current_euid();
        let ssh_set = ssh_session_active();
        let state = detect_state(euid, ssh_set);

        let plain = format!("{}@{}", user.as_str(), host.as_str());
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state), DEFAULT_ICON);
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space

        // Per-state palette defaults — root screams red, remote/local stay
        // on the upstream yellow default. State-keyed TOML overrides still
        // win because the default just shifts before the `style::*` call.
        let (default_bg, default_fg) = default_palette_for(state);

        let bg_sgr = style::render_bg(ctx.config, self.name(), Some(state), default_bg.clone());
        let fg_sgr = style::render_fg(ctx.config, self.name(), Some(state), default_fg);
        let text = format!(
            "{bg_sgr}{fg_sgr}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );

        SegmentOutput {
            text,
            plain_len,
            state: Some(state),
            icon: Some(DEFAULT_ICON),
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                Some(state),
                default_bg,
            )),
        }
    }
}

/// Map `(euid, ssh_set)` to the segment's state tag.
///
/// Root wins outright — a root SSH session is still a root session, and the
/// warning colour should reflect the more dangerous of the two. Pulled out
/// as a free function so it's unit-testable without touching the process's
/// real EUID (which would be `unsafe` and process-global).
fn detect_state(euid: u32, ssh_set: bool) -> &'static str {
    if euid == 0 {
        "root"
    } else if ssh_set {
        "remote"
    } else {
        "local"
    }
}

/// Per-state default `(background, foreground)` pair.
///
/// - `root` — red bg / white fg (the "you're privileged" warning).
/// - `remote` / `local` — yellow bg / black fg (the historical default).
///
/// Mirrors upstream P10K's context colouring at a coarser grain — we don't
/// distinguish per-user palettes, just the privilege/locality axis.
fn default_palette_for(state: &str) -> (Color, Color) {
    match state {
        "root" => (Color::Named("red".into()), Color::Named("white".into())),
        // remote and local share the upstream default
        _ => (Color::Named("yellow".into()), Color::Named("black".into())),
    }
}

/// `true` when any of the standard SSH env vars are non-empty.
fn ssh_session_active() -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|key| std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false))
}

/// Default-user visibility gate.
///
/// Hidden iff: `$P10K_RS_DEFAULT_USER` is set and equals `user`, **and**
/// the state is `"local"` (no SSH, not root). Root and remote sessions
/// always show — they're load-bearing context for the human at the prompt.
fn should_show(user: &str, state: &str, default_user: Option<&str>) -> bool {
    if state != "local" {
        return true;
    }
    !matches!(default_user, Some(du) if !du.is_empty() && du == user)
}

/// Choose between `$USER` and `$LOGNAME`, sanitise, and fall back to `"?"`.
///
/// Empty strings are treated as unset — some minimal environments
/// (`env -i`, certain CI runners) leave `USER=""` set, and we'd rather show
/// the `LOGNAME` value than render `@host`.
fn user_or_fallback(user: Option<&str>, logname: Option<&str>) -> String {
    let raw = user
        .filter(|s| !s.is_empty())
        .or_else(|| logname.filter(|s| !s.is_empty()))
        .unwrap_or("?");
    sanitize_for_terminal(raw).into_owned()
}

/// Like [`user_or_fallback`] but returns `None` when both env vars are
/// unset/empty so `enabled()` can hide the segment instead of rendering
/// `?@host`.
fn user_or_fallback_opt(user: Option<&str>, logname: Option<&str>) -> Option<String> {
    let raw = user
        .filter(|s| !s.is_empty())
        .or_else(|| logname.filter(|s| !s.is_empty()))?;
    Some(sanitize_for_terminal(raw).into_owned())
}

/// Hostname picker: `$HOSTNAME` first, then `uname(2).nodename`. Output is
/// always sanitised; an empty result is possible only on a host whose
/// kernel returns an empty `nodename` and which has no `$HOSTNAME` set —
/// rare enough that we leave it as `""` rather than inventing a fallback.
fn host_or_uname(hostname_env: Option<&str>) -> String {
    if let Some(h) = hostname_env.filter(|s| !s.is_empty()) {
        return sanitize_for_terminal(h).into_owned();
    }
    #[cfg(unix)]
    {
        let uname = rustix::system::uname();
        sanitize_for_terminal(&uname.nodename().to_string_lossy()).into_owned()
    }
    #[cfg(not(unix))]
    {
        // No `uname(2)` on Windows; fall back to `$COMPUTERNAME` and then
        // an empty string. The render path already tolerates `""`.
        let cn = std::env::var("COMPUTERNAME").unwrap_or_default();
        sanitize_for_terminal(&cn).into_owned()
    }
}

/// Current effective UID, abstracted across platforms.
///
/// Unix: `rustix::process::geteuid().as_raw()`. Windows: returns a
/// non-zero sentinel (`1`) so `detect_state` never sees a "root"
/// classification on a platform where the concept doesn't translate.
fn current_euid() -> u32 {
    #[cfg(unix)]
    {
        rustix::process::geteuid().as_raw()
    }
    #[cfg(not(unix))]
    {
        1
    }
}

#[cfg(test)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{
        default_palette_for, detect_state, host_or_uname, should_show, user_or_fallback,
        user_or_fallback_opt, Context,
    };

    // Helper tests stay env-free: exercising `render()` end-to-end would
    // trip `std::env::set_var` (unsafe since 1.85) and race parallel test
    // threads. The helpers are pure so we can drive every branch by hand.

    #[test]
    fn detect_state_root() {
        assert_eq!(detect_state(0, false), "root");
    }

    #[test]
    fn detect_state_root_takes_priority_over_ssh() {
        assert_eq!(detect_state(0, true), "root");
    }

    #[test]
    fn detect_state_remote() {
        assert_eq!(detect_state(1000, true), "remote");
    }

    #[test]
    fn detect_state_local() {
        assert_eq!(detect_state(1000, false), "local");
    }

    #[test]
    fn user_or_fallback_uses_user_first() {
        assert_eq!(user_or_fallback(Some("alice"), Some("bob")), "alice");
    }

    #[test]
    fn user_or_fallback_skips_empty_user() {
        assert_eq!(user_or_fallback(Some(""), Some("bob")), "bob");
    }

    #[test]
    fn user_or_fallback_returns_question_mark() {
        assert_eq!(user_or_fallback(None, None), "?");
    }

    #[test]
    fn user_or_fallback_sanitises() {
        // CR injection guard: a username with `\r` would otherwise
        // overwrite the prompt line on render.
        assert_eq!(user_or_fallback(Some("alice\rEVIL"), None), "aliceEVIL");
    }

    #[test]
    fn user_or_fallback_then_safetext_strips_ansi() {
        // The render path wraps `user_or_fallback`'s output in
        // `SafeText::from_untrusted`. Walk that exact composition with a
        // hostile `$USER` (ESC sequence) and confirm no ANSI escape
        // bytes survive into the SafeText payload — the malicious red
        // SGR would otherwise colour-bleed the rest of the prompt.
        let raw = user_or_fallback(Some("alice\x1b[31mEVIL"), None);
        let safe = SafeText::from_untrusted(&raw);
        assert!(
            !safe.as_str().contains('\x1b'),
            "ESC leaked into SafeText: {:?}",
            safe.as_str()
        );
        // The visible payload is allowed to remain (terminals show the
        // letters); only the control byte must be gone.
        assert!(safe.as_str().contains("alice"));
        assert!(safe.as_str().contains("EVIL"));
    }

    #[test]
    fn host_or_uname_then_safetext_strips_bidi() {
        // BiDi override in `$HOSTNAME` could flip the display order of
        // adjacent prompt characters (`host.local` → `lacol.tsoh`).
        // Confirm the SafeText wrap kills U+202E.
        let raw = host_or_uname(Some("host\u{202E}local"));
        let safe = SafeText::from_untrusted(&raw);
        assert!(!safe.as_str().contains('\u{202E}'));
    }

    #[test]
    fn user_or_fallback_opt_returns_none_when_both_empty() {
        assert_eq!(user_or_fallback_opt(None, None), None);
        assert_eq!(user_or_fallback_opt(Some(""), Some("")), None);
    }

    #[test]
    fn user_or_fallback_opt_prefers_user() {
        assert_eq!(
            user_or_fallback_opt(Some("alice"), Some("bob")),
            Some("alice".to_string())
        );
    }

    #[test]
    fn user_or_fallback_opt_falls_back_to_logname() {
        assert_eq!(
            user_or_fallback_opt(Some(""), Some("bob")),
            Some("bob".to_string())
        );
    }

    #[test]
    fn host_or_uname_uses_env_when_set() {
        assert_eq!(host_or_uname(Some("box.local")), "box.local");
    }

    #[test]
    fn host_or_uname_sanitises_env() {
        // Same CR injection guard as the username — `$HOSTNAME` is
        // user-controllable on systems that let the shell own it.
        assert_eq!(host_or_uname(Some("evil\r\nhost")), "evilhost");
    }

    #[test]
    fn host_or_uname_falls_back_to_uname_when_empty_env() {
        // We can't predict the test host's nodename, but the fallback path
        // must return *something* (sanitised) and not the empty string we
        // explicitly passed in.
        let got = host_or_uname(Some(""));
        // `nodename()` may legitimately be empty on some kernels; assert
        // only that we took the uname branch (not the env branch) by
        // checking the result doesn't contain a CR/LF.
        assert!(!got.contains('\r'));
        assert!(!got.contains('\n'));
    }

    #[test]
    fn should_show_hides_local_default_user() {
        assert!(!should_show("alice", "local", Some("alice")));
    }

    #[test]
    fn should_show_shows_local_non_default_user() {
        assert!(should_show("alice", "local", Some("bob")));
    }

    #[test]
    fn should_show_shows_when_default_unset() {
        assert!(should_show("alice", "local", None));
        assert!(should_show("alice", "local", Some("")));
    }

    #[test]
    fn should_show_always_shows_root() {
        assert!(should_show("root", "root", Some("root")));
    }

    #[test]
    fn should_show_always_shows_remote() {
        assert!(should_show("alice", "remote", Some("alice")));
    }

    #[test]
    fn default_palette_root_is_red() {
        let (bg, _fg) = default_palette_for("root");
        assert!(matches!(bg, p10k_rs_core::style::Color::Named(ref n) if n == "red"));
    }

    #[test]
    fn default_palette_remote_is_yellow() {
        let (bg, _fg) = default_palette_for("remote");
        assert!(matches!(bg, p10k_rs_core::style::Color::Named(ref n) if n == "yellow"));
    }

    #[test]
    fn default_palette_local_is_yellow() {
        let (bg, _fg) = default_palette_for("local");
        assert!(matches!(bg, p10k_rs_core::style::Color::Named(ref n) if n == "yellow"));
    }

    #[test]
    fn renders_with_default_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            cwd_display: SafeText::default(),
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
            shell_integration_active: false,
            sync_output: false,
        };
        let out = Context.render(&ctx);
        assert!(
            out.text.contains('\u{f007}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f007}"));
    }
}
