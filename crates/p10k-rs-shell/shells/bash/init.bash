# p10k-rs bash init script.
#
# Sourced into the user's interactive bash via:
#   eval "$(p10k-rs init bash)"
#
# Wires a PROMPT_COMMAND function that asks `p10k-rs prompt --shell bash`
# for a fresh PS1 before every prompt render. Re-sourcing is a no-op so
# users can `source` it more than once without doubling the hook.
#
# What works:
#  - $? capture for the `status` and `prompt_char` segments.
#  - All segments that don't rely on shell-side state (dir, vcs, etc.).
#  - Branch / cwd sanitisation: the binary always sanitises untrusted
#    input before emission, regardless of shell.
#
# What doesn't:
#  - Right-side prompt (`RPROMPT`): bash has no native equivalent. Only
#    the left ribbon is built; any `[layout.right]` configured in TOML
#    is silently dropped for bash users. A future slice may emulate
#    right-alignment via cursor-position escapes, but the trade-off
#    (no resize handling, fragile under multiline input) is real.
#  - `command_execution_time`: bash has no clean preexec hook (a
#    `trap DEBUG` workaround exists but interacts badly with completion
#    and subshells). We pass `--last-duration-ms 0`, so the segment
#    stays below its 3-second threshold and hides — same as today.
#  - `gitstatusd` daemon backend: the FIFO orchestration is zsh-specific
#    (uses zsh-only `exec {fd}<>` and `add-zsh-hook zshexit` for cleanup).
#    The binary falls back to the `git`-shell-out backend automatically.
#  - Transient prompt: zsh-only (uses ZLE widgets — `zle-line-finish` +
#    `zle reset-prompt`). Bash readline has no comparable redraw hook.
#  - Instant prompt: zsh's PROMPT-SUBST cached-dump trick doesn't map
#    onto bash's prompt rendering model; a bash-native equivalent lands
#    in a future slice if at all.

if [[ -n "${_P10K_RS_INSTALLED:-}" ]]; then
  return 0
fi
_P10K_RS_INSTALLED=1

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init bash` time via `__P10K_RS_BIN__` substitution; the binary
# resolves it from `std::env::current_exe()`. Re-run `eval "$(p10k-rs init
# bash)"` if the binary moves.
_P10K_RS_BIN='__P10K_RS_BIN__'

__p10k_rs_set_prompt() {
  # Capture $? first thing so nothing in this function clobbers it.
  local _P10K_RS_LAST_STATUS=$?
  PS1="$("$_P10K_RS_BIN" prompt --shell bash --render-side left --last-status $_P10K_RS_LAST_STATUS --last-duration-ms 0 2>/dev/null) "
  return $_P10K_RS_LAST_STATUS
}

# Prepend to PROMPT_COMMAND so we don't clobber any user-defined hook.
# Bash 5+ supports PROMPT_COMMAND as an array; we use the legacy string
# form which works in 3.x through 5.x. The idempotency guard at the top
# of the file prevents duplicate appends on re-source.
if [[ -n "${PROMPT_COMMAND:-}" ]]; then
  PROMPT_COMMAND="__p10k_rs_set_prompt;${PROMPT_COMMAND}"
else
  PROMPT_COMMAND="__p10k_rs_set_prompt"
fi
