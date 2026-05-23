# Importing from Powerlevel10k

If you already have a `~/.p10k.zsh`, the `p10k-rs import`
subcommand gives you a translated `config.toml` head-start.

## One-shot import

```bash
p10k-rs import ~/.p10k.zsh > ~/.config/p10k-rs/config.toml
```

Reload zsh (`exec zsh`) and your prompt should look broadly
familiar.

## What the importer actually does

It's a **best-effort textual translator**, not an emulator. The
importer does **not** execute your zsh config — it just reads
the file as text and looks for `POWERLEVEL9K_*` variable
assignments.

That has two consequences worth knowing:

- **Pure variable assignments translate cleanly.** Direct
  `POWERLEVEL9K_FOO=bar`, `POWERLEVEL9K_FOO=(a b c)`,
  `POWERLEVEL9K_FOO='hex string'` shapes all work.
- **Dynamic values don't.** If your `.p10k.zsh` computes a
  variable from a shell function (`POWERLEVEL9K_FOO=$(my_helper)`)
  or has zsh-conditional branches (`[[ $LANG == *.UTF-8 ]] && ...`),
  the importer can't follow those branches statically. You'll
  get the literal text — usually a warning to stderr.

## What's supported

| Powerlevel9k variable | Maps to |
|---|---|
| `POWERLEVEL9K_LEFT_PROMPT_ELEMENTS` | `[layout].left` |
| `POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS` | `[layout].right` |
| `POWERLEVEL9K_MODE` | `mode` |
| `POWERLEVEL9K_INSTANT_PROMPT` | `instant_prompt` |
| `POWERLEVEL9K_<SEG>_FOREGROUND` | `[segment.<seg>].foreground` |
| `POWERLEVEL9K_<SEG>_BACKGROUND` | `[segment.<seg>].background` |
| `POWERLEVEL9K_<SEG>_<STATE>_FOREGROUND` | `[segment.<seg>.states.<state>].foreground` |
| `POWERLEVEL9K_<SEG>_<STATE>_BACKGROUND` | `[segment.<seg>.states.<state>].background` |

Colour value forms understood: P9k indexed numbers (`0`–`255`),
named colours (`red`, `brightblue`, …), and `#rrggbb` / `#rgb`
hex literals.

## What's not yet supported

The translator currently has gaps for:

- Per-segment icon overrides (`POWERLEVEL9K_<SEG>_ICON`).
- Segment separator characters and powerline chevron overrides.
- Frame styles and ruler styles.
- Most of the `_SHOW_*` and `_HIDE_*` conditional gates (a few
  land via `show_in_dir` / `show_on_command`).
- `POWERLEVEL9K_TRANSIENT_PROMPT` modes — `p10k-rs` has its own
  four-mode `transient_prompt` setting; check
  [the configuration chapter](docs/src/config/index.md).

Unrecognised variables are printed to **stderr** so you can see
what didn't translate:

```
import: POWERLEVEL9K_DIR_SHORTENED_FOREGROUND: unsupported variable
import: POWERLEVEL9K_VCS_LOADING_FOREGROUND: state 'loading' not in our schema
```

Pipe stdout to your config, stderr to your screen:

```bash
p10k-rs import ~/.p10k.zsh > ~/.config/p10k-rs/config.toml
# stderr lands on the terminal — read it
```

## Diff-driven import

Working iteratively? Run the importer, hand-edit the result,
then re-run to see what changed:

```bash
p10k-rs import ~/.p10k.zsh > /tmp/imported.toml
diff -u ~/.config/p10k-rs/config.toml /tmp/imported.toml
```

The importer's output is deterministic (no timestamps, no
random ordering), so diffs are clean across runs.

## After importing

Validate the result without restarting zsh:

```bash
p10k-rs config check
# OK: /home/u/.config/p10k-rs/config.toml parses cleanly
```

Then `exec zsh` to see the live prompt. If something looks off,
the user guide has the full schema — start at
[docs/src/SUMMARY.md](docs/src/SUMMARY.md), or skip the import
entirely and start from one of the ten bundled themes:

```bash
p10k-rs theme list
p10k-rs theme install nord    # or whichever you like
```

The catalogue lives at [themes/README.md](themes/README.md).

## Why the importer is best-effort

The Powerlevel9k variable namespace has hundreds of options
accumulated over a decade of upstream development. Faithfully
covering all of them would mean re-implementing P9k. The
importer gets you ~80% of a common config translated; the
remaining 20% is hand-tuning. See the test corpus in
`crates/p10k-rs-config/src/import/` for the patterns that are
known to round-trip cleanly.
