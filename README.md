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

> **Status: done.** This project is complete and no longer under
> active development. It reached its goal at v1.0.0; no further work
> is planned. The code remains available as-is.

> **v1.0.0 — stability commitment.** `p10k-rs` now follows
> SemVer for the surfaces listed in [STABILITY.md](STABILITY.md):
> the binary CLI, TOML config schema, per-shell init protocol,
> and release artifacts. The Rust crate API is binary-only and
> may break in minor releases. See `.github/release-notes/v1.0.0.md`
> for the full narrative.

> [!WARNING]
> **THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.**
> Free, AI-assisted hobby project. **No warranty. No support. No SLA.
> No liability accepted.** May contain defects of any kind or stop
> being maintained without notice. **Use entirely at your own risk.**
> Don't run it where consequences matter.
>
> Full disclaimer, limitation of liability, and assumption-of-risk
> terms in **[POLICIES.md](POLICIES.md)**. By using this software you
> accept those terms in addition to the [MIT](LICENSE-MIT) and
> [Apache-2.0](LICENSE-APACHE) licenses.

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
| Pick a bundled theme and switch to it in one command | [themes/](themes/README.md) |
| See what works today (segments, features, supported shells) | [STATUS.md](STATUS.md) |
| Import an existing `~/.p10k.zsh` | [IMPORTING.md](IMPORTING.md) |
| Verify a release signature, report a defect privately | [SECURITY.md](SECURITY.md) |
| Diagnose a slow / wedged prompt (`p10k-rs daemon-health`) | [daemon-health reference](docs/src/reference/daemon-health.md) |
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
