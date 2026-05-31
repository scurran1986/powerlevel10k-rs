# Stability commitment

`p10k-rs` v1.0.0 commits to Semantic Versioning for the surfaces
listed below. Other surfaces are explicitly not stable. This document
is the authoritative reference; `docs/src/stability.md` is the
companion mdBook page.

> **TL;DR.** What ships through the `p10k-rs` binary, the TOML config
> schema, the per-shell init protocol, and the GitHub release artifacts
> is covered by SemVer. The internal Rust crate API is **not** — those
> crates are not published to crates.io and may break in any release.

## What's stable

### Binary CLI

The `p10k-rs` executable's subcommands, flags, exit codes, and stdout
contracts. Specifically:

- `p10k-rs prompt --shell <name> [...]` — the render path the shell init
  scripts invoke. `--render-side` accepts `left`, `right`, or
  `transient`. Exit codes: 0 (rendered), 2 (transient `keep-prompt`
  signal — `same-dir` / `unique-dir` mismatch).
- `p10k-rs init <shell>` — emits the shell-specific init script.
  Supported shells: `zsh`, `bash`, `fish`, `pwsh`, `nu`. Adding new
  shells is non-breaking; removing one is a major bump.
- `p10k-rs configure` — the 3-question Q&A wizard (preset / glyph mode /
  palette).
- `p10k-rs import <path>` — best-effort Powerlevel10k `~/.p10k.zsh`
  import.
- `p10k-rs statusline --host <name>` — AI-host statusline payload.
- `p10k-rs segment-list` — newline-separated list of every segment this
  build ships, including auto-detect heuristics.
- `p10k-rs config check [--config <path>]` — parse + schema-validate a
  TOML config. Exits 0 on success.
- `p10k-rs theme list | show <name> | install <name> [--force]` —
  bundled theme catalogue and installer.
- `p10k-rs verify [--binary <path>] [--json]` — gitstatusd supply-chain
  verification (T0.5). Exit codes: 0 (`OK`), 2 (`MISMATCH`), 3
  (`NOT_FOUND`), 4 (`UNSUPPORTED_ARCH`).
- `p10k-rs daemon-health [--json]` — gitstatusd daemon diagnostic.
  Exit codes: 0 (`OK`), 2 (`WEDGED`), 3 (`DEAD`), 4 (`NOT_WIRED`),
  5 (`ERROR`). One-line text form: `OK pid=<pid> wedge=<state>`.
- `p10k-rs doctor [--json]` — runtime environment diagnostic across
  Nerd Font, gitstatusd, config, shell init, instant-prompt cache,
  OSC 7, and the WSL font story. Exit codes: 0 (all OK / SKIP), 1
  (warnings only), 2 (at least one error).
- `p10k-rs version [--json]` — binary + bundled-gitstatusd + target
  triple diagnostic (in addition to the clap-generated `--version`).
- `p10k-rs --help` and `p10k-rs --version` — standard clap behaviour.

New subcommands, new flags, and new options on existing subcommands
are allowed in minor releases. Removing or changing the semantics of
an existing subcommand, flag, exit-code, or documented stdout
contract is a major bump.

### TOML configuration schema

Every field documented under `docs/src/schema/` and `docs/src/config/`
plus every documented variant of every enum the schema accepts.

The schema-root struct (`Config`) and most config-facing structs and
enums in `p10k-rs-config` are marked `#[non_exhaustive]` so adding
new variants or fields is non-breaking at the schema layer:

- Structs: `Config`, `Layout`, `SegmentConfig`, `StateOverrides`,
  `Padding`, `DirTruncate`, `FrameStyle`, `RulerStyle`, `Separators`,
  `ShellIntegration`, `AiConfig`, `HostConfig`.
- Enums: `ColorMode`, `DirTruncateStrategy`, `ShellIntegrationMode`,
  `HostMode`.

A small set of TOML-facing enums are **not** `#[non_exhaustive]` for
pre-1.0 historical reasons (`Color`, `InstantPromptMode`,
`TransientPromptMode`). Adding variants to those enums is reserved
for a major bump; in practice they have stabilised — see
`themes/` and the bundled `every_bundled_theme_parses` test for the
shape of values flowing through them.

The Powerlevel9k import path (`p10k-rs import`) is stable for the
documented P9K options. Recognising additional P9K options is
non-breaking; dropping an already-documented one is a major bump.

### Per-shell init script protocol

What `p10k-rs init <shell>` emits is the contract between the binary
and the user's shell session. Once a user has sourced an init script,
the following are stable:

