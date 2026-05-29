# Per-shell init

Five init scripts ship in `crates/p10k-rs-shell/shells/`. `zsh` is the
fully-wired daily driver; `bash`, `fish`, `pwsh`, and `nu` ship working scripts
whose installer integration is shell-specific. The binary always
sanitises untrusted input before emission, regardless of which shell is
sourcing the prompt.

Source the right one with:

```bash
# zsh
eval "$(p10k-rs init zsh)"

# bash
eval "$(p10k-rs init bash)"

# fish
p10k-rs init fish | source
```

```powershell
# pwsh (PowerShell 7+ / 5.1)
& p10k-rs init pwsh | Invoke-Expression
```

```nu
# nu (Nushell 0.97+) — two-step save-then-source (auto-sourced)
mkdir ($nu.data-dir | path join vendor autoload)
p10k-rs init nu | save -f ($nu.data-dir | path join vendor autoload p10k-rs.nu)
```

All scripts are idempotent — re-sourcing is a no-op.

## Feature parity

| Feature | zsh | bash | fish | pwsh | nu |
|---|---|---|---|---|---|
| Left prompt | yes | yes | yes | yes | yes |
| Right prompt (`RPROMPT`) | yes | no (bash has no native equivalent) | not wired — fish supports a `fish_right_prompt` function but our `init.fish` doesn't define one yet | no — pwsh has no `RPROMPT` analogue; `[layout.right]` is silently dropped | yes — `$env.PROMPT_COMMAND_RIGHT` is first-class, so `[layout.right]` renders |
| `$?` / exit-code capture | yes | yes | yes | yes — `$LASTEXITCODE`; `$null` treated as 0 | yes — `$env.LAST_EXIT_CODE`; `$null` defaulted to 0 |
| `command_execution_time` | yes | no (no clean preexec; passes `--last-duration-ms 0`) | yes (uses `fish_preexec` event) | no — no clean preexec analogue; passes `--last-duration-ms 0` | yes — `$env.CMD_DURATION_MS` is native wall-clock duration |
| `gitstatusd` daemon backend | yes | no (FIFO plumbing is zsh-specific) | no (FIFO plumbing is zsh-specific) | no — falls back to `git` shell-out automatically | no — falls back to `git` shell-out / gitoxide automatically |
| `git` shell-out fallback | yes | yes | yes | yes | yes |
| Transient prompt | yes (via ZLE widgets) | no (readline has no comparable redraw hook) | wired — Enter-key bind sets `_p10k_rs_transient=1` then `commandline -f repaint` redraws (since `8ad919b`) | no — PSReadLine's `OnEnterKeyDown` hook exists but the redraw model differs from zsh's `zle reset-prompt` | no — `$env.TRANSIENT_PROMPT_COMMAND` exists; wiring the four modes is a later slice |
| Instant prompt | yes (cached PROMPT-SUBST dump) | no (does not map onto bash's prompt model) | no (does not map onto fish's prompt model) | no — does not map onto pwsh's prompt model | no — Nushell startup is fast; no dump-sourcing path |

## bash specifics

- Hooks via `PROMPT_COMMAND` (string form for bash 3.x–5.x compatibility).
- No native right prompt: any `[layout.right]` configured in TOML is
  silently dropped for bash users. A future slice may emulate
  right-alignment via cursor-position escapes; the trade-off (no
  resize handling, fragile under multiline input) is real.
- `command_execution_time`: bash has no clean preexec hook (a `trap
  DEBUG` workaround interacts badly with completion and subshells), so
  the segment stays below its 3-second threshold and hides.

## fish specifics

- Hooks via `fish_prompt` and a `fish_preexec` event handler.
- `command_execution_time` uses GNU coreutils `date +%s%3N` when
  available; falls back to whole-second precision on systems without
  `%3N` (macOS BSD `date`).
- No daemon backend or instant prompt — same reasons as bash.

## PowerShell (pwsh)

Both `pwsh` (PowerShell 7+, cross-platform) and `powershell` (Windows-only
5.x legacy) resolve to the same init script. It targets the 5.1 / 7+
intersection and feature-probes the rest at load time.

### Activation

One-liner for the current session:

```powershell
& p10k-rs init pwsh | Invoke-Expression
```

Persistent form for `$PROFILE`:

```powershell
Invoke-Expression (& p10k-rs init pwsh | Out-String)
```

