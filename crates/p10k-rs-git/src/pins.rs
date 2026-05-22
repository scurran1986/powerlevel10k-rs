//! gitstatusd binary sha256 pins (T0.5).
//!
//! Parses `crates/p10k-rs-git/data/gitstatusd-pins.toml` — the single
//! source of truth shared with `install.sh` and the `p10k-rs verify`
//! subcommand. The TOML schema is intentionally flat so the bash side
//! can read it with `awk`; this module gives Rust callers a typed view.
//!
//! Layout:
//!
//! ```toml
//! version = "v1.5.4"
//!
//! [pins.x86_64-linux-gnu]
//! upstream_file  = "gitstatusd-linux-x86_64"
//! tarball_sha256 = "..."
//! binary_sha256  = "..."
//! ```
//!
//! `version` is the upstream `romkatv/gitstatus` release tag every pin
//! refers to. `pins.<triple>` maps a rustc-shaped host triple to the
//! upstream artifact filename plus its tarball / extracted-binary
//! sha256 digests.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The pin file committed at
/// `crates/p10k-rs-git/data/gitstatusd-pins.toml`.
///
/// Baked into the binary at build time via [`include_str!`] (see
/// [`PINS_TOML`]) so a `cargo install`d `p10k-rs` doesn't need the
/// repo on disk to run `p10k-rs verify`. install.sh reads the same
/// file directly from the repo on each invocation.
pub const PINS_TOML: &str = include_str!("../data/gitstatusd-pins.toml");

/// Parsed view of the pin file.
///
/// Stable schema. Callers should access `pins` via [`Pins::for_triple`]
/// rather than poking the `BTreeMap` directly so the lookup stays
/// case-stable and triple-normalised.
#[derive(Debug, Clone, Deserialize)]
pub struct Pins {
    /// Upstream `romkatv/gitstatus` release tag every pin refers to.
    /// Example: `"v1.5.4"`.
    pub version: String,
    /// Per-triple pin entries keyed by `"<arch>-<os>-<abi>"` strings
    /// matching rustc target-triple shape (`"x86_64-linux-gnu"`,
    /// `"aarch64-darwin"`, …).
    #[serde(default)]
    pub pins: BTreeMap<String, PinEntry>,
}

/// One per-triple pin entry.
///
/// `upstream_file` is the filename upstream actually publishes
/// (without the `.tar.gz` suffix) — kept separate from the triple key
/// because upstream's naming convention (`<uname_s>-<uname_m>`) differs
/// from rustc's target-triple shape.
#[derive(Debug, Clone, Deserialize)]
pub struct PinEntry {
    /// Upstream artifact filename, no `.tar.gz`. The release URL is
    /// `https://github.com/romkatv/gitstatus/releases/download/{version}/{upstream_file}.tar.gz`.
    pub upstream_file: String,
    /// sha256 of the released `.tar.gz` archive. install.sh verifies
    /// this before extraction.
    pub tarball_sha256: String,
    /// sha256 of the extracted binary on disk. `p10k-rs verify`
    /// re-hashes the installed file against this.
    pub binary_sha256: String,
}

/// Errors produced when loading or querying the pin file.
#[derive(Debug, thiserror::Error)]
pub enum PinsError {
    /// The embedded pin file failed to parse as TOML. Indicates the
    /// committed file is malformed — caught by the test suite.
    #[error("failed to parse pins TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// No pin entry exists for the supplied host triple. The verify
    /// subcommand turns this into an `UNSUPPORTED_ARCH` exit code.
    #[error("no pin entry for host triple {0}")]
    UnsupportedTriple(String),
}

impl Pins {
    /// Parse a pin file from a TOML string.
    ///
    /// Use [`Pins::load_embedded`] for the production path; this is
    /// the seam tests use to drive a fixture pin file.
    pub fn from_toml(s: &str) -> Result<Self, PinsError> {
        Ok(toml::from_str(s)?)
    }

    /// Parse the pin file baked into the binary at build time.
    ///
    /// Returns an error only if the committed pin file no longer
    /// parses — a programming error caught by `cargo test` before
    /// release.
    pub fn load_embedded() -> Result<Self, PinsError> {
        Self::from_toml(PINS_TOML)
    }

