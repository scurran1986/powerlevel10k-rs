# Changelog

All notable changes to `p10k-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 minor bumps may be breaking; breakage is documented when it occurs.

## [Unreleased]

### Added
- Workspace skeleton: nine crates wired through `[workspace.dependencies]` with
  centralised version pinning, workspace-wide lints, release profile, and
  rustfmt / clippy / cargo-deny / dependabot configuration.
- CI: fmt, clippy, test, doc, and `cargo-deny` on stable Rust across
  ubuntu-latest and macos-latest.
- ADR index in `docs/adr/`. ADR 0001 (gitstatusd-class latency strategy) is
  reserved for the day-1 spike contractor.

### Notes
- The workspace member `crates/spike-gitstatus` is referenced but its manifest
  is owned by another contractor; `cargo check` will fail until that crate's
  files land.
