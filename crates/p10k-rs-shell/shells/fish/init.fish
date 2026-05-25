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
#  - Transient prompt (v0.2): all four `TransientPromptMode` variants
#    (Off / Always / SameDir / UniqueDir) honored via the same
#    exit-code-2 contract zsh uses. See "Transient prompt" block below.
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

# Transient prompt — fish wire-up (v0.2).
#
# Fish has no ZLE / no `zle-line-finish`. The idiom is:
#
#   1. Override `fish_prompt` itself so it can emit either the full
#      ribbon or the collapsed `❯`, switching on a flag.
#   2. Bind Enter (\r) to a wrapper that sets the flag, forces a
#      repaint (re-runs `fish_prompt` so the user's scrollback gets
#      the collapsed form), then executes the line. The flag is
#      cleared by `fish_postexec` so the next prompt renders full
#      again.
#
# Wire protocol mirrors the zsh widget (T1.8):
#   exit 0, non-empty stdout → mode=always / same-dir / unique-dir
#                              match; emit the collapsed render
#   exit 0, empty stdout     → mode=off; emit the full render so the
#                              user never sees a blank line
#   exit 2 (any stdout)      → mode=same-dir / unique-dir mismatch;
#                              fall back to the full render so the
#                              full ribbon stays in scrollback
#
# `--last-prompt-cwd` carries the cwd of the prompt *before* the one
# being collapsed (see `_P10K_RS_PREV_PROMPT_CWD` slot shift below).
# `--prompt-cwd-history-file` points at a per-shell NUL-separated
# history file. The binary only consults it under `unique-dir`; an
# empty / missing file degrades naturally to `same-dir` semantics.
set -g _p10k_rs_transient 0
set -g _P10K_RS_PREV_PROMPT_CWD ''
set -g _P10K_RS_CURR_PROMPT_CWD ''
# Per-shell cwd-history file. Lives under the user's XDG runtime dir
# with a `mktemp` template so a co-tenant can't pre-plant a path.
# `fish` doesn't expose a `zshexit`-equivalent that's both reliable
# AND scoped to interactive sessions, so we lean on the OS to reap
# the file: XDG_RUNTIME_DIR is tmpfs-cleaned at logout on systemd
# hosts, and a `mktemp` fallback in $TMPDIR is acceptable on macOS.
# Failure to create the file is non-fatal — the binary handles a
# missing file as empty history (degrades unique-dir → same-dir).
set -g _P10K_RS_CWD_HISTORY_FILE ''
set -l _p10k_rs_runtime_base "$XDG_RUNTIME_DIR"
test -z "$_p10k_rs_runtime_base"; and set _p10k_rs_runtime_base "$TMPDIR"
test -z "$_p10k_rs_runtime_base"; and set _p10k_rs_runtime_base /tmp
set -l _p10k_rs_dir (mktemp -d -- "$_p10k_rs_runtime_base/p10k-rs-fish.XXXXXXXX" 2>/dev/null)
if test -n "$_p10k_rs_dir"
    chmod 0700 "$_p10k_rs_dir" 2>/dev/null
    set -g _P10K_RS_CWD_HISTORY_FILE "$_p10k_rs_dir/cwd-history"
end

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

function _p10k_rs_postexec --on-event fish_postexec
    # Clear the transient flag so the next prompt renders the full
    # ribbon again. Belt-and-braces: `fish_prompt` also clears it
    # after consuming, but a binding that bypassed the prompt (rare,
    # but possible via `commandline -f execute` from a key bound by
    # the user) would otherwise leave the flag stuck on.
    set -g _p10k_rs_transient 0
end

# Enter-key wrapper. Sets the transient flag, repaints (re-runs
# `fish_prompt` with the flag observed → collapsed render in
# scrollback), then executes the accepted line. `commandline -f
# repaint` is synchronous-enough that the collapsed render lands
# before the command's output, matching the zsh `zle reset-prompt`
# ordering inside `zle-line-finish`.
function _p10k_rs_transient_execute
    set -g _p10k_rs_transient 1
    commandline -f repaint
    commandline -f execute
end

# Bind Enter in every mode fish supports out of the box. Vi-mode
# users get insert+normal; default-mode users only need the unnamed
# bind. `bind --erase` first so re-sourcing this file replaces our
# own binding cleanly without stacking duplicates.
bind --erase \r 2>/dev/null
bind \r _p10k_rs_transient_execute
if functions --query fish_vi_key_bindings; or functions --query fish_hybrid_key_bindings
    bind --erase -M insert \r 2>/dev/null
    bind --erase -M default \r 2>/dev/null
    bind -M insert \r _p10k_rs_transient_execute
    bind -M default \r _p10k_rs_transient_execute
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

    if test "$_p10k_rs_transient" = 1
        # Transient render path. Drain the flag first so a fault below
        # doesn't leave it stuck on (the postexec hook is the
        # belt-and-braces, this is the suspenders).
        set -g _p10k_rs_transient 0
        set -l args prompt --shell fish --render-side transient
        if test -n "$_P10K_RS_PREV_PROMPT_CWD"
            set args $args --last-prompt-cwd "$_P10K_RS_PREV_PROMPT_CWD"
        end
        if test -n "$_P10K_RS_CWD_HISTORY_FILE"; and test -s "$_P10K_RS_CWD_HISTORY_FILE"
            set args $args --prompt-cwd-history-file "$_P10K_RS_CWD_HISTORY_FILE"
        end
        set -l transient ("$_P10K_RS_BIN" $args 2>/dev/null)
        set -l rc $status
        if test $rc -eq 0; and test -n "$transient"
            # Emit collapsed render. Same trailing-space discipline as
            # the full-render path so the cursor lands one column past
            # the rendered `❯`.
            echo -n "$transient"
            echo -n ' '
            # Append OUTGOING prev cwd to the per-shell history file
            # BEFORE the slot shift overwrites it. NUL-separated so
            # cwds containing newlines (POSIX-legal, vanishingly rare)
            # survive the round-trip intact. The binary caps history
            # at 64 entries on read.
            if test -n "$_P10K_RS_CWD_HISTORY_FILE"; and test -n "$_P10K_RS_PREV_PROMPT_CWD"
                printf '%s\0' "$_P10K_RS_PREV_PROMPT_CWD" >>"$_P10K_RS_CWD_HISTORY_FILE" 2>/dev/null
            end
            set -g _P10K_RS_PREV_PROMPT_CWD "$_P10K_RS_CURR_PROMPT_CWD"
            set -g _P10K_RS_CURR_PROMPT_CWD "$PWD"
            return
        end
        # rc == 2 (KeepPrompt) OR rc == 0 with empty stdout (Off mode)
        # OR binary fault. Fall through to the full render so the user
        # never sees a blank line and the full ribbon stays in
        # scrollback. Mirrors the zsh widget's `rc != 0` early-return
        # which leaves PROMPT alone for the next reset-prompt cycle.
    end

    "$_P10K_RS_BIN" prompt --shell fish --last-status $rs --last-duration-ms $elapsed_ms 2>/dev/null
    echo -n ' '
    # Shift the cwd-history slots forward — same discipline as the zsh
    # precmd. `_P10K_RS_PREV_PROMPT_CWD` is the cwd where the prompt
    # above the current one was rendered; `_P10K_RS_CURR_PROMPT_CWD`
    # is the cwd of the prompt we just emitted. The transient render
    # path reads the prev slot to decide whether to collapse.
    set -g _P10K_RS_PREV_PROMPT_CWD "$_P10K_RS_CURR_PROMPT_CWD"
    set -g _P10K_RS_CURR_PROMPT_CWD "$PWD"
end
