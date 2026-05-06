# p10k-rs

A Rust port and spiritual successor to [Powerlevel10k][p10k]. Single static
binary, declarative TOML config, multi-shell on day one (zsh, fish, bash),
with `gitstatusd`-class git latency as the load-bearing performance claim.

> **Status:** pre-alpha. Workspace skeleton only. The day-1 spike
> (`crates/spike-gitstatus`) gates whether the project proceeds. Don't
> install this yet.

## Why this project exists

Starship is the polished baseline. It deliberately ships none of the four
features Powerlevel10k users actually leave for: instant prompt, transient
prompt, show-on-command, and the configuration wizard — plus
sub-millisecond git status. We ship those.

See [`MVP-SPEC.md`][spec] in the planning bundle for the v0.1 scope.

[p10k]: https://github.com/romkatv/powerlevel10k
[spec]: ../.planning/powerlevel10k-rs/MVP-SPEC.md

## Workspace layout

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import
  p10k-rs-segments   # segment implementations
  p10k-rs-git        # gitstatus replacement (gix + rustix hot loop)
  p10k-rs-shell      # per-shell init scripts
  p10k-rs-wizard     # `configure` TUI
  p10k-rs-ai         # OSC, host detection, statusline
  p10k-rs-ipc        # daemon protocol (post-MVP placeholder)
  spike-gitstatus    # day-1 latency spike (separate ownership)
```

See [`ARCHITECTURE.md`][arch] for crate APIs and rationale.

[arch]: ../.planning/powerlevel10k-rs/ARCHITECTURE.md

## Building

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

MSRV is **stable - 2** (currently 1.84). Pinned in `rust-toolchain.toml`.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option. Contributions are accepted under the same terms (see
[CONTRIBUTING.md](CONTRIBUTING.md)).
