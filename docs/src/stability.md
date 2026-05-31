# Stability

`p10k-rs` v1.0.0 commits to [Semantic Versioning][semver] for the
surfaces below. Anything not listed here is explicitly not stable.

[semver]: https://semver.org/spec/v2.0.0.html

## Covered by SemVer

- **Binary CLI.** Subcommands, flags, exit codes, and stdout
  contracts of the `p10k-rs` executable: `prompt`, `init`, `configure`,
  `import`, `statusline`, `segment-list`, `config check`, `theme`,
  `verify`, `daemon-health`, `doctor`, `version`. Adding new
  subcommands / flags is non-breaking; removing or changing the
  semantics of an existing one is a major bump.
- **TOML configuration schema.** Every documented field and enum
  variant under [Configuration](./config/index.md) and
  [Schema (full)](./reference/schema.md). Most config-facing structs
  and enums are marked `#[non_exhaustive]` so additions are
  non-breaking.
- **Per-shell init script protocol.** The `_P10K_RS_*` env vars,
  `_p10k_rs_*` function names, and hook-registration order set up
  by `p10k-rs init <shell>` (zsh, bash, fish, pwsh, nu). Once a
  user has sourced an init script, the contract is stable.
- **Release artifacts.** Sigstore-signed multi-arch binary archives
  (`x86_64`/`aarch64` Linux + macOS, `x86_64` Windows) with SBOMs
  and `.sig` / `.crt` sidecars at the GitHub release page.

## Explicitly not stable

- **Rust crate API.** The workspace crates (`p10k-rs`,
  `p10k-rs-core`, `p10k-rs-config`, …) are **not** published to
  crates.io and may break in any release. Consume them as a git
  dependency at your own risk.
- **Plugin API.** None ships at v1.0 by design (keeping attack
  surface small was a Security-MAX goal). A plugin API may land in
  a future minor release with its own contract.
- **Internal implementation details.** The `gitstatusd` wire format,
  `SafeText` field layout, instant-prompt dump file format, the
  `tracing` log format, and the FIFO / mktemp directory layout
  under `$XDG_RUNTIME_DIR/p10k-rs/` are all opaque.
- **AI host detection.** The detectable host set
  (`ClaudeCode`, `Goose`, `Aider`, `Cursor`, `Warp`, `Generic`)
  may grow in minor releases. "Absent pending upstream" statusline
  contracts may flip to "present" without notice.

## MSRV policy

Minimum Supported Rust Version is **stable − 2**. v1.0.0 ships with
MSRV `1.88.0`. MSRV bumps are not considered breaking; CI enforces
the floor via `.github/workflows/msrv.yml`.

## Deprecation policy

Stable surfaces get one full minor release of deprecation
(`#[deprecated]` for Rust items, runtime stderr warnings for
CLI / TOML / init-script surfaces) before removal in a major
release, with the deprecation logged in `CHANGELOG.md` and a
migration recipe in [Upgrading from v0.1](./migration.md).

## Security

See [`SECURITY.md`](https://github.com/scurran1986/powerlevel10k-rs/blob/main/SECURITY.md)
for the responsible-disclosure policy. Security fixes may ship in
patch releases regardless of API stability commitments.

---

For the canonical, more detailed version of this document — including
the full subcommand exit-code table and the list of which config
structs / enums carry `#[non_exhaustive]` — see
[`STABILITY.md`](https://github.com/scurran1986/powerlevel10k-rs/blob/main/STABILITY.md)
at the repository root.
