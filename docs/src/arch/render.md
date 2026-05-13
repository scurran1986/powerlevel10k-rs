# Render pipeline

`p10k-rs-core::render_prompt()` is the pure function that turns a
`RenderCtx` plus a list of segments into a `Prompt { left, right,
transient }`. It walks the configured segments in order, calls
`Segment::enabled()` to gate them, then `Segment::render()` for each
that survives, and joins the results with a separator before running
`wrap_for_shell()` for the active shell's prompt-escape rules.

The shape mirrors upstream Powerlevel10k's `_p9k_set_prompt`, but cut
to the bone: no in-band caching, no zselect-style multiplexer, no
`eval` of remote payloads. The MVP is synchronous — spawn-per-prompt —
because the load-bearing latency comes from one place: `gitstatusd`.
Anything that does not need to be async (status, prompt_char, dir,
time, context, vi_mode, root_indicator) finishes in microseconds inside
`render()` itself.

## Sync vs async

| Segment kind | Where computed |
|---|---|
| Static (`dir`, `prompt_char`, `status`, `time`, `context`, …) | Inline in `Segment::render()` during `render_prompt()`. |
| Cached vcs (gitstatusd answered inside the sync budget) | Sync drain of the daemon FIFO. See `p10k-rs-git::Gitstatusd`. |
| vcs fallback (no daemon, or daemon timeout) | `ShellOut` spawns `git status --porcelain=v1 --branch`. |
| Future async segments (battery, public_ip, disk_usage, …) | _TODO: confirm in the code — not in the MVP set._ |

The MVP is "spawn-per-prompt synchronous". `tokio` is explicitly out of
scope until v0.2; the daemon already amortises the only latency that
matters.

## Instant prompt

Slice 8 ships the instant-prompt cache: a dump file holds a rendered
PROMPT/RPROMPT from the previous shell session, so the first prompt
paints in sub-millisecond time before the real init finishes. The
hermetic-content constraint applies — only segments whose render does
not depend on env state or external processes are included in the
instant dump. `instant_prompt = "verbose" | "quiet" | "off"` in
`[<top-level>]` selects the behaviour.

## Reset-prompt

Upstream's `zle .reset-prompt` is shell-specific. The Rust trait
sketch in the planning notes proposes a `Shell::request_repaint()`
method per backend; `zsh` and `fish` can do mid-edit repaint cleanly,
`bash` flickers, and `nushell` requires polling. The MVP only refreshes
on the next prompt cycle for non-zsh shells.

## Further reading

- `~/.planning/powerlevel10k-rs/03-render-pipeline.md` — full upstream
  pipeline analysis the Rust port is modelled on.
- [ADR-0001](https://github.com/scurran1986/powerlevel10k-rs/blob/main/docs/adr/0001-git-backend.md)
  — why the git backend is the only place worth being async.
