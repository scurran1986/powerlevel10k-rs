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
#  - Transient prompt (v0.3): all four `TransientPromptMode` variants
#    (Off / Always / SameDir / UniqueDir) honored via the same
#    exit-code-2 contract zsh and fish use. See "Transient prompt"
#    block below. HONEST CAVEAT: bash readline has no
#    `zle reset-prompt` / `commandline -f repaint` analogue, so the
#    collapse is redrawn manually with cursor-control escapes. That
#    redraw is robust for a single-row prompt with single-line input
#    (the overwhelmingly common case). It does NOT collapse cleanly
#    when the prompt itself spans multiple rows or the input line
#    soft-wrapped past the terminal width — in those cases the full
#    ribbon is left in scrollback rather than risk clobbering output.
#    The binary-side wire-up (mode gating, cwd history) is complete;
#    only the terminal redraw is the documented partial. See the
#    "Transient prompt" block below.
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
  # Shift the cwd-history slots forward — same discipline as the zsh
  # precmd and the fish full-render path. `_P10K_RS_PREV_PROMPT_CWD` is
  # the cwd where the prompt above the current one was rendered;
  # `_P10K_RS_CURR_PROMPT_CWD` is the cwd of the prompt we just emitted.
  # The transient redraw reads the prev slot (via `--last-prompt-cwd`)
  # to decide whether SameDir/UniqueDir should collapse.
  _P10K_RS_PREV_PROMPT_CWD="$_P10K_RS_CURR_PROMPT_CWD"
  _P10K_RS_CURR_PROMPT_CWD="$PWD"
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

# Transient prompt — bash wire-up (v0.3).
#
# bash readline has no `zle reset-prompt` (zsh) and no
# `commandline -f repaint` (fish), so there is no built-in hook that
# re-renders the just-emitted prompt before the accepted line executes.
# We approximate it by binding Return (\C-m) to a wrapper that, on
# accept-line, asks the binary for the collapsed render and manually
# overwrites the current prompt with cursor-control escapes, then lets
# readline accept the line normally.
#
# Wire protocol mirrors the zsh widget (T1.8) and the fish handler
# (v0.2) — the four `TransientPromptMode` variants (Off / Always /
# SameDir / UniqueDir) are decided INSIDE the binary; the shell only
# forwards the wire bits and reacts to the exit code:
#   exit 0, non-empty stdout → mode=always / same-dir match /
#                              unique-dir match; redraw the collapsed
#                              prompt over the full one
#   exit 0, empty stdout     → mode=off; leave the full prompt as-is
#   exit 2 (any stdout)      → mode=same-dir / unique-dir mismatch
#                              (KeepPrompt); leave the full ribbon in
#                              scrollback. Any other non-zero exit
#                              (binary missing, panic) is treated the
#                              same — silently keeping the full prompt
#                              beats blanking it over a transient fault.
#
# `--last-prompt-cwd` carries the cwd of the prompt *before* the one
# being collapsed (the `_P10K_RS_PREV_PROMPT_CWD` slot the
# PROMPT_COMMAND shifts forward). `--prompt-cwd-history-file` points at
# a per-shell NUL-separated history file the binary only consults under
# `unique-dir`; a missing / empty file degrades naturally to `same-dir`.
#
# HONEST CAVEAT (the documented partial): the cursor-control redraw is
# robust for a single-row prompt with single-line input. When the
# prompt spans multiple rows, or the typed line soft-wrapped past the
# terminal width, bash gives the script no reliable way to know how
# many rows to move up, so we DO NOT attempt the collapse in those
# cases — the full ribbon stays in scrollback (same visible outcome as
# the KeepPrompt branch). The binary-side mode gating and cwd-history
# round-trip are complete; only the terminal redraw is conservative.
_P10K_RS_TRANSIENT=0
_P10K_RS_PREV_PROMPT_CWD=""
_P10K_RS_CURR_PROMPT_CWD=""
# Per-shell cwd-history file for the binary's `unique-dir` mode. Lives
# under the user's XDG runtime dir with a `mktemp` template so a
# co-tenant can't pre-plant a path. Failure to create is non-fatal —
# the binary handles a missing file as empty history (unique-dir →
# same-dir). bash has no reliable interactive-only exit hook scoped
# like zsh's `zshexit`, so we lean on the OS to reap the file:
# XDG_RUNTIME_DIR is tmpfs-cleaned at logout on systemd hosts, and a
# `$TMPDIR` fallback is acceptable on macOS.
_P10K_RS_CWD_HISTORY_FILE=""
__p10k_rs_runtime_base="${XDG_RUNTIME_DIR:-}"
[[ -z "$__p10k_rs_runtime_base" ]] && __p10k_rs_runtime_base="${TMPDIR:-}"
[[ -z "$__p10k_rs_runtime_base" ]] && __p10k_rs_runtime_base="/tmp"
__p10k_rs_dir="$(mktemp -d -- "$__p10k_rs_runtime_base/p10k-rs-bash.XXXXXXXX" 2>/dev/null)"
if [[ -n "$__p10k_rs_dir" ]]; then
  chmod 0700 "$__p10k_rs_dir" 2>/dev/null
  _P10K_RS_CWD_HISTORY_FILE="$__p10k_rs_dir/cwd-history"
