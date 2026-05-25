# Homebrew formula

Bottle-only formula for installing `p10k-rs` on macOS from the prebuilt,
sigstore-signed tarballs published by the release pipeline.

## Install locally

From a clone of this repo:

```sh
brew install --formula ./packaging/homebrew/p10k-rs.rb
```

This pulls the matching darwin tarball (arm64 or x86_64) from the
GitHub release, drops `p10k-rs` into `$(brew --prefix)/bin`, and prints
the shell-init caveat. Linux is out of scope here — use `install.sh`
from the repo root instead.

## Graduation path

The formula currently lives in-repo for convenience while the project
is in early-alpha and the maintainer is the only consumer. Once v0.2 is
proven on a few more macOS installs, the next step is one of:

- **Maintainer tap** (`brew tap scurran1986/p10k-rs`) — a dedicated repo
  named `homebrew-p10k-rs` containing this formula. Lowest-friction
  bridge between in-repo and homebrew-core.
- **homebrew-core PR** — once the project hits ≥75 GitHub stars, a
  stable release cadence, and reproducible sha-pinned downloads (the
  pipeline already does this), it qualifies for the upstream tap.

## Filling the sha256 placeholders

Each `on_macos` block ships `sha256 "PLACEHOLDER_PENDING_RELEASE_PIPELINE_SHA"`.
Two ways to fix them before publishing the formula:

1. **Read the `.sha256` sidecar from the release.** The release
   pipeline already uploads `*.tar.gz.sha256` for every tarball:

   ```sh
   gh release download v0.2.2 -p '*.sha256' -R scurran1986/powerlevel10k-rs
   cat p10k-rs-0.2.2-aarch64-apple-darwin.tar.gz.sha256
   ```

2. **Recompute by hand** (useful when bumping the version locally):

   ```sh
   curl -sLO https://github.com/scurran1986/powerlevel10k-rs/releases/download/v0.2.2/p10k-rs-0.2.2-aarch64-apple-darwin.tar.gz
   shasum -a 256 p10k-rs-0.2.2-aarch64-apple-darwin.tar.gz
   ```

Paste the hex digest into the matching `sha256` field. Do the same for
the x86_64 tarball. A `script/update-homebrew-formula.sh` to automate
this is a fine future addition — kept out for now to stay conservative
on tooling.
