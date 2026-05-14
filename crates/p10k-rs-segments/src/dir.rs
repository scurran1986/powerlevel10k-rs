//! `dir` — current working directory segment.
//!
//! Cwd painted black-on-blue (P10K-classic palette), with `$HOME` collapsed
//! to `~`. Truncation policies and the writable / read-only state will land
//! later. ANSI escapes are emitted raw; the renderer post-processes them for
//! the target shell (e.g. zsh's `%{…%}` bracketing) and uses the declared
//! `background` colour to paint powerline transition arrows.

use std::path::Path;

use p10k_rs_config::{DirTruncate, DirTruncateStrategy};
use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Cap on entries pulled from `read_dir` when computing the
/// shortest-unique prefix for a component. A pathological directory with
/// hundreds of thousands of children would otherwise stall the prompt;
/// after this many siblings we conservatively treat the component as
/// non-unique and emit the first character. See [`DirTruncateStrategy::ToUnique`].
const UNIQUE_PREFIX_READDIR_CAP: usize = 200;

const DEFAULT_ICON: &str = "\u{f07b}"; // Nerd Font v3: folder (FA-style)
const NOT_WRITABLE_ICON: &str = "\u{f023}"; // Nerd Font v3: padlock

/// Ellipsis glyph used as the truncation marker.
///
/// `U+2026` (HORIZONTAL ELLIPSIS) — a single visual column under
/// East-Asian-wide-aware terminals; passes [`sanitize_for_terminal`]
/// (not a control codepoint).
const ELLIPSIS: &str = "\u{2026}";

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
        // Read `$HOME` from the per-prompt env snapshot rather than the
        // global env. The snapshot caches the value across this prompt
        // render — avoids one libc-mutex'd `getenv` per call on every
        // shell precmd hook.
        let collapsed = home_collapse(&raw, ctx.env.home.as_deref());
        let truncate = ctx
            .config
            .segments
            .get(self.name())
            .map(|s| s.truncate.clone())
            .unwrap_or_default();
        let collapsed = truncate_path(&collapsed, &truncate, Some(ctx.cwd));
        // Probe write permission on the *real* cwd path before we paint.
        // On any errno (broken cwd, EACCES on the parent, etc.) we treat
        // the directory as not writable — that's the safer default for a
        // visual cue (matches P10K's behaviour: when in doubt, show the
        // padlock). State-keyed TOML overrides flow through the
        // `style::*` helpers below.
        let state_tag = writability_state(ctx.cwd);
        let (default_bg, default_fg) = default_palette_for(state_tag);
        let default_icon = if state_tag == "not_writable" {
            NOT_WRITABLE_ICON
        } else {
            DEFAULT_ICON
        };
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state_tag), default_icon);
        let bg_sgr = style::render_bg(ctx.config, self.name(), Some(state_tag), default_bg.clone());
        let fg_sgr = style::render_fg(ctx.config, self.name(), Some(state_tag), default_fg);
        let text = format!(
            "{bg_sgr}{fg_sgr}{icon} {collapsed}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        // plain_len: chars-of-collapsed + 1 (icon, single Nerd Font codepoint
        // renders as 1 visual col) + 1 (space). saturating_add guards the
        // u16::MAX overflow path.
        let plain_len = u16::try_from(collapsed.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: Some(state_tag),
            icon: Some(default_icon),
            background: Some(default_bg),
        }
    }
}

/// Per-state default `(background, foreground)` pair.
///
/// - `writable` — blue/black (P10K-classic default, unchanged from slice
///   28A).
/// - `not_writable` — yellow/black (P10K's `DIR_NOT_WRITABLE_*` "warning"
///   hue).
///
/// Pulled out as a free function so we can pick the default *before*
/// calling [`style::render_bg`] / [`style::render_fg`] — those helpers
/// take a single default and don't know about state defaults. Mirrors
/// the `vi_mode.rs` slice 34 pattern.
fn default_palette_for(state: &str) -> (Color, Color) {
    match state {
        "not_writable" => (Color::Named("yellow".into()), Color::Named("black".into())),
        // `writable` and any future variant fall back to the
        // P10K-classic blue/black.
        _ => (Color::Named("blue".into()), Color::Named("black".into())),
    }
}

