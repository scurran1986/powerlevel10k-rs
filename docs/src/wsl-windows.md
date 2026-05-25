# WSL2 + Windows Terminal

If you run `p10k-rs` inside a WSL2 distro with Windows Terminal as the
emulator, segment icons (user, folder, branch glyphs) can render as `◆`
placeholder diamonds even though the Powerline chevrons between
segments (`▶`) render fine. This page is the fix.

## The symptom

The diagnostic signature is **chevrons render, icons don't**:

- `▶` between segments — correct.
- `◆` (a hollow/filled diamond) where the folder, branch, or user icon
  should be — wrong. The terminal is drawing the Unicode replacement
  glyph because it can't find the codepoint in any installed font.

If both chevrons *and* icons are broken, you have a different problem
(no Powerline-aware font at all). This page targets the
chevrons-yes-icons-no case specifically.

## Why it happens

Windows Terminal is a Windows process. WSL2 runs your shell inside a
Linux VM, but the rendering is done by the Windows-side terminal,
which only sees fonts installed on Windows. Fonts you have installed
*inside* the WSL distro are invisible to it.

Most "Powerline" fonts that ship on Windows by default include the
Powerline private-use range (`U+E0A0`–`U+E0BF`) — that's why chevrons
work — but **not** the broader Nerd Font icon set
(`U+E5FA`–`U+F2FF` and friends) that `p10k-rs` segments use for
folder, branch, language, and host glyphs.

Installing the font inside WSL does nothing. The font has to live on
the Windows side.

## The fix

Four steps. Five minutes.

### 1. Download MesloLGS NF

Grab the four `.ttf` files from the upstream Powerlevel10k media repo:

<https://github.com/romkatv/powerlevel10k-media>

- `MesloLGS NF Regular.ttf`
- `MesloLGS NF Bold.ttf`
- `MesloLGS NF Italic.ttf`
- `MesloLGS NF Bold Italic.ttf`

Download them to the Windows side (not inside WSL).

### 2. Install on Windows

Right-click each `.ttf` in Windows Explorer and pick
**Install for all users**. Per-user install also works; system-wide
avoids surprises if multiple Windows accounts hit the same WSL distro.

### 3. Point Windows Terminal at the font

Open Windows Terminal → **Settings** → **Open JSON file**. Add or
edit the `defaults` profile font face:

```json
"profiles": {
  "defaults": {
    "font": { "face": "MesloLGS NF" }
  }
}
```

Save. The GUI settings panel also exposes this under
**Profiles → Defaults → Appearance → Font face**, but editing
`settings.json` is faster and survives terminal upgrades cleanly.

### 4. Restart Windows Terminal

Close every Windows Terminal window. Reopen. The icons should now
render.

## Alternative Nerd Fonts

MesloLGS NF is the path of least resistance because it's the font the
upstream prompt and most screenshots assume, but any **Nerd Font v3**
(or later) covers the same codepoint range:

- FiraCode Nerd Font
- JetBrainsMono Nerd Font
- Hack Nerd Font

Earlier Nerd Font releases (v2.x) ship a narrower icon set and may
still leave some `p10k-rs` glyphs as `◆`. If you're picking a font in
2026, pick v3.

## Verification

Render the prompt once without sourcing it into your shell:

```bash
p10k-rs prompt --shell zsh
```

Folder and branch icons should render properly. If you still see `◆`,
double-check that Windows Terminal actually picked up the font (the
font name in `settings.json` must match the family name exactly —
"MesloLGS NF", not "Meslo LGS NF" or "MesloLGS Nerd Font").

> A future `p10k-rs doctor` subcommand (v0.2) will auto-detect the
> WSL + Windows Terminal combination and warn when icons in the
> active config aren't covered by any reachable font. Until then,
> verification is manual.

## Other terminals on Windows

Same fix pattern, different settings location:

| Terminal | Where to set the font |
|---|---|
| Alacritty | `font.normal.family` in `alacritty.toml` |
| WezTerm | `font = wezterm.font("MesloLGS NF")` in `~/.wezterm.lua` |
| VS Code integrated terminal | `terminal.integrated.fontFamily` in `settings.json` |

The font still has to be installed on Windows in all three cases for
the same reason — these are Windows processes rendering a Linux
shell's output.

## Linux and macOS users

Not relevant. Native Linux and macOS terminals load fonts from the
same OS the shell runs on, so installing the font once (via your
package manager, Homebrew cask, or `~/.local/share/fonts`) is enough.
