# p10k-rs zsh init script.
#
# Sourced into the user's interactive zsh via:
#   eval "$(p10k-rs init zsh)"
#
# Wires a precmd hook that asks `p10k-rs prompt --shell zsh` for a fresh
# prompt string before every prompt render. Re-sourcing is a no-op so users
# can `source` it more than once without doubling the hook.
#
# Render-path safety:
#  - The binary's `wrap_for_shell` pass doubles literal `%` to `%%` in text
#    content so an attacker-controlled branch name or cwd can't trigger zsh
#    PROMPT-expansion (`%n`, `%m`, `%/`, `$(…)` under PROMPT_SUBST). SGR
#    escapes from segments are wrapped in `%{ }` for correct width tracking.
#  - `sanitize_for_terminal` strips control bytes (CR, ESC, BEL, …) at every
#    untrusted-input boundary so OSC/CSI/DCS sequences can't ride a branch
#    name or directory path into the terminal's state machine.
#
# Transient prompt (slice 35):
#   On `zle-line-finish` (fires after the user accepts a line and
#   before the command runs) we ask the binary for a collapsed PROMPT
#   and `zle reset-prompt` so the scrollback shows a clean history of
#   `❯ command` lines instead of the full multi-line prompt. The
#   binary returns an empty string when `transient_prompt = off`, so
#   the swap is a cheap no-op in that case; opting in is purely a TOML
#   change with no zsh-side toggle.
#
# Right-side prompt (`RPROMPT`) is wired below: the precmd hook invokes
# `p10k-rs prompt --render-side right` and assigns the output to
# `RPROMPT`. Empty output (no `[layout.right]` configured, or every
# segment hidden) yields `RPROMPT=""`, which zsh treats as no right
# prompt — i.e. the historical behaviour is preserved when users don't
# opt in.

if [[ -n "${_P10K_RS_INSTALLED:-}" ]]; then
  return 0
fi
typeset -g _P10K_RS_INSTALLED=1

# Absolute path to the binary that emitted this script. Injected at
# `p10k-rs init zsh` time via `__P10K_RS_BIN__` substitution; the binary
# resolves it from `std::env::current_exe()`. Re-run `eval "$(p10k-rs init
# zsh)"` if the binary moves.
typeset -g _P10K_RS_BIN='__P10K_RS_BIN__'

# Absolute path to the `gitstatusd` daemon binary, injected the same way.
# Empty if `p10k-rs init` couldn't locate one — the prompt then falls
# back to the slow `git`-shell-out backend automatically.
typeset -g _P10K_RS_GITSTATUSD_BIN='__P10K_RS_GITSTATUSD_BIN__'

# Slice 8 — instant prompt cache.
#
# Path to the dump file the binary writes after every render. Sourced
# below before any heavy init (zmodload, daemon spawn, hook setup) so
# PROMPT is set immediately on shell startup; user can type before the
# rest of init finishes. The real precmd then overwrites PROMPT with a
# fresh render at the first prompt.
#
# Per-user: `${XDG_CACHE_HOME:-$HOME/.cache}/p10k-rs/dump-<user>.zsh`.
# A stale cache shows the previous shell session's last cwd until the
# first precmd fires — acceptable trade for masking gitstatusd's
# ~2 s first-call cost on kernel-class repos.
typeset -g _p10k_rs_dump="${XDG_CACHE_HOME:-$HOME/.cache}/p10k-rs/dump-${USER:-${USERNAME:-default}}.zsh"
[[ -r $_p10k_rs_dump ]] && source $_p10k_rs_dump 2>/dev/null

# `zsh/datetime` exposes `$EPOCHSECONDS` for command-time tracking. The
# bare `zmodload zsh/datetime` form is the one that actually populates the
# parameter; the `-F b:EPOCHSECONDS` filter form (in earlier slice 5) leaves
# it empty in some shells.
zmodload zsh/datetime

