# powerlevel10k-rs — Self-Audit (Phase 5)

**Audit date:** 2026-05-30
**Codebase HEAD:** `aef6a0b` (`test(config): freeze TOML schema via insta snapshot`)
**Scope:** the full workspace under `crates/`, the release pipeline in
`.github/workflows/release.yml`, the shell init scripts under
`crates/p10k-rs-shell/shells/`, and the install scripts at the repo root.
**Auditor:** Claude Opus 4.7, maintainer-supervised. **This is a
self-audit per the v1.0 plan Phase 5, not a paid third-party audit.**
Where a finding cites a file:line, the citation was verified against
the working tree at HEAD `aef6a0b`. Where a finding admits it cannot
be verified from code alone, it says so.

## Executive summary

For the **render path** (A1 + A5), `p10k-rs` ships a strong
`SafeText`-chokepoint design with C0/C1/DEL stripping, BiDi /
zero-width / tag-char / variation-selector filtering, NFC
normalisation, and zsh-side `%` doubling plus `$` / backtick /
backslash escaping (T1.12 / slice γ guard against
CVE-2021-45444-style `PROMPT_SUBST` RCE). For the **filesystem /
IPC** path (A2 + A3), gitstatusd is gated by `open_owned_safely`
(T1.14), FIFO opens are gated by `open_fifo_safely` (S_IFIFO
re-check on the fd, owner check), the instant-prompt dump is written
`O_NOFOLLOW | O_CREAT | O_EXCL` at mode 0600 and the zsh init refuses
to source it unless `EUID` and mode 0600 match, and per-shell FIFO
directories are created with `mktemp -d` under `$XDG_RUNTIME_DIR`
with `umask 077`. For the **supply chain** (A4), the release pipeline
is already SLSA-L3-shaped: cosign keyless `sign-blob` plus
`actions/attest-build-provenance` plus a SHA-pinned third-party
Actions table plus a `cargo-deny` advisories/licenses/bans/sources
gate, plus a `cargo-semver-checks` gate, plus an `insta` snapshot
freeze of the TOML config schema, plus a sigstore-pinned + sha256-pinned
`gitstatusd` (T0.5). `#![forbid(unsafe_code)]` is in every workspace
crate except `p10k-rs-git`, which carries the unsafe budget but ships
zero `unsafe` blocks today.

**Residual gaps** (as of HEAD `aef6a0b`; some are in flight on
sibling lanes of this swarm):

1. `crates/p10k-rs-segments/src/context.rs` reads `$USER`,
   `$LOGNAME`, `$HOSTNAME`, `$P10K_RS_DEFAULT_USER`, and
   `$COMPUTERNAME` via raw `std::env::var(...)`; sanitisation is
   applied inside the segment via `sanitize_for_terminal` (lines
   175 / 185 / 194 / 199 / 206), so the rendered output is safe,
   but the `String`-returning helpers above the sanitiser are
   the "type-enforced SafeText at the trait boundary" gap (slice
   β in THREAT-MODEL.md).
2. No `O_CLOEXEC` on FIFO / instant-prompt-dump fd opens; a
   `posix_spawn` from a future helper would inherit them.
3. The instant-prompt dump's *parent directory* (`$XDG_CACHE_HOME/p10k-rs/`)
   is `create_dir_all`'d but not owner-checked before writing
   (the file *itself* is `O_NOFOLLOW | O_CREAT | O_EXCL` + 0600, so
   pre-plant of the dump file is defeated; pre-plant of a hostile
   parent dir is not.)
4. The instant-prompt dump has no content signature; the zsh-side
   gate trusts mode + owner. A same-uid attacker who can write to
   the dump file (e.g. via an unrelated process bug) still wins.
   Deferred — needs coordinated Rust + zsh change.
5. ~10 segments read env vars into raw `String`; sanitisation is
   applied before render, but is not type-enforced at the
   `Segment` trait boundary (slice β).

Lane A of the v1.0 swarm may close items 1–3 before this audit
ships. If it does, the per-attacker docs below remain accurate for
state-at-the-commit-this-audit-landed; the next audit cadence will
need to re-walk.

## Documents

| # | Attacker model | Doc |
|---|---|---|
| A1 | Malicious repo author | [`a1-malicious-repo.md`](a1-malicious-repo.md) |
| A2 | Local same-user attacker | [`a2-same-user.md`](a2-same-user.md) |
| A3 | Local different-user attacker | [`a3-different-user.md`](a3-different-user.md) |
| A4 | Supply-chain attacker | [`a4-supply-chain.md`](a4-supply-chain.md) |
| A5 | Terminal-aware attacker | [`a5-terminal-aware.md`](a5-terminal-aware.md) |

## How this audit is scoped

- **In scope:** the workspace at HEAD `aef6a0b`, the release workflow,
  the four shell init scripts, the install scripts, the cargo-deny
  config, the cargo-semver-checks + insta gates.
- **Out of scope** (per `SECURITY.md` → "Out of scope"): attacks
  requiring local root, hardware fault injection, compromise of
  Fulcio / Rekor / rustc / crates.io trust roots, denial of service
  in the prompt render path that does not affect the parent shell.
- **Not exercised in this audit:** dynamic fuzzing (planned for
  Phase 4 of the v1.0 plan), runtime sandbox (`seccomp` /
  `landlock`; out of scope for v1.0), Windows ACL probing on the
  `open_owned_safely` path (open issue tracked against v0.2.4
  release notes).

## Citations format

Every "done" claim cites `path/to/file.rs:LINE` against HEAD `aef6a0b`.
Every "partial" or "open" claim cites the closest verified file:line
that surfaces the residual gap. If a claim has no code citation, it
is labelled `(no-code-citation)` and the auditor's reasoning is
inline.
