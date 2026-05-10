//! `command_execution_time` — duration of the last foreground command.
//!
//! Cyan duration string when the last command took ≥ 3 seconds. Below the
//! threshold the segment is `enabled() == false` and the renderer skips it.
//! The threshold is hardcoded for now; TOML config will expose it later
//! (matches upstream p10k's `POWERLEVEL9K_COMMAND_EXECUTION_TIME_THRESHOLD`).
//!
//! Duration arrives via [`RenderCtx::last_duration`], which the binary fills
//! from the `--last-duration` CLI arg, which the zsh init script computes
//! from `$EPOCHSECONDS` deltas in the `preexec` / `precmd` hook pair.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Below this many milliseconds the segment hides. Mirrors upstream p10k's
/// default of 3 seconds.
const THRESHOLD_MS: u128 = 3000;

/// Command-execution-time segment.
#[derive(Debug, Default)]
pub struct CommandExecutionTime;

impl Segment for CommandExecutionTime {
    fn name(&self) -> &'static str {
        "command_execution_time"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        ctx.last_duration.as_millis() >= THRESHOLD_MS
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let ms = ctx.last_duration.as_millis();
        let formatted = format_duration_ms(ms);
        let plain_len = u16::try_from(formatted.chars().count()).unwrap_or(u16::MAX);
        // 36 = ANSI cyan.
        let text = format!("\x1b[36m{formatted}\x1b[39m");
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
        }
    }
}

/// Format a millisecond count as `"NNs"`, `"MmSSs"`, or `"HhMMm"`.
///
/// Sub-second precision (e.g. `1.5s`) will land when the zsh init script
/// gets `$EPOCHREALTIME` plumbing; today only integer seconds arrive from
/// `$EPOCHSECONDS`, so showing `1.234s` here would be a lie.
fn format_duration_ms(ms: u128) -> String {
    // Clamp to u64 — anything past 584 million years isn't a real prompt.
    let secs = u64::try_from(ms / 1000).unwrap_or(u64::MAX);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{format_duration_ms, CommandExecutionTime};

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, dur: Duration) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: dur,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    #[test]
    fn hidden_below_threshold() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env, Duration::from_secs(2));
        assert!(!CommandExecutionTime.enabled(&c));
    }

    #[test]
    fn visible_at_threshold() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env, Duration::from_secs(3));
        assert!(CommandExecutionTime.enabled(&c));
    }

    #[test]
    fn renders_seconds() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let c = ctx(&cfg, &env, Duration::from_secs(7));
        let out = CommandExecutionTime.render(&c);
        assert!(out.text.contains("7s"));
        assert!(out.text.contains("\x1b[36m"));
    }

    #[test]
    fn format_minute_second() {
        assert_eq!(format_duration_ms(0), "0s");
        assert_eq!(format_duration_ms(59_000), "59s");
        assert_eq!(format_duration_ms(60_000), "1m00s");
        assert_eq!(format_duration_ms(125_000), "2m05s");
        assert_eq!(format_duration_ms(3_599_000), "59m59s");
        assert_eq!(format_duration_ms(3_600_000), "1h00m");
        assert_eq!(format_duration_ms(7_321_000), "2h02m");
    }
}
