# Security Policy

## Threat model

`p10k-rs` runs on every prompt render. Its inputs come from
attacker-influenceable sources — git branch names, working
directories, environment variables, and a `gitstatusd` helper that
parses untrusted `.git/` contents. The render path enforces a
`SafeText` chokepoint that strips C0/C1/DEL and routes shell-output
through per-shell encoders, and IPC to `gitstatusd` runs over
pre-opened FIFOs with ownership and mode checks. Release binaries
are built in GitHub Actions, signed with sigstore (keyless OIDC),
and carry SLSA build-provenance attestations. The supply-chain
path — `cargo install` source, GitHub release tarballs, the
`get.sh` bootstrap — is the surface this document is most
concerned with.

## Supported versions

Only the latest minor release receives security fixes. Older
minors do not — upgrade.

| Version | Supported |
|---|---|
| 0.1.x (latest) | yes |
| < 0.1 (latest) | no |

## Reporting a vulnerability

Use **GitHub Security Advisories** (private vulnerability
reporting):

<https://github.com/scurran1986/powerlevel10k-rs/security/advisories/new>

Expect an initial response within 72 hours. If GHSA is unavailable
and the issue is **not** sensitive (no exploit details, no
embargoed third-party), a public issue tagged `security` is an
acceptable fallback. Do not post exploit details to a public
issue.

## Signing identity

Release tarballs are signed via sigstore keyless OIDC. The Fulcio
certificate binds to this workflow file and tag ref:

```
https://github.com/scurran1986/powerlevel10k-rs/.github/workflows/release.yml@refs/tags/v.+
```

OIDC issuer: `https://token.actions.githubusercontent.com`.

Build-provenance attestations are published to GitHub Attestations
via `actions/attest-build-provenance` and verifiable with the `gh`
CLI.

## Verifying a release

Two independent checks. Run both — they cover different links in
the chain.

### 1. Sigstore signature (cosign)

```bash
cosign verify-blob \
  --bundle p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz.cosign.bundle \
  --certificate-identity-regexp 'https://github.com/scurran1986/powerlevel10k-rs/.github/workflows/release.yml@refs/tags/v.+' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz
```

A non-zero exit means the signature does **not** chain to a
release-workflow run of this repo. Do not install.

### 2. SLSA build provenance (gh CLI)

```bash
gh attestation verify p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz \
  --repo scurran1986/powerlevel10k-rs
```

The same command also verifies the unpacked binary directly —
substitute the binary path for the tarball.

## Verifying the gitstatusd helper

Starting in **v0.1.5** (T0.5), `install.sh` defaults to the `pinned`
helper-acquisition mode: it downloads `gitstatusd` for the local host
triple from upstream's GitHub release and refuses to install the
binary unless its sha256 matches the value committed at
[`crates/p10k-rs-git/data/gitstatusd-pins.toml`](crates/p10k-rs-git/data/gitstatusd-pins.toml).
This pins the binary that runs on every prompt render to a digest
the maintainers reviewed at release time, not whatever your distro
or `brew` happen to ship.

Users can re-run the same comparison at any time without re-installing:

```bash
p10k-rs verify
```

Output is one stable line per outcome, with distinct exit codes so
scripts can branch without parsing:

| Outcome | Stdout | Exit |
|---|---|---|
| Match | `OK <triple> <version> <sha-prefix>` | 0 |
| Sha differs | `MISMATCH expected=<hex> got=<hex>` | 2 |
| Binary missing | `NOT_FOUND <reason>` | 3 |
| Host arch unknown | `UNSUPPORTED_ARCH <triple>` | 4 |

`p10k-rs verify --binary <path>` skips the auto-locate probe and
hashes the file at `<path>` directly — useful for validating a
candidate binary before placing it in the install prefix.

### Reproducing the verification

The pin file lists, per supported triple, the sha256 of the upstream
tarball and the sha256 of the binary it extracts to. Downstream
packagers (homebrew formula, AUR, nixpkgs, distro) should mirror
this check:

```bash
# 1. Read the committed binary sha256 for your host triple.
triple="x86_64-linux-gnu"   # or aarch64-linux-gnu / x86_64-darwin / aarch64-darwin
expected=$(awk -v t="[pins.$triple]" -v k=binary_sha256 '
  $0 == t { in_section = 1; next }
  /^\[/   { in_section = 0; next }
  in_section && $1 == k {
    v = $0; sub(/^[^=]*=[ \t]*/, "", v); sub(/^"/, "", v); sub(/"[ \t]*$/, "", v);
    print v; exit
  }' crates/p10k-rs-git/data/gitstatusd-pins.toml)

# 2. Compute the sha256 of the gitstatusd binary your package shipped.
actual=$(sha256sum /path/to/your/gitstatusd | awk '{print $1}')

# 3. Compare.
[ "$expected" = "$actual" ] && echo OK || echo "MISMATCH expected=$expected got=$actual"
```

### Pin bump cadence

A weekly workflow (`.github/workflows/pin-gitstatusd.yml`) probes
upstream `romkatv/gitstatus` for new releases. If a newer tag has
per-triple binaries available, the workflow opens a PR updating the
pin file to the new tag and shas; maintainer review lands the bump.
Releases that ship only signatures (no per-triple binaries) — as
upstream v1.5.5 did — are skipped automatically.

### Failure semantics

`install.sh` never hard-fails on a pinned-download miss. If the
network is unreachable, the sha doesn't match, or the host triple
isn't in the pin table, the installer prints a clear warning and
the binary falls back to its slower `ShellOut` git path at runtime —
the prompt still renders, just without the daemon-class latency
floor. This keeps an install on a flaky network from leaving the
user without a shell prompt at all.

## Out of scope

The threat model does not cover:

- Attacks requiring local root or another local user with
  equivalent privilege.
- Hardware fault injection, side-channel, or physical attacks.
- Compromise of upstream trust roots (Fulcio, Rekor, the GitHub
  Actions OIDC issuer, rustc, crates.io).
- Denial of service in the prompt render path that does not
  affect the parent shell.

## Past advisories

None.
