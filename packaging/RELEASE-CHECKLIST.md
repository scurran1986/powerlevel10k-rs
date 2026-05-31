# Release checklist

Actionable runbook for cutting a `p10k-rs` release. Pair with the live
`git log` and `.github/workflows/release.yml` — those are the source
of truth; this file is the order-of-operations.

The pipeline does the heavy lifting (6-triple build matrix, sigstore
keyless sign-blob, SLSA build-provenance attestation, sha256 sidecars,
SBOM upload) automatically on a semver tag push. The maintainer's job
is the pre-tag hygiene, the tag itself, and the post-tag manifest
bumps for the four packaging channels.

> **Trust `git log` and `gh release view` over this doc.** Memory
> notes at `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/`
> record prior round-trips you'll want to avoid (see [traps](#traps-prior-round-trips)).

---

## 0. Decide the version

Pre-1.0 bump rules per `CHANGELOG.md`:

- `0.x.y → 0.x.(y+1)` for bug fixes only, no API breaks.
- `0.x.y → 0.(x+1).0` for new features OR breaking changes (pre-1.0
  minor bumps are allowed to break).

Workspace is currently at the version in `[workspace.package].version`
in the root `Cargo.toml`.

## 1. Pre-tag hygiene

### 1a. Bump versions

Bump **ten** places in the root `Cargo.toml`:

1. `[workspace.package].version = "0.x.y"`
2. Each of the 9 internal path-dep specs in `[workspace.dependencies]`:
   `p10k-rs-core`, `p10k-rs-config`, `p10k-rs-segments`, `p10k-rs-git`,
   `p10k-rs-jj`, `p10k-rs-shell`, `p10k-rs-wizard`, `p10k-rs-ai`,
   `p10k-rs-ipc`. All carry an explicit `version = "0.x.y"` next to
   `path = "crates/<name>"`.

```sh
ver=0.x.y
# eyeball-grep first; then sed in one shot
grep -nE '^(version = "|p10k-rs-[a-z]+ *= *\{ *path)' Cargo.toml
```

Refresh `Cargo.lock` so the bump propagates:

```sh
cargo build --workspace --locked   # fails if lock is stale → drop --locked
cargo build --workspace            # refreshes Cargo.lock
```

### 1b. Local gate sweep

The five gates CI runs, in order. **Do not pipe to `tail` / `head`** —
shell pipelines swallow the cargo exit code and you'll ship a red
release thinking it was green (see
`feedback_gate_checking.md` in memory).

```sh
cargo build   --workspace --locked
cargo test    --workspace --locked
cargo clippy  --workspace --all-targets --locked -- -D warnings
cargo fmt     --all -- --check
cargo doc     --no-deps --workspace --locked
mdbook build  docs
```

### 1c. Semver gate

CI enforces this via `.github/workflows/semver-checks.yml`. Run it
locally to fail fast:

```sh
prev=$(git tag --sort=-creatordate | head -1)
cargo semver-checks check-release --workspace --baseline-rev "$prev"
```

Pre-1.0 breaks are allowed but should be **deliberate** and called out
in CHANGELOG + the per-version release note.

### 1d. Supply-chain gate

```sh
cargo deny check
```

Advisories, licences, banned/duplicate deps, and source allow-list
all must be clean.

### 1e. CHANGELOG + release notes

- Move the `## [Unreleased]` section into a new `## [0.x.y] - YYYY-MM-DD`
  block. Preserve the Added / Changed / Fixed / Removed / Security
  subheadings from Keep a Changelog. Add a fresh empty `## [Unreleased]`
  at the top.
- Write `.github/release-notes/v0.x.y.md`. This file becomes the
  GitHub release body verbatim (`softprops/action-gh-release` reads
  it via `body_path`). **No file = empty release body** — the
  pipeline only emits a workflow warning, not an error.

### 1f. Commit the pre-tag work

```sh
git add Cargo.toml Cargo.lock CHANGELOG.md .github/release-notes/v0.x.y.md
git commit -m "chore(release): v0.x.y"
git push origin main
```

## 2. Tag and push

```sh
ver=0.x.y
git tag -a "v${ver}" -m "v${ver}"
git push origin "v${ver}"
```

`release.yml` fires on the tag push. Six build jobs run in parallel:

