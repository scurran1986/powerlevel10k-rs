# p10k-rs bash init script.
#
# Sourced into the user's interactive bash via:
#   eval "$(p10k-rs init bash)"
#
# Wires a PROMPT_COMMAND hook that asks `p10k-rs prompt --shell bash` for
# a fresh prompt string before every prompt render. Re-sourcing is a no-op.
#
# What works:
#  - $? capture for the `status` and `prompt_char` segments.
#  - All segments that don't rely on shell-side state (dir, vcs, etc.).
#  - Branch / cwd sanitisation (the binary always sanitises; shell-side
#    bracketing for prompt-width tracking is zsh-specific and not relevant
#    here — bash uses `\[…\]` for the same purpose, which the binary does
#    not yet emit; visible-width may be slightly off in deeply-styled prompts).
#
# What doesn't:
#  - `command_execution_time`: bash has no clean preexec hook (a `trap DEBUG`
#    workaround exists but interacts badly with completion). We pass
#    `--last-duration-ms 0` so the segment stays below threshold and hides.
#  - `gitstatusd` daemon backend: the FIFO orchestration is zsh-specific
#    (uses zsh-only `exec {fd}<>` and `add-zsh-hook zshexit` for cleanup).
#    The binary falls back to the `git`-shell-out backend automatically.
#  - Instant prompt: the cached-dump trick relies on zsh's PROMPT-SUBST
#    semantics; bash equivalent lands in a future slice.

if [[ -n "${_P10K_RS_INSTALLED:-}" ]]; then
  return 0
fi
_P10K_RS_INSTALLED=1

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init bash` time via `__P10K_RS_BIN__` substitution.
_P10K_RS_BIN='__P10K_RS_BIN__'

__p10krs_setup_prompt() {
  local rs=$?
  PS1="$("$_P10K_RS_BIN" prompt --shell bash --last-status $rs --last-duration-ms 0 2>/dev/null) "
  return $rs
}

# Prepend to PROMPT_COMMAND so we don't clobber any user-defined hook.
# Bash 5+ supports PROMPT_COMMAND as an array; we use the legacy string
# form which works in 3.x through 5.x. Idempotency guard above prevents
# duplicate appends on re-source.
if [[ -n "${PROMPT_COMMAND:-}" ]]; then
  PROMPT_COMMAND="__p10krs_setup_prompt;${PROMPT_COMMAND}"
else
  PROMPT_COMMAND="__p10krs_setup_prompt"
fi