The `Out-String` form is required in `$PROFILE` because PowerShell evaluates
profile lines differently from interactive pipeline expressions.

### Binary path

The script stores the absolute path of the `p10k-rs` binary that emitted it in
`$script:_P10K_RS_BIN`. If you move the binary after sourcing the script,
re-run `& p10k-rs init pwsh | Invoke-Expression` to rebind the path.

### What's reduced

- **`gitstatusd` daemon**: no FIFO IPC on Windows. The binary falls back to the
  `git` shell-out backend automatically — no configuration needed.
- **`command_execution_time`**: pwsh has no clean preexec analogue. The script
  passes `--last-duration-ms 0`, so the segment stays below its threshold and
  hides — same posture as bash.
- **Transient prompt**: PSReadLine exposes `OnEnterKeyDown` hooks, but the
  redraw model differs from zsh's `zle reset-prompt`. A pwsh-native transient
  implementation is not currently planned.
- **Right-side prompt**: pwsh has no `RPROMPT` equivalent. Any `[layout.right]`
  block in your TOML config is silently dropped, same as bash.
- **Instant prompt**: does not map onto pwsh's prompt model.

For the broader Windows OS feature-cfg-gate table (euid, ioctl probes, DECSET
support, etc.) see [docs/src/windows.md](../windows.md).

### Fallback prompt

On any binary failure the `prompt` function returns a minimal native prompt:

```
PS <cwd>>
```

This keeps the shell usable if `p10k-rs` is missing or returns an error. The
binary's stderr is suppressed (`2>$null`) so a broken render never replaces
the prompt with a stack trace.

## Nushell (nu)

Both `nu` and `nushell` resolve to the same init script. Nushell's parser is
static, so it cannot `source` a string straight off a pipe the way pwsh's
`Invoke-Expression` can — activation is a two-step "save then source".

### Activation

Nushell 0.97+ auto-sources scripts dropped into the vendor autoload directory.
Save the script there once:

```nu
mkdir ($nu.data-dir | path join vendor autoload)
p10k-rs init nu | save -f ($nu.data-dir | path join vendor autoload p10k-rs.nu)
```

Older Nushell has no autoload dir — save the script anywhere, then add a
`source` line to your `config.nu`:

```nu
p10k-rs init nu | save -f ~/.config/nushell/p10k-rs.nu
# then in config.nu:
#   source ~/.config/nushell/p10k-rs.nu
```

The script wires `$env.PROMPT_COMMAND` (and `_RIGHT`) to closures that ask
`p10k-rs prompt --shell nu` for a fresh ribbon on every reedline redraw.
Re-sourcing only reassigns the same closures, so it is an idempotent no-op.

### Binary path

The absolute path of the `p10k-rs` binary that emitted the script is baked in
at `p10k-rs init nu` time. If you move the binary after saving the script,
re-run the save step to rebind the path.

### What's reduced / what's gained

Unlike bash and pwsh, Nushell exposes two features that work here:

- **`command_execution_time`**: `$env.CMD_DURATION_MS` tracks wall-clock
  command duration natively, so this segment is live — no preexec workaround
  needed.
- **Right-side prompt**: Nushell has a first-class
  `$env.PROMPT_COMMAND_RIGHT`, so any `[layout.right]` block in your TOML
  config renders flush to the right edge.

The reductions match the bash/pwsh posture:

- **`gitstatusd` daemon**: no FIFO IPC wiring. The binary falls back to the
  `git` shell-out / gitoxide backend automatically — no configuration needed.
- **Transient prompt**: Nushell exposes `$env.TRANSIENT_PROMPT_COMMAND`, but
  wiring the four `TransientPromptMode` variants is a later slice. The full
  ribbon stays in scrollback.
- **Instant prompt**: no dump-sourcing path. Nushell startup is already fast,
  so the cold-cache mask (a zsh-specific optimisation) does not apply.

### Fallback prompt

On a non-zero exit from the binary the closures return a minimal native prompt:

```
<cwd>>
```

built from `$env.PWD` plus `"> "`. The binary's stdout, stderr, and exit code
are captured via `| complete`, so a transient stderr stack trace never
replaces the prompt.

## zsh specifics

- The only fully-wired shell. `install.sh` appends an
  `eval "$(p10k-rs init zsh)"` line to `~/.zshrc`.
- All four headline P10K features (instant, transient, show-on-command,
  daemon-backed git) target zsh first.
