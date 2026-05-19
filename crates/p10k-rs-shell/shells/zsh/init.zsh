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
# T1.18 — refuse to source the instant-prompt dump unless it's a regular
# file, owner-matches this UID, and mode is exactly 0600. The dump is
# `source`d as code at every shell startup; defends against an attacker
# (or a careless tool) overwriting the dump with a hostile payload while
# the shell is paused / long-running, or a co-tenant pre-planting a
# symlink at the dump path on a multi-user host.
#
# Uses zstat where available (zsh/stat module). On exotic platforms
# where the module isn't loadable we fail closed — skip the source,
# accept a slightly slower first prompt next shell.
if [[ -f $_p10k_rs_dump && ! -L $_p10k_rs_dump ]]; then
  zmodload -F zsh/stat b:zstat 2>/dev/null && {
    typeset -A _p10k_rs_dump_st
    if zstat -A _p10k_rs_dump_st +mode +uid -- $_p10k_rs_dump 2>/dev/null \
       && (( _p10k_rs_dump_st[uid] == EUID )) \
       && (( (_p10k_rs_dump_st[mode] & 0777) == 0600 )); then
      source $_p10k_rs_dump 2>/dev/null
    fi
    unset _p10k_rs_dump_st
  }
fi

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

  # Launch daemon in the background under resource caps (T1.15).
  # `-t 4` gives it 4 worker threads (matches upstream p10k's default
  # for non-monorepo workloads). stderr to /dev/null because the daemon
  # is chatty on info-level logs.
  #
  # Resource caps:
  #   ulimit -v 524288   # 512 MiB virtual memory ceiling (Linux only).
  #   ulimit -t 30       # 30 s cumulative CPU per process.
  # Defends against a runaway daemon on a pathological repo / corrupted
  # index / fork-bomb path pegging the box. The cap fires inside a
  # subshell so it doesn't leak into the user's interactive shell.
  #
  # Portability note: `ulimit -v` is a Linux/glibc extension; macOS
  # (Darwin) silently ignores it because RLIMIT_AS isn't enforced
  # there. The CPU cap (`-t`) is POSIX and works on both. macOS users
  # are documented to rely on `-t` alone — the RSS ceiling is a
  # Linux-only belt-and-braces.
  ( ulimit -v 524288 2>/dev/null; ulimit -t 30 2>/dev/null
    exec "$_P10K_RS_GITSTATUSD_BIN" -t 4 < "$req" > "$resp" 2>/dev/null
  ) &!
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

# OSC 133 shell-integration markers (T1.5 / T1.9 bundle).
#
# The binary's `render_prompt` emits OSC 7 + OSC 133 A/B around PS1
# when `Config::shell_integration.mode` resolves to "active" (see
# `resolve_shell_integration` in crates/p10k-rs/src/main.rs). The shell
# pairs that with `C` (start of output) at preexec and `D;<exit>` (end
# of output) at precmd. Hosts and modern terminals (Claude Code,
# Cursor, VSCode, Ghostty, WezTerm, iTerm2, Windows Terminal, Kitty)
# use the matched A→B→C→D sequence to slice scrollback into semantic
# command boundaries.
#
# Auto-detect mirrors the binary's logic: emit when any AI-host env
# var OR any of the recognised modern-terminal fingerprints is present.
# Suppressed under Warp (`TERM_PROGRAM=WarpTerminal`) regardless —
# Warp's block model breaks on OSC 133 A. See warp#6718.
#
# Detect once at sourcing time, not per-prompt — these env vars don't
# change within a shell session. We can't read the TOML schema's
# `shell_integration.mode` from zsh without a fork; the binary owns
# the policy at render time for OSC A/B, and zsh approximates the
# same default-on-auto policy for C/D. A user who set `mode = "off"`
# in TOML will get A/B suppressed (no host-side decorations) but C/D
# will still emit — a follow-up slice can plumb the binary's decision
# back via a sourced env var if that mismatch matters.
typeset -g _P10K_RS_AI_HOST=""
if [[ -n "${CLAUDECODE:-}" ]]; then
  _P10K_RS_AI_HOST="claude-code"
elif [[ -n "${AIDER_API_KEY:-}${AIDER_MODEL:-}${AIDER_AUTO_COMMITS:-}" ]]; then
  _P10K_RS_AI_HOST="aider"
elif [[ -n "${CURSOR_TRACE_ID:-}${CURSOR_SESSION_ID:-}" ]]; then
  _P10K_RS_AI_HOST="cursor"
