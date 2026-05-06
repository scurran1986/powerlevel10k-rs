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
