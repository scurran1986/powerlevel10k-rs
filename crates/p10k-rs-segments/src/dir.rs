//! `dir` — current working directory segment.
//!
//! Slice 1: emit the cwd as plain text, with `$HOME` collapsed to `~`. Color,
//! truncation policies, and the writable/read-only state come in later
//! slices. The point of this version is to prove the render pipeline works
//! end-to-end with at least one segment that has real input.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

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
        let raw = ctx.cwd.display().to_string();
        let home = std::env::var("HOME").ok();
        let text = home_collapse(&raw, home.as_deref());
        let plain_len = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
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
}
