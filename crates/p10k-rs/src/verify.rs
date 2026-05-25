//! `p10k-rs verify` subcommand (T0.5).
//!
//! Re-hashes the gitstatusd helper binary on disk and compares its
//! sha256 against the committed pin in
//! `crates/p10k-rs-git/data/gitstatusd-pins.toml` (embedded at build
//! time via [`p10k_rs_git::pins::PINS_TOML`]).
//!
//! Output is one stable line per outcome, with distinct exit codes so
//! a CI script can branch on the result without parsing the text:
//!
//! | Outcome           | Stdout                                    | Exit |
//! |-------------------|-------------------------------------------|------|
//! | Match             | `OK <triple> <version> <sha-prefix>`      | 0    |
//! | Sha differs       | `MISMATCH expected=<hex> got=<hex>`       | 2    |
//! | Binary missing    | `NOT_FOUND <reason>`                      | 3    |
//! | Host arch unknown | `UNSUPPORTED_ARCH <triple>`               | 4    |
//!
//! `install.sh` runs the same comparison in bash before placing the
//! binary; this subcommand is the user-facing version they can run
//! anytime — e.g. after `brew upgrade`, when they suspect tampering,
//! or to confirm the helper their distro packaged matches the pin.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use p10k_rs_git::pins::{self, Pins};

/// Outcome of a verify run. `exit_code` and `render` together form the
/// subcommand's full wire contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VerifyOutcome {
    /// Binary's sha256 matched the pin entry for the current host.
    Ok {
        /// Canonical host triple the pin entry was looked up under.
        triple: String,
        /// `version` field from the pin file (e.g. `"v1.5.4"`).
        version: String,
        /// First 12 hex chars of the matched sha256, for terse output.
        sha_prefix: String,
    },
    /// Binary exists and was hashed, but its digest does not match the
    /// pin. The full 64-char hex strings are surfaced so the user can
    /// paste them into a bug report.
    Mismatch {
        /// Expected sha256, lowercase hex, 64 chars.
        expected: String,
        /// Actual sha256 of the on-disk binary.
        got: String,
    },
    /// Binary could not be located or could not be read.
    NotFound(String),
    /// Host arch / OS combination is not in the pin file. Includes the
    /// triple we tried so the user can file a "please add my arch" bug.
    UnsupportedArch(String),
}

impl VerifyOutcome {
    /// Exit code the subcommand returns for this outcome. See module
    /// docs for the table.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::Ok { .. } => 0,
            Self::Mismatch { .. } => 2,
            Self::NotFound(_) => 3,
            Self::UnsupportedArch(_) => 4,
        }
    }

    /// One-line stdout representation. Stable wire format — downstream
    /// scripts and the SECURITY.md verification recipe both depend on
    /// the exact shape.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Ok {
                triple,
                version,
                sha_prefix,
            } => format!("OK {triple} {version} {sha_prefix}"),
            Self::Mismatch { expected, got } => {
                format!("MISMATCH expected={expected} got={got}")
            }
            Self::NotFound(reason) => format!("NOT_FOUND {reason}"),
            Self::UnsupportedArch(triple) => format!("UNSUPPORTED_ARCH {triple}"),
        }
    }

    /// Machine-readable JSON representation. Field set per variant:
    ///
    /// - `Ok`:              `{"status":"OK","triple":"<t>","version":"<v>","sha_prefix":"<12hex>"}`
    /// - `Mismatch`:        `{"status":"MISMATCH","expected":"<64hex>","got":"<64hex>"}`
    /// - `NotFound`:        `{"status":"NOT_FOUND","reason":"<escaped>"}`
    /// - `UnsupportedArch`: `{"status":"UNSUPPORTED_ARCH","triple":"<t>"}`
    ///
    /// Hand-rolled to avoid pulling `serde_json` for one call site —
    /// matches the encoder pattern used by `daemon-health --json`
    /// (v0.1.10) and `version --json` (v0.2.1). Strings are
    /// JSON-escaped via [`json_escape`]; the sha hex strings can't
    /// contain anything that needs escaping but pass through the
    /// encoder anyway for symmetry.
    pub(crate) fn render_json(&self) -> String {
        match self {
            Self::Ok {
                triple,
                version,
                sha_prefix,
            } => format!(
                "{{\"status\":\"OK\",\"triple\":\"{}\",\"version\":\"{}\",\"sha_prefix\":\"{}\"}}",
                json_escape(triple),
                json_escape(version),
                json_escape(sha_prefix)
            ),
            Self::Mismatch { expected, got } => format!(
                "{{\"status\":\"MISMATCH\",\"expected\":\"{}\",\"got\":\"{}\"}}",
                json_escape(expected),
                json_escape(got)
            ),
            Self::NotFound(reason) => format!(
                "{{\"status\":\"NOT_FOUND\",\"reason\":\"{}\"}}",
                json_escape(reason)
            ),
            Self::UnsupportedArch(triple) => format!(
                "{{\"status\":\"UNSUPPORTED_ARCH\",\"triple\":\"{}\"}}",
                json_escape(triple)
            ),
        }
    }
}