# ---------------------------------------------------------------------------
# gitstatusd daemon orchestration (slice 6, ADR-0001).
#
# Strategy: spawn one long-lived `gitstatusd` per shell. Talk to it via two
# named FIFOs that the parent shell holds open R/W for life — that keeps
# both ends alive across `p10k-rs prompt` invocations without us having to
# pass numbered fds into children.
#
# `p10k-rs prompt` discovers the FIFO paths via two env vars and chooses
# the `Gitstatusd` backend over the slow `ShellOut` fallback.
# ---------------------------------------------------------------------------
typeset -g _P10K_RS_FIFO_DIR=""
typeset -gi _P10K_RS_FIFO_REQ_FD=0
typeset -gi _P10K_RS_FIFO_RESP_FD=0
typeset -gi _P10K_RS_DAEMON_PID=0

_p10k_rs_start_daemon() {
  [[ -n "$_P10K_RS_GITSTATUSD_BIN" && -x "$_P10K_RS_GITSTATUSD_BIN" ]] || return 1

  # Slice 9 security: unpredictable directory name (`mktemp` template) instead
  # of `$$` so a co-tenant on a multi-user host can't pre-plant FIFOs at a
  # guessable path. `chmod 0700` and `mkfifo -m 0600` (in a `umask 077`
  # subshell, belt-and-braces) ensure no other UID can read the IPC channel.
  local base="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}"
  local dir
  dir="$(mktemp -d -- "$base/p10k-rs.XXXXXXXX" 2>/dev/null)" || return 1
  chmod 0700 "$dir" 2>/dev/null
  local req="$dir/req" resp="$dir/resp"
  ( umask 077 && mkfifo -m 0600 "$req" "$resp" ) 2>/dev/null || return 1

  # Keep both FIFOs alive for the lifetime of this shell. R/W opens (`<>`)
  # don't block, so this is safe even before the daemon attaches.
  exec {_P10K_RS_FIFO_REQ_FD}<>"$req"
  exec {_P10K_RS_FIFO_RESP_FD}<>"$resp"

  # Launch daemon in the background. `-t 4` gives it 4 worker threads
  # (matches upstream p10k's default for non-monorepo workloads). stderr
  # to /dev/null because the daemon is chatty on info-level logs.
  "$_P10K_RS_GITSTATUSD_BIN" -t 4 < "$req" > "$resp" 2>/dev/null &!
  _P10K_RS_DAEMON_PID=$!

  _P10K_RS_FIFO_DIR="$dir"
  export _P10K_RS_GITSTATUSD_REQ="$req"
  export _P10K_RS_GITSTATUSD_RESP="$resp"
  return 0
}

_p10k_rs_stop_daemon() {
  if (( _P10K_RS_DAEMON_PID > 0 )); then
    kill -- $_P10K_RS_DAEMON_PID 2>/dev/null
  fi
  if (( _P10K_RS_FIFO_REQ_FD > 0 )); then
    exec {_P10K_RS_FIFO_REQ_FD}>&-
  fi
  if (( _P10K_RS_FIFO_RESP_FD > 0 )); then
    exec {_P10K_RS_FIFO_RESP_FD}<&-
  fi
  # Slice 9 security: refuse to `rm -rf` anything that doesn't match our
  # `mktemp` template — defends against accidental clobber if the variable
  # gets corrupted by a misbehaving plugin.
  if [[ -n "$_P10K_RS_FIFO_DIR" && "$_P10K_RS_FIFO_DIR" == */p10k-rs.* && -d "$_P10K_RS_FIFO_DIR" ]]; then
    rm -rf -- "$_P10K_RS_FIFO_DIR"
  fi
}

autoload -Uz add-zsh-hook

# Wall-clock seconds at the start of the current foreground command. Set in
# `preexec`, consumed and reset in `precmd`. Zero means "no command since
# last prompt" — covers the very first prompt and ^C-on-empty-line cases.
typeset -gi _p10k_rs_cmd_start=0