| Triple                          | Runner          | Asset                                                |
|---------------------------------|-----------------|------------------------------------------------------|
| `x86_64-unknown-linux-gnu`      | `ubuntu-latest` | `p10k-rs-${ver}-x86_64-unknown-linux-gnu.tar.gz`     |
| `aarch64-unknown-linux-gnu`     | `ubuntu-latest` | `p10k-rs-${ver}-aarch64-unknown-linux-gnu.tar.gz`    |
| `x86_64-apple-darwin`           | `macos-14`      | `p10k-rs-${ver}-x86_64-apple-darwin.tar.gz`          |
| `aarch64-apple-darwin`          | `macos-14`      | `p10k-rs-${ver}-aarch64-apple-darwin.tar.gz`         |
| `x86_64-pc-windows-msvc`        | `windows-latest`| `p10k-rs-${ver}-x86_64-pc-windows-msvc.zip`          |
| `aarch64-pc-windows-msvc`       | `windows-latest`| `p10k-rs-${ver}-aarch64-pc-windows-msvc.zip`         |

Each archive gets a `.sha256` sidecar and a `.cosign.bundle` from
sigstore. The `sbom` job (`needs: build`) appends one `.spdx.json` to
the release once all six builds finish.

## 3. Post-tag verification

```sh
ver=0.x.y
gh release view "v${ver}" --json assets --jq '.assets[].name' | sort
```

Expected line count: **19**

- 4 unix tarballs × 3 sidecars (`.tar.gz`, `.tar.gz.sha256`, `.tar.gz.cosign.bundle`) = **12**
- 2 windows zips × 3 sidecars (`.zip`, `.zip.sha256`, `.zip.cosign.bundle`) = **6**
- 1 SBOM (`p10k-rs-v${ver}.spdx.json`) = **1**

If the count is short, inspect the failed job's logs:

```sh
gh run list --workflow release.yml --limit 5
gh run view <run-id> --log-failed
```

Verify the SLSA build-provenance attestation on at least one asset:

```sh
gh attestation verify \
  "p10k-rs-${ver}-x86_64-unknown-linux-gnu.tar.gz" \
  --repo seaburdz/powerlevel10k-rs
```

A failed attestation = a tampered asset. Do **not** proceed to the
manifest bumps below.

## 4. Packaging manifests (manual, post-release)

Each channel pulls hashes from the published release artifacts. The
manifest version + hashes are intentionally *not* auto-bumped by the
release pipeline — every channel has slightly different submission
ergonomics and a failed manifest shouldn't gate the binary release.

### 4a. Homebrew (`packaging/homebrew/p10k-rs.rb`)

```sh
gh release download "v${ver}" -p '*darwin*.sha256' -R seaburdz/powerlevel10k-rs
# Paste the two hex digests into the on_macos blocks. Bump `version`.
```

See `packaging/homebrew/README.md` for the tap-graduation path
(`homebrew-p10k-rs` repo → eventually homebrew-core PR).

### 4b. AUR `p10k-rs` (source build) — `packaging/aur/p10k-rs/PKGBUILD`

```sh
cd packaging/aur/p10k-rs
# Bump pkgver to ${ver}; pkgrel back to 1.
updpkgsums                    # pacman-contrib
makepkg --printsrcinfo > .SRCINFO
```

Then push the PKGBUILD + .SRCINFO to the AUR git repo:

```sh
git clone ssh://aur@aur.archlinux.org/p10k-rs.git aur-p10k-rs
cp packaging/aur/p10k-rs/{PKGBUILD,.SRCINFO} aur-p10k-rs/
cd aur-p10k-rs && git commit -am "v${ver}" && git push
```

### 4c. AUR `p10k-rs-bin` (binary) — `packaging/aur/p10k-rs-bin/PKGBUILD`

Same flow as 4b, but the binary variant. `updpkgsums` will fetch
the linux tarballs and embed the published `.sha256` digests.

### 4d. Nix flake (`flake.nix`)

```sh
# Bump version string in flake.nix to match Cargo.toml.
# If flake.lock exists, refresh inputs:
nix flake update
git add flake.nix flake.lock
```

> `flake.lock` is **not yet committed** in this repo. Generation
> requires a contributor with `nix` installed (see
> `packaging/nix/README.md`).

### 4e. Scoop (`packaging/scoop/p10k-rs.json`)

```sh
gh release download "v${ver}" -p '*windows*.zip.sha256' -R seaburdz/powerlevel10k-rs
# Bump `version`, the two `url`s, and the two `architecture.*.hash` fields.
```

`extract_dir` per arch tracks the `p10k-rs-${ver}-${triple}/` staging
dir that `Compress-Archive` produces in the release workflow — don't
forget to bump those alongside the version.

### 4f. install scripts (no manifest bump, sanity-check only)

`install.sh` and `install.ps1` pin the asset URL pattern. The pattern
itself is version-agnostic (it derives the URL from `--version` or
"latest"), so no source change is needed — but smoke-test the latest
release end-to-end on at least one unix and one Windows host before
calling the release done:

