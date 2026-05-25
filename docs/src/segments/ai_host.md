# `ai_host`

AI coding-assistant host badge. Surfaces the active AI host (Claude
Code, Aider, Cursor, Goose) as a magenta band so it's immediately
obvious you're inside an agent shell.

## When it appears

When the binary detects one of the known AI host environments via
env-var probes performed in `p10k-rs-ai`. Hidden when no host is
detected (`HostKind::None` or any `Generic` variant).

Detected hosts:

| Host | Label |
|---|---|
| Claude Code | `claude-code` |
| Aider | `aider` |
| Cursor | `cursor` |
| Goose | `goose` |

## Default render

```
 claude-code
```

(cog icon + space + host label, white-on-magenta)

## Config

```toml
[segment.ai_host]
foreground = "white"
background = "magenta"
icon = ""           # default: Nerd Font v3 cog
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `white` | Label colour |
| `background` | colour | `magenta` | Ribbon colour |
| `icon` | string | `` | Glyph rendered before the label |

Standard `padding`, `disabled`, `show_in_dir`, `disabled_dir_pattern`
fields apply.

## Notes / gotchas

- Hiding the segment per-host (e.g. "hide under Claude Code, show
  under Cursor") goes through `[ai.host.<name>].mode = "hidden"`, not
  the segment block. See [troubleshooting](../troubleshooting.md).
- `HostKind` is `#[non_exhaustive]`; future hosts land via additive
  enum variants without a schema break.

## See also

- [`ai_status`](./ai_status.md) — paired sidecar segment that reads
  model name + context-window usage from disk.
- [ai_status sidecar contract](../reference/ai-status.md).
