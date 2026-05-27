#
# Nix flake for p10k-rs.
#
# Builds the MIT/Apache-2.0 binary (`p10k-rs`) only. The `gitstatusd`
# helper is GPL-3.0 and is *not* bundled here — users install it
# separately via `p10k-rs install-gitstatusd` after first run. See
# `THIRD-PARTY-LICENSES.md` and `docs/adr/0001-gitstatusd-architecture.md`
# for the boundary rationale.
#
# Toolchain: pinned to `rust-toolchain.toml` (currently 1.88.0) via
# `rust-overlay`. Don't drift from that pin here — bump the toolchain
# file and let this flake follow.
#
# To regenerate the lockfile (initial setup or input bump):
#
#     nix flake lock
#
# `flake.lock` is intentionally absent from the first commit because the
# author's environment doesn't have `nix` on the PATH; the first user (or
# CI) with `nix` available should run `nix flake lock` and commit the
# result. See `packaging/nix/README.md`.
#
{
  description = "Rust port of Powerlevel10k. Single static binary, declarative TOML config, multi-shell, gitstatusd-class git latency.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Honor the pinned toolchain in rust-toolchain.toml. The file
        # declares `channel = "1.88.0"` plus rustfmt + clippy components.
        rustToolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Filter the source tree to just what `cargo build` reads. Keeps
        # store paths small and avoids spurious rebuilds when docs or
        # shell scripts change.
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        commonArgs = {
          inherit src;
          strictDeps = true;

          # No native build inputs needed today — the binary statically
          # links its Rust deps and shells out to `git` / `gitstatusd`
          # at runtime (which the user installs separately).
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];

          # Workspace builds a single shipping binary; restrict cargo to
          # that crate so we don't compile dev-only artifacts.
          cargoExtraArgs = "--locked -p p10k-rs";
        };

        # Build dependency-only artifacts once and reuse the store path
        # across the real build and any future check derivations. This is
        # the canonical crane two-phase pattern.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        p10k-rs = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "p10k-rs";
          # Mirrors `version.workspace` in Cargo.toml. Bump this in
          # lockstep with the workspace `[workspace.package].version`
          # at each release tag.
          version = "0.2.6";

          # `cargo test` is gated by the workspace test suite + CI; the
          # nix build path is for binary delivery, so skip tests here to
          # keep `nix build` fast. CI still runs the full matrix.
          doCheck = false;

          meta = with pkgs.lib; {
            description = "Rust port of Powerlevel10k. Single static binary, declarative TOML config, multi-shell, gitstatusd-class git latency.";
            homepage = "https://github.com/scurran1986/powerlevel10k-rs";
            changelog = "https://github.com/scurran1986/powerlevel10k-rs/blob/main/CHANGELOG.md";
            license = [ licenses.mit licenses.asl20 ];
            mainProgram = "p10k-rs";
            platforms = platforms.unix;
          };
        });
      in
      {
        packages = {
          default = p10k-rs;
          p10k-rs = p10k-rs;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = p10k-rs;
          name = "p10k-rs";
        };

        devShells.default = pkgs.mkShell {
          # The pinned toolchain brings cargo + rustc + rustfmt + clippy
          # in one drv; `rust-analyzer` ships separately from nixpkgs.
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            # Convenience extras for in-tree work. None of these are
            # needed by the build itself.
            pkgs.git
            pkgs.cargo-deny
          ];

          # Help rust-analyzer find the toolchain's sysroot.
          RUST_SRC_PATH =
            "${rustToolchain}/lib/rustlib/src/rust/library";
        };

        # `nix flake check` runs this. Cheapest useful gate.
        checks = {
          inherit p10k-rs;
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
