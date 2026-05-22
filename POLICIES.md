# Policies

How this project is maintained, who's behind the code, what
the support story is, and the trademark and license notices.
Read this **before** depending on `p10k-rs` for anything that
matters.

> **NOT LEGAL ADVICE.** This document is written in plain
> language by the maintainer. It is not drafted, reviewed, or
> blessed by an attorney. It exists to make the warranty and
> liability terms of the underlying [MIT](LICENSE-MIT) and
> [Apache-2.0](LICENSE-APACHE) licenses more conspicuous; the
> license texts remain the controlling legal instruments. If
> your use case has real legal, financial, regulatory, safety,
> or commercial stakes, **consult counsel** — do not rely on
> this document.

---

## Disclaimer of warranty

**THE SOFTWARE IS PROVIDED "AS IS" AND "AS AVAILABLE", WITHOUT
WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE, TITLE, NONINFRINGEMENT, ACCURACY, RELIABILITY,
AVAILABILITY, AND QUIET ENJOYMENT.**

The maintainer makes **no representations and no warranties** —
express, implied, statutory, or otherwise — about the software's
suitability for any purpose, its correctness, its reliability,
its availability, its compatibility with any system, its fitness
to handle any particular workload, or its compliance with any
standard, regulation, or law. The maintainer does **not warrant**
that the software is free of defects or that any defects will
be corrected.

Any reliance you place on the software is **strictly at your
own risk**.

## Limitation of liability

**TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW**, in no
event shall the maintainer, contributors, copyright holders, or
any party associated with the distribution of the software be
liable for **any** claim, damages, losses, costs, or other
liability of **any** kind — whether direct, indirect, incidental,
special, exemplary, consequential, punitive, or otherwise —
arising from, out of, or in connection with the software, its
use, its inability to be used, its modification, its
distribution, or its integration with any other software,
hardware, system, or service, including without limitation:

- Loss, corruption, or unauthorised disclosure of data.
- Lost profits, lost revenue, lost savings, lost opportunity, or business interruption.
- System downtime, system damage, hardware damage, or service degradation.
- Cost of substitute software, services, or labour.
- Third-party claims or demands of any nature.
- Regulatory, compliance, or contractual penalties of any kind.
- Personal injury, emotional distress, or reputational harm.

This limitation applies **regardless of the legal theory** —
contract, tort (including negligence), strict liability, statute,
or otherwise — and applies **even if** the maintainer has been
advised of the possibility of such damages and even if any
limited remedy is found to have failed of its essential purpose.

Some jurisdictions do not allow the exclusion or limitation of
incidental or consequential damages, so the above limitation
may not apply to you in full; in that case it applies to the
maximum extent permitted by law in your jurisdiction.

## Free service. No consideration. No contract.

`p10k-rs` is distributed **at no cost**. The maintainer
receives **no payment, no compensation, and no consideration**
of any kind in exchange for the software, its distribution, or
its continued maintenance. **No contract, no agreement, and no
relationship of any kind** is created between you and the
maintainer by your downloading, installing, modifying,
distributing, or using the software, other than the terms set
forth in the [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE)
license texts you accepted when you obtained the software.

**There is no service-level agreement. There is no support
contract. There is no maintenance obligation. There is no
upgrade obligation. There is no obligation to fix defects of
any kind.** None of these exist explicitly, and none can be
implied or inferred from any past activity of the maintainer,
the existence of this repository, prior responses to issues
or pull requests, public statements, social-media posts,
release cadence, blog posts, or any other interaction or
course of dealing.

## Assumption of risk

By downloading, installing, modifying, distributing, or using
`p10k-rs`, you acknowledge and accept that:

1. **The software may contain defects of any kind** that have not been discovered and may never be discovered.
2. **The software is AI-assisted** ("vibe coded") and may contain plausible-looking errors that escape ordinary human review.
3. **The software may stop being maintained** at any time, without notice, without migration guidance, and without successor.
4. **Defects of any nature may be addressed slowly, partially, or not at all**, at the maintainer's sole discretion.
5. **Breaking changes** to configuration schema, CLI surface, segment names, output format, or any other interface may land between any two versions before v1.0, including patch versions.
6. **Backups, redundancy, monitoring, and incident response are your responsibility** — the software offers none of these and assumes none for you.
7. **The maintainer's spare time, mood, life circumstances, and personal interest** are the sole determinants of project activity, and may shift without notice or explanation.

If any of these consequences are unacceptable for your use case,
**do not use this software**. Choose a commercial product backed
by a vendor with explicit support obligations, warranty
commitments, and indemnification, or maintain your own fork
under terms acceptable to your organisation.

## Indemnification

You agree, to the fullest extent permitted by law, to
**indemnify, defend, and hold harmless** the maintainer,
contributors, copyright holders, and all parties associated
with the distribution of the software from and against any and
all claims, demands, suits, proceedings, damages, losses,
liabilities, costs, and expenses — including reasonable
attorneys' fees and court costs — arising out of or related to:

- Your use, modification, distribution, or sublicensing of the software.
- Your integration of the software with any other product, system, or service.
- Any breach by you of the [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) licenses, or of this document.
- Any claim that your use, modification, or distribution of the software infringes the intellectual-property rights of a third party.
- Any data, content, or configuration you process through the software.

This obligation survives your cessation of use of the software.

## Severability

If any provision of this document is held to be unenforceable
or invalid under applicable law, that provision shall be
construed, limited, modified, or, if necessary, severed to the
minimum extent necessary to render the remainder enforceable.
The unenforceability of any single provision does not affect
the enforceability of the rest.

## License authority

If there is any conflict between this document and the [MIT
license](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE)
text under which the software is distributed, **the license
texts control**. This document exists to make those licenses'
warranty and liability provisions more conspicuous; it does
not modify, expand, narrow, or supersede them.

---

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
- **No commitment to fix defects of any kind.** Defects will
  be addressed when and if the maintainer has time and
  interest. A private reporting channel is available — see
  [SECURITY.md](SECURITY.md) — but no response time is
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