fi

# Resolved shell-integration flag for OSC 133 C/D emission. Empty
# means "off"; any non-empty value enables. Warp is hard-suppressed
# regardless of every other signal.
typeset -g _P10K_RS_SHELL_INTEGRATION=""
if [[ "${TERM_PROGRAM:-}" == "WarpTerminal" ]]; then
  _P10K_RS_SHELL_INTEGRATION=""
elif [[ -n "$_P10K_RS_AI_HOST" \
        || -n "${TERM_PROGRAM:-}" \
        || -n "${WT_SESSION:-}" \
        || -n "${GHOSTTY_RESOURCES_DIR:-}" \
        || -n "${KITTY_WINDOW_ID:-}" ]]; then
  _P10K_RS_SHELL_INTEGRATION=1
fi

_p10k_rs_preexec() {
  _p10k_rs_cmd_start=$EPOCHSECONDS
  # `$1` is the full command line about to run (already history-expanded).
  _P10K_RS_UPCOMING_CMD="$1"
  # OSC 133 `C` — start of command output. Emitted between the user
  # accepting the line and the command actually running, so the host
  # can mark "everything after this byte and before the next prompt is
  # command output". `printf` is a builtin, no fork.
  if [[ -n "$_P10K_RS_SHELL_INTEGRATION" ]]; then
    printf '\033]133;C\007'
  fi
}

_p10k_rs_precmd() {
  local rs=$?
  # OSC 133 `D;<exit>` — end of command output, carrying `$?`. Emit
  # before any other precmd work so the marker lands tight against the
  # last byte the command wrote. Mirrors `osc133_command_end(rs)` in
  # `p10k-rs-ai`.
  if [[ -n "$_P10K_RS_SHELL_INTEGRATION" ]]; then
    printf '\033]133;D;%d\007' "$rs"
  fi
  # T1.2 — re-arm bracketed-paste mode (`DECSET 2004`). zsh >= 5.1
  # enables this itself, but command output is allowed to disable it
  # (any program emitting `\e[?2004l` permanently blinds the shell to
  # pasted-input markers until something turns it back on). Belt and
  # braces: emit the enable sequence every precmd so a stray reset
  # from `less`, `vim`, `ssh`, etc. doesn't leave the user without
  # paste protection on the *next* command line. Write directly to
  # /dev/tty so the sequence reaches the terminal even when stdout
  # is captured (rare in precmd, but cheap insurance).
  printf '\033[?2004h' > /dev/tty 2>/dev/null
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
  # T1.22 — route binary stderr to the diagnostics log instead of
  # /dev/null. The binary's tracing-appender subscriber (T1.21)
  # already captures structured events to the same file, but anything
  # the binary prints to bare stderr (panic backtraces from depths
  # outside the subscriber's reach, libc warnings, etc.) used to
  # vanish. Appending here gives us one canonical channel for "what
  # broke?" diagnostics. `mkdir -p` is a guard for the very first
  # invocation in a fresh $XDG_STATE_HOME; the binary creates the
  # dir itself but only after `init_tracing` runs, and any pre-
  # tracing stderr from earlier startup needs the dir to exist.
  mkdir -p "${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs" 2>/dev/null
  PROMPT="$("$_P10K_RS_BIN" prompt --shell zsh --render-side left --last-status $rs --last-duration-ms $elapsed_ms --upcoming-command "$upcoming" --dump "$_p10k_rs_dump" 2>>"${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs/diagnostics.log") "
  RPROMPT="$("$_P10K_RS_BIN" prompt --shell zsh --render-side right --last-status $rs --last-duration-ms $elapsed_ms --upcoming-command "$upcoming" 2>>"${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs/diagnostics.log")"
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
  # T1.1 — clear RPROMPT before redraw. Without this the right-side
  # ribbon lingers in the scrollback next to the collapsed transient
  # `❯`, which is both ugly and confusing (the rest of the prompt
  # collapsed, but RPROMPT is still painting its full ribbon). zsh's
  # `reset-prompt` reads the current values of both PROMPT and RPROMPT,
  # so we have to blank RPROMPT first.
  RPROMPT=""
  zle reset-prompt 2>/dev/null
}
zle -N zle-line-finish _p10k_rs_zle_line_finish

# Best-effort daemon start. If it fails (binary missing, mkfifo denied,
# etc.) the prompt silently uses the ShellOut fallback.
_p10k_rs_start_daemon || true
