# Schema (full)

Source of truth:
[`crates/p10k-rs-config/src/lib.rs`][src]. The struct field rustdoc is
the canonical spec; this page is a flat reference. `deny_unknown_fields`
is set everywhere — typo'd keys surface a parse error rather than
silently doing nothing.

## Top level — `Config`

| Field | Type | Description |
|---|---|---|
| `schema_version` | `u32` | Schema version. Currently `1`. |
| `mode` | `Mode` | Glyph mode (default `nerd-font-v3`). |
| `colors` | `ColorMode` | Color emission mode (default `ansi256`). |
| `layout` | `Layout` | Left/right segment lists, frame, ruler, separators. |
| `instant_prompt` | `InstantPromptMode` | Instant-prompt behaviour (default `verbose`). |
| `transient_prompt` | `TransientPromptMode` | Transient-prompt behaviour (default `off`). |
| `segment` | `HashMap<String, SegmentConfig>` | Per-segment config, keyed by name. Serialised as `[segment.<name>]`. |
| `ai` | `AiConfig` | AI integration toggles. |

## `Mode`

`ascii` &middot; `awesome` &middot; `nerd-font-v2` &middot;
`nerd-font-v3` (default) &middot; `compatible`.

## `ColorMode`

`ansi8` &middot; `ansi256` (default) &middot; `true-color`.

## `Layout`

| Field | Type | Description |
|---|---|---|
| `left` | `Vec<SegmentRef>` | Ordered list of segments on the left. |
| `right` | `Vec<SegmentRef>` | Ordered list of segments on the right. |
| `left_top_only` | `Vec<SegmentRef>` | Subset of `left` pinned to line 1 of a multi-line prompt; the rest fall to line 2. Honoured only when the frame is active. |
| `right_top_only` | `Vec<SegmentRef>` | Symmetric reservation for the right prompt; currently inert (RPROMPT is single-line). |
| `frame` | `Option<FrameStyle>` | Decorative frame around the prompt. |
| `ruler` | `Option<RulerStyle>` | Horizontal divider above the prompt. |
| `separators` | `Separators` | Glyphs that join segments and subsegments. |

## `Separators`

| Field | Type | Description |
|---|---|---|
| `left` | `Option<String>` | Glyph between segments on the left side. |
| `right` | `Option<String>` | Glyph between segments on the right side. |
| `subsegment` | `Option<String>` | Glyph between subsegments inside one segment. |

## `FrameStyle`

| Field | Type | Description |
|---|---|---|
| `glyph` | `Option<String>` | Frame glyph (sanitised at parse). |
| `foreground` | `Option<Color>` | Frame foreground colour. |
| `bottom_glyph` | `Option<String>` | Bottom-left frame glyph on the prompt-char line. Default `╰─`. |

## `RulerStyle`

| Field | Type | Description |
|---|---|---|
| `glyph` | `Option<String>` | Ruler glyph (sanitised at parse). |
| `foreground` | `Option<Color>` | Ruler foreground colour. |

## `SegmentConfig` — `[segment.<name>]`

| Field | Type | Description |
|---|---|---|
| `disabled` | `bool` | Skip the segment entirely. |
| `foreground` | `Option<Color>` | Foreground colour. |
| `background` | `Option<Color>` | Background colour. |
| `marker_foreground` | `Option<Color>` | Foreground colour for index markers (`* ! + ~ ? ≡`) on the `vcs` segment (slice 59). Paints markers independently from the branch text. Ignored by other segments. |
| `icon` | `Option<String>` | Override the default icon glyph (sanitised at parse). |
| `padding` | `Padding` | Whitespace cells on either side. |
| `truncate` | `DirTruncate` | Cwd truncation policy (only the `dir` segment reads it). |
| `show_on_command` | `Option<Vec<String>>` | Render only when the upcoming command's first word matches one of these. Zsh-only today (`line-pre-redraw` widget); other shells get the last-command approximation per the zsh-init comment. |
| `show_in_dir` | `Option<Vec<Glob>>` | Render only when the cwd matches one of these globs. |
| `show_on_upglob` | `Option<Vec<Glob>>` | Render only when a file or directory matching one of these globs exists in the cwd or any ancestor directory (walking upward toward the filesystem root). Classic use: gate a language segment to its project tree, e.g. `["package.json"]`. |
| `disabled_dir_pattern` | `Option<Glob>` | Disable the segment when the cwd matches this glob. |
| `states` | `HashMap<String, StateOverrides>` | Per-state overrides keyed by segment-defined state tag. |

