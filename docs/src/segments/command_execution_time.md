# `command_execution_time`

Duration of the last foreground command. Hidden when the command
took less than 3 seconds.

## When it appears

When `RenderCtx::last_duration >= 3 seconds`. The binary fills the
field from the `--last-duration` CLI arg, which the zsh init
computes from `$EPOCHSECONDS` deltas in `preexec` / `precmd`.

## Default render

```
 7s
 2m05s
 1h12m
```

(stopwatch icon + space + duration, black-on-yellow)

Format ladder:

- `< 60s` → `NNs`
- `< 60m` → `MmSSs`
- `>= 60m` → `HhMMm`

## Config

```toml
[segment.command_execution_time]
foreground = "black"
background = "yellow"
icon = "\u{f43a}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `yellow` | Ribbon colour |
| `icon` | string | `\u{f43a}` | Default Nerd Font v3 stopwatch glyph |

## Notes / gotchas

- The 3-second threshold is currently hardcoded
  (`THRESHOLD_MS = 3000`). A future config field will mirror upstream
  `POWERLEVEL9K_COMMAND_EXECUTION_TIME_THRESHOLD`.
- Sub-second precision (`1.5s`) is not surfaced today — the zsh init
  only forwards `$EPOCHSECONDS` integer seconds. Showing `1.234s`
  without `$EPOCHREALTIME` plumbing would be a lie.
- Bash currently passes `--last-duration-ms 0` so the segment never
  fires under bash; see gotcha #5 in `STATE.md`.

## See also

- [Per-shell init](../reference/shell.md).
