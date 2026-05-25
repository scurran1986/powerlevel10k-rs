# `docker_context`

Docker current-context. Shows `docker:<context>` when a non-default
Docker context is active.

## When it appears

Detection precedence:

1. `$DOCKER_CONTEXT` — explicit override.
2. `$DOCKER_HOST` — socket override; shows the (truncated) host string.
3. `~/.docker/config.json` `currentContext` field.

Hidden when the resolved value is `default` (the implicit local engine)
or unresolvable. Matches upstream P10K's `DOCKER_CONTEXT_DEFAULT_FILTER`.

## Default render

```
 docker:remote-build
```

(whale icon + space + `docker:<context>`, black-on-cyan)

## Config

```toml
[segment.docker_context]
foreground = "black"
background = "cyan"
icon = "\u{f308}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `cyan` | Ribbon colour |
| `icon` | string | `\u{f308}` | Default Nerd Font v3 Docker whale glyph |

## Notes / gotchas

- Context names and `$DOCKER_HOST` values pass through
  `sanitize_for_terminal` before render.
- Output is truncated to 40 visual chars with a trailing `…` — long
  `tcp://…` URLs don't blow out the prompt width.
- The `~/.docker/config.json` parser only reads `currentContext`;
  everything else (`auths`, `credsStore`, plugins) is ignored.

## See also

- [`kubecontext`](./kubecontext.md) — sibling cloud-context segment.
- [`aws`](./aws.md) — AWS profile counterpart.
