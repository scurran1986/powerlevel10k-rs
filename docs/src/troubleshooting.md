# Troubleshooting

First step for anything weird:

```sh
p10k-rs doctor
```

Nine probes against the common fresh-install snags. See the
[doctor subcommand](./reference/doctor.md) reference for what each
check covers and what the exit codes mean.

## Common issues

### Glyphs render as `◆` placeholders

A Nerd Font is required for the segment icons (folder, branch,
python, etc.). On native Linux / macOS install MesloLGS NF (or any
Nerd Font v3) and set it as the terminal's primary font.

On WSL2 + Windows Terminal the font has to live on the *Windows* side,
not the WSL distro:

```sh
p10k-rs install-fonts --windows
```

…then set Windows Terminal → Settings → your profile → Appearance →
Font face to `MesloLGS NF`. See
[WSL2 + Windows Terminal](./wsl-windows.md) for the full walkthrough.

### Prompt feels slow or git state stops showing

The `gitstatusd` daemon may have wedged or crashed. Diagnose:

```sh
p10k-rs daemon-health
```

See [daemon-health subcommand](./reference/daemon-health.md) for the
status / exit-code wire. The `precmd` health-check hook respawns a
dead or wedged daemon automatically on the next prompt; if it isn't
recovering, your shell init likely isn't sourcing `p10k-rs init zsh`
correctly — `p10k-rs doctor` flags that.

### `verify` reports a sha256 mismatch

```text
MISMATCH expected=… got=…
```

The installed `gitstatusd` binary doesn't match the bundled supply-chain
pin. Either you updated `gitstatusd` independently of `p10k-rs`, or
something on PATH is shadowing the expected binary. Re-run `install.sh`
or set `$P10K_RS_GITSTATUSD_BIN` to the correct path. See
[SECURITY.md](https://github.com/scurran1986/powerlevel10k-rs/blob/main/SECURITY.md)
for the verification recipe.

### Config doesn't seem to take effect

```sh
p10k-rs doctor    # is `config_file_present` / `config_file_parses` OK?
```

The resolution order is `$P10K_RS_CONFIG` → `$XDG_CONFIG_HOME/p10k-rs/config.toml`
→ `~/.config/p10k-rs/config.toml`. If `config_file_parses` reports
`ERROR`, the message includes the schema error from
`p10k_rs_core::Config::load_from_path`. Truncate to the failing field
in [the schema reference](./reference/schema.md).

### Instant prompt feels wrong after a terminal switch

Slice 63 embeds the sanitised `$TERM` in the instant-prompt dump
filename (`dump-<user>-<term>.zsh`), so switching terminals
auto-invalidates the cache. If you're seeing stale dumps anyway, check
that `$TERM` differs between the two terminals (some terminal
multiplexers preserve the outer `TERM`); otherwise nuke
`~/.cache/p10k-rs/dump-*` and let the next prompt rebuild.

## Filing a bug

Include the output of all three diagnostics — they capture everything
the maintainer is likely to ask for:

```sh
p10k-rs version --json
p10k-rs verify --json
p10k-rs doctor --json
```

File at <https://github.com/scurran1986/powerlevel10k-rs/issues>.
