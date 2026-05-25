# `virtualenv`

Python virtual-environment segment. Shows `(<basename>)` in yellow when
a `venv` (or `pyenv-virtualenv`) environment is active.

## When it appears

When `$VIRTUAL_ENV` is set and non-empty. Python's `venv` activator and
`pyenv-virtualenv` both export this on `source activate`.

## Default render

```
 (myproject)
```

(python icon + space + `(<basename>)`, black-on-yellow)

The basename comes from the path: `/home/x/projects/api/.venv` →
`.venv`. Use `VIRTUAL_ENV_PROMPT` if you want a custom label; that's
out of scope for this segment in the MVP.

## Config

```toml
[segment.virtualenv]
foreground = "black"
background = "yellow"
icon = "\u{e235}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `yellow` | Ribbon colour |
| `icon` | string | `\u{e235}` | Default Nerd Font v3 python glyph |

## Notes / gotchas

- The basename is attacker-influenceable in the same way `cwd` is — a
  venv at `/tmp/evil\rOVERWRITE/` would otherwise let a CR ride into the
  prompt. `sanitize_for_terminal` strips CR / LF / ANSI / OSC before
  render.
- The segment reads `$VIRTUAL_ENV` directly rather than threading it
  through `RenderCtx` — same hot-path trade-off as `anaconda`. Adding a
  snapshot field for one segment isn't worth the indirection.

## See also

- [`anaconda`](./anaconda.md) — conda counterpart.
- [`pyenv`](./pyenv.md) — pyenv shim version.
- [`pixi`](./pixi.md) — Pixi project alternative.
