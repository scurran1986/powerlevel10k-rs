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

pub mod ai_host;
pub mod anaconda;
pub mod aws;
pub mod background_jobs;
pub mod command_execution_time;
pub mod context;
pub mod dir;
pub mod kubecontext;
pub mod node_version;
pub mod nodenv;
pub mod os_icon;
pub mod prompt_char;
pub mod pyenv;
pub mod python_version;
pub mod root_indicator;
pub mod rust_version;
pub mod status;
pub mod terraform;
pub mod time;
pub mod vcs;
pub mod vi_mode;
pub mod virtualenv;

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
        // AI-host badge (slice 46): visible only when running inside a
        // detected AI shell (Claude Code, Aider, Cursor).
        "ai_host",
    ]
}

/// Construct a `Segment` instance by its config name.
///
/// Returns `None` for names not in this build's MVP set. Callers are expected
/// to log and skip — an unknown name in a user config should be a non-fatal
/// `tracing::warn!`, not an error.
///
/// All 21 segments enumerated in [`segment_names`] are now wired.
#[must_use]
pub fn build(name: &str) -> Option<Box<dyn Segment>> {
    match name {
        "ai_host" => Some(Box::new(ai_host::AiHost)),
        "anaconda" => Some(Box::new(anaconda::Anaconda)),
        "aws" => Some(Box::new(aws::Aws)),
        "background_jobs" => Some(Box::new(background_jobs::BackgroundJobs)),
        "command_execution_time" => Some(Box::new(command_execution_time::CommandExecutionTime)),
        "context" => Some(Box::new(context::Context)),
        "dir" => Some(Box::new(dir::Dir)),
        "kubecontext" => Some(Box::new(kubecontext::Kubecontext)),
        "node_version" => Some(Box::new(node_version::NodeVersion)),
        "nodenv" => Some(Box::new(nodenv::Nodenv)),
        "os_icon" => Some(Box::new(os_icon::OsIcon)),
        "prompt_char" => Some(Box::new(prompt_char::PromptChar)),
        "pyenv" => Some(Box::new(pyenv::Pyenv)),
        "python_version" => Some(Box::new(python_version::PythonVersion)),
        "root_indicator" => Some(Box::new(root_indicator::RootIndicator)),
        "rust_version" => Some(Box::new(rust_version::RustVersion)),
        "status" => Some(Box::new(status::Status)),
        "terraform" => Some(Box::new(terraform::Terraform)),
        "time" => Some(Box::new(time::Time)),
        "vcs" => Some(Box::new(vcs::Vcs)),
        "vi_mode" => Some(Box::new(vi_mode::ViMode)),
        "virtualenv" => Some(Box::new(virtualenv::Virtualenv)),
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
            "ai_host",
            "anaconda",
            "aws",
            "background_jobs",
            "command_execution_time",
            "context",
            "dir",
            "kubecontext",
            "node_version",
            "nodenv",
            "os_icon",
            "prompt_char",
            "pyenv",
            "python_version",
            "root_indicator",
            "rust_version",
            "status",
            "terraform",
            "time",
            "vcs",
            "vi_mode",
            "virtualenv",
        ] {
            let seg = build(name).unwrap_or_else(|| panic!("build({name}) returned None"));
            assert_eq!(seg.name(), name, "build({name}).name() mismatch");
        }
    }

    #[test]
    fn build_covers_every_advertised_segment_name() {
        // Cross-check: every name in `segment_names()` MUST resolve via `build()`.
        // Tracks the MVP-SPEC § 1.2 invariant — advertisement matches reality.
        for name in super::segment_names() {
            assert!(
                build(name).is_some(),
                "segment_names() advertises '{name}' but build('{name}') returned None"
            );
        }
    }

    #[test]
    fn importer_known_segment_names_match() {
        // The p9k importer in `p10k-rs-config::import` keeps a static snapshot
        // of segment names so it can disambiguate per-segment color variables
        // without depending on this crate. Pin the lists in sync — any new
        // segment added here must also appear there.
        let mut local: Vec<&'static str> = super::segment_names().to_vec();
        let mut remote: Vec<&'static str> = p10k_rs_config::import::KNOWN_SEGMENT_NAMES.to_vec();
        local.sort_unstable();
        remote.sort_unstable();
        assert_eq!(
            local, remote,
            "segment_names() and p10k_rs_config::import::KNOWN_SEGMENT_NAMES drifted"
        );
    }

    #[test]
    fn build_returns_none_for_unknown() {
        assert!(build("definitely_not_a_segment").is_none());
        assert!(build("").is_none());
    }
}