```sh
curl -sSL https://raw.githubusercontent.com/seaburdz/powerlevel10k-rs/v${ver}/install.sh | bash -s -- --version v${ver}
```

## 5. crates.io

**Current state: NOT published.** `cargo search p10k-rs` returns no
results as of v1.0.0. This is now **deliberate** per
[STABILITY.md](../STABILITY.md): the Rust crate API is binary-only
and the workspace crates' Rust API is explicitly not committed to
SemVer. Publishing the crates would create an external expectation
that conflicts with that stance. The earlier framing of "v1.0
criterion #5 prerequisite" has been replaced by the STABILITY.md
contract.

When the maintainer is ready to publish anyway (e.g. for a future
plugin API in a later major release), the path is:

```sh
cargo login                                    # one-time, store API token
# Publish in dependency order. Each `cargo publish -p <crate>` blocks
# until the index updates (~30s) before the next can resolve it.
cargo publish -p p10k-rs-ipc
cargo publish -p p10k-rs-core
cargo publish -p p10k-rs-config
cargo publish -p p10k-rs-shell
cargo publish -p p10k-rs-git
cargo publish -p p10k-rs-jj
cargo publish -p p10k-rs-ai
cargo publish -p p10k-rs-wizard
cargo publish -p p10k-rs-segments
cargo publish -p p10k-rs                       # binary last
```

Pre-publish gotchas to expect on the first attempt:

- Every `pub` item without a `///` doc comment will be a warning; the
  workspace already enforces `missing_docs = "warn"` so this should
  be clean, but `cargo publish` is a fresh check.
- crates.io requires a `description`, `license`, and `repository`
  field on every published crate's `[package]`. Verify present
  before the first publish: `grep -L description crates/*/Cargo.toml`.
- The first publish reserves the name; subsequent versions are
  straightforward.
- After publication, fold a `cargo publish --dry-run -p <crate>`
  gate into the release checklist (step 1f-ish) so subsequent
  releases catch publish breakage before tagging.

## 6. Superseded-banner protocol

If a release page is incomplete (the canonical case: v0.2.5's Windows
shell-step failure, before v0.2.6 was the reship), add a banner
pointing at the successor so users don't pull the half-baked release:

```sh
gh release edit v0.2.5 --notes-file - <<'EOF'
> [!WARNING]
> **Superseded by v0.2.6.** This release's Windows zips never
> uploaded because of a bash-on-pwsh issue in the workflow. Install
> v0.2.6 instead: https://github.com/seaburdz/powerlevel10k-rs/releases/tag/v0.2.6

<paste original notes below this banner>
EOF
```

Do **not** delete the original release — the tag, sigstore signatures,
and Rekor log entries are immutable history. Banner + supersede is
the right pattern.

## Traps (prior round-trips)

Read these memory notes before a release if you've never cut one
before; they capture round-trips the next maintainer should not have
to relive:

- `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/project_windows_port_2026_05_26.md`
  — v0.2.5 → v0.2.6 round-trip. v0.2.5's pipeline succeeded at build /
  package / sign / attest but failed at the `bash` "locate release
  notes" step on `windows-latest`. Fix was `shell: bash` (Git Bash
  ships on the runner image). If a future release adds a step that
  uses `[[ ... ]]` or `set -euo pipefail` syntax, add `shell: bash`
  explicitly or the Windows jobs will fail the same way.
- `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/project_session_2026_05_25.md`
  — v0.2.3 added the Windows MSVC matrix and the build itself was
  red (real `rustix` portability hole). v0.2.4 reverted the matrix
  while the cfg-gating work landed. Lesson: if a tag has any chance
  of a partial build, batch the manifest bumps (step 4) until you've
  verified the release page is complete.
- `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/feedback_gate_checking.md`
  — `cargo X | tail` swallows the cargo exit code; do **not** claim
  "all green" off a piped check. Run gates raw, look at the prompt
  exit indicator. Several "green" releases were red under the hood
  for exactly this reason.
- `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/feedback_pages_source_gotcha_2026_05_24.md`
  — orthogonal to the binary release, but bites: the GitHub Pages
  source mode must be "GitHub Actions" (not "Deploy from a branch")
  for the docs workflow to deploy. Misconfigured source surfaces as
  `HttpError: Not Found` with a misleading hint.

Additional context: `~/.planning/powerlevel10k-rs/NEXT-STEPS.md`
records the v0.2-era "manual external steps" list. Treat as
historical context; this checklist is the current source of truth.
