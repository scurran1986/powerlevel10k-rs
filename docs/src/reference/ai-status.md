# ai_status sidecar contract

The `ai_status` segment surfaces the currently-active AI model and
context-window usage in the prompt. It reads a small JSON file the AI
host (or a user-written hook) drops in a uid-private runtime
directory; no daemon, no network.

## Where the file lives

```
$XDG_RUNTIME_DIR/p10k-rs/ai/<host>.json
```

`<host>` is the kebab-case slug the `ai_host` segment renders:
`claude-code`, `aider`, `cursor`, `goose`.

`$XDG_RUNTIME_DIR` is the systemd-managed uid-private directory
(`/run/user/<uid>` on Linux). When the variable is unset the segment
renders empty — there is no fallback to `/tmp` because that path is
shared and would weaken the load-bearing "writer == reader uid"
assumption. Users on systems without systemd can export
`XDG_RUNTIME_DIR=/run/user/$(id -u)` (creating it `0o700`) at session
start to participate.

The directory tree and the file are presumed owner-only; the segment
opens the file through `open_owned_safely`, which rejects symlinks
(`O_NOFOLLOW`), non-regular files, and any inode whose owner uid is
not the current effective uid (or root).

## JSON schema

```json
{
  "schema_version": 1,
  "host": "claude-code",
  "model": "claude-sonnet-4-6",
  "status": "idle",
  "context_used_tokens": 12345,
  "context_window_tokens": 200000,
  "last_updated_unix": 1716609600
}
```

| Field                   | Type           | Required | Notes                                                                                       |
|-------------------------|----------------|----------|---------------------------------------------------------------------------------------------|
| `schema_version`        | `u32`          | yes      | Currently `1`. Reserved for future migration.                                               |
| `host`                  | `string`       | yes      | Self-identifier. Sanitised before any use.                                                  |
| `model`                 | `string`       | no       | Active model name. Capped at 64 graphemes for the prompt render.                            |
| `status`                | `string`       | no       | Free-form: `"idle"`, `"thinking"`, `"tool_use"`, `"error"`, …. Capped at 32 graphemes.      |
| `context_used_tokens`   | `u32`          | no       | Tokens used in the active context window.                                                   |
| `context_window_tokens` | `u32`          | no       | Total token budget. Combined with `context_used_tokens` to render `<pct>%`.                 |
| `last_updated_unix`     | `u64`          | no       | Seconds since the Unix epoch. Absent or older than 300 s → segment renders empty (stale).   |

Unknown fields are ignored. Writers MAY add keys without breaking
older readers; readers MUST NOT trip on unrecognised fields.

## Render shape

When the file is fresh and parses cleanly, the segment renders:

```
<icon> <model> <pct>% [<status>]
```

Sub-tokens are omitted when their backing fields are absent. The
icon, foreground, and background follow the standard
`[segment.ai_status]` config block (see
[Schema](./schema.md)). Defaults: magenta background, white foreground,
robot-face Nerd Font glyph.

## Limits and failure modes

- The file is read once per prompt invocation. Reads are capped at
  **16 KiB**; anything larger is refused (segment renders empty).
- Bad JSON, missing file, oversize file, foreign-owned file, or symlink
  → segment renders empty without panicking and without polluting
  stderr.
- Stale (no timestamp, or `last_updated_unix` more than **300 s** before
  `now`) → segment renders empty. A clock that briefly jumps backward
  causing a "future" timestamp is treated as fresh, not stale.
- The segment only attempts to read the sidecar when an AI host is
  detected from environment variables (`ai_host`'s detector); a shell
  outside an AI session pays nothing.

## What hosts can do to write it

The canonical writer is the AI host itself. Pattern (pseudocode):

```python
import json, os, pathlib, tempfile, time

runtime = pathlib.Path(os.environ["XDG_RUNTIME_DIR"]) / "p10k-rs" / "ai"
runtime.mkdir(parents=True, exist_ok=True, mode=0o700)

payload = {
    "schema_version": 1,
    "host": "claude-code",
    "model": current_model_name(),
    "status": current_status(),
    "context_used_tokens": session.tokens_used,
    "context_window_tokens": session.window_size,
    "last_updated_unix": int(time.time()),
}

dest = runtime / "claude-code.json"
# Atomic replace: write to a temp file, fsync, then rename.
fd, tmp = tempfile.mkstemp(dir=runtime, prefix=".claude-code.", suffix=".tmp")
with os.fdopen(fd, "w") as f:
    json.dump(payload, f)
    f.flush()
    os.fsync(f.fileno())
os.replace(tmp, dest)
os.chmod(dest, 0o600)
```

Always write through a temp file + `rename(2)` so the reader never sees
a half-written payload. The reader's 16 KiB cap is enforced both before
and during the read, so a runaway writer can't blow up the prompt path
even if it loses the atomic-replace pattern.

## Writing a custom hook

The same shape works for users wiring their own hooks. For Claude Code,
a `Stop` and `ToolUse` hook (defined in `.claude/settings.json`) can
shell out to a tiny `bash` / `python` script that emits the JSON
above. Example minimal Claude Code hook config:

```json
{
  "hooks": {
    "Stop": [
      {"command": "$HOME/.local/bin/p10k-rs-ai-status-write"}
    ],
    "ToolUse": [
      {"command": "$HOME/.local/bin/p10k-rs-ai-status-write"}
    ]
  }
}
```

…with `~/.local/bin/p10k-rs-ai-status-write` doing the temp-file +
`rename(2)` dance above. The segment renders the latest snapshot
whenever the prompt redraws.

## See also

- [`ai_host`](../segments/index.md#ai_host) — the env-var-driven badge
  that pairs with this segment.
- [`PRIVACY.md`](https://github.com/scurran1986/powerlevel10k-rs/blob/main/PRIVACY.md)
  — what the binary writes to disk, including this sidecar path.
- [Configuration schema](./schema.md) — the `[segment.ai_status]`
  block (foreground / background / icon / padding).
