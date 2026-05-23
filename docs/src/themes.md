# Bundled themes

`p10k-rs` ships ten ready-made prompt themes. Each is a small TOML
file (~30-60 lines) demonstrating a useful subset of the config
schema. Use a theme as a starting point, then edit
`~/.config/p10k-rs/config.toml` to taste.

## Quick tour

```bash
# See the catalogue with one-line descriptions
p10k-rs theme list

# Inspect a theme's TOML without installing
p10k-rs theme show catppuccin-mocha

# Install — copies the theme to ~/.config/p10k-rs/config.toml.
# An existing config is saved as ~/.config/p10k-rs/config.toml.bak first.
p10k-rs theme install catppuccin-mocha

# Force overwrite without a backup
p10k-rs theme install rainbow --force
```

Reload zsh (`exec zsh`, or just open a new terminal) to see the
change.

## Catalogue

| Name | One-liner |
|---|---|
| `lean` | Single-line minimal. `follow_terminal` colours, `compatible` glyphs. Best in dumb terminals. |
| `classic` | Two-line with chevron frame. `ansi256` colours. Balanced default. |
| `rainbow` | Powerline ribbons with chip backgrounds. `true-color` + Nerd Font v3. Upstream P10K rainbow look. |
| `pure` | sindresorhus/pure-zsh inspired. Two-line, no frame, no chips. |
| `catppuccin-mocha` | [Catppuccin Mocha](https://github.com/catppuccin/catppuccin) pastel palette. |
| `dracula` | [Dracula](https://draculatheme.com/) high-contrast purple. |
| `gruvbox-dark` | [Gruvbox Dark](https://github.com/morhetz/gruvbox) earthy retro. |
| `nord` | [Nord](https://www.nordtheme.com/) arctic blues + greys. |
| `solarized-dark` | [Solarized Dark](https://ethanschoonover.com/solarized/) Ethan Schoonover classic. |
| `tokyo-night` | [Tokyo Night](https://github.com/enkia/tokyo-night-vscode-theme) cool nightscape. |

Themes tagged `nerd-font-v3` assume your terminal renders a Nerd
Font. If you don't have one yet, [install one](https://www.nerdfonts.com/),
or pick a `compatible`-mode theme — those only use box-drawing
characters from the basic Unicode plane.

## How a theme is structured

A theme is a regular `config.toml`. The smallest possible one looks
like:

```toml
schema_version = 1
mode = "compatible"           # ascii / awesome / nerd-font-v2 / nerd-font-v3 / compatible
colors = "ansi256"            # ansi8 / ansi256 / true-color / follow_terminal
transient_prompt = "same-dir" # off / always / same-dir / unique-dir

[layout]
left = ["dir", "vcs", "prompt_char"]
right = ["status", "time"]

[segment.dir]
foreground = "blue"

[segment.vcs.states.dirty]
foreground = "yellow"
```

The full schema lives at [config/](config/index.md); per-state
override blocks, frame / ruler glyphs, truecolor hex literals, and
the rest are documented there.

## Bundling vs. on-disk

Themes are committed at [`themes/`](https://github.com/scurran1986/powerlevel10k-rs/tree/main/themes)
**and** embedded into the binary at build time via
`include_str!`. So:

- A `cargo install`d build can `p10k-rs theme install <name>`
  without the repo on disk.
- A clone of the repo can also `cp themes/<name>.toml
  ~/.config/p10k-rs/config.toml` manually if preferred.

The two paths produce byte-identical output.

## Validation

Every bundled theme is parsed at test time via `Config::from_toml`
(see the `every_bundled_theme_parses` unit test in
`crates/p10k-rs/src/themes.rs`). A theme that doesn't deserialise
cleanly fails CI — so anything published in this catalogue is
guaranteed to load.

## Contributing a theme

Open a PR with one new `themes/<name>.toml` plus an entry in:

1. `themes/README.md` catalogue table.
2. The `THEMES` const in `crates/p10k-rs/src/themes.rs`.
3. The `catalogue_count` test in the same file.
4. The catalogue table in this chapter.

Lint locally before pushing:

```bash
p10k-rs config check --config themes/your-theme.toml
```

That parses and schema-validates without rendering.
