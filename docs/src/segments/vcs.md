# `vcs`

Version-control segment for git repositories. Branch name painted
black-on-green, with a trailing `*` marker when the working tree is
dirty. Powered by `gitstatusd` on the hot path; falls back to
`ShellOut` → `GixBackend` (slice 60) when the daemon is unavailable.

## When it appears

When the cwd sits inside a git repository (`ctx.git.is_some()`). Hidden
outside repos so users who shell into non-repo directories see nothing.

## Default render

```
 main                # clean
 feat/widget *       # dirty (red `*`)
 main !rebase        # in-progress action (red action label)
 abcdef0            # detached HEAD — 7-char short SHA + commit glyph
```

(git icon + space + label + markers, black-on-green)

Per-state markers:

| State | Marker | Default colour |
|---|---|---|
| `clean` | (none) | — |
| `dirty` | `*` | red |
| `action` | `!<action>` | red |
| `detached` | ` <sha>` | (label glyph) |

`action` covers rebase / merge / cherry-pick / bisect / revert / am — the
six in-progress states `gitstatusd` reports.

## Config

```toml
[segment.vcs]
foreground = "black"
background = "green"
icon = "\u{f1d3}"
marker_foreground = "red"   # slice 59 — recolour `*` and action label

[segment.vcs.states.dirty]
background = "yellow"

[segment.vcs.states.action]
background = "red"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{f1d3}` | Default Nerd Font v3 git glyph |
| `marker_foreground` | colour | `red` | Dirty `*` and action label colour (slice 59) |

Per-state overrides via `[segment.vcs.states.<state>]` (`clean`,
`dirty`, `action`, `detached`).

## Notes / gotchas

- **`gitstatusd` is the hot-path backend.** `ShellOut` and `GixBackend`
  are fallbacks. See [ADR-0001](../arch/render.md) for the rationale.
- **Branch and action labels pass through `SafeText`** at the binary
  boundary — `\r` / OSC / ANSI injection from a hostile branch name
  can't ride into the prompt.
- **Detached HEAD** renders a `mdi-source-commit` glyph + 7-char SHA
  rather than silently substituting hex for a branch name — visually
  announces "you are not on a branch."
- **Marker colour** routes through `marker_foreground` (slice 59).
  Earlier releases hardcoded red; that's now opt-in via the field.

## See also

- [`jj`](./jj.md) — Jujutsu counterpart.
- [daemon-health subcommand](../reference/daemon-health.md) — diagnose
  a wedged `gitstatusd` daemon.
- [Render pipeline](../arch/render.md) — backend selection and fallback.
