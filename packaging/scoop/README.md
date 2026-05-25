# Scoop manifest

[Scoop](https://scoop.sh/) manifest for installing `p10k-rs` on Windows from
the prebuilt, sigstore-signed zips that the release pipeline *will* publish
once Windows targets are wired in.

## Current blocker

**No Windows binary exists in the v0.2.2 release.** The release pipeline
(`.github/workflows/release.yml`) only builds four unix tarballs:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Until the pipeline adds Windows triples, the URLs in `p10k-rs.json` resolve
to 404 and the `hash` fields are deliberate placeholders
(`PLACEHOLDER_PENDING_WINDOWS_RELEASE_PIPELINE`). The manifest is shipped now
so the schema, checkver, and autoupdate plumbing can be reviewed independently
of the pipeline change.

## Unblock checklist (changes needed in `.github/workflows/release.yml`)

1. **Add two entries to the build matrix** (`jobs.build.strategy.matrix.include`):
   - `target: x86_64-pc-windows-msvc`, `os: windows-latest`, `cross: false`
   - `target: aarch64-pc-windows-msvc`, `os: windows-latest`, `cross: false`
     (cross-compiles cleanly on the x86_64 windows-latest runner with
     `rustup target add aarch64-pc-windows-msvc`).
2. **Branch the `package` step on OS** so Windows produces a `.zip` instead of
   a `.tar.gz`:
   - Use PowerShell `Compress-Archive` or `7z a` to zip
     `p10k-rs.exe` + the three doc files.
   - Emit `${stage}.zip` and `${stage}.zip.sha256` outputs.
   - Skip `strip` on Windows (MSVC `link.exe` does its own; no `strip` on PATH).
3. **Extend the sigstore + attestation steps** to subject the `.zip` instead of
   the `.tar.gz` on Windows runners (or just attest both — `subject-path`
   accepts a glob).
4. **Confirm the `softprops/action-gh-release` upload globs** pick up the new
   `*.zip` and `*.zip.sha256` files.

Once those land and a release is cut, backfill `p10k-rs.json`:

```sh
gh release download v0.2.2 -p '*windows*.sha256' -R scurran1986/powerlevel10k-rs
# paste the hex digests into the two `hash` fields, drop the placeholder
```

After that the manifest is install-ready as-is — autoupdate will pull
subsequent versions automatically via the GitHub releases API.

## Install (once Windows binaries ship)

Direct from the repo (no bucket):

```pwsh
scoop install https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/packaging/scoop/p10k-rs.json
```

Or, if/when a dedicated bucket repo (`scoop-p10k-rs`) is graduated out of this
tree:

```pwsh
scoop bucket add p10k-rs https://github.com/scurran1986/scoop-p10k-rs
scoop install p10k-rs
```

The latter is the more conventional flow for multi-app maintainer buckets;
the in-repo manifest stays around as the canonical source either way.
