## Summary

<!-- What changes, and why. 1-3 sentences. -->

## Test plan

- [ ] `cargo build --workspace --locked`
- [ ] `cargo test --workspace --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked`

## Checklist

- [ ] All commits signed off (`git commit -s`) — DCO required per [CONTRIBUTING.md](../CONTRIBUTING.md)
- [ ] Tests added or updated for behaviour changes
- [ ] Doc comments on any new `pub` items
- [ ] No new dependencies, OR the new dep is justified in this PR's description
- [ ] PR style consistent with project conventions (see `CLAUDE.md`)

## Heads up

This is a hobby project. The maintainer may merge, modify, reject,
or ignore this PR for any reason or no reason. There is no SLA. See
[README.md § "Maintenance and support"](../README.md#maintenance-and-support)
for the project's stance. Submitting a PR means you agree to license
your contribution under the project's dual MIT / Apache-2.0 terms
(inbound = outbound).
