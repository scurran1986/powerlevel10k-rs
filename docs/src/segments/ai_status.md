# `ai_status`

AI host sidecar status. Reads a small JSON file the AI host (or a
user-written hook) drops in `$XDG_RUNTIME_DIR/p10k-rs/ai/<host>.json`
and renders the currently-active model and context-window usage.

## When it appears

All of the following must hold:

- An AI host is detected (same gate as [`ai_host`](./ai_host.md)).
- `$XDG_RUNTIME_DIR` is set and points at a uid-private directory.
- A sidecar file `$XDG_RUNTIME_DIR/p10k-rs/ai/<host>.json` exists,
  is owner-owned, is a regular file, and is at most 16 KiB.
- The JSON parses cleanly.
- `last_updated_unix` is within the last 300 seconds (5 minutes).

Any failure renders the segment empty without logging — the host is
presumed to have crashed or moved on.

## Default render

```
 claude-sonnet-4-6 25% [thinking]
```

Format: `<icon> <model> <used%> [<status>]`. Sub-tokens are omitted
when their backing fields are absent.

## Config

```toml
[segment.ai_status]
foreground = "white"
background = "magenta"
icon = "\u{f06a9}"        # default: Nerd Font v3 robot face
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `white` | Body colour |
| `background` | colour | `magenta` | Ribbon colour |
| `icon` | string | `\u{f06a9}` | Glyph rendered before the model |

Standard `padding`, `disabled`, `show_in_dir`, `disabled_dir_pattern`
fields apply.

## Notes / gotchas

- **Security posture is load-bearing.** The sidecar opens through
  `open_owned_safely` on Unix — symlinks (`O_NOFOLLOW`), foreign-owned
  files, and non-regular files are refused. Reads are hard-capped at
  16 KiB.
- **No `/tmp` fallback.** When `$XDG_RUNTIME_DIR` is unset the segment
  stays empty rather than reading a shared path. Users on non-systemd
  systems can export `XDG_RUNTIME_DIR=/run/user/$(id -u)` (mode `0o700`)
  at session start.
- Strings (`model`, `status`, `host`) pass through `SafeText` with
  byte caps (64 / 32 / 32 graphemes). A maliciously-named model
  containing ESC sequences cannot leak into the rendered prompt.
- `used%` is clamped to `0..=100`; a zero-window or missing-count
  payload omits the percentage instead of rendering `NaN%`.

## See also

- [ai_status sidecar contract](../reference/ai-status.md) — full JSON
  schema, writer pseudocode, and security notes.
- [`ai_host`](./ai_host.md) — paired env-var-driven badge.
