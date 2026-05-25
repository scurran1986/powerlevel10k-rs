# daemon-health subcommand

Diagnostic for the per-shell `gitstatusd` daemon-respawn channel (slice 64).
Reports whether the daemon is healthy, wedged, dead, or channel not wired.

## When to run

Run `p10k-rs daemon-health` when:
- The prompt feels slow and you suspect daemon failure
- You want to confirm the daemon is alive before diving into other diagnostics
- You're writing a shell script that branches on daemon state

## Outcomes

The subcommand prints one stable line and exits with a code. Parse the exit code
in scripts; the stdout text is for manual inspection.

| Outcome | Stdout | Exit |
|---------|--------|------|
| Healthy | `OK pid=<pid> wedge=none` | 0 |
| Wedged | `WEDGED pid=<pid> wedge_age_ms=<n>` | 2 |
| Daemon dead | `DEAD pid=<pid>` | 3 |
| Channel not wired | `NOT_WIRED` | 4 |
| I/O error | `ERROR <reason>` | 5 |

## What "wedged" means

A *wedge sentinel* is a file touched by the daemon when it detects it cannot
make progress on a git status request. The `precmd` health-check hook (phase 3)
monitors this sentinel and respawns the daemon if it's stale.

`wedge_age_ms` tells you how old the sentinel is. A fresh wedge (age close to 0)
means the daemon just deadlocked; an old wedge (age > 30s) suggests the respawn
precmd hasn't fired yet. See ADR-0001 § Follow-ups for the full daemon-respawn
architecture.

## What `NOT_WIRED` means

The env vars `_P10K_RS_GITSTATUSD_PID_FILE` and `_P10K_RS_GITSTATUSD_WEDGE` are
exported by `p10k-rs init zsh` when you source it into an interactive zsh shell.
They point to the per-shell daemon's PID file and wedge sentinel.

Running `daemon-health` outside an interactive zsh that sourced the init
(e.g., from a cron job, CI script, or non-zsh shell) will always print
`NOT_WIRED`. This is correct — the daemon-respawn channel doesn't exist in
those contexts.

## Example: shell-script branching

```sh
if ! p10k-rs daemon-health >/dev/null; then
  echo "p10k-rs daemon needs attention:"
  p10k-rs daemon-health
fi
```

Check the exit code to decide whether to restart the shell session or investigate
further. Exit 2 (wedged) or 3 (dead) indicate the health-check hook should have
respawned the daemon on the next prompt; if you see this repeatedly, file an issue.

## `--json` for machine-readable output

Pass `--json` to emit a single JSON object on stdout instead of the one-line text
form. Same exit codes; same outcomes. Useful when scripting from non-shell
consumers (Python, Go, a Prometheus textfile collector, monitoring tooling).

| Outcome | JSON |
|---------|------|
| Healthy | `{"status":"OK","pid":<n>,"wedge":null}` |
| Wedged | `{"status":"WEDGED","pid":<n>,"wedge_age_ms":<n>}` |
| Daemon dead | `{"status":"DEAD","pid":<n>}` |
| Channel not wired | `{"status":"NOT_WIRED"}` |
| I/O error | `{"status":"ERROR","reason":"<escaped>"}` |

`reason` strings are JSON-escaped (`"`, `\`, and C0 controls handled
explicitly; non-ASCII UTF-8 passes through unchanged). The shape is part of
the public contract — additive field changes only in future releases; existing
fields keep their names and types.

```sh
# branch on a structured field rather than parse the text form
status=$(p10k-rs daemon-health --json | jq -r .status)
case "$status" in
  OK)        ;;
  WEDGED)    echo "daemon wedged; precmd hook should respawn next prompt" ;;
  DEAD)      echo "daemon dead; respawn pending" ;;
  NOT_WIRED) echo "init zsh not sourced or env vars missing" ;;
  ERROR)     echo "I/O error reading channel state" ;;
esac
```
