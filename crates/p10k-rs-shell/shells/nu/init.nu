# p10k-rs Nushell init script.
#
# Nushell's parser is static, so it can't `source` a string straight off
# a pipe the way pwsh's `Invoke-Expression` can. The bootstrap is a
# two-step "save then source":
#
#   # Nushell 0.97+ (auto-sourced from the vendor autoload dir):
#   mkdir ($nu.data-dir | path join vendor autoload)
#   p10k-rs init nu | save -f ($nu.data-dir | path join vendor autoload p10k-rs.nu)
#
#   # Older Nushell — save anywhere, then add to your config.nu:
#   p10k-rs init nu | save -f ~/.config/nushell/p10k-rs.nu
#   #   source ~/.config/nushell/p10k-rs.nu
#
# Wires `$env.PROMPT_COMMAND` (and `_RIGHT`) to closures that ask
# `p10k-rs prompt --shell nu` for a fresh ribbon on every reedline
# redraw. Re-sourcing only reassigns the same closures, so it is an
# idempotent no-op; `$env._P10K_RS_INSTALLED` is set as a marker but no
# early-return guard is needed (and a sourced Nushell script can't
# `return` from its top level anyway).
#
# What works:
#  - $env.LAST_EXIT_CODE drives the `status` / `prompt_char` segments.
#  - $env.CMD_DURATION_MS feeds `command_execution_time` — Nushell
#    tracks wall-clock command duration natively (unlike bash/pwsh, which
#    have no clean preexec analogue), so this segment is live.
#  - Right-side prompt: Nushell has a first-class
#    $env.PROMPT_COMMAND_RIGHT, so `[layout.right]` renders here (unlike
#    bash/pwsh, which silently drop it).
#  - ANSI escapes flow through unchanged; reedline tracks prompt width
#    natively, so no zsh-style `%{...%}` zero-width markers are emitted
#    (`--shell nu` routes through the non-zsh pass-through in
#    `wrap_for_shell`).
#
# What doesn't (documented reductions; mirror the bash/pwsh posture and
# docs/src/reference/shell.md):
#  - gitstatusd daemon backend: no FIFO IPC wiring here; the binary falls
#    back to the `git` shell-out / gitoxide backend automatically.
#  - Transient prompt: Nushell exposes $env.TRANSIENT_PROMPT_COMMAND, but
#    wiring the four TransientPromptMode variants (Off / Always / SameDir
#    / UniqueDir) is a later slice. The full ribbon stays in scrollback.
#  - Instant prompt: no dump-sourcing path. Nushell startup is already
#    fast; the cold-cache mask is a zsh-specific optimisation.

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init nu` time via the __P10K_RS_BIN__ substitution. Single
# quotes keep it a literal — `cmd_init` rejects paths containing a single
# quote, control bytes, or DEL before substituting.
$env._P10K_RS_INSTALLED = "1"

# Left ribbon — PROMPT_COMMAND. reedline calls this closure on every
# redraw and uses its string return verbatim as the prompt (the binary
# emits no trailing newline, so we return stdout unchanged — the trailing
# space is part of the ribbon). `| complete` captures stdout, stderr, and
# the exit code without letting a transient stderr stack trace replace the
# prompt; on a non-zero exit we fall back to a minimal native prompt.
$env.PROMPT_COMMAND = {||
    let status = ($env.LAST_EXIT_CODE? | default 0)
    let dur = ($env.CMD_DURATION_MS? | default "0")
    let out = (
        ^'__P10K_RS_BIN__' prompt --shell nu --render-side left --last-status $status --last-duration-ms $dur | complete
    )
    if $out.exit_code == 0 { $out.stdout } else { $"($env.PWD)> " }
}

# Right ribbon — PROMPT_COMMAND_RIGHT. reedline renders this flush to the
# right edge of the prompt line. Empty `[layout.right]` (the default)
# yields empty stdout, so no right prompt shows.
$env.PROMPT_COMMAND_RIGHT = {||
    let status = ($env.LAST_EXIT_CODE? | default 0)
    let dur = ($env.CMD_DURATION_MS? | default "0")
    let out = (
        ^'__P10K_RS_BIN__' prompt --shell nu --render-side right --last-status $status --last-duration-ms $dur | complete
    )
    if $out.exit_code == 0 { $out.stdout } else { "" }
}

# The binary renders the whole ribbon, including the prompt char (`❯`).
# Blank Nushell's own indicators so they don't double up after ours.
$env.PROMPT_INDICATOR = ""
$env.PROMPT_INDICATOR_VI_INSERT = ""
$env.PROMPT_INDICATOR_VI_NORMAL = ""
$env.PROMPT_MULTILINE_INDICATOR = "... "
