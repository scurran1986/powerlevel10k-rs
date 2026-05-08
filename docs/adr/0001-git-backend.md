# ADR-0001: Git Status Backend

**Status:** Accepted (Sean signed off 2026-05-06 after spike).
**Decision date:** 2026-05-06.
**Supersedes:** none.
**Superseded by:** none.

## Context

Powerlevel10k's headline differentiator is sub-millisecond `vcs` segment latency
even on enormous repos. The C++ `gitstatusd` daemon achieves this via a custom
multi-threaded walker (`getdents64`-batched, parent-fd-relative syscalls,
mtime-keyed untracked cache). Any port of p10k that ships a slow `vcs` segment
fails the user's mental model of "p10k is the fast prompt."

The Day-1 spike (see `MVP-SPEC.md` § 0) tested three implementations against
ripgrep and the v6.6 linux kernel:

1. **gix-only** — straight `gix-status` API call.
2. **hybrid** — `gix` for index/refs, custom `rustix` parent-fd walker with a
   per-directory mtime untracked-cache, mirroring the gitstatusd recipe.
3. **gitstatusd subprocess** — fork/exec the prebuilt C++ daemon per call.

Long-lived gitstatusd over its wire protocol (the production integration mode)
was measured separately as the ground truth.

## Numbers (linux kernel v6.6, hot, WSL2)

| Impl | Mean | vs gitstatusd long-lived |
|---|---|---|
| gitstatusd long-lived (n=200) | 83.8 ms | 1.0× |
| gix-only (n=20) | 1360 ms | **16.2×** slower |
| hybrid (n=20) | 2900 ms | **34.6×** slower |
| gitstatusd subprocess (n=20) | 1196 ms | architectural antipattern |

WSL2 carries a ~3× syscall tax vs native Linux (gitstatusd hits ~25 ms native vs
83.8 ms WSL2). The tax applies equally to gix-only, so **ratios** are valid.
On native Linux we'd expect gix-only at ~400 ms vs ≤ 80 ms allowable per
`MVP-SPEC.md`. The 16× ratio is the load-bearing signal.

Full measurements and methodology in
`bench/results/SPIKE-VERDICT-20260506T184527Z.md`.

## Decision

**`p10k-rs-git` will be a `gitstatusd` client over the wire protocol, not an
in-process scanner.**

Concretely:

- Spawn one `gitstatusd` worker per `p10k-rs` process.
- Communicate via the documented wire format (request/response with `\x1F` field
  separators, `\x1E` record terminator) over the daemon's stdin/stdout.
- Cache the daemon handle for the lifetime of the prompt process.
- Vendor the prebuilt `gitstatusd` binary download/verify logic from the
  upstream `gitstatus/install` script (sha256-pinned, multi-arch).
- Fall back to `gix-status` (in-process, slower-but-correct) when no
  `gitstatusd` is available — useful for unsupported architectures and as a
  belt-and-braces correctness check.

The `spike-gitstatus` crate has discharged its purpose. It will be removed from
the workspace in a follow-up commit; its source remains in git history at
commit `<this commit's parent>` for posterity.

## Pivot directions considered

| Option | Pro | Con | Verdict |
|---|---|---|---|
| **(1) `gitstatusd-rs` shim** | 2-3 month timeline; gitstatusd is "frozen" upstream so it's a stable target; lowest risk | Adds an external binary dependency; native binary per arch | **Chosen** |
| (2) C++ FFI to `libgitstatus` | Avoids IPC overhead | gitstatusd doesn't expose a stable C ABI; we'd own a wrapper layer; MSRV / linker complexity | Rejected — IPC overhead is ~0.8 ms, not worth the build complexity |
| (3) Pure-Rust port of gitstatusd's algorithm | No external dependency; eventually decouples from C++ entirely | 9-12 months minimum; we'd be reimplementing a decade of engineering against a moving target | Rejected for v0.1; revisit post-v1 if `gitstatusd` upstream fully stalls |

## Consequences

### Architectural

- `p10k-rs-git` becomes a daemon-client crate, not a scanner. Dependencies shift: drop direct `gix-status` use on the hot path; keep `gix` only for non-status git work (eventually `vcs` fallback for repos `gitstatusd` doesn't support).
- A new responsibility lands: daemon lifecycle (spawn, restart on crash, health check, graceful shutdown). This is a small state machine, not a research project.
- Vendoring + verifying `gitstatusd` binaries is a CI / packaging concern. Start with linux/x86_64 + linux/aarch64 + darwin/aarch64 + darwin/x86_64; the upstream `install.info` table covers what we need.

### Operational

- Distribution: ship one `gitstatusd` binary per supported triple alongside the `p10k-rs` binary. License is GPL-3.0-or-later for `gitstatusd`; check that p10k-rs's MIT/Apache-2.0 licensing remains compatible (combined work distribution implications). **Action item: licensing review before v0.1 ship.**
- Cold-start: daemon spawn takes <50 ms. Instant prompt continues to work because the cached `p10k-dump` is rendered before the daemon is ready; first real prompt blocks on daemon up-and-ready.

### Schedule

- Deletes the spike crate's path-forward (it was scaffolding for option 3). Saves ~3 weeks of original ROADMAP estimates.
- Adds the daemon-lifecycle work, ~1 week.
- Net effect: roughly schedule-neutral, with much higher confidence.

## Follow-ups (tracked separately)

- Update `ROADMAP.md` to reflect the pivot — drop the in-process scanner phase, add the daemon-client phase.
- Update `ARCHITECTURE.md` § 2.4 to describe the daemon-client design.
- File a `gitstatusd` licensing-compatibility check task before v0.1 ship.
- Remove `crates/spike-gitstatus/` from the workspace once the next commit lands.
- Strip `gix.features = ["status"]` from `[workspace.dependencies]` since the hot path no longer uses it. Keep `revision` for ahead/behind on the daemon-client side; the daemon does not return ahead/behind without a configured upstream, and our own walker for that is cheap.
