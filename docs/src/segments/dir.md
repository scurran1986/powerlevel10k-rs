# `dir`

Current working directory. Cwd painted black-on-blue, `$HOME`
collapsed to `~`, with optional component truncation.

## When it appears

Always-on.

## Default render

```
 ~/github/powerlevel10k-rs
```

(folder icon + space + collapsed path, black-on-blue)

Per-state palette:

| State | Background | Foreground | Icon |
|---|---|---|---|
| `writable` | blue | black | folder (`\u{f07b}`) |
| `not_writable` | yellow | black | padlock (`\u{f023}`) |

`not_writable` fires when `access(W_OK)` on the cwd fails (root-owned
dir, EACCES on parent, broken cwd, etc.). The padlock + yellow
warning is the same shape upstream P10K's `DIR_NOT_WRITABLE_*` ships.

## Config

```toml
[segment.dir]
foreground = "black"
background = "blue"
icon = "\u{f07b}"

[segment.dir.truncate]
strategy = "to_last"        # none / to_last / middle / to_unique
length = 3

[segment.dir.states.not_writable]
foreground = "black"
background = "yellow"
icon = "\u{f023}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `blue` | Ribbon colour |
| `icon` | string | `\u{f07b}` | Default Nerd Font v3 folder glyph |
| `truncate.strategy` | enum | `none` | `none` / `to_last` / `middle` / `to_unique` |
| `truncate.length` | u8 | `3` | Components to keep (`0` is treated as `1`) |

Truncation strategies:

| Strategy | Effect |
|---|---|
| `none` | Full home-collapsed path |
| `to_last` | `…/<last length>` |
| `middle` | `<first>/…/<last length-1>` |
| `to_unique` | Each non-final component shortened to its shortest unique prefix on disk |

## Notes / gotchas

- `to_unique` issues one `read_dir` per non-final component, capped
  at 200 entries per parent. Slow on NFS / FUSE. Opt-in only.
- Cwd flows through `SafeText` at the binary boundary — branch and
  cwd injection vectors (`\r`, OSC, ANSI) are stripped before the
  segment sees them.
- `cwd_display` is built once in the binary; the segment is
  zero-copy.

## See also

- [`vcs`](./vcs.md) — pairs with `dir` to show repo state.
- [Configuration schema](../reference/schema.md).
