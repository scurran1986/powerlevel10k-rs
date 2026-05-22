```
   ╭─  p10k-rs
   ╰─❯  A Rust port and spiritual successor to Powerlevel10k.
```

[![release](https://img.shields.io/github/v/release/scurran1986/powerlevel10k-rs?label=release&color=blueviolet)](https://github.com/scurran1986/powerlevel10k-rs/releases)
[![ci](https://img.shields.io/github/actions/workflow/status/scurran1986/powerlevel10k-rs/ci.yml?branch=main&label=ci&logo=github)](https://github.com/scurran1986/powerlevel10k-rs/actions/workflows/ci.yml)
[![rust](https://img.shields.io/badge/Rust-1.88+-orange?logo=rust)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue)](LICENSE-MIT)

Single static binary. Declarative TOML config. Multi-shell.
`gitstatusd`-class git latency.

> [!WARNING]
> **No warranty. No support. Use at your own risk.** Experimental,
> AI-assisted hobby project. May have bugs, security issues, or stop
> being maintained. Don't run it where it matters. See
> [POLICIES.md](POLICIES.md).

## Quick start

One line. Clones the repo to `~/.local/share/powerlevel10k-rs`,
builds the binary, wires zsh:

```bash
curl -fsSL https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/get.sh | bash
```

Open a new zsh terminal — the prompt is live.

**Requirements:** `cargo` ([rustup.rs](https://rustup.rs)), `zsh`,
`git`, `curl`. The installer drops the binary at
`~/.cargo/bin/p10k-rs` and appends an `eval "$(p10k-rs init zsh)"`
line to `~/.zshrc`.

**Upgrade:** re-pipe the same `curl ... | bash` command.

**Uninstall:**

```bash
~/.local/share/powerlevel10k-rs/install.sh --uninstall
```

**Verify the install (T0.5):**

```bash
p10k-rs verify
# OK x86_64-linux-gnu v1.5.4 02b7bc11a70a
```

## Where to go next

| If you want to… | Read |
|---|---|
| Configure the prompt (TOML schema, colours, layout) | [User guide](docs/src/SUMMARY.md) |
| See what works today (segments, features, supported shells) | [STATUS.md](STATUS.md) |
| Import an existing `~/.p10k.zsh` | [IMPORTING.md](IMPORTING.md) |
| Verify a release signature / understand the threat model | [SECURITY.md](SECURITY.md) |
| Confirm there's no telemetry | [PRIVACY.md](PRIVACY.md) |
| Hack on the code | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Understand maintenance, AI-development model, trademarks | [POLICIES.md](POLICIES.md) |
| Read the slice-by-slice history | [CHANGELOG.md](CHANGELOG.md) |
| Read the architectural decision records | [docs/adr/](docs/adr/) |

## License

Dual-licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option. `gitstatusd` is
bundled separately under GPL-3.0; see
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).