/// Minimal JSON string escape — quote, backslash, C0 controls. Same
/// pattern as `daemon_health::json_escape` and main's
/// `json_escape_str`; hand-rolled per call site rather than hoisted
/// because the project doesn't otherwise pull `serde_json` and a
/// "json utils" module would be premature abstraction for three
/// small uses.
fn json_escape(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Crate-public alias for [`sha256_of_path`] so sibling modules
/// (notably `doctor`) can re-use the stream-hasher without duplicating
/// the buffer-sizing rationale. The `pub` is `pub(crate)`; the wire
/// surface is unchanged.
pub(crate) fn sha256_of_path_pub(path: &Path) -> std::io::Result<String> {
    sha256_of_path(path)
}

/// Stream-hash `path` with SHA-256 and return the 64-char lowercase
/// hex digest.
///
/// The helper binary is on the order of MB; streamed reads keep memory
/// flat regardless of size, and the 8 `KiB` buffer is small enough to
/// stay in L1 while still amortising the per-`read(2)` syscall cost.
fn sha256_of_path(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute the verify outcome given a parsed pin set and an optional
/// explicit binary path.
///
/// Pure modulo the filesystem read of the binary and the auto-locate
/// fallback. The test suite drives every branch by passing a synthetic
/// [`Pins`] plus a scratch binary path, so the locator branch is the
/// only one that needs a real binary on disk.
pub(crate) fn verify_with_pins(binary_path: Option<&Path>, pins: &Pins) -> VerifyOutcome {
    // Resolve which triple we're on first — if it's not in the pin
    // table at all, the binary path is irrelevant.
    let Some(triple) = pins::detect_host_triple() else {
        return VerifyOutcome::UnsupportedArch(format!(
            "{}-{}",
            std::env::consts::ARCH,
            std::env::consts::OS,
        ));
    };
    let Ok(entry) = pins.for_triple(triple) else {
        return VerifyOutcome::UnsupportedArch(triple.to_owned());
    };
    // Locate the binary: explicit `--binary <path>` wins, else the
    // same `$P10K_RS_GITSTATUSD_BIN` → `$PATH` probe the prompt path
    // uses, so verify checks the exact binary the prompt would load.
    let path = match binary_path {
        Some(p) => p.to_path_buf(),
        None => match p10k_rs_git::locate_gitstatusd() {
            Some(p) => p,
            None => {
                return VerifyOutcome::NotFound(
                    "no gitstatusd on PATH or via $P10K_RS_GITSTATUSD_BIN".to_owned(),
                );
            }
        },
    };
    let got = match sha256_of_path(&path) {
        Ok(s) => s,
        Err(e) => return VerifyOutcome::NotFound(format!("{}: {}", path.display(), e)),
    };
    if got == entry.binary_sha256 {
        let sha_prefix: String = got.chars().take(12).collect();
        VerifyOutcome::Ok {
            triple: triple.to_owned(),
            version: pins.version.clone(),
            sha_prefix,
        }
    } else {
        VerifyOutcome::Mismatch {
            expected: entry.binary_sha256.clone(),
            got,
        }
    }
}

/// `p10k-rs verify [--binary <path>]` entrypoint.
///
/// Loads the embedded pin file (parsing failure is a release-blocking
/// bug surfaced via the test `embedded_pins_parse`), runs the verify
/// computation, prints the stable wire line, and exits with the
/// outcome's mapped code so callers can branch on it without parsing.
pub(crate) fn cmd_verify(binary_path: Option<&Path>, json: bool) -> Result<()> {
    let pins = Pins::load_embedded().context("loading embedded gitstatusd pins")?;
    let outcome = verify_with_pins(binary_path, &pins);
    if json {
        println!("{}", outcome.render_json());
    } else {
        println!("{}", outcome.render());
    }
    let code = outcome.exit_code();
    if code == 0 {
        Ok(())
    } else {
        std::process::exit(code)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{sha256_of_path, verify_with_pins, VerifyOutcome};
    use p10k_rs_git::pins::{self, Pins};
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    /// Build a scratch file path under `std::env::temp_dir()`. Matches
    /// the `scratch_git_dir` helper in `p10k-rs-git/src/lib.rs` rather
    /// than pulling in `tempfile` (workspace policy avoids the dep).
    fn scratch_path(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "p10krs-verify-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ))
    }

    /// Build a synthetic pin set with exactly one entry for `triple`.
    /// Lets tests drive every branch without depending on the committed
    /// pin file's contents (which change as upstream releases).
    fn fixture_pins(triple: &str, binary_sha: &str) -> Pins {
        let toml = format!(
            "version = \"v1.2.3\"\n\
             [pins.{triple}]\n\
             upstream_file = \"gitstatusd-fixture\"\n\
             tarball_sha256 = \"{tarball}\"\n\
             binary_sha256 = \"{binary_sha}\"\n",
            tarball = "0".repeat(64),
        );
        Pins::from_toml(&toml).unwrap()
    }

    #[test]
    fn sha256_of_path_matches_known_vector() {
        // sha256("hello") = 2cf2...9824 — RFC fixture. Pins the streaming
        // hasher against arithmetic regressions in case we ever swap the
        // buffer size or backend crate.
        let p = scratch_path("hello");
        std::fs::write(&p, b"hello").unwrap();
        let got = sha256_of_path(&p).unwrap();
        assert_eq!(
            got,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn ok_when_sha_matches() {
        let Some(triple) = pins::detect_host_triple() else {
            // Exotic host (riscv etc.) — no triple, no path to exercise
            // here. The unsupported-arch test below covers that branch.
            return;
        };
        let p = scratch_path("ok");
        let bytes = b"matched binary content";
        std::fs::write(&p, bytes).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let sha = format!("{:x}", hasher.finalize());

        let pins = fixture_pins(triple, &sha);
        let outcome = verify_with_pins(Some(&p), &pins);
        match &outcome {
            VerifyOutcome::Ok {
                triple: t,
                version,
                sha_prefix,
            } => {
                assert_eq!(t, triple);
                assert_eq!(version, "v1.2.3");
                assert_eq!(sha_prefix.len(), 12);
                assert!(sha.starts_with(sha_prefix));
            }
            other => panic!("expected Ok, got {other:?}"),
        }
        assert_eq!(outcome.exit_code(), 0);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn mismatch_when_sha_differs() {
        let Some(triple) = pins::detect_host_triple() else {
            return;
        };
        let p = scratch_path("mm");
        std::fs::write(&p, b"actual bytes that won't match").unwrap();
        // Plausible-looking but wrong sha — 64 hex chars of `f`.
        let pins = fixture_pins(triple, &"f".repeat(64));
        let outcome = verify_with_pins(Some(&p), &pins);
        match &outcome {
            VerifyOutcome::Mismatch { expected, got } => {
                assert_eq!(expected.len(), 64);
                assert_eq!(got.len(), 64);
                assert_ne!(expected, got);
            }
            other => panic!("expected Mismatch, got {other:?}"),
        }
        assert_eq!(outcome.exit_code(), 2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn not_found_when_explicit_path_missing() {
        let Some(triple) = pins::detect_host_triple() else {
            return;
        };
        // Deliberately don't `write` to this path — the file doesn't exist.
        let p = scratch_path("nope");
        let pins = fixture_pins(triple, &"a".repeat(64));
        let outcome = verify_with_pins(Some(&p), &pins);
        assert!(
            matches!(outcome, VerifyOutcome::NotFound(_)),
            "expected NotFound, got {outcome:?}"
        );
        assert_eq!(outcome.exit_code(), 3);
    }

    #[test]
    fn unsupported_arch_when_triple_missing_from_pins() {
        // Pin file with NO entry for our triple should yield
        // UnsupportedArch — same exit code as a truly unknown host arch.
        let Some(_triple) = pins::detect_host_triple() else {
            return;
        };
        let pins = fixture_pins("riscv64-illumos-musl", &"a".repeat(64));
        let outcome = verify_with_pins(None, &pins);
        assert!(matches!(outcome, VerifyOutcome::UnsupportedArch(_)));
        assert_eq!(outcome.exit_code(), 4);
    }

    #[test]
    fn render_format_is_stable() {
        // Pinning the wire format — SECURITY.md documents these exact
        // strings as the verification recipe for downstream packagers.
        let ok = VerifyOutcome::Ok {
            triple: "x86_64-linux-gnu".to_owned(),
            version: "v1.5.4".to_owned(),
            sha_prefix: "02b7bc11a70a".to_owned(),
        };
        assert_eq!(ok.render(), "OK x86_64-linux-gnu v1.5.4 02b7bc11a70a");

        let mm = VerifyOutcome::Mismatch {
            expected: "aa".repeat(32),
            got: "bb".repeat(32),
        };
        assert!(mm.render().starts_with("MISMATCH expected="));
        assert!(mm.render().contains(" got="));

        let nf = VerifyOutcome::NotFound("oops".to_owned());
        assert_eq!(nf.render(), "NOT_FOUND oops");

        let ua = VerifyOutcome::UnsupportedArch("powerpc64-aix".to_owned());
        assert_eq!(ua.render(), "UNSUPPORTED_ARCH powerpc64-aix");
    }

    #[test]
    fn render_json_format_is_stable() {
        // Same pinning as the text form, JSON shape this time. Consumers
        // scripting against `verify --json` depend on these exact field
        // names and ordering (the encoder is hand-rolled per call site,
        // so order is determined by the format!() call).
        let ok = VerifyOutcome::Ok {
            triple: "x86_64-linux-gnu".to_owned(),
            version: "v1.5.4".to_owned(),
            sha_prefix: "02b7bc11a70a".to_owned(),
        };
        assert_eq!(
            ok.render_json(),
            r#"{"status":"OK","triple":"x86_64-linux-gnu","version":"v1.5.4","sha_prefix":"02b7bc11a70a"}"#
        );

        let mm = VerifyOutcome::Mismatch {
            expected: "aa".repeat(32),
            got: "bb".repeat(32),
        };
        let mm_json = mm.render_json();
        assert!(mm_json.starts_with(r#"{"status":"MISMATCH","expected":""#));
        assert!(mm_json.contains(r#"","got":""#));

        let nf = VerifyOutcome::NotFound("oops".to_owned());
        assert_eq!(
            nf.render_json(),
            r#"{"status":"NOT_FOUND","reason":"oops"}"#
        );

        let ua = VerifyOutcome::UnsupportedArch("powerpc64-aix".to_owned());
        assert_eq!(
            ua.render_json(),
            r#"{"status":"UNSUPPORTED_ARCH","triple":"powerpc64-aix"}"#
        );
    }

    #[test]
    fn render_json_escapes_reason_quotes_and_backslashes() {
        // NotFound carries an OS error message; nothing prevents weird
        // path chars from reaching the encoder. Pin the escape pass.
        let nf = VerifyOutcome::NotFound(r#"path "/weird\name" failed"#.to_owned());
        assert_eq!(
            nf.render_json(),
            r#"{"status":"NOT_FOUND","reason":"path \"/weird\\name\" failed"}"#
        );
    }
}