# Slice 44: feed the `show_on_command` gate.
#
# Upstream Powerlevel10k drives `show_on_command` by re-rendering as the
# user types — a zle widget watches `$BUFFER` keystroke-by-keystroke
# and updates the prompt so `aws ...` reveals the `aws` segment the
# instant the verb is typed. That is correct but expensive: one
# `p10k-rs prompt` subprocess per keystroke.
#
# MVP path (this slice): capture the LAST accepted command at preexec
# and feed it to the NEXT precmd via `--upcoming-command`. The segment
# then appears next to the prompt right after the user ran a matching
# command, not before. That's the upstream behaviour for the common
# case ("I just ran `aws ...`; show me the aws context next to the
# return-status segment") at a fraction of the cost. The "before"
# variant lands when a zle-line-pre-redraw widget is added in a follow-up
# slice.
typeset -g _P10K_RS_UPCOMING_CMD=""

_p10k_rs_preexec() {
  _p10k_rs_cmd_start=$EPOCHSECONDS
  # `$1` is the full command line about to run (already history-expanded).
  _P10K_RS_UPCOMING_CMD="$1"
}

_p10k_rs_precmd() {
  local rs=$?
  local elapsed_ms=0
  if (( _p10k_rs_cmd_start > 0 )); then
    elapsed_ms=$(( (EPOCHSECONDS - _p10k_rs_cmd_start) * 1000 ))
    _p10k_rs_cmd_start=0
  fi
  local upcoming="$_P10K_RS_UPCOMING_CMD"
  _P10K_RS_UPCOMING_CMD=""
  # Detect dead daemon and respawn. `kill -0 $pid` exits 0 if the process
  # exists, non-zero otherwise. ~1ms cost per prompt; a wedged or crashed
  # daemon would otherwise force every prompt onto the slow ShellOut path
  # for the rest of the shell's life.
  if (( _P10K_RS_DAEMON_PID > 0 )) && ! kill -0 -- $_P10K_RS_DAEMON_PID 2>/dev/null; then
    _p10k_rs_stop_daemon
    _p10k_rs_start_daemon || true
  fi
  # Two subprocess calls per precmd — one per side. The gitstatusd
  # daemon does the heavy lifting and its result is cheap to fetch
  # twice (the second call still goes over the FIFO, but the daemon
  # caches the per-cwd snapshot in-process). Splitting per-side keeps
  # the binary's wire format trivial: each invocation prints one
  # ribbon, no in-band separators to parse.
  PROMPT="$("$_P10K_RS_BIN" prompt --shell zsh --render-side left --last-status $rs --last-duration-ms $elapsed_ms --upcoming-command "$upcoming" --dump "$_p10k_rs_dump" 2>/dev/null) "
  RPROMPT="$("$_P10K_RS_BIN" prompt --shell zsh --render-side right --last-status $rs --last-duration-ms $elapsed_ms --upcoming-command "$upcoming" 2>/dev/null)"
  return $rs
}

add-zsh-hook preexec _p10k_rs_preexec
add-zsh-hook precmd _p10k_rs_precmd
add-zsh-hook zshexit _p10k_rs_stop_daemon

# Transient prompt widget. Runs once per accepted line, just before
# the command is dispatched: we ask the binary for the collapsed
# PROMPT, assign it, and call `zle reset-prompt` so the user's
# scrollback gets the minimal form. When `transient_prompt = off`,
# the binary prints nothing — PROMPT becomes empty, the redraw is a
# no-op, and the next precmd refills PROMPT with the full ribbon. No
# `RPROMPT` swap: the right prompt naturally clears on redraw.
_p10k_rs_zle_line_finish() {
  local transient
  transient="$("$_P10K_RS_BIN" prompt --shell zsh --render-side transient 2>/dev/null)"
  PROMPT="$transient"
  zle reset-prompt 2>/dev/null
}
zle -N zle-line-finish _p10k_rs_zle_line_finish

# Best-effort daemon start. If it fails (binary missing, mkfifo denied,
# etc.) the prompt silently uses the ShellOut fallback.
_p10k_rs_start_daemon || true