/// Probe `cwd` for write permission and return the state tag.
///
/// Uses `rustix::fs::access(cwd, Access::WRITE_OK)` — the POSIX `access(2)`
/// shim — which performs the same real-UID/GID check the kernel would
/// apply to a subsequent `open(O_WRONLY)`. We don't actually open the
/// directory; this is a cheap permission probe that doesn't perturb
/// atime.
///
/// Fallback: on *any* errno (broken cwd that no longer exists, EACCES on
/// a parent, ENOTDIR if the cwd was racily replaced with a file, etc.)
/// we report `"not_writable"`. That's the safer visual default — a
/// padlock is a strictly better warning than a silently-green prompt on
/// a directory that's actually broken.
fn writability_state(cwd: &Path) -> &'static str {
    match rustix::fs::access(cwd, rustix::fs::Access::WRITE_OK) {
        Ok(()) => "writable",
        Err(_) => "not_writable",
    }
}

/// Apply the configured truncation strategy to a (home-collapsed) path string.
///
/// Pure function over the already-sanitised `path` and the parsed
/// [`DirTruncate`] config. Operates on the textual form so a leading `~`
/// counts as the first component (matches upstream P10K behaviour).
///
/// `fs_root` is the **real** filesystem cwd backing `path`. Only the
/// [`DirTruncateStrategy::ToUnique`] arm consults it (to list sibling
/// directories for the shortest-unique-prefix algorithm); every other
/// strategy is a pure string transform and ignores it. Pass `None` when
/// no filesystem context is available — `ToUnique` will then fall back
/// to the input unchanged.
///
/// Algorithm:
/// - Splits on `/` and keeps the empty leading element (for absolute paths
///   like `/a/b/c` → `["", "a", "b", "c"]`) so the leading slash is preserved
///   by re-joining.
/// - `length = 0` is normalised to `1` so a misconfiguration can't render an
///   empty cwd.
/// - Paths with fewer non-empty components than `length` are returned
///   unchanged (no marker needed — nothing was elided). The `ToUnique`
///   strategy skips that early-out — even a short path can benefit from
///   per-component shortening.
///
/// Returns the input unchanged when `strategy == None`.
fn truncate_path(path: &str, cfg: &DirTruncate, fs_root: Option<&Path>) -> String {
    if matches!(cfg.strategy, DirTruncateStrategy::None) {
        return path.to_owned();
    }
    // Split into the leading-slash marker (if any) plus the components.
    let (leading, body) = if let Some(rest) = path.strip_prefix('/') {
        ("/", rest)
    } else {
        ("", path)
    };
    let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();

    // `ToUnique` has no `length` semantics — it shortens every non-final
    // component independently. Handle it before the length-based early-out.
    if matches!(cfg.strategy, DirTruncateStrategy::ToUnique) {
        return truncate_to_unique(leading, &parts, fs_root);
    }

    let length = cfg.length.max(1) as usize;
    if parts.len() <= length {
        return path.to_owned();
    }
    // `None` is short-circuited above; the catch-all below covers it plus
    // any future `#[non_exhaustive]` variant.
    match cfg.strategy {
        DirTruncateStrategy::ToLast => {
            // …/<last `length` components>. The marker stands in for both
            // the elided components and any leading slash.
            let tail = &parts[parts.len() - length..];
            format!("{ELLIPSIS}/{}", tail.join("/"))
        }
        DirTruncateStrategy::Middle => {
            // <first>/…/<last `length - 1` components>. When `length == 1`
            // there's no tail; degenerates to `<first>/…`.
            let first = parts[0];
            if length == 1 {
                format!("{leading}{first}/{ELLIPSIS}")
            } else {
                let tail = &parts[parts.len() - (length - 1)..];
                format!("{leading}{first}/{ELLIPSIS}/{}", tail.join("/"))
            }
        }
        // `DirTruncateStrategy` is `#[non_exhaustive]` so future variants
        // compile without breaking this match. Unknown strategy falls back
        // to the safe no-op path.
        _ => path.to_owned(),
    }
}