    /// Look up the pin entry for a host triple. Returns
    /// [`PinsError::UnsupportedTriple`] if no entry exists.
    pub fn for_triple(&self, triple: &str) -> Result<&PinEntry, PinsError> {
        self.pins
            .get(triple)
            .ok_or_else(|| PinsError::UnsupportedTriple(triple.to_owned()))
    }
}

/// Detect the current host's canonical triple in the same shape the
/// pin file's `[pins.<triple>]` headers use.
///
/// Mirrors the bash `detect_host_triple` in `install.sh`; the two must
/// agree on the four supported strings:
///
/// - `x86_64-linux-gnu`
/// - `aarch64-linux-gnu`
/// - `x86_64-darwin`
/// - `aarch64-darwin`
///
/// Returns `None` on any other host. Callers map that to an
/// `UNSUPPORTED_ARCH` user-facing message rather than a panic — a
/// freshly-bootstrapped exotic host should still get a useful error,
/// not a backtrace.
#[must_use]
pub fn detect_host_triple() -> Option<&'static str> {
    // Resolved at compile time per build target. Keeping it const lets
    // `p10k-rs verify` on a cross-compiled binary report the triple it
    // was *built for*, which is what the user actually cares about
    // (their downloaded gitstatusd is matched to the same triple).
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-linux-gnu"),
        ("macos", "x86_64") => Some("x86_64-darwin"),
        ("macos", "aarch64") => Some("aarch64-darwin"),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn embedded_pins_parse() {
        // The committed pin file must always parse. If this fails, the
        // pin-probe workflow's PR landed broken TOML.
        let pins = Pins::load_embedded().expect("embedded pins must parse");
        assert!(
            pins.version.starts_with('v'),
            "version should look like 'v1.5.x', got {:?}",
            pins.version
        );
        // All four canonical triples are present and have non-empty
        // tarball + binary sha digests.
        for triple in [
            "x86_64-linux-gnu",
            "aarch64-linux-gnu",
            "x86_64-darwin",
            "aarch64-darwin",
        ] {
            let entry = pins.for_triple(triple).unwrap_or_else(|e| {
                panic!("triple {triple} should have a pin entry, got {e}");
            });
            assert!(
                !entry.upstream_file.is_empty(),
                "{triple}: upstream_file empty"
            );
            assert_eq!(
                entry.tarball_sha256.len(),
                64,
                "{triple}: tarball sha256 should be 64 hex chars, got {}",
                entry.tarball_sha256.len()
            );
            assert_eq!(
                entry.binary_sha256.len(),
                64,
                "{triple}: binary sha256 should be 64 hex chars, got {}",
                entry.binary_sha256.len()
            );
            assert!(
                entry.tarball_sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{triple}: tarball sha256 must be ASCII hex"
            );
            assert!(
                entry.binary_sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{triple}: binary sha256 must be ASCII hex"
            );
        }
    }

    #[test]
    fn unsupported_triple_returns_typed_error() {
        let pins = Pins::load_embedded().unwrap();
        let err = pins
            .for_triple("riscv64-illumos-musl")
            .expect_err("unsupported triple must error");
        assert!(matches!(err, PinsError::UnsupportedTriple(_)));
    }

    #[test]
    fn malformed_toml_returns_parse_error() {
        let err = Pins::from_toml("not valid toml = =").expect_err("garbage must fail");
        assert!(matches!(err, PinsError::Parse(_)));
    }

    #[test]
    fn detect_host_triple_returns_known_value_on_supported_hosts() {
        // The four supported triples are exactly the ones in the pin
        // file. Drift between the two would mean `p10k-rs verify` on
        // a perfectly normal host returns UNSUPPORTED_ARCH.
        if let Some(triple) = detect_host_triple() {
            let pins = Pins::load_embedded().unwrap();
            assert!(
                pins.for_triple(triple).is_ok(),
                "detected triple {triple} is not in the pin file"
            );
        }
    }
}
