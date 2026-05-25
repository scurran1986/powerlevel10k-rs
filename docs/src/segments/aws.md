# `aws`

AWS profile indicator. Shows `aws:<profile>` when an AWS CLI profile
or vault session is selected.

## When it appears

Reads the first non-empty value from, in order:

1. `$AWS_VAULT` — set by aws-vault during a vault session.
2. `$AWS_PROFILE` — the standard aws-cli profile selector.
3. `$AWS_DEFAULT_PROFILE` — the legacy aws-cli name; back-compat.

Hidden when all three are unset or empty.

## Default render

```
 aws:prod-readonly
```

(cloud icon + space + `aws:<profile>`, black-on-yellow)

## Config

```toml
[segment.aws]
foreground = "black"
background = "yellow"
icon = "\u{f270}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `yellow` | Ribbon colour |
| `icon` | string | `\u{f270}` | Default Nerd Font v3 AWS cloud glyph |

## Notes / gotchas

- Profile names are user-controlled strings; they pass through
  `sanitize_for_terminal` so a name with control bytes can't inject
  CR / ANSI into the prompt line.
- `$AWS_VAULT` wins outright — that matches the actual session
  precedence (your CLI is talking to the vault, not the on-disk
  profile).

## See also

- [`kubecontext`](./kubecontext.md) — sibling cloud-context indicator.
- [`docker_context`](./docker_context.md) — Docker engine selector.