- The `_P10K_RS_*` environment variables the init script exports.
- The `_p10k_rs_*` (and shell-equivalent) function names registered
  in the user's shell.
- The order in which hooks are added to `precmd_functions` /
  `preexec_functions` (zsh) and the equivalents for `bash`, `fish`,
  `pwsh`, and `nu`.
- The `--render-side transient` exit-code contract (0 = emit, 2 =
  keep-prompt) the init script consumes.

Additions (new helper functions, new env vars) are non-breaking.
Renames or removals are a major bump.

### Release artifacts

Sigstore-signed multi-arch binary archives at
`github.com/scurran1986/powerlevel10k-rs/releases`:

| Triple                          | Format    |
|---------------------------------|-----------|
| `x86_64-unknown-linux-gnu`      | `.tar.gz` |
| `aarch64-unknown-linux-gnu`     | `.tar.gz` |
| `x86_64-apple-darwin`           | `.tar.gz` |
| `aarch64-apple-darwin`          | `.tar.gz` |
| `x86_64-pc-windows-msvc`        | `.zip`    |

Each archive ships an SBOM and a `.sig` / `.crt` pair for keyless
sigstore verification. The set of triples may grow in minor
releases; dropping a triple is a major bump.

## What's explicitly not stable

### Rust crate API

`p10k-rs` is distributed as a binary, not a library crate. The
workspace crates (`p10k-rs`, `p10k-rs-core`, `p10k-rs-config`,
`p10k-rs-segments`, `p10k-rs-git`, `p10k-rs-jj`, `p10k-rs-shell`,
`p10k-rs-wizard`, `p10k-rs-ai`, `p10k-rs-ipc`) are **not published**
to crates.io and their Rust API may break in any release.

Anyone consuming these crates as a git or path dependency does so at
their own risk. `cargo-semver-checks` runs in CI to keep the API
sane release-over-release, but the crate-level surface is not a
SemVer commitment.

### Plugin API

No public plugin API ships at v1.0. The decision is intentional:
keeping the attack surface small was a Security-MAX design goal. A
plugin API is on the roadmap but is not promised, and if it ships
it will arrive in a minor release with its own stability contract
from day one.

### Internal implementation details

These surfaces may change in any release:

- The `gitstatusd` wire-protocol parsing inside
  `p10k-rs-git/src/gitstatusd.rs`.
- The `SafeText` chokepoint's internal representation (the
  interface is stable within the binary; field layout may change).
- The instant-prompt dump file format. Users should treat
  `~/.cache/p10k-rs/dump-*.zsh` as opaque to anything other than
  the matching `p10k-rs` binary that wrote it.
- The `tracing` log format under `$XDG_STATE_HOME/p10k-rs/`.
- The FIFO / mktemp directory layout under `$XDG_RUNTIME_DIR/p10k-rs/`
  and the cwd-history file the zsh init writes for `unique-dir`
  transient mode.

### AI host detection

The set of detectable hosts (`ClaudeCode`, `Goose`, `Aider`, `Cursor`,
`Warp`, `Generic`) may expand in minor releases. Per-host statusline
contracts that are documented as "absent pending upstream" today may
flip to "present" without notice once the upstream protocol is
published. Adding a detection entry is non-breaking; removing one is
a major bump.

## MSRV policy

Minimum Supported Rust Version is **stable − 2**. v1.0.0 ships with
MSRV `1.88.0`, pinned in `rust-toolchain.toml`. MSRV bumps are **not**
considered breaking; they happen as the floor of stable − 2 advances.
The CI MSRV gate (`.github/workflows/msrv.yml`) protects this contract.

## Deprecation policy

Before removing a stable surface in a major release:

- The surface is marked deprecated for one full minor release. For
  Rust-visible items that's a `#[deprecated]` attribute; for CLI /
  TOML / init-script surfaces it's a runtime warning on stderr.
- A `CHANGELOG.md` entry under `### Deprecated` calls out the
  deprecation.
- A migration recipe lands in `docs/src/migration.md`.

Major releases consolidate deprecations into removals.

## Security disclosure

See `SECURITY.md` for the responsible-disclosure policy. Security
fixes may ship in patch releases regardless of API stability
commitments — a fix that requires breaking a documented contract
will ship as a patch release with the break documented in the
`CHANGELOG.md` `### Security` section.

## Cross-references

- `SECURITY.md` — responsible-disclosure policy and threat model
  context.
- `packaging/RELEASE-CHECKLIST.md` — release-time operational
  runbook; the "crates.io NOT-published" note lives there.
- `CHANGELOG.md` — release-by-release ledger.
- `docs/src/stability.md` — the mdBook companion page summarising
  this document for the docs site.
