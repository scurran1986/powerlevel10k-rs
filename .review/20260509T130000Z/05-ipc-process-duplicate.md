# Lane 5: IPC + process (duplicate fanout) — extracted from rate-limited transcript

**Source:** `/tmp/claude-1000/-home-seaburdz-github-powerlevel10k-rs/794df3aa-6985-44d2-aa7e-e6c8fbbda924/tasks/a72c6c249d3a5ca13.output`
**Lines / size:** 51 lines / 150 KB
**Status when limit hit:** Limit hit at L51 after a series of permission denials. The agent was given the same prompt as lane 1 (IPC + process surface, with the same nine attack questions and the same target file `01-ipc-process.md`). This is the **earlier-spawned IPC agent** (mtime 01:28) before the orchestrator relaunched IPC as the write-capable lane 1 (`af3c4dc9`, mtime 01:31). It was a read-only agent type; the prompt asked it to write findings, which it could not do.

**Tool budget burn:** 8 Reads (file load) → 1 ToolSearch → 6 Bash calls (all denied) → 5 ast-grep calls (all denied). Limit hit before any analysis, drafting, or write attempt. The agent never produced a single finding statement.

## Why this lane exists

The orchestrator's `RESUME-TOMORROW.md` says only four agents fanned out, but the filesystem shows five `tasks/*.output` symlinks. The fifth (this one, `a72c6c249d3a5ca13`, mtime 01:28) is the **first** IPC agent — spawned read-only, hit permission denials immediately on its `cargo audit` and `grep -rn` calls because of sandbox policy on read-only-typed agents, and was effectively dead by 01:30. Lane 1 (`af3c4dc9dd44335cf`, mtime 01:31) was the orchestrator's relaunch with a write-capable general-purpose type. Lane 1 didn't get further than reading files either before its rate limit fired.

So both IPC lanes (this one + lane 1) effectively produced zero analysis. The IPC investigation in this audit cycle is **entirely empty**.

## Findings

None. The agent never reached an analysis step. The Bash and ast-grep tools were sandbox-denied; the agent attempted to fall back to additional tool variants and burned its budget on retries.

## Investigation in flight (incomplete)

Same nine questions as lane 1 (TOCTOU, UID-check edge cases, init.zsh walk, fd inheritance, signal/cleanup, `$PATH` trust, daemon timeout, FIFO cross-talk, instant-prompt dump) — none reached.

## Confidence + caveats

This file exists for completeness — to document that **the IPC lane was attempted twice and produced nothing both times.** That is itself the actionable finding for tomorrow's resume:

1. The IPC lane is the *most important* lane to re-launch tomorrow.
2. The orchestrator's pre-fanout IPC findings in `RESUME-TOMORROW.md` (FIFO byte-injection, no req/resp correlation, install.sh:126 dev-machine path, non-UTF-8 silent emptying, no version/checksum) are the only IPC substance the audit currently has.
3. Re-launch IPC as `general-purpose` (write-capable), with the lane-1 prompt verbatim, in a session that does not block Bash. The `cargo audit` denial is independent of agent type — it is a host sandbox policy that needs adjustment or the audit needs a manual `cargo install --locked cargo-audit` step before the agent runs.

## Naming note

This file is named `05-ipc-process-duplicate.md` per orchestrator request to identify lane 5 by its content. The matching planned target file `01-ipc-process.md` was never written by either IPC agent.
