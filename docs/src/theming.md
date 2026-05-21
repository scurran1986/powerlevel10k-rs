# Theming and layout

The prompt is a powerline ribbon: each segment renders into its own
coloured cell, separated by glyphs that the next segment's background
absorbs. The ribbon shape lives in `[layout]` — `left` and `right`
order the cells, `separators.left` / `separators.right` /
`separators.subsegment` choose the join glyphs. All three glyph fields
pass through `sanitize_for_terminal` at parse time, so control bytes
like `\r` or `\x1b` are stripped before the renderer sees them.

Two decorative wrappers sit around the ribbon: `[layout.frame]` draws
corner glyphs around the prompt (default `╰─` at the bottom-left of the
prompt-char line), and `[layout.ruler]` draws a horizontal divider above
the prompt. Both accept a `glyph` and `foreground` colour. Per-segment
styling rides on top of that — `[segment.<name>]` carries `foreground`,
`background`, `icon`, `padding`, and `marker_foreground`, and
`[segment.<name>.states.<tag>]` overrides those when a segment tags its
output with that state (the canonical example is `vcs.states.dirty`
painting the branch name red when the working tree is dirty).
`marker_foreground` (slice 59) paints the `vcs` index markers
(`* ! + ~ ? ≡`) independently from the branch text:

```toml
[segment.vcs]
foreground = "green"
marker_foreground = "yellow"   # markers use yellow; branch text uses green
```
