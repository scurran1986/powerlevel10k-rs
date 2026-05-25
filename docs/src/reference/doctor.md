# doctor subcommand

Runtime diagnostic for the common environmental snags that break a fresh
install: missing Nerd Font glyphs, missing `gitstatusd`, broken shell
init, stale instant-prompt cache permissions, unparseable config, OSC 7
unsupported terminals, and the WSL + Windows font gap.

## When to run

- First thing after a fresh install (or upgrade) on a new host
- When the prompt renders garbled glyphs (`◆` placeholders)
- When git state stops showing or feels slow
- Before filing a bug report — the JSON output captures everything we'd ask for

## Outcomes

Each check reports one of four statuses and contributes to the exit code:

| Status | Meaning | Affects exit |
|---|---|---|
| `OK` | Probe passed | no |
| `WARN` | User should look at it, but the prompt still renders | exit ≥ 1 |
| `ERROR` | Almost certainly broken — fix before expecting a working prompt | exit ≥ 2 |
| `SKIP` | Could not be evaluated in this environment | no |

| Exit code | Meaning |
|---|---|
| 0 | All `OK` or `SKIP` |
| 1 | At least one `WARN`, no `ERROR` |
| 2 | At least one `ERROR` |

## Output formats

```text
$ p10k-rs doctor
[OK   ] gitstatusd_binary: found at /usr/local/bin/gitstatusd
[WARN ] shell_init_sourced: no `_P10K_RS_GITSTATUSD_*` env vars — run `eval "$(p10k-rs init zsh)"` from your shell rc
[SKIP ] osc7_supported: TERM_PROGRAM not set; cannot infer terminal capabilities
doctor: warnings present (exit 1)
```

`--json` emits a schema-versioned envelope (`p10krs.doctor/v1`) for scripts:

```text
$ p10k-rs doctor --json
{"schema":"p10krs.doctor/v1","exit_code":1,"checks":[
  {"name":"gitstatusd_binary","status":"OK","message":"found at …"},
  {"name":"shell_init_sourced","status":"WARN","message":"no `_P10K_RS_GITSTATUSD_*` env vars — …"}
]}
```

Field order is stable. `name` keys are snake_case so `jq '.checks[] | select(.status=="ERROR")'`
works without quoting.

## Checks

| Name | Probes |
|---|---|
| `nerd_font_glyphs` | `$LANG` / `$LC_*` for UTF-8 locale + WSL gate |
| `gitstatusd_binary` | `p10k_rs_git::locate_gitstatusd()` matches the prompt's lookup |
| `gitstatusd_version_pin` | Bundled sha256 pins vs. installed binary |
| `config_file_present` | `$P10K_RS_CONFIG` / `$XDG_CONFIG_HOME` / `~/.config/p10k-rs/config.toml` |
| `config_file_parses` | Schema-validate the resolved config file |
| `shell_init_sourced` | `_P10K_RS_GITSTATUSD_*` env vars (set by `p10k-rs init zsh`) |
| `instant_prompt_cache_writable` | `~/.cache/p10k-rs` permissions and writability |
| `osc7_supported` | `$TERM_PROGRAM` against a known-good list |
| `wsl_windows_font_warning` | WSL + Windows-side font story gap |

`nerd_font_glyphs` is a heuristic — it flags the obvious failure modes
(non-UTF-8 locale, WSL) but cannot read the OS font registry from a
TTY-only process. A `SKIP` here is not a guarantee that fonts are fine;
verify visually that folder / branch glyphs render.

## See also

- [daemon-health subcommand](./daemon-health.md) — paired diagnostic for the gitstatusd respawn channel.
- [Per-shell init](./shell.md) — what `eval "$(p10k-rs init zsh)"` actually does.
