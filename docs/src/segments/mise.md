# `mise`

Cross-language version manager indicator. Renders `mise` (or
`mise:<profile>` when a profile is selected) when `mise` is active.

`rtx` is accepted as a deprecated alias — mise was renamed in 2024.

## When it appears

When `$MISE_DATA_DIR` is set and non-empty. `mise` exports this on
activation.

Profile name precedence (when present):

1. `$MISE_PROFILE`
2. `$MISE_DEFAULT_TOOL_VERSIONS`

When neither names a profile, renders bare `mise`.

## Default render

```
 mise                  # active, no profile
 mise:production       # active with profile
```

(toolbox icon + space + label, black-on-green)

## Config

```toml
[segment.mise]
foreground = "black"
background = "green"
icon = "\u{f0a9b}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{f0a9b}` | Default Nerd Font v3 mdi-tools glyph |

## Notes / gotchas

- No subprocess. The segment never shells out to `mise current` —
  env vars are the source of truth.
- `rtx` in your TOML resolves to the same segment via an alias arm
  in `build()`; both keys produce identical output.
- Profile names pass through `sanitize_for_terminal` — a hostile
  `.mise.toml` profile name with control bytes can't ride into the
  prompt.

## See also

- [`fnm`](./fnm.md), [`nodenv`](./nodenv.md), [`pyenv`](./pyenv.md) —
  single-language version managers.
- [`anaconda`](./anaconda.md), [`pixi`](./pixi.md) — env-manager peers.
