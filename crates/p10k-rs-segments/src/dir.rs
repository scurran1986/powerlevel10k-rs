//! `dir` — current working directory segment.
//!
//! Cwd in blue, with `$HOME` collapsed to `~`. Truncation policies and the
//! writable / read-only state will land later. ANSI escapes are emitted raw;
//! the renderer post-processes them for the target shell (e.g. zsh's
//! `%{…%}` bracketing).

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

const DEFAULT_ICON: &str = "\u{f07b}"; // Nerd Font v3: folder (FA-style)

/// Current-directory segment.
///
/// Reads [`RenderCtx::cwd`] and emits its display string with the user's home
/// directory abbreviated to `~`.
#[derive(Debug, Default)]
pub struct Dir;

impl Segment for Dir {
    fn name(&self) -> &'static str {
        "dir"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Sanitise before home-collapse so a malicious cwd containing
        // control bytes can't ride the unfiltered path into PROMPT (C2).
        let raw = sanitize_for_terminal(&ctx.cwd.display().to_string());
        let home = std::env::var("HOME").ok();
        let collapsed = home_collapse(&raw, home.as_deref());
        let icon = ctx
            .config
            .segments
            .get(self.name())
            .and_then(|sc| sc.icon.as_deref())
            .unwrap_or(DEFAULT_ICON);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("blue".into()));
        let text = format!("{fg}{icon} {collapsed}{}", style::reset_fg());
        // plain_len: chars-of-collapsed + 1 (icon, single Nerd Font codepoint
        // renders as 1 visual col) + 1 (space). saturating_add guards the
        // u16::MAX overflow path.
        let plain_len = u16::try_from(collapsed.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
        }
    }
}

/// Collapse a leading `home` directory in `path` to `~`. Returns the input
/// unchanged if `home` is `None` or doesn't prefix the path.
///
/// `home` is taken explicitly so this is a pure function we can unit-test
/// without mutating process-global env state — `std::env::set_var` is
/// `unsafe` since Rust 1.85 and this crate forbids unsafe blocks anyway.
fn home_collapse(path: &str, home: Option<&str>) -> String {
    let Some(home) = home else {
        return path.to_owned();
    };
    if let Some(rest) = path.strip_prefix(home) {
        // Only collapse on a directory boundary: `/home/sean/x` → `~/x`,
        // not `/home/seanson/x` → `~son/x`.
        if rest.is_empty() || rest.starts_with('/') {
            return format!("~{rest}");
        }
    }
    path.to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::*;

    #[test]
    fn home_collapse_exact() {
        let home = Some("/home/sean");
        assert_eq!(home_collapse("/home/sean", home), "~");
        assert_eq!(home_collapse("/home/sean/code", home), "~/code");
        assert_eq!(
            home_collapse("/home/seanson/code", home),
            "/home/seanson/code"
        );
        assert_eq!(home_collapse("/etc/passwd", home), "/etc/passwd");
        assert_eq!(home_collapse("/etc/passwd", None), "/etc/passwd");
    }

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, cwd: &'a Path) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    #[test]
    fn renders_with_default_folder_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/tmp/example");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert!(
            out.text.contains('\u{f07b}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f07b}"));
    }

    #[test]
    fn cwd_with_carriage_return_is_stripped() {
        // C2 reproducer: a directory whose name contains `\r` would
        // otherwise let an attacker overwrite the start of the prompt
        // line on render.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/tmp/start\rEVIL");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert!(!out.text.contains('\r'), "CR survived: {:?}", out.text);
        assert!(out.text.contains("/tmp/startEVIL"));
    }

    #[test]
    fn cwd_with_osc_escape_is_stripped() {
        // C2 reproducer: a directory whose name contains an OSC 0
        // sequence would relabel the user's terminal tab. The segment
        // legitimately emits its own SGR escapes (`\x1b[34m`, `\x1b[39m`)
        // for the blue colour — what must be gone is the attacker's OSC
        // introducer (`\x1b]`) and the `\x07` BEL terminator.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/tmp/main\x1b]0;TARS-OWNED\x07");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert!(
            !out.text.contains("\x1b]"),
            "OSC introducer survived: {:?}",
            out.text
        );
        assert!(!out.text.contains('\x07'), "BEL survived: {:?}", out.text);
        // Visible payload is preserved (no escapes around it now).
        assert!(out.text.contains("/tmp/main]0;TARS-OWNED"));
    }
}
