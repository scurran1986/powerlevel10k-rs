# Nix flake

Flake-based install path for `p10k-rs`. Builds the MIT/Apache-2.0 binary
only — the GPL-3.0 `gitstatusd` helper is installed separately by the
user after first run (see boundary note below).

## Install

One-shot install into a profile:

```sh
nix profile install github:scurran1986/powerlevel10k-rs
```

Pin to a release tag:

```sh
nix profile install github:scurran1986/powerlevel10k-rs/v0.2.2
```

After install, finish setup by fetching the `gitstatusd` helper:

```sh
p10k-rs install-gitstatusd
```

Then wire your shell up per `README.md` (`p10k-rs init zsh`, etc.).

## Run without installing

```sh
nix run github:scurran1986/powerlevel10k-rs -- --help
```

## Dev shell

From a checkout:

```sh
nix develop
```

Brings in the pinned toolchain (`1.88.0` per `rust-toolchain.toml`),
`rust-analyzer`, `git`, and `cargo-deny`. Run the standard gates:

```sh
cargo build --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
```

## gitstatusd boundary

The flake intentionally does **not** package `gitstatusd`. That helper
is GPL-3.0; bundling it would force the whole derivation under GPL-3.0
and erase the dual MIT/Apache-2.0 license of the rest of the binary.
`p10k-rs install-gitstatusd` fetches a signed release from
romkatv/gitstatusd into the user's data dir at runtime, which keeps the
boundary clean. See `THIRD-PARTY-LICENSES.md` and
`docs/adr/0001-gitstatusd-architecture.md`.

## Maintenance

- On each release tag, bump the `version` string in `flake.nix` to match
  `[workspace.package].version` in `Cargo.toml`.
- Refresh inputs (`nixpkgs`, `rust-overlay`, `crane`, `flake-utils`)
  periodically with `nix flake update`, then commit the updated
  `flake.lock`.
- If `rust-toolchain.toml` bumps the channel, no flake change is needed
  — `rust-overlay.fromRustupToolchainFile` follows the file.
- `flake.lock` is initially absent (the author's environment doesn't
  ship `nix`). The first contributor with `nix` available should run
  `nix flake lock` at the repo root and commit the result.