/// Shorten each non-final component in `parts` to its shortest unique
/// prefix among its siblings on disk; preserve the final component
/// verbatim.
///
/// `leading` is `"/"` for absolute paths and `""` otherwise — it is
/// re-prepended to the joined output.
///
/// `fs_root` is the real filesystem path that `parts` describes. It is
/// used to anchor the per-component `read_dir` walk. When `fs_root` is
/// `None`, or when no parts need walking (≤1 component), the textual
/// input is returned unchanged.
///
/// The walk descends real filesystem components only — when the leading
/// component is `~` (home-collapsed) we re-resolve via `$HOME` so the
/// sibling listing happens against the actual home parent.
fn truncate_to_unique(leading: &str, parts: &[&str], fs_root: Option<&Path>) -> String {
    // Nothing to elide for empty/single-component paths.
    if parts.len() <= 1 {
        return format!("{leading}{}", parts.join("/"));
    }
    let Some(fs_root) = fs_root else {
        return format!("{leading}{}", parts.join("/"));
    };

    // Build the real filesystem ancestor chain matching `parts`. The
    // textual `parts` may start with `~`; expand that to `$HOME` so the
    // ancestor walk lands on a real directory.
    //
    // We walk by stripping `parts.len()` ancestors off `fs_root`. Index 0
    // = leaf, index parts.len()-1 = root. We need ancestors[i] for the
    // *parent* of `parts[i]`, so ancestor[parts.len()-i] is the parent
    // (the path one level above parts[i]).
    let ancestors: Vec<&Path> = fs_root.ancestors().collect();
    // ancestors[0] == fs_root (leaf), ancestors[parts.len()] == parent of root component.
    // If the cwd path has fewer ancestors than textual parts (mismatch
    // between textual home-collapse and real path depth), fall back to
    // the simple "first char" rule per component.

    let mut out: Vec<String> = Vec::with_capacity(parts.len());
    for (i, &part) in parts.iter().enumerate() {
        // Last component is always preserved in full.
        if i + 1 == parts.len() {
            out.push(part.to_owned());
            continue;
        }
        // Parent dir on disk = ancestor at depth `parts.len() - i`.
        // (For i = 0, parent is parts.len() levels up from the leaf.)
        let parent_idx = parts.len() - i;
        let parent: Option<&Path> = ancestors.get(parent_idx).copied();
        let prefix = match parent {
            Some(p) => shortest_unique_prefix(p, part).unwrap_or_else(|| first_char(part)),
            None => first_char(part),
        };
        out.push(prefix);
    }
    format!("{leading}{}", out.join("/"))
}

/// Return the shortest prefix of `name` (in chars) that no other entry of
/// `parent` shares. Returns `None` if `read_dir` fails — caller decides
/// the fallback. Returns the full `name` if every prefix length collides
/// with a sibling (e.g. an identical duplicate, which `read_dir` can't
/// actually return; defensive only).
///
/// The directory listing is capped at [`UNIQUE_PREFIX_READDIR_CAP`]
/// entries; on overflow we conservatively return `None` so the caller
/// falls back to a single character — better latency than a perfectly
/// minimal prefix on a directory with 50k siblings.
fn shortest_unique_prefix(parent: &Path, name: &str) -> Option<String> {
    let rd = std::fs::read_dir(parent).ok()?;
    let mut siblings: Vec<String> = Vec::new();
    let mut count = 0usize;
    for entry in rd.take(UNIQUE_PREFIX_READDIR_CAP + 1) {
        count += 1;
        if count > UNIQUE_PREFIX_READDIR_CAP {
            return None;
        }
        let entry = entry.ok()?;
        let file_name = entry.file_name();
        let s = file_name.to_string_lossy().into_owned();
        if s == name {
            continue;
        }
        siblings.push(s);
    }
    let name_chars: Vec<char> = name.chars().collect();
    if name_chars.is_empty() {
        return Some(String::new());
    }
    for take in 1..=name_chars.len() {
        let candidate: String = name_chars[..take].iter().collect();
        // `str::starts_with(&str)` is char-boundary safe, so prefix-by-chars
        // never mis-cuts a multi-byte codepoint.
        let collides = siblings.iter().any(|s| s.starts_with(&candidate));
        if !collides {
            return Some(candidate);
        }
    }
    Some(name.to_owned())
}

