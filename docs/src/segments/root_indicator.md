# `root_indicator`

Visible warning when the shell is running as root.

## When it appears

When the effective UID is 0 (`geteuid() == 0`). EUID, not RUID — a
setuid-root binary that dropped privileges shouldn't keep flashing
red.

## Default render

```
 
```

(user-secret icon, red, no label)

Single glyph is enough — anything more would be noise. The colour
carries the warning.

## Config

```toml
[segment.root_indicator]
foreground = "red"
icon = "\u{f2be}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `red` | Glyph colour |
| `icon` | string | `\u{f2be}` | Default Nerd Font v3 `user-secret` glyph |

## Notes / gotchas

- Users who never want this segment can drop `"root_indicator"` from
  `[layout].left` / `.right` in their config. It's wired by default
  because the visual warning is load-bearing — a forgotten root shell
  has been the start of more than one outage.
- No background colour by default — the glyph + red foreground is
  meant to read as "raw warning," not "ribbon segment." Override
  `background` if you prefer a banded look.

## See also

- [`context`](./context.md) — full `user@host` rendering with SSH /
  privilege awareness.
