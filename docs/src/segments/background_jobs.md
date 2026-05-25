# `background_jobs`

Count of suspended / running background jobs. Hidden when the shell
has no jobs (the common case).

## When it appears

When `RenderCtx::jobs > 0`. The binary fills the field from the
`--jobs` CLI arg, which the zsh init script captures via
`$#jobstates` at prompt-render time.

## Default render

```
 ⚙3
```

(cog icon + space + `⚙<count>`, black-on-cyan)

## Config

```toml
[segment.background_jobs]
foreground = "black"
background = "cyan"
icon = "\u{f013}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `cyan` | Ribbon colour |
| `icon` | string | `\u{f013}` | Default Nerd Font v3 cog glyph |

## Notes / gotchas

- The `⚙` glyph in the rendered body is literal text and **not**
  configurable independently of `icon` today; the only way to alter it
  is a per-segment fork. File an issue if you need this.
- Bash today never populates `--jobs` (no clean hook); the segment
  stays hidden under bash. Tracked as gotcha #5 in `STATE.md`.

## See also

- [Per-shell init](../reference/shell.md) — what each shell wires.
