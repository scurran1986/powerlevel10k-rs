# Scoop manifest

[Scoop](https://scoop.sh/) manifest for installing `p10k-rs` on Windows
from the prebuilt, sigstore-signed zips that the release pipeline
publishes (`x86_64-pc-windows-msvc` + `aarch64-pc-windows-msvc` since
v0.2.6).

## Install

Direct from the repo (no bucket):

```pwsh
scoop install https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/packaging/scoop/p10k-rs.json
```

Or, if/when a dedicated bucket repo (`scoop-p10k-rs`) is graduated out
of this tree:

```pwsh
scoop bucket add p10k-rs https://github.com/scurran1986/scoop-p10k-rs
scoop install p10k-rs
```

The latter is the more conventional flow for multi-app maintainer
buckets; the in-repo manifest stays around as the canonical source
either way.

## Verifying the zip with sigstore

The release pipeline publishes a `.cosign.bundle` next to every
Windows zip. Verify before installing if you don't trust the mirror
path:

```pwsh
$ver = '0.2.6'
$triple = 'x86_64-pc-windows-msvc'   # or aarch64-pc-windows-msvc
$base = "https://github.com/scurran1986/powerlevel10k-rs/releases/download/v$ver"

iwr "$base/p10k-rs-$ver-$triple.zip" -OutFile "p10k-rs.zip"
iwr "$base/p10k-rs-$ver-$triple.zip.cosign.bundle" -OutFile "p10k-rs.zip.cosign.bundle"

cosign verify-blob `
  --bundle "p10k-rs.zip.cosign.bundle" `
  --certificate-identity-regexp `
    "^https://github.com/scurran1986/powerlevel10k-rs/.github/workflows/release.yml@refs/tags/v$ver`$" `
  --certificate-oidc-issuer https://token.actions.githubusercontent.com `
  "p10k-rs.zip"
```

`cosign.exe` is available via `scoop install cosign`.

## What's on Windows vs Unix

`p10k-rs` on Windows is a reduced-functionality build (no
`gitstatusd` daemon, no `root_indicator`, no privilege-aware `context`,
no DECSET 2026 / OSC 4 / `TIOCGWINSZ` probes). See
[`docs/src/windows.md`](../../docs/src/windows.md) for the per-feature
status table.

Prompt rendering, every segment that doesn't depend on Unix-only
syscalls, and the entire diagnostic CLI surface (`doctor`, `verify`,
`daemon-health`, `version`) work as on Unix.

## Refreshing the manifest

Bump `version` + the two `architecture.*.hash` fields to match the
published `.zip.sha256` sidecars on each release tag:

```sh
gh release download v<ver> -p '*windows*.zip.sha256' -R scurran1986/powerlevel10k-rs
# paste the hex digests into the two `hash` fields
```

`extract_dir` per arch tracks the `p10k-rs-<version>-<triple>/`
staging dir that `Compress-Archive` produces in the release workflow.
The `autoupdate` block uses `$version` interpolation so a Scoop bucket
running `scoop bucket update` picks up future tags automatically.
