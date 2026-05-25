# `node_version`

Node.js runtime version. Renders `node:<version>` when the cwd sits
inside a Node project.

## When it appears

When the cwd (or any ancestor up to 64 levels) contains a
`package.json` file. When enabled, spawns `node --version` with a
500 ms wall-clock budget; renders empty on timeout, non-zero exit,
or missing binary.

## Default render

```
 node:20.10.0
```

(nodejs icon + space + `node:<version>`, black-on-green)

The leading `v` from `node --version` output is stripped.

## Config

```toml
[segment.node_version]
foreground = "black"
background = "green"
icon = "\u{e718}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `green` | Ribbon colour |
| `icon` | string | `\u{e718}` | Default Nerd Font v3 nodejs glyph |

## Notes / gotchas

- **Per-prompt subprocess.** Spawns `node --version` on every render
  in a Node project. Marker-gated so non-JS directories pay nothing.
  `LC_ALL=C` is set so a hostile locale can't reshape output.
- **500 ms deadline.** A wedged `node` binary (NordVPN freeze,
  hung NFS, corrupted toolchain) cannot lock the prompt — the
  segment renders empty after the deadline.
- Stdout is treated as untrusted — a hostile `$PATH` shadow of
  `node` cannot inject ANSI / `%`-expansion.
- The walker trips on `/tmp/package.json` (gotcha #2 in `STATE.md`);
  one ignored test documents this. Real fix: boundary at `.git` /
  `$HOME`.

## See also

- [`fnm`](./fnm.md), [`nodenv`](./nodenv.md) — version-manager
  segments that read env vars instead of spawning.
- [`mise`](./mise.md) — cross-language manager.
