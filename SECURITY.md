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
