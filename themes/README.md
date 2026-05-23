# Bundled themes

Ten ready-made prompt themes shipped with `p10k-rs`. Each is a small
TOML file (~30-60 lines) demonstrating the schema. Pick one, install
it, restart your shell.

## Catalogue

| Name | Look | `colors` | `mode` |
|---|---|---|---|
| [`lean`](lean.toml) | Single-line minimal | `follow_terminal` | `compatible` |
| [`classic`](classic.toml) | Two-line with chevron frame | `ansi256` | `compatible` |
| [`rainbow`](rainbow.toml) | Powerline ribbons w/ backgrounds | `true-color` | `nerd-font-v3` |
| [`pure`](pure.toml) | Minimal two-line (Pure-zsh inspired) | `follow_terminal` | `compatible` |
| [`catppuccin-mocha`](catppuccin-mocha.toml) | Soothing pastel | `true-color` | `nerd-font-v3` |
| [`tokyo-night`](tokyo-night.toml) | Cool nightscape | `true-color` | `nerd-font-v3` |
| [`gruvbox-dark`](gruvbox-dark.toml) | Retro warm earthy | `true-color` | `nerd-font-v3` |
| [`nord`](nord.toml) | Arctic blues + greys | `true-color` | `nerd-font-v3` |
| [`solarized-dark`](solarized-dark.toml) | Ethan Schoonover classic | `true-color` | `compatible` |
| [`dracula`](dracula.toml) | High-contrast purple | `true-color` | `nerd-font-v3` |

`Nerd Font`-mode themes assume your terminal renders a Nerd Font (for
the OS icon, branch glyphs, etc.). If you don't have one installed,
either install a Nerd Font (https://www.nerdfonts.com/) or pick
a `Unicode`-mode theme — they only use box-drawing characters from
the basic Unicode plane.

## Install with the `theme` subcommand

```bash
# See what's available
p10k-rs theme list

# Print a theme's TOML without installing (good for diffs / inspection)
p10k-rs theme show catppuccin-mocha

# Install — copies the theme to ~/.config/p10k-rs/config.toml.
# If a config already exists, it's saved as config.toml.bak first.
p10k-rs theme install catppuccin-mocha

# Force overwrite an existing config without a backup
p10k-rs theme install rainbow --force
```

Reload your shell (`exec zsh` or open a new terminal) to see the
change.

## Install manually

The bundled themes are also committed at this repo path, so you can
copy one directly without going through the binary:

```bash
mkdir -p ~/.config/p10k-rs
cp ~/.local/share/powerlevel10k-rs/themes/nord.toml \
   ~/.config/p10k-rs/config.toml
```

(Substitute your install prefix; the bootstrap puts the source tree
at `~/.local/share/powerlevel10k-rs`.)

## Customise

The themes are starting points. Once installed, edit
`~/.config/p10k-rs/config.toml` and add, remove, or recolour
whatever you like — the full schema lives in the user guide at
[`docs/src/config/`](../docs/src/config/index.md) and the
[`Reference: Schema (full)`](../docs/src/reference/schema.md) page.

Common tweaks:

- Add a segment to the layout: append to `[layout].left` or `right`.
  See [STATUS.md](../STATUS.md) for the 31 available segment names.
- Per-state colours: `[segment.vcs.states.dirty]`,
  `[segment.prompt_char.states.error]`, etc.
- Truecolor: `foreground = "#ff6600"` or `foreground = "#f60"`
  (CSS shorthand) or `foreground = [255, 102, 0]` (RGB array).
- Transient prompt mode: `transient_prompt = "off" | "always" |
  "same-dir" | "unique-dir"`.

## Contributing a theme

Open a PR against this directory with one `.toml` file plus an entry
in this README. Lint locally with:

```bash
p10k-rs config check --config themes/your-theme.toml
```

That'll parse and schema-validate without rendering.

## Validation

Every bundled theme is parsed at build time as a unit test (see
`crates/p10k-rs/src/themes.rs`). A theme that doesn't deserialise
cleanly fails CI — so anything in this directory is guaranteed to
load.
