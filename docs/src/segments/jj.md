# `jj`

Jujutsu VCS segment. Mirror of [`vcs`](./vcs.md) for `.jj/` working
copies — shows the primary bookmark (or short change-id), with
dirty / conflict markers.

## When it appears

When `ctx.jj.is_some()` — i.e. the cwd sits inside a Jujutsu working
copy. Producer lives in `p10k-rs-jj`. Auto-hidden outside jj repos,
so safe to keep in the always-on group; users who run only git pay
nothing for it.

## Default render

```
 main                # clean
 feat/widget *       # dirty (red `*`)
 main !              # conflict
 abcdef12            # no bookmark — short change-id fallback
```

(jj icon + space + label + markers, black-on-green)

Per-state:

| State | When |
|---|---|
| `clean` | No dirty / conflict / divergence |
| `dirty` | Working copy has uncommitted changes |
| `conflict` | Unresolved conflict (`!` marker) |
| `diverged` | Divergent change |

## Config

```toml
[segment.jj]
foreground = "black"
background = "green"
icon = "\u{e702}"

[segment.jj.states.dirty]
foreground = "yellow"

[segment.jj.states.conflict]
background = "red"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Label colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{e702}` | Default git glyph (shared with `vcs`) |

## Notes / gotchas

- **Slow backend.** `is_fast()` returns `false` — the shell-out
  backend spawns `jj` twice per render (log + status). A future
  daemon analogue (analogous to `gitstatusd` for git) would flip this
  to `true`.
- Visual band matches `vcs` (black-on-green) intentionally — a user
  with both in their layout reads either as "a VCS segment". The
  icon glyph is the disambiguator. Override `icon` if you want
  glyph-level disambiguation in your TOML.
- Bookmark name is the preferred label; falls back to the first 8
  chars of `change_id` when no bookmark is set.
- The dirty `*` marker colour is currently hardcoded red and is not
  routed through `marker_foreground`. Tracked alongside `vcs` marker
  styling.

## See also

- [`vcs`](./vcs.md) — git counterpart.
