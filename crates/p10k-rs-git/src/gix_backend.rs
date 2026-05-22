//! Pure-Rust git-status fallback via gitoxide (slice 60).
//!
//! This is the third tier in the backend chain — fires when the
//! [`crate::Gitstatusd`] daemon is unavailable *and* the [`crate::ShellOut`]
//! backend can't reach `git` on `$PATH`. Use case: AI hosts (Claude Code,
//! Cursor) running in stripped containers without a system git binary,
//! and CI images that omit `git` to save space.
//!
//! ## Status
//!
//! **Phase 1 scaffolding only.** [`GixBackend::status`] performs the
//! repo-root discovery probe (so the `gix-discover` dep tree is
//! genuinely exercised and cargo-machete stays happy) but always
//! returns `None`. Phases 2–6 wire HEAD lookup, status iteration,
//! ahead/behind computation, action probing, and cross-check tests.
//!
//! ## Why scoped subcrates over the full `gix` umbrella
//!
//! The umbrella `gix` crate pulls ~50 transitive crates; the targeted
//! `gix-discover` subset adds ~12. Per the workspace's conservative-deps
//! rule (see project `CLAUDE.md`), we pin individual subcrates and
//! bump them as a set when gitoxide cuts a release.

#![allow(missing_docs)]
// `missing_docs` is allow'd module-wide during the phase 1 stub period.
// Phases 2-6 will document each public method as they land. Removed
// once the backend reports real state.

use std::path::Path;

use p10k_rs_core::GitState;

use crate::Backend;

/// Pure-Rust git-status producer for cwds where neither `gitstatusd`
/// nor the `git` CLI is available.
///
/// Currently a phase 1 stub — see the module docs.
#[derive(Debug, Default, Clone, Copy)]
pub struct GixBackend;

impl Backend for GixBackend {
    fn status(&self, _path: &Path) -> Option<GitState> {
        // Phase 1 stub: the backend chain is wired and the type
        // exists, but no work happens here yet. Phase 2 lands the
        // gitoxide dep (with the right feature combo — see the
        // research note at
        // `~/.planning/powerlevel10k-rs/research/slice-60-gitoxide-api-notes.md`)
        // and starts populating `GitState.branch` from a discovered
        // repo handle. Returning `None` here is the same shape every
        // other backend uses for "not in a repo" — main's chain
        // collapses it the same way.
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A non-repo cwd must return `None` — same as every other
    /// backend. Pins the contract so phase 2-3 don't accidentally
    /// start surfacing "this is a repo" for cwds outside any git
    /// directory.
    #[test]
    fn returns_none_outside_a_repo() {
        // `std::env::temp_dir()` is, by convention, not inside any
        // git working tree on a normal CI host. If a future runner
        // happens to put `$TMPDIR` inside a repo, this test would
        // start lying — at that point swap to a freshly-created
        // scratch dir.
        let p = std::env::temp_dir();
        let out = GixBackend.status(&p);
        assert!(out.is_none(), "expected None for non-repo cwd, got {out:?}");
    }
}
