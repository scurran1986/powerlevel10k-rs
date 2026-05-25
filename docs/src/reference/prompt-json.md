# `prompt --json` payload

`p10k-rs prompt --json` emits a machine-readable snapshot of the
current prompt render. Use it when you need the prompt's state without
parsing ANSI text — for example:

- **AI hosts** (Claude Code, Cursor, Aider, Goose) that want to surface
  branch / dirty / cwd state alongside their own UI without screen-
  scraping the styled prompt.
- **Integration partners** building IDE plugins or status-bar widgets
  that consume the same per-segment information the shell sees.
- **Debugging** what the binary is actually deciding to render — every
  enabled segment shows up with its name, state, icon, background, and
  both styled + plain text.

The wire shape is governed by JSON Schema
[`docs/schema/p10krs.prompt.v1.json`](https://github.com/scurran1986/powerlevel10k-rs/blob/main/docs/schema/p10krs.prompt.v1.json).
Schema version is encoded in every payload as `schema_version:
"p10krs.prompt/v1"` — a future breaking change ships behind a new
version while keeping `v1` valid for at least one minor cycle.

## Quick look

```bash
$ p10k-rs prompt --shell zsh --json --last-status 0
{
  "schema_version": "p10krs.prompt/v1",
  "produced_at_unix": 1716609600,
  "p10k_rs_version": "0.2.2",
  "shell": "zsh",
  "exit_status": 0,
  "host": "claude-code",
  "cwd": "/home/seaburdz/github/powerlevel10k-rs",
  "ansi_text": "…full styled prompt with ANSI escapes…",
  "plain_text": " seaburdz@host  ~/github/powerlevel10k-rs  main ❯",
  "left": [
    {
      "name": "dir",
      "ansi": "[48;…m /path[39m[49m",
      "plain": " /path",
      "state": "writable",
      "icon": "",
      "background": "blue"
    }
  ],
  "right": [],
  "git": {
    "branch": "main",
    "ahead": 0,
    "behind": 0,
    "staged": 0,
    "unstaged": 3,
    "untracked": 1,
    "has_conflicts": false,
    "action": null
  }
}
```

The JSON is pretty-printed with a trailing newline. Pipe through `jq`
for ad-hoc queries; the shape is stable.

## Fields

| Field | Type | Notes |
|---|---|---|
| `schema_version` | string (const) | Always `"p10krs.prompt/v1"`. |
| `produced_at_unix` | integer | Unix seconds at payload assembly. |
| `p10k_rs_version` | string | Binary version (matches `version --json`). |
| `shell` | enum string | `zsh` \| `bash` \| `fish`. |
| `exit_status` | integer | What the shell forwarded via `--last-status`. |
| `host` | string \| null | Detected AI host short label, or `null`. |
| `cwd` | string | Current working directory. |
| `ansi_text` | string | Styled prompt for the requested side, escapes included. |
| `plain_text` | string | Same with ANSI / zsh markers stripped. |
| `left` | array of `Segment` | Per-segment breakdown for `layout.left`. |
| `right` | array of `Segment` | Per-segment breakdown for `layout.right`. |
| `git` | `Git` \| null | Git state for cwd, or `null` outside a repo. |

### `Segment`

| Field | Type | Notes |
|---|---|---|
| `name` | string | Segment id (matches the TOML config key). |
| `ansi` | string | Styled segment output, no surrounding separators. |
| `plain` | string | Plain-text view of `ansi`. |
| `state` | string \| null | Segment state tag (`ok`, `error`, `writable`, …). |
| `icon` | string \| null | Icon glyph the segment chose. |
| `background` | `Color` \| null | Background colour (named / hex / index / `[r,g,b]`). |

Foreground is not surfaced per-segment today: segments render their
own foreground inline as part of `ansi`, and the renderer doesn't
plumb the resolved colour back out independently. If you need the
exact colour, parse the SGR escape inside `ansi`. A future schema
version may promote it to a first-class field.

### `Git`

Mirrors `p10k_rs_core::GitState` minus the rarely-consumed
`commit` / `tag` / `stash` slots. All counts default to 0 when the
backend can't surface them (the `ShellOut` fallback, for example,
only populates `branch` + dirty signals).

| Field | Type | Notes |
|---|---|---|
| `branch` | string | Current branch (empty when detached / unknown). |
| `ahead` | integer | Commits ahead of upstream. |
| `behind` | integer | Commits behind upstream. |
| `staged` | integer | Staged changes (tree-vs-index). |
| `unstaged` | integer | Unstaged changes (index-vs-worktree). |
| `untracked` | integer | Untracked files. |
| `has_conflicts` | boolean | Unmerged-stage paths present. |
| `action` | string \| null | `merge` / `rebase` / `cherry-pick` / `revert` / `bisect`. |

## What's *not* in the payload

- **Transient prompt.** `--json` always emits the full left / right
  ribbons; the collapsed transient view is a styled-text-only concept
  that doesn't map cleanly onto a structured snapshot.
- **Instant-prompt dump.** `--json` skips the on-disk dump (`--dump`)
  even when both are passed. The dump exists to mask first-prompt
  gitstatusd latency, which doesn't apply to a one-shot diagnostic
  invocation.
- **Foreground colour per segment.** See the `Segment` table above.

## Stability

The `v1` shape is wire-stable: a payload that validates today will
continue to validate after a `p10k-rs` minor bump. Additive changes
(new optional fields) ship within `v1`; breaking changes ship a new
schema version (`v2`) while keeping `v1` valid for at least one
minor release cycle.
