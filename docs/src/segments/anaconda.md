# `anaconda`

Conda environment indicator. Shows `conda:<name>` when a conda
environment is activated.

## When it appears

When `$CONDA_DEFAULT_ENV` is set and non-empty. Conda's activation
hooks export this on `conda activate`.

## Default render

```
 conda:base
```

(snake icon + space + `conda:<name>`, black-on-green)

## Config

```toml
[segment.anaconda]
foreground = "black"
background = "green"
icon = "\u{e73c}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{e73c}` | Default Nerd Font v3 python-alt glyph |

## Notes / gotchas

- When an environment is created via `conda create -p /some/path`,
  `$CONDA_DEFAULT_ENV` carries the full path. The segment takes the
  basename in that case (`/home/x/conda/myproj` → `myproj`).
- The basename passes through `sanitize_for_terminal` before render
  so a path with `\r` / ESC bytes can't ride into the prompt.

## See also

- [`virtualenv`](./virtualenv.md) — Python `venv` activator counterpart.
- [`pyenv`](./pyenv.md) — pyenv shim version.
- [`pixi`](./pixi.md) — conda's lockfile-driven alternative.
