# Per-shell init

Three init scripts ship in `crates/p10k-rs-shell/shells/`. `zsh` is the
fully-wired daily driver; `bash` and `fish` ship working scripts whose
installer integration lands in a later slice. The binary always
sanitises untrusted input before emission, regardless of which shell is
sourcing the prompt.

Source the right one with:

```bash
# zsh
eval "$(p10k-rs init zsh)"

# bash
eval "$(p10k-rs init bash)"

# fish
p10k-rs init fish | source
```

All three scripts are idempotent — re-sourcing is a no-op.

## Feature parity

| Feature | zsh | bash | fish |
|---|---|---|---|
| Left prompt | yes | yes | yes |
| Right prompt (`RPROMPT`) | yes | no (bash has no native equivalent) | not wired — fish supports a `fish_right_prompt` function but our `init.fish` doesn't define one yet |
| `$?` capture | yes | yes | yes |
| `command_execution_time` | yes | no (no clean preexec; passes `--last-duration-ms 0`) | yes (uses `fish_preexec` event) |
| `gitstatusd` daemon backend | yes | no (FIFO plumbing is zsh-specific) | no (FIFO plumbing is zsh-specific) |
| `git` shell-out fallback | yes | yes | yes |
| Transient prompt | yes (via ZLE widgets) | no (readline has no comparable redraw hook) | wired — Enter-key bind sets `_p10k_rs_transient=1` then `commandline -f repaint` redraws (since `8ad919b`) |
| Instant prompt | yes (cached PROMPT-SUBST dump) | no (does not map onto bash's prompt model) | no (does not map onto fish's prompt model) |

## bash specifics

- Hooks via `PROMPT_COMMAND` (string form for bash 3.x–5.x compatibility).
- No native right prompt: any `[layout.right]` configured in TOML is
  silently dropped for bash users. A future slice may emulate
  right-alignment via cursor-position escapes; the trade-off (no
  resize handling, fragile under multiline input) is real.
- `command_execution_time`: bash has no clean preexec hook (a `trap
  DEBUG` workaround interacts badly with completion and subshells), so
  the segment stays below its 3-second threshold and hides.

## fish specifics

- Hooks via `fish_prompt` and a `fish_preexec` event handler.
- `command_execution_time` uses GNU coreutils `date +%s%3N` when
  available; falls back to whole-second precision on systems without
  `%3N` (macOS BSD `date`).
- No daemon backend or instant prompt — same reasons as bash.

## zsh specifics

- The only fully-wired shell. `install.sh` appends an
  `eval "$(p10k-rs init zsh)"` line to `~/.zshrc`.
- All four headline P10K features (instant, transient, show-on-command,
  daemon-backed git) target zsh first.
