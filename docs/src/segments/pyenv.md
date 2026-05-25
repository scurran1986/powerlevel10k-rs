# `pyenv`

Active pyenv Python version segment. Shows `py:<version>` in yellow when
a pyenv shim has resolved a Python version.

## When it appears

When `$PYENV_VERSION` is set and non-empty. pyenv exports this on
shim resolution (`pyenv shell`, `pyenv local`, or a `.python-version`
walk).

## Default render

```
 py:3.12.1
```

(python icon + space + `py:<version>`, black-on-yellow)

Special values pass through unchanged: `py:system`, `py:2.7.18`,
`py:2.7.18/envs/legacy` (the env-name form pyenv emits when a venv is
attached to a base version).

## Config

```toml
[segment.pyenv]
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

- The segment reads `$PYENV_VERSION` directly rather than shelling out
  to `pyenv version` — env-var-first matches pyenv's own prompt
  integration and avoids a fork per render.
- `$PYENV_VERSION` is attacker-influenceable via a malicious
  `.python-version` file or hostile parent process. The value flows
  through `sanitize_for_terminal` before render.

## See also

- [`virtualenv`](./virtualenv.md) — Python `venv` counterpart.
- [`anaconda`](./anaconda.md) — conda counterpart.
- [`nodenv`](./nodenv.md) — Node.js version-manager analogue.
