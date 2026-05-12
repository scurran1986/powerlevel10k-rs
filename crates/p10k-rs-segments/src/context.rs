//! `context` — `user@host` segment with privilege/SSH awareness.
//!
//! Always-visible identity line: who you are, where you are. The render is
//! gated by a `state` tag so users can recolour the segment per situation
//! via TOML overrides like `[segment.context.states.root].foreground = "red"`.
//!
//! State tag mapping (first match wins):
//! - `"root"`   — effective UID is 0 (privileged shell, regardless of SSH).
//! - `"ssh"`    — any of `$SSH_CONNECTION` / `$SSH_CLIENT` / `$SSH_TTY` set.
//! - `"normal"` — local, unprivileged.
//!
//! Both `$USER` (preferred) and `$LOGNAME` (fallback) are attacker-shaped
//! input on shared systems, and `uname(2).nodename` is whatever the host's
//! admin set it to. All three pass through [`sanitize_for_terminal`] before
//! they reach `text` — see `dir.rs` / `virtualenv.rs` for the same pattern.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (person). Override via
/// `[segment.context].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f007}";

/// User-and-host context segment.
///
/// Renders `<user>@<host>` in yellow by default, tagged with one of
/// `"root" | "ssh" | "normal"` so TOML can swap the colour per state.
#[derive(Debug, Default)]
pub struct Context;

impl Segment for Context {
    fn name(&self) -> &'static str {
        "context"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        true
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // User: $USER wins; fall back to $LOGNAME; final fallback "?". Both
        // env reads are kept inline (not pushed into `EnvSnapshot`) because
        // they're cheap and only this segment cares.
        let user_env = std::env::var("USER").ok();
        let logname_env = std::env::var("LOGNAME").ok();
        let user = user_or_fallback(user_env.as_deref(), logname_env.as_deref());

        // Hostname via `uname(2)` — avoids spawning `hostname(1)` and works
        // identically on Linux and macOS. `nodename` is the kernel's view of
        // the host's name; `to_string_lossy()` keeps us alive on the
        // (vanishingly rare) non-UTF-8 case.
        let uname = rustix::system::uname();
        let host = sanitize_for_terminal(&uname.nodename().to_string_lossy());

        let euid = rustix::process::geteuid().as_raw();
        let ssh_set = std::env::var("SSH_CONNECTION").is_ok()
            || std::env::var("SSH_CLIENT").is_ok()
            || std::env::var("SSH_TTY").is_ok();
        let state = detect_state(euid, ssh_set);

        let plain = format!("{user}@{host}");
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state), DEFAULT_ICON);
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space

        // Yellow bg default; root/ssh users can override per state via TOML.
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            Some(state),
            Color::Named("yellow".into()),
        );
        let fg = style::render_fg(
            ctx.config,
            self.name(),
            Some(state),
            Color::Named("black".into()),
        );
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );

        SegmentOutput {
            text,
            plain_len,
            state: Some(state),
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("yellow".into())),
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
        "ssh"
    } else {
        "normal"
    }
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
    sanitize_for_terminal(raw)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{detect_state, user_or_fallback, Context};

    // Helper tests only for the state/user logic. Exercising `render()`
    // end-to-end is fine — we only *read* env (`$USER`, `$LOGNAME`,
    // `$SSH_*`), never set it, so we don't trip the `unsafe`
    // `std::env::set_var` rule from 1.85+ or race parallel test threads.

    #[test]
    fn detect_state_root() {
        assert_eq!(detect_state(0, false), "root");
    }

    #[test]
    fn detect_state_root_takes_priority_over_ssh() {
        assert_eq!(detect_state(0, true), "root");
    }

    #[test]
    fn detect_state_ssh() {
        assert_eq!(detect_state(1000, true), "ssh");
    }

    #[test]
    fn detect_state_normal() {
        assert_eq!(detect_state(1000, false), "normal");
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
    fn renders_with_default_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
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
