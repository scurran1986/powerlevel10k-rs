# `nodenv`

Node.js version-manager segment. Shows `node:<version>` in green when
a nodenv shim has resolved a version.

## When it appears

When `$NODENV_VERSION` is set and non-empty. nodenv exports this on
shim resolution (`.node-version` walk, `nodenv shell`, or a global
default).

## Default render

```
 node:20.11.0
```

(node icon + space + `node:<version>`, black-on-green)

## Config

```toml
[segment.nodenv]
foreground = "black"
background = "green"
icon = "\u{e718}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{e718}` | Default Nerd Font v3 `mdi-nodejs` glyph |

## Notes / gotchas

- The segment reads `$NODENV_VERSION` directly rather than shelling out
  to `nodenv version-name` per render — that would reintroduce the
  latency tax this project exists to avoid.
- A hostile `.node-version` file with a CR byte would otherwise let an
  attacker overwrite the prompt line; `sanitize_for_terminal` strips
  CR / LF / ANSI / OSC before render.

## See also

- [`fnm`](./fnm.md) — modern Node version-manager alternative.
- [`node_version`](./node_version.md) — cwd-derived Node version
  (independent of any version manager).
- [`mise`](./mise.md) — multi-language successor that handles Node
  alongside Python / Ruby / etc.
