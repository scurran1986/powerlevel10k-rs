# `context`

`user@host` identity line with privilege / SSH awareness.

## When it appears

Always-on **except** when "you on your own machine":
`$P10K_RS_DEFAULT_USER == $USER`, the state is `local` (no SSH,
not root), and no SSH env var is set. Root and SSH sessions always
show — they're load-bearing context for the human at the prompt.

State tag (first match wins):

| Tag | When |
|---|---|
| `root` | EUID is 0 |
| `remote` | Any of `$SSH_CONNECTION`, `$SSH_CLIENT`, `$SSH_TTY` is set |
| `local` | Otherwise |

## Default render

```
 alice@workstation       # local — yellow bg / black fg
 alice@server-3          # remote — yellow bg / black fg
 root@workstation        # root — RED bg / white fg
```

## Config

```toml
[segment.context]
foreground = "black"
background = "yellow"
icon = "\u{f007}"

[segment.context.states.root]
foreground = "white"
background = "red"

[segment.context.states.remote]
foreground = "black"
background = "yellow"

[segment.context.states.local]
foreground = "black"
background = "yellow"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` (local/remote), `white` (root) | Text colour |
| `background` | colour | `yellow` (local/remote), `red` (root) | Ribbon colour |
| `icon` | string | `\u{f007}` | Default Nerd Font v3 person glyph |

Per-state `[segment.context.states.<tag>]` overrides win over the
segment block. State tags: `root`, `remote`, `local`.

## Notes / gotchas

- `$P10K_RS_DEFAULT_USER` is the hide-toggle. Set it to your
  unprivileged username; root and SSH override the hide.
- Both `$USER`/`$LOGNAME` and `$HOSTNAME`/`uname(2).nodename` pass
  through `sanitize_for_terminal` before render — a hostile hostname
  containing CR / ESC can't ride into the prompt.
- Root wins over SSH outright (root-over-SSH is still root).

## See also

- [`root_indicator`](./root_indicator.md) — single-glyph alternative.
- [Theming](../theming.md) — per-state colour overrides.