/// Return the first character of `s` as a `String`, or `s` cloned if it
/// is empty. Used as the IO-failure fallback in [`truncate_to_unique`].
fn first_char(s: &str) -> String {
    s.chars().next().map(|c| c.to_string()).unwrap_or_default()
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
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
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

    /// Construct a [`DirTruncate`] config with the given strategy / length.
    /// Builds via `Default::default()` + field assignment because
    /// `DirTruncate` is `#[non_exhaustive]` and struct-expression syntax
    /// (with or without `..Default::default()`) is forbidden from outside
    /// the defining crate.
    fn trunc(strategy: DirTruncateStrategy, length: u8) -> DirTruncate {
        let mut t = DirTruncate::default();
        t.strategy = strategy;
        t.length = length;
        t
    }

    #[test]
    fn truncate_to_last_keeps_only_n_components() {
        let cfg = trunc(DirTruncateStrategy::ToLast, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg, None), "\u{2026}/d/e");
    }

    #[test]
    fn truncate_middle_keeps_first_and_last_n() {
        let cfg = trunc(DirTruncateStrategy::Middle, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg, None), "/a/\u{2026}/e");
        let cfg3 = trunc(DirTruncateStrategy::Middle, 3);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg3, None), "/a/\u{2026}/d/e");
    }

    #[test]
    fn truncate_none_passes_through() {
        let cfg = trunc(DirTruncateStrategy::None, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg, None), "/a/b/c/d/e");
    }

    #[test]
    fn truncate_short_path_no_op() {
        // Component count (2: "a", "b") <= length (3) — return unchanged.
        let cfg = trunc(DirTruncateStrategy::ToLast, 3);
        assert_eq!(truncate_path("/a/b", &cfg, None), "/a/b");
        let mid = trunc(DirTruncateStrategy::Middle, 3);
        assert_eq!(truncate_path("/a/b", &mid, None), "/a/b");
    }

    #[test]
    fn truncate_with_home_collapse() {
        // The truncator runs after `home_collapse`, so `~` is the first
        // component. With ToLast/length=2 only the last two survive.
        let collapsed = home_collapse("/home/sean/proj/sub/deep", Some("/home/sean"));
        assert_eq!(collapsed, "~/proj/sub/deep");
        let cfg = trunc(DirTruncateStrategy::ToLast, 2);
        assert_eq!(truncate_path(&collapsed, &cfg, None), "\u{2026}/sub/deep");
    }

    #[test]
    fn truncate_length_zero_normalises_to_one() {
        // A misconfigured `length = 0` must not render an empty cwd.
        let cfg = trunc(DirTruncateStrategy::ToLast, 0);
        assert_eq!(truncate_path("/a/b/c", &cfg, None), "\u{2026}/c");
    }

    /// Unique-suffix helper for scratch dirs — avoids pulling `tempfile`
    /// in for one test (matches the existing `p10k-rs-config` pattern).
    fn scratch_suffix() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{}", std::process::id(), nanos)
    }

    #[test]
    fn truncate_to_unique_basic() {
        // Build a scratch tree with known siblings:
        //   scratch/alpha/<sibs>/...
        //     where scratch has children {alpha, alphabet}  -> "alpha" needs 5 chars to be unique
        //                                                     (collides with "alphabet" at prefixes 1..=5)
        //                                                     wait — at len=5 we have "alpha" vs "alpha"-prefix-of-"alphabet"
        //                                                     "alphabet".starts_with("alpha") → true, collides
        //                                                     so unique prefix is the full "alpha" (6 == name length).
        //                                                     The function returns the full name in that case.
        //   alpha has children {beta, gamma}                -> "beta" unique at 1 char ("b").
        //   beta has children {leaf}                        -> last component, preserved.
        //
        // We build:
        //   scratch_root/
        //     alpha/
        //       beta/
        //         leaf/
        //     alphabet/        (sibling to force prefix-collision at scratch_root level)
        //
        // truncate against fs_root = scratch_root/alpha/beta/leaf
        // textual path = "/<scratch_root>/alpha/beta/leaf" (full absolute form).
        //
        // The first three components on the textual path (everything in
        // scratch_root's ancestry) will be tested for unique prefixes
        // against their actual parents on disk — but we only assert on
        // the components we control (alpha, beta), since scratch_root's
        // ancestors are system dirs whose sibling counts vary.

        let root = std::env::temp_dir().join(format!("p10krs-to-unique-{}", scratch_suffix()));
        std::fs::create_dir_all(root.join("alpha").join("beta").join("leaf"))
            .expect("mkdir alpha/beta/leaf");
        std::fs::create_dir_all(root.join("alphabet")).expect("mkdir alphabet");

        let fs_root = root.join("alpha").join("beta").join("leaf");
        let textual = fs_root.display().to_string();
        let cfg = trunc(DirTruncateStrategy::ToUnique, 0);
        let out = truncate_path(&textual, &cfg, Some(&fs_root));

        // Final component is always preserved in full.
        assert!(out.ends_with("/leaf"), "leaf must be preserved: {out}");
        // "alpha" is the parent of "beta"; its siblings under scratch_root
        // are {"alphabet"}. "alpha" prefix collides with "alphabet" for
        // every length up to 5; at length 5 it's "alpha" which prefixes
        // "alphabet" too, so the function returns the full name.
        // Therefore the path still contains `/alpha/`.
        assert!(out.contains("/alpha/"), "alpha must survive in: {out}");
        // "beta" under "alpha" has no siblings; first-char prefix "b"
        // suffices (1-char unique).
        assert!(out.contains("/b/leaf"), "beta should shorten to 'b': {out}");

        // Clean up.
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn truncate_to_unique_handles_io_failure() {
        // Point fs_root at a path whose ancestors don't exist on disk.
        // `read_dir` on each parent fails → every non-final component
        // falls back to its first char; the leaf survives. The function
        // must not panic.
        let bogus = std::path::PathBuf::from(format!(
            "/definitely/does/not/exist/{}/{}/leaf",
            scratch_suffix(),
            "xyz"
        ));
        let textual = bogus.display().to_string();
        let cfg = trunc(DirTruncateStrategy::ToUnique, 0);
        let out = truncate_path(&textual, &cfg, Some(&bogus));

        // Final component preserved.
        assert!(
            out.ends_with("/leaf"),
            "leaf must be preserved on IO failure: {out}"
        );
        // Output starts with a leading slash (absolute path preserved).
        assert!(
            out.starts_with('/'),
            "leading slash preserved on IO failure: {out}"
        );
        // No panic and a reasonable (non-empty) result.
        assert!(!out.is_empty());
    }

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, cwd: &'a Path) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
        }
    }

    /// Path the platform guarantees is writable for the running process.
    /// Used by tests that need a known-writable cwd so the new slice-39
    /// `not_writable` probe can't trip them. `std::env::temp_dir()`
    /// returns `$TMPDIR` (Unix) or `%TEMP%` (Windows) and is created with
    /// the running user as owner — so `access(W_OK)` will succeed.
    fn writable_scratch() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn renders_with_default_folder_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = writable_scratch();
        let out = Dir.render(&ctx(&cfg, &env, &path));
        assert!(
            out.text.contains('\u{f07b}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f07b}"));
        // Slice 28A: blue background SGR present and declared on the output
        // so the renderer can paint matching powerline arrows.
        assert!(
            out.text.contains("\x1b[48;5;4m"),
            "blue bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("blue".into())));
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

    #[test]
    fn writable_state_keeps_blue() {
        // The scratch dir is owner-writable; the writability probe must
        // return `Ok(())` and we should land on the P10K-classic blue
        // palette plus the writable state tag.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = writable_scratch();
        let out = Dir.render(&ctx(&cfg, &env, &path));
        assert_eq!(out.state, Some("writable"));
        assert_eq!(out.icon, Some("\u{f07b}"));
        assert!(
            out.text.contains("\x1b[48;5;4m"),
            "blue bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("blue".into())));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn not_writable_state_shifts_palette() {
        // `/proc/1` exists for every Linux process tree and is *never*
        // writable by an unprivileged user (root-owned, mode 555 on the
        // pid dir). access(W_OK) returns EACCES → the segment lands on
        // the not_writable state and the yellow warning palette.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/proc/1");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert_eq!(out.state, Some("not_writable"));
        // Padlock glyph swaps in for the folder default.
        assert_eq!(out.icon, Some("\u{f023}"));
        assert!(
            out.text.contains('\u{f023}'),
            "padlock icon missing: {:?}",
            out.text
        );
        // Yellow bg (`48;5;3` in Ansi256) — the P10K warning hue.
        assert!(
            out.text.contains("\x1b[48;5;3m"),
            "yellow bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("yellow".into())));
    }
}
