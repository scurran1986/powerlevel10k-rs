# Windows status

v0.2.4 ships Unix-only. The codebase type-checks clean against
`x86_64-pc-windows-msvc` after a three-phase Unix-API audit, but no
Windows binary has been released yet. Phase 4 — the PowerShell installer,
re-enabling the release matrix, and a first Windows tag run — is the
remaining work. This page covers what works now, what doesn't, and how it
gets there.

## Feature status

| Feature | Unix | Windows |
|---|---|---|
| Prompt rendering | ✅ | ✅ |
| Git status via gitstatusd daemon | ✅ | ❌ — FIFO IPC; falls back to ShellOut |
| Git status via ShellOut | ✅ | ✅ |
| Git status via gix backend | ✅ | ✅ |
| `dir` writable probe | ✅ `access(W_OK)` | ⚠️ `metadata().permissions().readonly()` — coarse |
| `context` privilege awareness | ✅ euid check | ❌ — no euid equivalent; UAC is a different model |
| `root_indicator` segment | ✅ | ❌ — same reason as above |
| DECSET 2026 synchronized output probe | ✅ | ❌ |
| OSC 4 truecolor palette probe | ✅ | ❌ |
| `terminal_width` ioctl probe | ✅ | ❌ — falls through to `$COLUMNS` or 80 |
| Config file TOCTOU safety (`open_owned_safely`) | ✅ | ⚠️ — reduced to plain `File::open`; ACL probe is a future slice |

## Why this state

`p10k-rs-core` historically depended unconditionally on
`rustix::termios`, `std::os::fd::AsFd`, and `rustix::process::geteuid`
— all Unix-only APIs. Phases 1–3 of the portability milestone audited
every call site and wrapped them behind `#[cfg(unix)]` / `#[cfg(windows)]`
guards, so the workspace now cross-compiles without errors. Phase 4
(installer + release-matrix re-enable + first tag run) ships the actual
binary.

## Building from source today

If you want to try the prompt on Windows now:

```pwsh
rustup target add x86_64-pc-windows-msvc
cargo build --release --workspace --target x86_64-pc-windows-msvc
```

The binary at `target\x86_64-pc-windows-msvc\release\p10k-rs.exe` will
render prompts. The daemon-fast git path and terminal-capability probes
won't work (see the table above), but the prompt itself renders correctly.

## Roadmap to a shipped Windows binary

Phase 4 items, in order:

1. **`install.ps1`** — PowerShell installer parallel to the existing
   `install.sh`
2. **Re-enable the release matrix** — the `x86_64-pc-windows-msvc` entries
   in `.github/workflows/release.yml` are scaffolded but inert; a one-line
   matrix edit re-enables them
3. **First Windows tag run** — validates `Compress-Archive`,
   `Get-FileHash`, and sigstore signing on `windows-latest`

No dates. Phase 4 lands when it lands.

## See also

- [WSL2 + Windows Terminal](./wsl-windows.md) — recommended path until
  the native Windows binary ships
- [doctor subcommand](./reference/doctor.md) — runtime self-check that
  reports which backends and probes are available
- [Troubleshooting](./troubleshooting.md)
