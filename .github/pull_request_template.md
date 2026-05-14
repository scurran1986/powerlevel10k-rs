<!-- One logical change per PR; commit message explains the *why*. -->

## What this changes

<!-- 1-3 sentences. Lead with the user-visible effect or the invariant
     being closed. -->

## Why

<!-- The motivation. A linked issue / review-swarm finding / ADR is
     fine. "Drive-by cleanup" is also fine if it's true. -->

## Checklist

Tick what applies. Untick what doesn't, with a note.

- [ ] **CHANGELOG.md updated** under `## [Unreleased]` (or this PR is
      purely internal — tests, docs/typo, planning bundle).
- [ ] `cargo test --workspace --locked` green locally.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
      green locally.
- [ ] `cargo fmt --all -- --check` green locally.
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked`
      green locally.
- [ ] `cargo deny check` green locally (if `Cargo.toml` / `deny.toml`
      touched).
- [ ] `cargo machete` green locally (if any `Cargo.toml` touched).

## Notes for the reviewer

<!-- Anything subtle: a non-obvious invariant being preserved, a test
     that proves the security property, a follow-up filed elsewhere. -->
