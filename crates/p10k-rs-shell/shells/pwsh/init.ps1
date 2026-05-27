# p10k-rs PowerShell init script.
#
# Sourced into the user's interactive pwsh via:
#   & p10k-rs init pwsh | Invoke-Expression
#   # or in $PROFILE:
#   #   Invoke-Expression (& p10k-rs init pwsh | Out-String)
#
# Wires a `prompt` function that asks `p10k-rs prompt --shell pwsh` for a
# fresh prompt string on every PowerShell redraw.  Re-sourcing is a no-op
# (the `$env:_P10K_RS_INSTALLED` flag flips on first source and stays
# set for the session).
#
# What works:
#  - $LASTEXITCODE capture for the `status` and `prompt_char` segments.
#  - All segments that don't need shell-side state (dir, vcs via
#    ShellOut, time, etc.).
#  - ANSI escapes flow through unchanged. Modern Windows Terminal /
#    pwsh 7+ honour them; legacy conhost on Win10 1809+ also handles
#    SGR + cursor moves once VT processing is enabled (which pwsh
#    enables by default for PSReadLine sessions).
#
# What doesn't (Windows-side reductions; mirror docs/src/windows.md):
#  - `gitstatusd` daemon backend: no FIFO IPC on Windows; the binary
#    falls back to the `git`-shell-out backend automatically.
#  - `command_execution_time`: pwsh has no clean preexec analogue.
#    We pass `--last-duration-ms 0`, so the segment hides — same as
#    bash's posture.
#  - Transient prompt: PSReadLine has hooks (e.g. `OnEnterKeyDown`)
#    but the redraw model differs from zsh's `zle reset-prompt`.
#    A pwsh-native transient slice lands later if at all.
#  - Right-side prompt: pwsh doesn't have a `RPROMPT` analogue; the
#    binary's `[layout.right]` is silently dropped, same as bash.

if ($env:_P10K_RS_INSTALLED) { return }
$env:_P10K_RS_INSTALLED = '1'

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init pwsh` time via __P10K_RS_BIN__ substitution.
$script:_P10K_RS_BIN = '__P10K_RS_BIN__'

function global:prompt {
    # Capture status FIRST so nothing in this function clobbers it. pwsh
    # doesn't surface `$?` as an exit code the same way bash does;
    # `$LASTEXITCODE` is the native-process exit and is `$null` when the
    # last statement was a pure cmdlet/pipeline.  Treat `$null` as 0.
    $status = $global:LASTEXITCODE
    if ($null -eq $status) { $status = 0 }

    # Call the binary. `2>$null` swallows the binary's stderr so a
    # transient error there never replaces the user's prompt with a
    # stack trace. Failure path below falls through to a minimal native
    # prompt rather than dying.
    $rendered = & $script:_P10K_RS_BIN prompt `
        --shell pwsh `
        --last-status $status `
        --last-duration-ms 0 2>$null

    # pwsh captures stdout into a string array (one element per line).
    # Join with "`n" (LF) to reconstruct what the binary emitted —
    # using "`r`n" would double-up the line endings on Windows.
    if ($LASTEXITCODE -eq 0 -and $rendered) {
        return ($rendered -join "`n")
    }

    # Fallback: minimal native pwsh-style prompt. Visible signal that
    # something went sideways without crashing the shell.
    return "PS $($executionContext.SessionState.Path.CurrentLocation)> "
}
