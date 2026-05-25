# `kubecontext`

Kubernetes current-context. Shows `k8s:<context>` when a kubeconfig
is readable.

## When it appears

When the resolved kubeconfig contains a `current-context:` entry.
Resolution:

1. `$KUBECONFIG` if set and non-empty (**first** colon-separated path
   only — multi-file merging is out of scope).
2. `~/.kube/config` as the fallback.

## Default render

```
 k8s:prod-us-east
```

(kubernetes wheel + space + `k8s:<context>`, black-on-cyan)

## Config

```toml
[segment.kubecontext]
foreground = "black"
background = "cyan"
icon = "\u{f10fe}"
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `foreground` | colour | `black` | Text colour |
| `background` | colour | `cyan` | Ribbon colour |
| `icon` | string | `\u{f10fe}` | Default Nerd Font v3 kubernetes glyph |

## Notes / gotchas

- **Hand-rolled YAML parse.** A single line is scraped out of
  kubeconfig — no YAML crate dependency. Will not handle anchors,
  multi-line scalars, or `current-context:` appearing inside an
  unrelated string literal. 95%-correct against real-world configs.
- **No file watch.** The file is re-read on every prompt. Cheap
  (one stat + small read on warm cache).
- Context names pass through `SafeText::from_untrusted` — anyone with
  a kubeconfig writes them, so they're treated as untrusted.

## See also

- [`docker_context`](./docker_context.md) — Docker engine counterpart.
- [`aws`](./aws.md) — AWS profile counterpart.
