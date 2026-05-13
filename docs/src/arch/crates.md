# Crate layout

The workspace splits along blast-radius lines: I/O-free libraries at
the bottom, side-effecting glue and binary entry at the top. The
dependency direction is one-way — `core` does not depend on `segments`
or `git`.

```
crates/
  p10k-rs            # binary entrypoint
  p10k-rs-core       # Segment trait, render pipeline (no I/O)
  p10k-rs-config     # TOML schema + Powerlevel9k import (data only)
  p10k-rs-segments   # segment implementations
  p10k-rs-git        # gitstatusd client + git shell-out fallback
  p10k-rs-shell      # per-shell init scripts
  p10k-rs-wizard     # `configure` TUI (stub)
  p10k-rs-ai         # AI-host detection / OSC emission (stub)
  p10k-rs-ipc        # FIFO plumbing
```

## `p10k-rs-core`

The render pipeline and shared types. **I/O-free** by design. Owns the
`Segment` trait (`name()`, `render()`, optional `enabled()` / `is_fast()`
gates), `RenderCtx` (per-prompt input bundle: config, shell, cwd, git,
jobs, duration, env snapshot), `SegmentOutput`, `Prompt`, and
`render_prompt()` — a pure function that walks segments, calls `enabled`
then `render`, joins with a space, and runs `wrap_for_shell`.
`safety::SafeText` and `style::*` live here too — they are the
attacker-controlled-input chokepoint and the SGR helpers segments
compose with. `Config` is re-exported from `p10k-rs-config` so
`RenderCtx` can name it without dependents pulling the config crate in.

## `p10k-rs-config`

The schema lives here. Loading, validation, defaulting, and the
Powerlevel9k importer (`p10k-rs import`) all live here, but the crate
is intentionally pure: no I/O for parsing — `Config::from_toml` takes a
string. `Config::load_default` is the only function in this crate that
touches the filesystem or environment, and it is opt-in.

## `p10k-rs-git`

The only crate in the workspace with an `unsafe` budget, and the
production hot path for the whole prompt. Read
[ADR-0001](https://github.com/scurran1986/powerlevel10k-rs/blob/main/docs/adr/0001-git-backend.md)
before changing anything here. Owns the `Backend` trait, the `ShellOut`
slow path (spawns `git status --porcelain=v1 --branch` per prompt), and
the `Gitstatusd` fast path (client of a long-lived daemon spawned by
the shell init script over two FIFOs). `GitState` itself lives in
`p10k-rs-core` so `RenderCtx` can hold an `Option<&GitState>` without a
dependency cycle.

## `p10k-rs-segments`

Thin assembly point. Per-segment logic lives in submodules; `lib.rs` is
the public registry. `segment_names()` returns the runtime-authoritative
list, `build(name)` constructs an instance, and a unit test enforces
that every advertised name resolves.

## `p10k-rs-shell`

Per-shell init scripts. `zsh` is fully wired today; `bash` and `fish`
ship init scripts whose installer wiring lands in a later slice. See
[Per-shell init](../reference/shell.md) for the feature parity table.

## `p10k-rs-ipc`

FIFO plumbing for the gitstatusd daemon. Pre-opened, mode-checked,
owner-checked before use — see [security](./security.md).

## `p10k-rs-wizard`, `p10k-rs-ai`

Stubs today. The wizard is the future `p10k-rs configure` TUI; the AI
crate covers host detection (`ai_host` segment) and OSC emission for
the `osc7` / `osc133` toggles in `[ai]`.

## `p10k-rs` (binary)

Argument parsing, config discovery (via `Config::load_default`), and
the `init` / `prompt` / `import` / `configure` subcommands. The
factory-default 5-segment layout lives in `main.rs::factory_default_config()`
and is byte-identical to the historical default.
