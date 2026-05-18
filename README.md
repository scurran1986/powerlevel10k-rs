# p10k-rs

A Rust port and spiritual successor to [Powerlevel10k][p10k]. Single
static binary, declarative TOML config, multi-shell support, with
`gitstatusd`-class git latency as the load-bearing performance claim.

[p10k]: https://github.com/romkatv/powerlevel10k

## Quickstart

One line. Clones the repo to `~/.local/share/powerlevel10k-rs`,
builds the binary, wires zsh:

```bash
curl -fsSL https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/get.sh | bash
```

Open a new zsh terminal — the prompt is live.

Requirements: `cargo` (install via [rustup](https://rustup.rs)),
`zsh`, `git`, `curl`. The installer drops the binary at
`~/.cargo/bin/p10k-rs`, appends an `eval "$(p10k-rs init zsh)"`
line to `~/.zshrc`, and symlinks `gitstatusd` next to the binary
if a canonical install is on `PATH` (otherwise the slow `git`
shell-out fallback kicks in; see
[docs/adr/0001-git-backend.md](docs/adr/0001-git-backend.md)).

Re-piping the same command upgrades an existing install. To
uninstall:

```bash
~/.local/share/powerlevel10k-rs/install.sh --uninstall
```

Prefer to clone yourself? The bootstrap is just:

```bash
git clone https://github.com/scurran1986/powerlevel10k-rs.git ~/.local/share/powerlevel10k-rs && ~/.local/share/powerlevel10k-rs/install.sh
```

## Status

**v0.1.1 shipped.** Daily-driver-grade for the maintainer. Multi-arch
release builds on every tag push (Linux x86_64/aarch64 GNU, macOS
x86_64/aarch64). Full slice ledger in `CHANGELOG.md` — the headline
features are listed below under "What works today".

**368 tests pass workspace-wide.** Builds clean on stable Rust 1.88
across all five gates (`build`, `test`, `clippy -D warnings`,
`fmt --check`, `doc -D warnings`) plus `cargo deny check` and
`cargo machete`.

## Why this project exists

Starship is the polished baseline. It deliberately ships none of
the four features Powerlevel10k users actually value: instant
prompt, transient prompt, show-on-command, and the configuration
wizard — plus sub-millisecond git status. We ship those.

Architectural decisions live in `docs/adr/`. The load-bearing one
is [ADR-0001](docs/adr/0001-git-backend.md): the production hot
path is a long-lived `gitstatusd` daemon over FIFOs.

## What works today

| Feature | State |
|---|---|
| All 21 MVP segments + 10 modern extras (`jj`, `ai_host`, `mise`, `fnm`, `pixi`, `docker_context`, `os_icon`, `node_version`, `python_version`, `rust_version`) | 31 segments wired |
| Per-segment styling via TOML (`foreground` / `background` / per-state overrides) | done |
| Four colour modes — `Ansi8` / `Ansi256` / `TrueColor` / `FollowTerminal` (OSC 4 palette probe) | done |
| Powerline ribbon, multi-line frame, ruler, right prompt (`RPROMPT`) | done |
| Transient prompt (zsh `zle-line-finish` collapses to a lone `❯`) | done |
| Instant prompt (sub-ms first shell via cached `dump.zsh`) | done |
| `gitstatusd` long-lived daemon over FIFOs (ADR-0001 hot path) | done |
| `git` shell-out fallback | done |
| Branch / cwd render-path sanitization (`SafeText`) | done |
| `p10k-rs import ~/.p10k.zsh` (P9k importer) | done |
| `p10k-rs configure` wizard | stub |
| `bash` init script (no RPROMPT, no preexec timing, no gitstatusd) | best-effort |
| `fish` init script | stub |
| Multi-arch binary distribution (Linux x86_64/aarch64 GNU, macOS x86_64/aarch64) | release workflow on tag |
| mdBook documentation (`docs/`) | published via `.github/workflows/docs.yml` |

## Verify a release

Release tarballs are sigstore-signed (keyless OIDC) and carry
SLSA build-provenance attestations. Two independent checks:

```bash
# Sigstore signature — bundle is published next to the tarball.
cosign verify-blob \
  --bundle p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz.cosign.bundle \
  --certificate-identity-regexp 'https://github.com/seaburdz/powerlevel10k-rs/.github/workflows/release.yml@refs/tags/v.+' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz

# Build provenance — works on the tarball or the unpacked binary.
gh attestation verify p10k-rs-0.1.3-x86_64-unknown-linux-gnu.tar.gz \
  --repo seaburdz/powerlevel10k-rs
```

A non-zero exit on either check means the artifact does not chain
to a release-workflow run of this repo — don't install. Full
threat model, reporting channel, and signing-identity details
live in [SECURITY.md](SECURITY.md).

## Importing an existing Powerlevel10k config

If you already have a `~/.p10k.zsh`, get a head-start:

```bash
p10k-rs import ~/.p10k.zsh > ~/.config/p10k-rs/config.toml
```

The importer is best-effort textual translation — it doesn't execute
your zsh config, just reads it. It handles:

- `POWERLEVEL9K_LEFT_PROMPT_ELEMENTS` / `RIGHT_PROMPT_ELEMENTS` → `[layout]`
- `POWERLEVEL9K_MODE`, `POWERLEVEL9K_INSTANT_PROMPT`
- `POWERLEVEL9K_<SEG>_FOREGROUND` / `BACKGROUND` (indexed, named, or `#rrggbb`)
- `POWERLEVEL9K_<SEG>_<STATE>_FOREGROUND` / `BACKGROUND`

Unrecognised variables are reported to stderr — you'll see exactly
what didn't translate. Pipe just stdout to your config file; stderr
is for you.

## Workspace layout

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import (data only)
  p10k-rs-segments   # segment implementations (31 segments)
  p10k-rs-git        # gitstatusd client + git shell-out fallback
  p10k-rs-jj         # Jujutsu VCS detection + state producer
  p10k-rs-shell      # per-shell init scripts (zsh end-to-end; bash best-effort; fish stub)
  p10k-rs-wizard     # `configure` TUI (stub)
  p10k-rs-ai         # AI-host detection + OSC 7/133 emission
  p10k-rs-ipc        # FIFO plumbing
```

## Build from source / hacking

```bash
git clone https://github.com/scurran1986/powerlevel10k-rs.git
cd powerlevel10k-rs
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --locked
# Dep-policy gates (also enforced in CI):
cargo deny check       # cargo install cargo-deny
cargo machete          # cargo install cargo-machete --version 0.7.0 --locked
```

MSRV: stable - 2 (currently **1.88**). Pinned in `rust-toolchain.toml`.

## Configuration

Drop a TOML file at `~/.config/p10k-rs/config.toml` (or point
`$P10K_RS_CONFIG` at one). Discovery order: `$P10K_RS_CONFIG`,
`$XDG_CONFIG_HOME/p10k-rs/config.toml`, `~/.config/p10k-rs/config.toml`.
Missing or broken file falls back silently to the factory default
(byte-identical to no-config behaviour).

`[layout].left` picks which segments render and in what order.
`[segment.<name>]` overrides per-segment foreground / background
under the active `ColorMode` (8-color, 256-color, or truecolor).
State-specific overrides — e.g. `[segment.vcs.states.dirty]` —
fire when the segment tags its output with that state.

```toml
schema_version = 1

[layout]
left = ["dir", "vcs", "command_execution_time", "status", "prompt_char"]

# Colour the cwd in red instead of the default blue.
[segment.dir]
foreground = "red"

# Magenta branch name when the working tree is dirty;
# yellow otherwise (the default).
[segment.vcs.states.dirty]
foreground = "magenta"
```

Colour values: a Powerlevel9k-style name (`"blue"`, `"brightred"`,
…), an ANSI 256 index (`0`–`255`), or an `[r, g, b]` triple for
truecolor. Padding, icons, separators, frame / ruler decoration,
and `show_in_dir` / `show_on_command` gating are accepted by the
parser but not yet driven into render — those land in subsequent
slices.

Full schema lives in `crates/p10k-rs-config/src/lib.rs`.

## Architecture

- [ADR-0001 — Git Status Backend](docs/adr/0001-git-backend.md):
  why `gitstatusd` over FIFOs, with the spike measurements that
  drove the pivot away from a pure-Rust scanner.
- More ADRs land as decisions are made. Index in `docs/adr/README.md`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Conservative dependencies,
`#![forbid(unsafe_code)]` everywhere except `p10k-rs-git` (where
the `unsafe` budget is documented per call site), doc comments on
every public item, typed errors in libraries.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)
  or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option. Contributions are accepted under the same terms.

`gitstatusd` is bundled as a separate static binary under GPL-3.0;
see [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) for the
bundling rationale per ADR-0001 § Operational.
