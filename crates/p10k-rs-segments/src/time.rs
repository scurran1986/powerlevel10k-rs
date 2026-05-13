//! `time` — current wall-clock time in `HH:MM:SS`.
//!
//! Always-on segment that renders the current time in white. We prefer the
//! user's local timezone, but the `time` crate refuses `now_local()` on
//! multi-threaded processes — `localtime_r` walks process-global tz state
//! that another thread could mutate mid-read, and the crate won't paper
//! over that with unsafe FFI. When the local lookup fails we fall back to
//! UTC so a wedged tz never breaks the prompt.
//!
//! Using the `time` crate's `local-offset` feature is the safest
//! cross-platform option here: it avoids us shelling out, avoids us
//! reaching for `chrono` (heavier, more transitive deps), and avoids
//! hand-rolled `unsafe` around `libc::localtime_r`.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};
use time::macros::format_description;
use time::OffsetDateTime;

const DEFAULT_ICON: &str = "\u{f017}"; // Nerd Font v3: clock

/// Current-time segment.
#[derive(Debug, Default)]
pub struct Time;

impl Segment for Time {
    fn name(&self) -> &'static str {
        "time"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        true
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let dt = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let formatted = format_time(dt);
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            None,
            Color::Named("brightblack".into()),
        );
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!(
            "{bg}{fg}{icon} {formatted}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        SegmentOutput {
            text,
            plain_len: 10, // "X HH:MM:SS" — icon glyph (1) + space (1) + 8 chars
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("brightblack".into())),
        }
    }
}

/// Format an `OffsetDateTime` as `HH:MM:SS`.
///
/// Falls back to `"--:--:--"` if the formatter rejects the input — which
/// in practice it won't for the fixed `[hour]:[minute]:[second]`
/// description, but libraries don't panic.
fn format_time(dt: OffsetDateTime) -> String {
    let fmt = format_description!("[hour]:[minute]:[second]");
    dt.format(fmt).unwrap_or_else(|_| String::from("--:--:--"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::style::Color;
    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};
    use time::OffsetDateTime;

    use super::{format_time, Time};

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::from_secs(0),
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
        }
    }

    #[test]
    fn name_is_time() {
        assert_eq!(Time.name(), "time");
    }

    #[test]
    fn renders_10_char_plain_length() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env);
        let out = Time.render(&c);
        assert_eq!(out.plain_len, 10);
    }

    #[test]
    fn renders_white_color() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env);
        let out = Time.render(&c);
        assert!(out.text.contains("\x1b[38;5;7m"));
    }

    #[test]
    fn renders_with_default_clock_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env);
        let out = Time.render(&c);
        assert!(out.text.contains('\u{f017}'));
        assert_eq!(out.icon, Some("\u{f017}"));
    }

    #[test]
    fn renders_brightblack_background() {
        // Powerline ribbon: time segment must emit a brightblack bg SGR and
        // surface that colour via `SegmentOutput.background` so the renderer
        // can paint the transition arrow in the matching foreground.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env);
        let out = Time.render(&c);
        assert!(out.text.contains("\x1b[48;5;8m"));
        assert!(out.text.contains("\x1b[49m"));
        assert_eq!(out.background, Some(Color::Named("brightblack".into())));
    }

    #[test]
    fn format_zero_time() {
        assert_eq!(format_time(OffsetDateTime::UNIX_EPOCH), "00:00:00");
    }
}
