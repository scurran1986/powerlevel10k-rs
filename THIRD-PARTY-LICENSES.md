# Third-Party Licenses

## gitstatusd

`p10k-rs` bundles unmodified `gitstatusd` binaries from the upstream
[romkatv/gitstatus](https://github.com/romkatv/gitstatus) project (pinned
tag v1.5.4) as part of release artifacts.

**License:** GPL-3.0-or-later

**Bundling:** The architecture (subprocess + stdin/stdout IPC pipes) is
arms-length communication between independent programs, not a derivative work.
Per GPL § 0, bundling binaries is "mere aggregation," not a copyleft trigger
on `p10k-rs` itself. The `p10k-rs` codebase remains MIT/Apache-2.0; the
bundled `gitstatusd` retains GPL-3.0.

**Obligations:**

1. Include `gitstatusd`'s GPL-3.0 license file alongside its binary in
   release artifacts.
2. Provide a written source-code offer per GPL § 6: a pointer to
   [github.com/romkatv/gitstatus](https://github.com/romkatv/gitstatus) at
   the pinned tag is sufficient.
3. Preserve upstream copyright notice: `gitstatusd --version` output in
   release documentation serves this purpose.

**Source availability:** The full source for the bundled version is available
at https://github.com/romkatv/gitstatus/releases/tag/v1.5.4.

---

## MesloLGS NF (optional, install-time download)

`install.sh` (default-on; opt out with `--no-fonts`) downloads four
**MesloLGS NF** font files from
[romkatv/powerlevel10k-media](https://github.com/romkatv/powerlevel10k-media)
at a pinned commit (`145eb9fbc2f42ee408dacd9b22d8e6e0e553f83d`), each
verified against an in-tree sha256. The files are written to
`~/.local/share/fonts/p10k-rs/` on Linux or `~/Library/Fonts/` on macOS.
We do **not** redistribute the font files in this repository or in release
artifacts; the installer fetches them from upstream at install time.

**Underlying components and licenses:**

- **Meslo LG (base font):** Apache-2.0, by André Berg. A modification of
  Apple's Menlo, itself derived from Bitstream Vera Sans Mono.
  See https://github.com/andreberg/Meslo-Font.
- **Nerd Fonts patches (icon glyphs from Font Awesome, Material Design
  Icons, Devicons, Octicons, etc.):** MIT, by Ryan L. McIntyre.
  See https://github.com/ryanoasis/nerd-fonts.
- **MesloLGS NF combination + repackaging:** distributed by Roman
  Perepelitsa in
  [romkatv/powerlevel10k-media](https://github.com/romkatv/powerlevel10k-media).
  The repository ships no LICENSE file; the binary fonts inherit the
  upstream Meslo LG (Apache-2.0) and Nerd Fonts (MIT) licenses on their
  respective contributions. Upstream Powerlevel10k installs the same
  files via its `configure` wizard on iTerm2 / Termux, so the
  redistribution-by-fetch pattern is consensus-accepted in the
  ecosystem.

**Obligations on the user:** None for personal use. Bundling these
fonts into a redistributed product would require carrying both upstream
licenses with the bundle; we sidestep that by never bundling, only
fetching at user-initiated install.

**Skipping the download:** `./install.sh --no-fonts` skips the font
install entirely. On WSL it is auto-skipped (the Linux-side font dir is
invisible to the Windows-side terminal); see the printed instructions
for the manual Windows install.

---

*This is the consensus open-source reading and not legal advice. For questions
or commercial use, consult an attorney familiar with GPL and font licensing.*
