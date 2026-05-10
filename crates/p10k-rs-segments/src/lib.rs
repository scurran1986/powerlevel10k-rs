//! Segment implementations for `p10k-rs`.
//!
//! Each segment is a small struct that implements
//! [`p10k_rs_core::Segment`]. The MVP set is enumerated in `MVP-SPEC.md`
//! § 1.2: 20 segments split between always-on, auto-detected, and
//! useful-enough-to-bundle. Anything outside that set lives behind a future
//! feature flag — see `01-segments.md` for the full P0/P1/P2 catalogue.
//!
//! The crate is otherwise a thin assembly point. Heavy logic per segment
//! lives in submodules; this lib.rs is the public registry.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use p10k_rs_core::Segment;

pub mod command_execution_time;
pub mod dir;
pub mod prompt_char;
pub mod status;
pub mod vcs;

/// Returns the static list of segment names this crate ships.
///
/// Intentionally returns a slice of `&'static str`s rather than constructed
/// `Segment` objects — the binary builds segment instances lazily once the
/// config picks the requested subset.
#[must_use]
pub fn segment_names() -> &'static [&'static str] {
    &[
        // Always-on (MVP-SPEC § 1.2).
        "dir",
        "prompt_char",
        "status",
        "command_execution_time",
        "background_jobs",
        "time",
        "context",
        "vi_mode",
        "root_indicator",
        "vcs",
        // Auto-detected.
        "virtualenv",
        "anaconda",
        "pyenv",
        "nodenv",
        "kubecontext",
        "terraform",
        "aws",
        "os_icon",
        // Useful enough to bundle.
        "node_version",
        "python_version",
        "rust_version",
    ]
}

/// Build the default layout: dir, vcs, command-execution-time, prompt-char.
///
/// Returns segment instances in render order. Each segment's `enabled()` is
/// the gate (`vcs` hides when not in a repo, `command_execution_time` hides
/// below the 3-second threshold).
///
/// Kept as a backstop while slice 13 wires the config loader in. Slice 13.5
/// will delete this in favour of the [`build`] registry once the binary
/// always assembles its layout from a `Config`.
#[must_use]
pub fn default_layout() -> Vec<Box<dyn Segment>> {
    vec![
        Box::new(dir::Dir),
        Box::new(vcs::Vcs),
        Box::new(command_execution_time::CommandExecutionTime),
        Box::new(status::Status),
        Box::new(prompt_char::PromptChar),
    ]
}

/// Construct a `Segment` instance by its config name.
///
/// Returns `None` for names not in this build's MVP set. Callers are expected
/// to log and skip — an unknown name in a user config should be a non-fatal
/// `tracing::warn!`, not an error.
///
/// Mapping today (slice 13):
///
/// | name | type |
/// |------|------|
/// | `dir` | [`dir::Dir`] |
/// | `vcs` | [`vcs::Vcs`] |
/// | `command_execution_time` | [`command_execution_time::CommandExecutionTime`] |
/// | `status` | [`status::Status`] |
/// | `prompt_char` | [`prompt_char::PromptChar`] |
///
/// Names from [`segment_names`] that don't appear above (e.g. `time`,
/// `kubecontext`) are accepted by [`segment_names`] as advertisement but
/// not yet implemented; `build` returns `None` for them and the renderer
/// drops the slot. They light up segment-by-segment in later slices.
#[must_use]
pub fn build(name: &str) -> Option<Box<dyn Segment>> {
    match name {
        "dir" => Some(Box::new(dir::Dir)),
        "vcs" => Some(Box::new(vcs::Vcs)),
        "command_execution_time" => Some(Box::new(command_execution_time::CommandExecutionTime)),
        "status" => Some(Box::new(status::Status)),
        "prompt_char" => Some(Box::new(prompt_char::PromptChar)),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::build;

    #[test]
    fn build_returns_known_segments() {
        for name in [
            "dir",
            "vcs",
            "command_execution_time",
            "status",
            "prompt_char",
        ] {
            let seg = build(name).unwrap_or_else(|| panic!("build({name}) returned None"));
            assert_eq!(seg.name(), name, "build({name}).name() mismatch");
        }
    }

    #[test]
    fn build_returns_none_for_unknown() {
        assert!(build("definitely_not_a_segment").is_none());
        assert!(build("").is_none());
    }
}
