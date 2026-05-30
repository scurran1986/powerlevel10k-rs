# Upgrading from v0.1 to v0.4

This page covers upgrading an existing `p10k-rs` install across the v0.1 →
v0.4 line. The short version: there are **no breaking config changes** in
the 0.x line so far. Pull the new binary, re-source the init script,
verify with `p10k-rs doctor` — done.

> **About v1.0.** v1.0 is intentionally still a future tag. The
> [ROADMAP](https://github.com/scurran1986/powerlevel10k-rs/blob/main/STATUS.md)
> requires a ≥ 6-month soak on v0.3 / v0.4 before the SemVer freeze, so
> "v0.1 → v1.0" doesn't exist yet. The realistic migration today is
> v0.1 → v0.4.

## TL;DR

```sh
# 1. install the new binary (one of these, per your packaging channel)
cargo install p10k-rs            # crates.io
brew upgrade p10k-rs              # Homebrew
scoop update p10k-rs              # Scoop
# or download the signed release tarball from the Releases page

# 2. rewrite the init script in your shell rc with the new binary path
eval "$(p10k-rs init zsh)"        # or bash / fish / pwsh / nu

# 3. confirm everything still works
p10k-rs doctor
p10k-rs daemon-health
p10k-rs verify
```

## Config compatibility

`schema_version = 1` is the only schema the 0.x line has ever shipped.
Every config that loads on v0.1 still loads unchanged on v0.4 —
verified by the `every_bundled_theme_parses` round-trip in
`crates/p10k-rs-config/src/lib.rs`. New fields are additive and
optional; defaults match the v0.1 behaviour.

If `p10k-rs doctor` reports `config_file_parses: ERROR` after the
upgrade, the message comes straight from the schema parser — see the
[full schema reference](./reference/schema.md).

## What's new since v0.1

### New shells

| Shell | Landed | Activation |
|---|---|---|
| `pwsh` (PowerShell 7+ / 5.1) | v0.3.0 | `& p10k-rs init pwsh \| Invoke-Expression` |
| `nu` (Nushell 0.97+) | v0.3.x | two-step save-then-source (see below) |

Nushell can't `eval` a piped string the way pwsh's `Invoke-Expression`
does — its parser is static, so activation is a two-step save:

```nu
mkdir ($nu.data-dir | path join vendor autoload)
p10k-rs init nu | save -f ($nu.data-dir | path join vendor autoload p10k-rs.nu)
```

Per-shell parity table and reductions live in
[Per-shell init](./reference/shell.md).

### bash transient prompt (partial)

v0.3.x wires the transient-prompt contract through bash. All four
`TransientPromptMode` variants (`off` / `always` / `same-dir` /
`unique-dir`) are gated by the binary the same way they are in zsh and
fish.

**Caveat:** bash exposes no reliable prompt-height count, so the cursor
redraw only collapses **single-row** prompts with single-line input.
Multi-row or soft-wrapped prompts decline to collapse and keep the full
ribbon in scrollback — the same visible outcome as a KeepPrompt fall-
through. The contract is honoured; only the terminal redraw is
conservative. See the [Per-shell init](./reference/shell.md) parity
table.

### New segment-visibility gate: `show_on_upglob`

Joins `show_on_command` and `show_in_dir` as the third show-* gate.
Walks the cwd's ancestor directories (lexical, symlink-cycle-immune)
and shows the segment when any ancestor entry's basename matches a
glob:

```toml
[segment.node_version]
show_on_upglob = ["package.json", "*.lock"]
```

### New diagnostics CLI

| Subcommand | Landed | Reference |
|---|---|---|
| `p10k-rs doctor` | v0.2.3 | [doctor](./reference/doctor.md) |
| `p10k-rs daemon-health` | v0.1.8 | [daemon-health](./reference/daemon-health.md) |
| `p10k-rs verify` | v0.1.5 | [SECURITY.md](https://github.com/scurran1986/powerlevel10k-rs/blob/main/SECURITY.md) |
| `p10k-rs version --json` | v0.2.1 | — |
| `p10k-rs prompt --json` | v0.2.2 | [prompt --json schema](./reference/prompt-json.md) |

All five accept `--json` and emit a stable wire format. They're the
first things to reach for after an upgrade — or when filing a bug.

### Supply chain + platform

- **Windows port closed** at v0.2.6. `install.ps1` ships; the release
  pipeline produces signed `x86_64-pc-windows-msvc` and
  `aarch64-pc-windows-msvc` zips. Per-feature reductions are tabled at
  [Windows status](./windows.md).
- **Sigstore-signed releases.** Every release artifact has a
  `*.cosign.bundle` sidecar and a `*.sha256`. SLSA build-provenance
  attestations are attached to the GitHub Release. See
  [SECURITY.md](https://github.com/scurran1986/powerlevel10k-rs/blob/main/SECURITY.md)
  for the verify recipe.
- **`#![forbid(unsafe_code)]` on every crate.** The whole workspace
  compiles with zero `unsafe` blocks — the historical "git crate has
  the unsafe budget" carve-out is gone.

### Internal change segment authors should know about

`RenderCtx::sync_output` is now a producer-set field (the binary fills
it from the `term_caps` probe) rather than a process-global lookup.
End users feel nothing; out-of-tree code that builds against
`p10k-rs-core` and constructs a `RenderCtx` directly needs to populate
the new field. There is no published plugin API yet — see "Caveats"
below.

## Upgrade steps in detail

1. **Pull the new binary.** Whatever channel you originally installed
   from. `cargo install p10k-rs` upgrades from crates.io; `brew
   upgrade`, `scoop update`, `pacman -Syu`, or `nix flake update` cover
   the others. The signed release tarball is the fallback.

2. **Re-source the init script.** `p10k-rs init <shell>` bakes the
   absolute path of the emitting binary into the script, so a binary
   move (or a packaging-channel switch) needs a re-source to pick up
   the new path. If your rc file already runs
   `eval "$(p10k-rs init zsh)"` on every shell start, you're done.

3. **Verify.** Three commands cover everything that historically goes
   wrong on upgrade:

   ```sh
   p10k-rs doctor          # nine probes against fresh-install snags
   p10k-rs daemon-health   # gitstatusd daemon state (zsh only)
   p10k-rs verify          # gitstatusd sha256 matches the bundled pin
   ```

## Caveats

- **Plugin API: not shipped, deferred past v1.0.** There is no public
  plugin surface today. The `Segment` trait uses `&'static str` and
  isn't exported as a stable extension point. v1.0 will ship without
  a frozen plugin API; it becomes an additive 1.x feature once the
  surface area is right. No 1.0 break for plugin authors — there's
  nothing to break.
- **bash transient: single-row only** (see above).
- **Nushell init unverified on a real Nushell host** as of v0.3.x —
  the script passes byte-level test pins, but interactive behaviour on
  a real Nushell session is the open validation item.

## Rollback

Tagged releases are immutable. Reinstall any prior tag:

```sh
cargo install p10k-rs --version 0.1.10
# or download the prior release tarball + verify the sigstore bundle
```

…then re-source the init script. Configs are forward- and
backward-compatible across the 0.x line, so no config edit is needed
to roll back.