## `StateOverrides` — `[segment.<name>.states.<tag>]`

| Field | Type | Description |
|---|---|---|
| `foreground` | `Option<Color>` | Foreground for this state. |
| `background` | `Option<Color>` | Background for this state. |
| `icon` | `Option<String>` | Icon override for this state (sanitised at parse). |

## `Padding`

| Field | Type | Description |
|---|---|---|
| `left` | `u8` | Whitespace cells to the left. |
| `right` | `u8` | Whitespace cells to the right. |

## `DirTruncate`

| Field | Type | Description |
|---|---|---|
| `strategy` | `DirTruncateStrategy` | `none` (default), `to_last`, `middle`, `to_unique`. |
| `length` | `u8` | Trailing components to keep (default `3`; `0` treated as `1`). |

`to_unique` is opt-in only — it issues one `read_dir` per non-final
component and is meaningfully more expensive on slow filesystems.

## `Color`

Untagged enum — TOML accepts four shapes:

| Shape | Example | Meaning |
|---|---|---|
| String | `"blue"`, `"brightred"`, `"wheat4"` | Powerlevel9k-style name (16 P9k-compat names land). |
| Integer | `33`, `196` | ANSI 256 index (0–255). |
| Array | `[120, 200, 0]` | Truecolor `[r, g, b]`. |
| Hex string | `"#ff6600"`, `"#f60"` | Truecolor hex literal (T1.24). Shorthand expands per CSS convention. `#`-prefixed strings that are not valid 3- or 6-digit hex are a parse error. |

## `InstantPromptMode`

`off` &middot; `quiet` &middot; `verbose` (default).

## `TransientPromptMode`

| Value | Behaviour |
|---|---|
| `off` | Transient prompt disabled. Every prompt stays at full width in scrollback. Default. |
| `always` | Every accepted command collapses the preceding prompt to a single chevron. |
| `same-dir` | Collapse only when the next prompt is in the same directory as the previous one. Full ribbon is kept in scrollback when you `cd`. Uses an exit-code-2 wire protocol between the binary and the zsh init (T1.8). |
| `unique-dir` | Collapse the previous prompt iff its cwd appears in `{cwd} ∪ history`, where `history` is the cwds of all prompts strictly older than the immediate previous prompt in this shell session. The zsh init maintains a per-shell NUL-separated history file inside the slice-9 mktemp dir and forwards it via `--prompt-cwd-history-file`. Approximation of the strict "collapse all but the most recent prompt at each unique directory" rule — the strict version needs per-prompt-line-position scrollback rewriting (multi-slice ZLE engineering, deferred). The approximation collapses strictly more than `same-dir` (every same-dir match plus every revisit) without incorrectly collapsing a prompt at a truly-unique cwd. |

## `AiConfig` — `[ai]`

| Field | Type | Description |
|---|---|---|
| `osc7` | `bool` | Emit OSC 7 (current working directory) sequences. |
| `osc133` | `bool` | Emit OSC 133 (semantic prompt) sequences. |
| `host` | `HashMap<String, HostConfig>` | Per-host opt-in. Serialised as `[ai.host.<id>]`. |
| `model` | `Option<String>` | Active model name (schema-only, slice 61). Accepted and stored; the AI statusline render path is still a stub — this field has no visible effect until that ships. |
| `context_tokens` | `Option<u32>` | Context window size hint in tokens (schema-only, slice 61). Same stub caveat as `model`. |

## `HostConfig` — `[ai.host.<id>]`

| Field | Type | Description |
|---|---|---|
| `enabled` | `bool` | Enable status JSON ingestion for this host. |

## `SegmentRef`, `Glob`

Both are transparent `String` newtypes. `SegmentRef` stays a newtype so
future syntax like `"vcs?max=3"` lands without a breaking change; `Glob`
is validated lazily by the segments that read it.

[src]: https://github.com/scurran1986/powerlevel10k-rs/blob/main/crates/p10k-rs-config/src/lib.rs
