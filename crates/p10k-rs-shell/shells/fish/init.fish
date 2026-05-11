# p10k-rs fish init script.
#
# Sourced into the user's interactive fish via:
#   p10k-rs init fish | source
#
# Wires `fish_prompt` and a `fish_preexec` event handler to ask
# `p10k-rs prompt --shell fish` for a fresh prompt before every render.
# Re-sourcing is a no-op.
#
# What works:
#  - $status capture for the `status` and `prompt_char` segments.
#  - `command_execution_time`: fish has a clean preexec event, so we can
#    measure the foreground command duration in millis.
#  - All segments that don't rely on shell-side state (dir, vcs, etc.).
#
# What doesn't:
#  - `gitstatusd` daemon backend: the FIFO orchestration is zsh-specific.
#    The binary falls back to the `git`-shell-out backend automatically.
#  - Instant prompt: zsh's PROMPT-SUBST cached-dump trick doesn't map onto
#    fish's prompt rendering model; a fish-native equivalent lands later.

if set -q _P10K_RS_INSTALLED
    exit 0
end
set -g _P10K_RS_INSTALLED 1

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init fish` time via `__P10K_RS_BIN__` substitution.
set -g _P10K_RS_BIN '__P10K_RS_BIN__'

# Wall-clock millis at the start of the current foreground command. Set
# in `fish_preexec`, consumed and cleared in `fish_prompt`. Unset means
# "no command since last prompt" — covers the very first prompt.
set -e _p10k_rs_cmd_start_ms 2>/dev/null

function _p10k_rs_preexec --on-event fish_preexec
    # `date +%s%3N` is GNU coreutils; fish itself doesn't expose epoch
    # millis. Fall back to `date +%s000` for systems without `%3N`
    # support (macOS BSD date) — loses millisecond precision but the
    # command_execution_time segment only cares about whole seconds
    # past its 3-second threshold anyway.
    if date +%s%3N >/dev/null 2>&1
        set -g _p10k_rs_cmd_start_ms (date +%s%3N)
    else
        set -g _p10k_rs_cmd_start_ms (math (date +%s) x 1000)
    end
end

function fish_prompt
    set -l rs $status
    set -l elapsed_ms 0
    if set -q _p10k_rs_cmd_start_ms
        if date +%s%3N >/dev/null 2>&1
            set -l now (date +%s%3N)
            set elapsed_ms (math $now - $_p10k_rs_cmd_start_ms)
        else
            set -l now (math (date +%s) x 1000)
            set elapsed_ms (math $now - $_p10k_rs_cmd_start_ms)
        end
        set -e _p10k_rs_cmd_start_ms
    end
    "$_P10K_RS_BIN" prompt --shell fish --last-status $rs --last-duration-ms $elapsed_ms 2>/dev/null
    echo -n ' '
end
