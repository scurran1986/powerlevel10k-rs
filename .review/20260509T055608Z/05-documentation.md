# Documentation Review — 20260509T055608Z

## Summary

Public-surface `///` docs are solid: every `pub` item I sampled carries a
block and `#![warn(missing_docs)]` is workspace-wide. The drift is in the
**prose docs around the slices**: README/CHANGELOG/ADR-0001 follow-ups/
RESUME.md describe a project that did not happen. README still says
"pre-alpha, workspace skeleton only", links to the planning bundle via
paths that do not resolve inside the repo, and CHANGELOG covers only the
bootstrap despite seven slices shipped. ADR-0001 says `spike-gitstatus`
"will be removed in a follow-up commit" — three slices later it is still
a workspace member, and `gix.features = ["status"]` is still pinned. Slice
numbering inside source comments is internally inconsistent. Nothing is a
correctness defect; it is a credibility problem that compounds.

## Findings

### [HIGH] CHANGELOG.md is frozen at the bootstrap commit
**Location:** `CHANGELOG.md:9-23`
**Issue:** The only `[Unreleased]` entry is the workspace skeleton and the
ADR-0001 reservation. Seven slice commits (`acdfb4a..a96c7a1`) have landed
since: minimum runnable prompt, ANSI colors + zsh `%{…%}` bracketing,
exit-status `prompt_char`, vcs via shell-out, command-execution time, the
gitstatusd long-lived daemon, and slice 7's hardening + rich vcs render. None
is in the changelog. The `Notes` section also claims `crates/spike-gitstatus`
"will fail `cargo check` until that crate's files land" — false; the crate
compiles.
**Suggested fix:** Add `Added` bullets per shipped slice; delete the
contractor-pending note.

### [HIGH] README.md misstates project state and links break
**Location:** `README.md:7-9, 21, 41`
**Issue:** Two gaps. (1) "Status: pre-alpha. Workspace skeleton only. The
day-1 spike (`crates/spike-gitstatus`) gates whether the project proceeds."
— the spike is signed off, ADR-0001 records GO-with-pivot, and `prompt`
works end-to-end. (2) Both link targets point outside the repo:
`../.planning/powerlevel10k-rs/MVP-SPEC.md` and `../.planning/.../ARCHITECTURE.md`.
On GitHub these 404; there is no `docs/MVP-SPEC.md` mirror.
**Suggested fix:** Update status to "7 slices shipped; zsh end-to-end;
gitstatusd daemon-client live (ADR-0001)". Either copy the planning files
into `docs/` or replace the links with a pointer to ADR-0001.

### [HIGH] ADR-0001 follow-ups still open three slices later
**Location:** `docs/adr/0001-git-backend.md:107-108`, `Cargo.toml:25,71`
**Issue:** ADR-0001 § Follow-ups says "Remove `crates/spike-gitstatus/` from
the workspace once the next commit lands" and "Strip `gix.features =
["status"]`". Slices 4–7 have all landed; both follow-ups are outstanding.
`Cargo.toml:25` still lists `crates/spike-gitstatus` as a member;
`Cargo.toml:71` still pins `gix = { … features = ["max-performance-safe",
"status", "revision"] }`. The ADR also references "this commit's parent" at
line 63 — those commits are `9cc8771..2357858`.
**Suggested fix:** Either close the follow-ups (preferred) or append "Status:
deferred to slice N" to each so the doc stops claiming work that has not
happened. Replace "this commit's parent" with `9cc8771..2357858`.

### [MEDIUM] Slice numbering in source comments drifted from `git log`
**Location:** `crates/p10k-rs-segments/src/vcs.rs:5-6,26-27`,
`crates/p10k-rs-segments/src/command_execution_time.rs:5-6`,
`crates/p10k-rs-segments/src/lib.rs:57`,
`crates/p10k-rs/src/main.rs:121,126`
**Issue:** `vcs.rs:5-6,27` says daemon client "lands in slice 5+" — actually
slice 6 (`16ad060`) and hardened slice 7 (`a96c7a1`).
`command_execution_time.rs:5-6` says "slice 6+ exposes it via TOML config" —
slice 6 was the daemon. `default_layout` (`segments/src/lib.rs:57`) and
`cmd_prompt` (`main.rs:121`) both say "slice-5 layout" but the layout shipped
in slice 4 (`17a921e`). `vcs.rs:27` predicts `is_fast()` flips true in slice
5+; still false post-slice-7.
**Suggested fix:** Stop forecasting slice numbers in source — they go stale.
Replace "slice 5+" with concrete phrasing like "the daemon backend".

### [MEDIUM] `RESUME.md` is two slices out of date
**Location:** `~/.planning/powerlevel10k-rs/RESUME.md:1-21, 132-159`
**Issue:** Header: "Last updated 2026-05-07; git state — clean, four
commits." HEAD is `a96c7a1`, fourteen commits in. The "files touched"
log ends at `2357858`. § 2 itemises work that shipped: `p10k-rs-git` as
daemon client, fallback path. § 4 tells the next session to start the
daemon client — which has been the last two slices.
**Suggested fix:** Add a session-4 stanza or rewrite reflecting post-slice-7.

