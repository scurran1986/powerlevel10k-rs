# AUR packaging for `p10k-rs`

Two PKGBUILDs live here, mirroring the
[`homebrew/`](../homebrew/) sibling but for Arch Linux:

| Variant | When to use | What it does |
|---|---|---|
| [`p10k-rs/`](p10k-rs/) | You already have a Rust toolchain and want the build to honour `rust-toolchain.toml`. | `cargo build --release --locked` from the GitHub source tarball. Runs the full workspace test suite during `check()`. |
| [`p10k-rs-bin/`](p10k-rs-bin/) | Everyone else. | Downloads the prebuilt, sigstore-signed Linux tarball for your arch from the GitHub release and drops the binary into `/usr/bin`. |

Both packages install the same binary (`p10k-rs`), per-shell init
scripts under `/usr/share/p10k-rs/`, dual licences under
`/usr/share/licenses/<pkgname>/`, and conflict with each other so a
host only ever has one installed at a time.

## Verifying the `-bin` tarball with sigstore

The release pipeline publishes a `.cosign.bundle` next to every
tarball. Verify it manually before installing if you don't trust your
mirror path:

```sh
ver=0.2.6
triple=x86_64-unknown-linux-gnu          # or aarch64-unknown-linux-gnu
base="https://github.com/scurran1986/powerlevel10k-rs/releases/download/v${ver}"
curl -fLO "${base}/p10k-rs-${ver}-${triple}.tar.gz"
curl -fLO "${base}/p10k-rs-${ver}-${triple}.tar.gz.cosign.bundle"

cosign verify-blob \
  --bundle "p10k-rs-${ver}-${triple}.tar.gz.cosign.bundle" \
  --certificate-identity-regexp \
    "^https://github.com/scurran1986/powerlevel10k-rs/.github/workflows/release.yml@refs/tags/v${ver}$" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "p10k-rs-${ver}-${triple}.tar.gz"
```

A clean `Verified OK` line means the tarball came from a tagged run of
this repo's `release.yml` workflow. The PKGBUILD does not invoke
`cosign` itself because makepkg's sandbox doesn't ship it by default
and a soft failure would be worse than no check at all.

## Refreshing checksums

Both PKGBUILDs ship real `sha256sums` plugged in from the published
`.tar.gz.sha256` sidecars at each release tag. Refresh by running
`updpkgsums` (from `pacman-contrib`) after bumping `pkgver`:

```sh
cd packaging/aur/p10k-rs       # or p10k-rs-bin
updpkgsums
makepkg --printsrcinfo > .SRCINFO
```

For the binary PKGBUILD, the hashes match what `gh release download v$ver
-p '*linux-gnu.tar.gz.sha256'` yields.

## Submitting to the AUR

The AUR is two separate git repos (`ssh://aur@aur.archlinux.org/p10k-rs.git`
and `ssh://aur@aur.archlinux.org/p10k-rs-bin.git`). Submission is a
manual, per-release maintainer step — there is no GitHub Action wired
to push for us, and we don't want one (no SSH-key-in-CI surface area):

```sh
# one-time, per package
git clone ssh://aur@aur.archlinux.org/p10k-rs.git aur-p10k-rs
cp packaging/aur/p10k-rs/{PKGBUILD,.SRCINFO} aur-p10k-rs/
cd aur-p10k-rs
git commit -am "v${ver}"
git push
```

Repeat for `p10k-rs-bin`. Bump `pkgrel` (not `pkgver`) when the only
change is to the PKGBUILD itself.
