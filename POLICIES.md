# Policies

How this project is maintained, who's behind the code, what
the support story is, and the trademark and license notices.
Read this **before** depending on `p10k-rs` for anything that
matters.

## TL;DR

- One-person hobby project. No SLA. No support. No warranty.
- Heavy AI-assisted development ("vibe coded"). Bugs may be
  subtle.
- Permissive license. Fork freely. The project may stop being
  maintained without notice.
- Powerlevel10k and gitstatusd are upstream projects by Roman
  Perepelitsa; this project is an independent Rust port and
  spiritual successor, not affiliated with or endorsed by them.

If any of that gives you pause, **don't use this for anything
important.** That's the responsible call, and the licenses
explicitly permit you to fork and audit, or pick a different
prompt project entirely.

## Maintenance and support

`p10k-rs` is a personal project, run by one person in spare
time. Concretely:

- **No SLA.** Issues may sit unanswered indefinitely. PRs may
  never be reviewed.
- **No security commitment.** Vulnerabilities will be addressed
  when and if the maintainer has time and interest. Use the
  private vulnerability reporting channel (see
  [SECURITY.md](SECURITY.md)) anyway — but no response time is
  promised.
- **No backward-compatibility promise** before v1.0. Schema,
  CLI, segment names, anything may change between minor
  versions. SemVer-style breakage is documented in
  [CHANGELOG.md](CHANGELOG.md) when it occurs; it is not
  prevented.
- **May be abandoned without notice.** If that happens, fork it.
  Permissive license, no questions asked.

If you need software with support SLAs and warranty commitments,
purchase a commercial product. `p10k-rs` is offered as-is for
people who want a Rust port of Powerlevel10k and accept it as a
hobby project.

## Development model: AI-assisted ("vibe coded")

`p10k-rs` is developed with substantial AI assistance. Most
code, tests, and documentation are produced through human + AI
collaboration. Implications you should know:

- **Bugs may be subtle.** AI-generated code can contain
  plausible-looking errors that experienced humans wouldn't
  make. Mitigations: tests (537 passing as of v0.1.5), CI gates
  (`clippy -D warnings`, `cargo deny`, `cargo machete`),
  type-system enforcement (the `SafeText` render-path chokepoint),
  and human review. The maintainer cannot promise these catch
  everything.
- **Code quality varies.** Different sessions and different
  agents produce different outcomes. Some modules are
  battle-tested (the render pipeline, the daemon FIFO client);
  others are newer and less proven (slice 60 design-doc work,
  per-host statusline contracts).
- **Decisions may not be documented in commits.** When an AI
  agent makes a design choice, the reasoning may live in chat
  history, not the commit message or an ADR. The [`docs/adr/`](docs/adr/)
  index is the canonical record for load-bearing architecture
  decisions; everything else is best-effort.
- **The maintainer is the human in the loop.** Every commit
  passes through human review before landing on main. CI gates
  are non-bypassable.

If any of that concerns you, **don't use this for anything
important.** Fork and audit, or pick a different prompt project.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Highlights:

- Conservative dependencies. Adding a crate needs rationale.
- `#![forbid(unsafe_code)]` everywhere except `p10k-rs-git`
  (where the `unsafe` budget is documented per call site —
  currently unused).
- Doc comments on every public item (`missing_docs = "warn"`
  at workspace level).
- Typed errors via `thiserror` in libraries; `anyhow` is the
  binary's glue only.
- Render-path inputs flow through `SafeText` — no shortcuts.

Contributions are welcome but **may be merged, modified,
rejected, or ignored at the maintainer's sole discretion** —
see "Maintenance and support" above.

## License

Dual-licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE) (or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- [MIT license](LICENSE-MIT) (or
  <https://opensource.org/licenses/MIT>)

at your option. Contributions are accepted under the same
terms.

`gitstatusd` is bundled as a **separate** static binary under
**GPL-3.0**; see [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md)
for the bundling rationale per ADR-0001 § Operational. The
GPL-3.0 binary is not statically linked into `p10k-rs` itself.

## Trademarks

**Powerlevel10k** is a project by Roman Perepelitsa
([romkatv/powerlevel10k](https://github.com/romkatv/powerlevel10k)).
`p10k-rs` is an **independent Rust port and spiritual successor**.
It is **not affiliated with, endorsed by, or sponsored by** the
upstream Powerlevel10k project or Roman Perepelitsa. References
to "Powerlevel10k" in this project are descriptive — identifying
the prompt design being ported — and not an assertion of
ownership or official status.

**gitstatusd** is likewise a separate project by Roman
Perepelitsa; see [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md)
for the bundling notice.

Other product names mentioned in this repository (terminals,
shells, cloud providers, AI hosts, package managers, etc.) are
trademarks of their respective owners. No challenge to any
trademark is intended. Their inclusion in segment names or
configuration examples is purely functional.
