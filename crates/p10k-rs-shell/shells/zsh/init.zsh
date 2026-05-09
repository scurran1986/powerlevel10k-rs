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

_p10k_rs_preexec() {
  _p10k_rs_cmd_start=$EPOCHSECONDS
}

_p10k_rs_precmd() {
  local rs=$?
  local elapsed_ms=0
  if (( _p10k_rs_cmd_start > 0 )); then
    elapsed_ms=$(( (EPOCHSECONDS - _p10k_rs_cmd_start) * 1000 ))
    _p10k_rs_cmd_start=0
  fi
  # Detect dead daemon and respawn. `kill -0 $pid` exits 0 if the process
  # exists, non-zero otherwise. ~1ms cost per prompt; a wedged or crashed
  # daemon would otherwise force every prompt onto the slow ShellOut path
  # for the rest of the shell's life.
  if (( _P10K_RS_DAEMON_PID > 0 )) && ! kill -0 -- $_P10K_RS_DAEMON_PID 2>/dev/null; then
    _p10k_rs_stop_daemon
    _p10k_rs_start_daemon || true
  fi
  PROMPT="$("$_P10K_RS_BIN" prompt --shell zsh --last-status $rs --last-duration-ms $elapsed_ms --dump "$_p10k_rs_dump" 2>/dev/null) "
  return $rs
}

add-zsh-hook preexec _p10k_rs_preexec
add-zsh-hook precmd _p10k_rs_precmd
add-zsh-hook zshexit _p10k_rs_stop_daemon

# Best-effort daemon start. If it fails (binary missing, mkfifo denied,
# etc.) the prompt silently uses the ShellOut fallback.
_p10k_rs_start_daemon || true
