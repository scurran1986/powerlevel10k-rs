# `fnm`

Fast Node Manager version. Shows `fnm:<version>` when `fnm`'s shell
integration is exporting a version.

## When it appears

When `$FNM_NODE_VERSION` is set and non-empty. `fnm env` exports
this whenever it resolves to a specific version (per-directory
`.node-version` / `.nvmrc`, `fnm use`, or a configured default).

## Default render

```
 fnm:v20.10.0
```

(nodejs icon + space + `fnm:<version>`, black-on-green)

## Config

```toml
[segment.fnm]
foreground = "black"
background = "green"
icon = "\u{f898}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{f898}` | Default Nerd Font v3 mdi-nodejs glyph |

## Notes / gotchas

- No subprocess. The segment never shells out to `fnm current` — the
  env var is the source of truth.
- fnm prefixes its exported version with a leading `v`
  (e.g. `v20.10.0`); the segment renders whatever fnm emits.
- When `$FNM_VERSION_FILE_STRATEGY=local` keeps the version out of
  the environment the segment hides. Reading the
  `<FNM_MULTISHELL_PATH>/installation` symlink target was considered
  but deferred to keep MVP zero-syscall.
- The value passes through `sanitize_for_terminal` — a
  `.node-version` with trailing CR can't ride into the prompt.

## See also

- [`nodenv`](./nodenv.md) — nodenv counterpart.
- [`node_version`](./node_version.md) — runtime-version segment.
- [`mise`](./mise.md) — cross-language version manager.