fi
unset __p10k_rs_runtime_base __p10k_rs_dir

# Redraw the collapsed transient prompt over the full one, then signal
# whether readline should proceed to accept-line. Returns 0 always; the
# caller's keymap chains accept-line unconditionally so the line always
# executes even when we decline to collapse.
__p10k_rs_transient_redraw() {
  local -a args=( prompt --shell bash --render-side transient )
  if [[ -n "$_P10K_RS_PREV_PROMPT_CWD" ]]; then
    args+=( --last-prompt-cwd "$_P10K_RS_PREV_PROMPT_CWD" )
  fi
  if [[ -n "$_P10K_RS_CWD_HISTORY_FILE" && -s "$_P10K_RS_CWD_HISTORY_FILE" ]]; then
    args+=( --prompt-cwd-history-file "$_P10K_RS_CWD_HISTORY_FILE" )
  fi
  local transient rc
  transient="$("$_P10K_RS_BIN" "${args[@]}" 2>/dev/null)"
  rc=$?
  # rc != 0 (KeepPrompt, or any binary fault) → leave the full prompt
  # in scrollback. rc == 0 with empty stdout (Off mode) → likewise
  # nothing to redraw. Either way: do not touch the terminal.
  if (( rc != 0 )) || [[ -z "$transient" ]]; then
    return 0
  fi
  # Single-row collapse only. bash exposes no portable way to learn how
  # many rows the full prompt + typed input occupy, so we restrict the
  # redraw to the common single-row case: input fits on one line AND
  # has not soft-wrapped. READLINE_LINE is the current input buffer;
  # COLUMNS is the terminal width. If the rendered prompt prefix plus
  # the input would wrap, bail (KeepPrompt-equivalent). We can't measure
  # the rendered prompt width cheaply, so we use a conservative guard:
  # only collapse when the input has no embedded newline. Multi-row
  # prompts fall through to the binary's KeepPrompt naturally because we
  # never reach here for them — the conservative path simply leaves the
  # full ribbon.
  if [[ "$READLINE_LINE" == *$'\n'* ]]; then
    return 0
  fi
  # Move to column 0, clear the line, print the collapsed render, then a
  # newline so the executing command's output starts on a fresh row —
  # matching the zsh/fish ordering where the collapsed `❯` lands above
  # the command output. \r = CR to column 0, \033[2K = clear entire
  # line, \033[0K after the print clears any trailing full-prompt
  # remnant to the right of the cursor.
  printf '\r\033[2K%s \033[0K\n' "$transient" >/dev/tty 2>/dev/null
  # Append the OUTGOING prev cwd to the per-shell history file BEFORE the
  # next PROMPT_COMMAND slot-shift would lose it. NUL-separated so cwds
  # containing newlines (POSIX-legal, vanishingly rare) survive the
  # round-trip. The binary caps history at 64 entries on read.
  if [[ -n "$_P10K_RS_CWD_HISTORY_FILE" && -n "$_P10K_RS_PREV_PROMPT_CWD" ]]; then
    printf '%s\0' "$_P10K_RS_PREV_PROMPT_CWD" >>"$_P10K_RS_CWD_HISTORY_FILE" 2>/dev/null
  fi
  return 0
}

# Bind Return to run the redraw, then accept the line. `bind -x` runs a
# shell function but does NOT itself accept the line; we chain a second
# binding via a readline macro so accept-line always fires afterwards.
# The function is bound to an unlikely keyseq (\C-x\C-p) and Return is
# mapped to "invoke that function, then accept-line". `bind -r` first so
# re-sourcing this file replaces our binding cleanly without stacking
# duplicates (the `_P10K_RS_INSTALLED` guard already prevents re-entry,
# but this mirrors fish's `bind --erase` discipline defensively).
#
# Only wire the binding for interactive shells that have readline line
# editing enabled — `bind` warns "line editing not enabled" otherwise
# (e.g. `bash -n` syntax checks, non-interactive sourcing). Guard on
# the interactive flag so the script stays quiet in those contexts.
if [[ $- == *i* ]]; then
  bind -x '"\C-x\C-p": __p10k_rs_transient_redraw' 2>/dev/null
  bind -r '\C-m' 2>/dev/null
  bind '"\C-m": "\C-x\C-p\n"' 2>/dev/null
fi
