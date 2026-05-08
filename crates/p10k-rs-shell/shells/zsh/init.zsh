# p10k-rs zsh init script — slice 1.
#
# Sourced into the user's interactive zsh via:
#   eval "$(p10k-rs init zsh)"
#
# Wires a precmd hook that asks `p10k-rs prompt --shell zsh` for a fresh
# prompt string before every prompt render. Re-sourcing is a no-op so users
# can `source` it more than once without doubling the hook.
#
# Limitations until later slices:
#  - Plain text only; no ANSI colors yet (those need %{...%} bracketing for
#    correct width tracking).
#  - No instant prompt; the binary always recomputes from scratch.
#  - No transient prompt collapse on accept-line.
#  - PROMPT_SUBST is left at the user's setting; output is captured at
#    assignment time, so `%` characters in cwd would be re-interpreted by
#    zsh. Slice 2 escapes them.

if [[ -n "${_P10K_RS_INSTALLED:-}" ]]; then
  return 0
fi
typeset -g _P10K_RS_INSTALLED=1

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init zsh` time via `__P10K_RS_BIN__` substitution; the binary
# resolves it from `std::env::current_exe()`. Re-run `eval "$(p10k-rs init
# zsh)"` if the binary moves.
typeset -g _P10K_RS_BIN='__P10K_RS_BIN__'

autoload -Uz add-zsh-hook

_p10k_rs_precmd() {
  local rs=$?
  PROMPT="$("$_P10K_RS_BIN" prompt --shell zsh 2>/dev/null) "
  return $rs
}

add-zsh-hook precmd _p10k_rs_precmd
