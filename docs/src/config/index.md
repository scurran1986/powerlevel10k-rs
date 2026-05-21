# Configuration

Drop a TOML file at `~/.config/p10k-rs/config.toml`, or point
`$P10K_RS_CONFIG` at one. Discovery order:

1. `$P10K_RS_CONFIG` if set.
2. `$XDG_CONFIG_HOME/p10k-rs/config.toml`.
3. `$HOME/.config/p10k-rs/config.toml`.

A missing or broken file falls back silently to the factory default
(byte-identical to no-config behaviour).

`[layout].left` picks which segments render and in what order.
`[segment.<name>]` overrides per-segment foreground / background under
the active `ColorMode`. State-specific overrides — e.g.
`[segment.vcs.states.dirty]` — fire when the segment tags its output
with that state.

```toml
schema_version = 1

[layout]
left = ["dir", "vcs", "command_execution_time", "status", "prompt_char"]

# Colour the cwd in red instead of the default blue.
[segment.dir]
foreground = "red"

# Magenta branch name when the working tree is dirty;
# yellow otherwise (the default).
[segment.vcs.states.dirty]
foreground = "magenta"
```

Colour values: a Powerlevel9k-style name (`"blue"`, `"brightred"`, …),
an ANSI 256 index (`0`–`255`), an `[r, g, b]` triple for truecolor, or
a hex literal (`"#rrggbb"` / `"#rgb"` shorthand) that expands per CSS
convention (`"#f60"` ≡ `"#ff6600"`). Hex literals require `colors =
"true-color"` to reach the terminal as RGB; on `ansi256` they are
quantised to the nearest palette entry.

```toml
# All four forms are equivalent for orange:
foreground = "orange"           # named
foreground = 214                # ANSI 256 index
foreground = [255, 175, 0]      # [r, g, b] triple
foreground = "#ffaf00"          # hex literal (T1.24)
```

See the [full schema reference](../reference/schema.md) for every
recognised field and the [segments catalogue](../segments/index.md) for
the per-segment knobs.