### [MEDIUM] Planning bundle not updated for the ADR-0001 pivot
**Location:** `~/.planning/powerlevel10k-rs/ROADMAP.md:12, 27`,
`~/.planning/powerlevel10k-rs/MVP-SPEC.md:9-39, 119-128`
**Issue:** ADR-0001 § Follow-ups commits to update `ROADMAP.md` and
`ARCHITECTURE.md § 2.4`; neither has happened. ROADMAP Phase 0 still says
"either green-light for `gix` or pivot plan"; Phase 1 says `p10k-rs-git`
will be "chosen backend from spike, no untracked-cache yet". MVP-SPEC § 0
still has the tri-impl scope and GO/PAUSE decision tree; § 3 criterion 1
is "Day-1 spike targets met" — they were not met, but the project pivoted.
**Suggested fix:** Add "§ 0 superseded by ADR-0001" at top of MVP-SPEC and
rewrite ROADMAP Phase 0/1; or mark both as "stale; see ADR-0001".

### [LOW] Source comments reference planning files only on Sean's host
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:22, 98, 164, 182`,
`crates/p10k-rs-config/src/lib.rs:9,257`,
`crates/p10k-rs-segments/src/lib.rs:7`,
`crates/p10k-rs-ai/src/lib.rs:12`,
`crates/p10k-rs-wizard/src/lib.rs:4`
**Issue:** Doc/inline comments reference `07-gitstatus.md`, `05-config-
parameters.md`, `08-security.md`, `01-segments.md`, `10-ai-integration.md`,
`06-wizard-and-presets.md` as sources of truth. None of these files lives in
this repo; all are at `/home/seaburdz/.planning/powerlevel10k-rs/`. A future
contributor or doc-builder cannot resolve the references.
**Suggested fix:** Either (a) check the planning bundle into `docs/planning/`
or (b) replace bundle citations with self-contained doc text. For the
gitstatusd wire format, an upstream URL pointer is durable.

### [LOW] `docs/adr/README.md` index title drifted from ADR-0001
**Location:** `docs/adr/README.md:23`
**Issue:** Index entry: "0001 | Git status backend (gitstatusd-rs shim) |
Accepted (2026-05-06)". The ADR file's title is just "Git Status Backend".
Cosmetic.
**Suggested fix:** Match the ADR title in the index, or rename the ADR.

### [LOW] `p10k-rs-ipc` doc may collide with ADR-0001 conceptually
**Location:** `crates/p10k-rs-ipc/src/lib.rs:1-20`
**Issue:** Module docs describe a v0.2 daemon as "length-prefixed `postcard`
or CBOR over an abstract Unix domain socket". ADR-0001 has now committed the
project to talking to `gitstatusd` over its `\x1F`/`\x1E` protocol; whether
`p10k-rs-ipc` is still meant to layer a *different* protocol on top for a
future pure-Rust daemon is unclear.
**Suggested fix:** One line acknowledging ADR-0001: "The hot-path git IPC is
the `gitstatusd` wire protocol (see `p10k-rs-git`); this crate is reserved for
the v0.2 prompt-side daemon, not the git daemon."

### [LOW] `spike-gitstatus/tests/correctness.rs` is still meaningful but undocumented
**Location:** `crates/spike-gitstatus/tests/correctness.rs:1-12`
**Issue:** ADR-0001 says the spike crate "has discharged its purpose" and
"will be removed". Until that removal commits, this integration test still
runs (`cargo test --workspace`) and gates the three implementations against
each other — including a `gitstatusd_baseline` that the production crate no
longer uses. The file-level comment doesn't mention ADR-0001 or the "this is
throwaway after pivot" framing. A future contributor will not know whether to
fix or delete it.
**Suggested fix:** Either delete the spike crate (per ADR follow-up #1) or add
a "Why is this still here?" doc-comment referencing ADR-0001 § Follow-ups.

### [INFO] `CONTRIBUTING.md` understates the pub-doc bar
**Location:** `CONTRIBUTING.md:15-16`
**Issue:** "Doc comments on every public item. `///` on every `pub` thing,
with one example for any non-obvious API." Workspace lint `missing_docs =
"warn"` enforces the doc; no public function I sampled has a `# Examples`
block. `Segment` has the only `# Example` (in `p10k-rs-core/src/lib.rs:32-50`,
marked `ignore`).
**Suggested fix:** Soften CONTRIBUTING ("examples encouraged for non-obvious
APIs"); revisit when wizard and AI phases ship real implementations.

## Things this review explicitly did NOT examine

- Rust idiom / lint compliance — review #01.
- Security of env-var handling and unsafe — review #02.
- Latency budget — review #03.
- Cognitive load / symbol naming — review #04.
- Conceptual ADR-vs-code drift — review #06 (I covered *referenced* drift
  only, e.g. follow-ups still listed as open).
- mdBook / docs.rs publishing — neither exists yet.
- `bench/results/SPIKE-VERDICT-*.md` content.

## Confidence

**Medium-high.** I read every `pub` doc in the workspace and verified each
referenced path, plus walked README, CHANGELOG, CONTRIBUTING, ADR-0001 +
index end-to-end. I cross-checked slice numbers against `git log` and the
planning bundle's RESUME.md. I did not run `cargo doc` to confirm the doc
tree builds; clippy's pedantic missing-docs / `# Errors` / `# Panics` lints
could fail and I would not catch it from a read-only pass.
