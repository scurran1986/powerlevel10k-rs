# p10k-rs

A Rust port and spiritual successor to [Powerlevel10k][p10k]. Single
static binary, declarative TOML config, multi-shell support, with
`gitstatusd`-class git latency as the load-bearing performance claim.

[p10k]: https://github.com/romkatv/powerlevel10k

## Quickstart

One command. Clones the repo, builds the binary, wires zsh:

```bash
git clone https://github.com/scurran1986/powerlevel10k-rs.git ~/.local/share/powerlevel10k-rs && ~/.local/share/powerlevel10k-rs/install.sh
```

Open a new zsh terminal — the prompt is live.

Requirements: `cargo` (install via [rustup](https://rustup.rs)), zsh,
git. The installer drops the binary at `~/.cargo/bin/p10k-rs`,
appends an `eval "$(p10k-rs init zsh)"` line to `~/.zshrc`, and
symlinks `gitstatusd` next to the binary if a canonical install is
on `PATH` (otherwise the slow `git` shell-out fallback kicks in;
see [docs/adr/0001-git-backend.md](docs/adr/0001-git-backend.md)).

To uninstall:

```bash
~/.local/share/powerlevel10k-rs/install.sh --uninstall
```

## Status

Early-alpha. Daily-driver-grade for the maintainer. Not packaged
for general use yet.

11 slices shipped:

1. **slice 1** — Minimum runnable prompt: `dir` + `prompt_char`
2. **slice 2** — ANSI color emission with zsh-aware `%{…%}` width tracking
3. **slice 3** — `$?`-aware `prompt_char` (green on success, red on failure)
4. **slice 4** — `vcs` segment via `git` shell-out
5. **slice 5** — `command_execution_time` (cyan duration past 3 s)
6. **slice 6** — `gitstatusd` long-lived daemon backend (ADR-0001 hot path)
7. **slice 7** — Daemon hardening: 2 s `poll(2)` timeout, auto-respawn, rich `vcs` render
8. **slice 8** — Instant prompt: sub-ms first prompt via cached dump file
9. **slice 9** — Review-driven hardening: FIFO security, GPL wiring, doc refresh
10. **slice 10** — `status` segment: exit code shown red on non-zero `$?`
11. **slice 11** — Render-path hardening: `%`-expansion + ANSI-injection sanitization

53 tests pass workspace-wide. Builds clean on stable Rust 1.88.

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
| `dir` segment with `~` collapse | done |
| `vcs` segment (branch + dirty + ahead/behind + conflict) | done |
| `command_execution_time` segment | done |
| `status` segment (exit-code on failure) | done |
| `prompt_char` segment ($?-aware) | done |
| Instant prompt (sub-ms first shell) | done |
| `gitstatusd` long-lived daemon backend | done |
| `git` shell-out fallback | done |
| Branch / cwd render-path sanitization | done |
| TOML config | placeholder |
| `p10k-rs configure` wizard | placeholder |
| `p10k-rs import ~/.p10k.zsh` (P9k import) | placeholder |
| `bash` / `fish` init scripts | placeholder |
| Multi-arch binary distribution | linux-x86_64 only |

The remaining MVP segments (`background_jobs`, `time`, `context`,
`vi_mode`, `kubecontext`, etc.) are enumerated in `MVP-SPEC.md`
§ 1.2 and surface in `segment_names()` in
`crates/p10k-rs-segments/src/lib.rs`.

## Workspace layout

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import (data only)
  p10k-rs-segments   # segment implementations
  p10k-rs-git        # gitstatusd client + git shell-out fallback
  p10k-rs-shell      # per-shell init scripts
  p10k-rs-wizard     # `configure` TUI (stub)
  p10k-rs-ai         # AI-host detection / OSC emission (stub)
```

## Build from source / hacking

```bash
git clone https://github.com/scurran1986/powerlevel10k-rs.git
cd powerlevel10k-rs
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

MSRV: stable - 2 (currently **1.88**). Pinned in `rust-toolchain.toml`.

## Configuration

The TOML config schema lives in `p10k-rs-config` but isn't wired
yet — `default_layout()` in `crates/p10k-rs-segments/src/lib.rs`
is the canonical 5-segment order today. Configurable layout lands
with the TOML-loader phase.

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
