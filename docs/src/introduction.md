# Introduction

`powerlevel10k-rs` is a Rust port and spiritual successor to
[Powerlevel10k][p10k]. A single static binary, declarative TOML config,
multi-shell prompt, with `gitstatusd`-class git latency as the
load-bearing performance claim.

Upstream Powerlevel10k appears unmaintained per its own issue tracker.
That is exactly why this project exists. The original gave a generation
of shell users instant prompts, transient prompts, show-on-command, and
sub-millisecond git status; this port keeps that feature set on a stack
that can ship binaries, run on stable Rust, and pull conservative
dependencies.

The architecture is deliberately boring. The render pipeline is pure
and I/O-free (`p10k-rs-core`); the schema is data only (`p10k-rs-config`);
the only crate with an `unsafe` budget is the git backend
(`p10k-rs-git`), which talks to a long-lived `gitstatusd` daemon over
FIFOs per [ADR-0001][adr]. Anything attacker-controlled — branch names,
cwd, segment icons from TOML — passes through `SafeText` before it hits
the prompt.

[p10k]: https://github.com/romkatv/powerlevel10k
[adr]: https://github.com/scurran1986/powerlevel10k-rs/blob/main/docs/adr/0001-git-backend.md
