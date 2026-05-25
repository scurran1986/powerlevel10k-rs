# Quick start

One line. Clones the repo to `~/.local/share/powerlevel10k-rs`, builds
the binary, wires zsh:

```bash
curl -fsSL https://raw.githubusercontent.com/scurran1986/powerlevel10k-rs/main/get.sh | bash
```

Open a new zsh terminal — the prompt is live.

> **WSL2 + Windows Terminal users:** if segment icons render as `◆`
> diamonds while the `▶` chevrons look fine, you need a Nerd Font
> installed on the **Windows** side. See
> [WSL2 + Windows Terminal](./wsl-windows.md).

Requirements: `cargo` (install via [rustup](https://rustup.rs)), `zsh`,
`git`, `curl`. The installer drops the binary at `~/.cargo/bin/p10k-rs`,
appends an `eval "$(p10k-rs init zsh)"` line to `~/.zshrc`, and
symlinks `gitstatusd` next to the binary if a canonical install is on
`PATH` (otherwise the slow `git` shell-out fallback kicks in).

Re-piping the same command upgrades an existing install. To uninstall:

```bash
~/.local/share/powerlevel10k-rs/install.sh --uninstall
```

## Install flags

The underlying `install.sh` accepts a few overrides:

| Flag | Effect |
|---|---|
| `--shell zsh` | Explicit shell selection (default `zsh`; only `zsh` is wired today) |
| `--no-rc` | Build + install binary, leave the shell rc file alone |
| `--no-build` | Skip the `cargo build` step (use an existing binary) |
| `--uninstall` | Reverse: remove the rc line and the binary |

`fish` and `bash` init scripts ship but their installer wiring lands in
a later slice — see [Per-shell init](./reference/shell.md) for what
each shell currently supports.

## Importing an existing Powerlevel10k config

```bash
p10k-rs import ~/.p10k.zsh > ~/.config/p10k-rs/config.toml
```

Best-effort textual translation — does not execute your zsh config.
Unrecognised variables are reported to stderr.
