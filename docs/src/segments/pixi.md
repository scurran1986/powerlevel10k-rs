# `pixi`

[Pixi] project segment. Shows `pixi:<name>` in green when a pixi
project environment is active.

[Pixi]: https://pixi.sh

## When it appears

When `$PIXI_PROJECT_NAME` is set and non-empty. Pixi exports this
(alongside `$PIXI_PROJECT_MANIFEST`) on `pixi shell` and `pixi run`.

## Default render

```
 pixi:my-project
```

(package icon + space + `pixi:<name>`, black-on-green)

## Config

```toml
[segment.pixi]
foreground = "black"
background = "green"
icon = "\u{f487}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{f487}` | Default Nerd Font v3 `mdi-package` glyph |

## Notes / gotchas

- Pixi is conda's lockfile-driven, Rust-native alternative. The segment
  exists because of upstream demand on Powerlevel10k issue #2798.
- Project names are usually bare identifiers, but pixi doesn't validate
  them strongly. The name flows through `sanitize_for_terminal` before
  render so CR / LF / ANSI bytes can't ride in.

## See also

- [`anaconda`](./anaconda.md) — classic conda counterpart.
- [`mise`](./mise.md) — multi-language version manager.
