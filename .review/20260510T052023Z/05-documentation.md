# Documentation Review — 20260510T052023Z

## Summary

Slice 11's new doc surface — `safety::sanitize_for_terminal` and the
`wrap_for_shell` invariants — is the strongest documentation in the
crate and ships exemplary `///` blocks with worked examples. Outside
that perimeter the README rewrite did **not** land the slice-9 doc
HIGHs the previous swarm queued (MSRV still wrong, slice list still
stops at 8) and slice 11 did not retire the load-bearing init.zsh:15-17
"slice 2 will escape `%`" comment that motivated the slice itself.
CHANGELOG is three slices stale. RESUME.md is two slices stale. Net:
slice 11 fixed the runtime bug but the documentation drift the same
auditor flagged in slice 9 has now compounded, not closed.

## Findings

### [HIGH] README still claims MSRV 1.84 and "eight slices complete"
**Location:** `README.md`
**Issue:** Line 11 advertises "Eight slices complete" and enumerates the
slice-1 through slice-8 deliverables — slice 9 (FIFO hardening, ADR-0001
follow-up closure), slice 10 (status segment), and slice 11 (render
sanitisation) are invisible to a reader landing on the repo. Line 48 says
"MSRV is **stable - 2** (currently 1.84)"; `rust-toolchain.toml:10` pins
`1.88.0` and `Cargo.toml:31` declares `rust-version = "1.88"`. This was a
HIGH in the slice-9 swarm summary. The README rewrite touched neither
line. A fresh contributor on stable - 2 (~1.93 today) is fine, but anyone
reading the doc as ground truth and trying to pin 1.84 fails immediately
(`home` 0.5.12 needs 1.88, per `rust-toolchain.toml:6`).
**Suggested fix:** Update line 11 to "Eleven slices complete" and append
slice 9/10/11 one-liners; update line 48 to "MSRV is **1.88** (pinned in
`rust-toolchain.toml`); policy is stable - 2, with a hard floor at the
highest `rust-version` advertised by any transitive dependency."

### [HIGH] CHANGELOG missing slices 9, 10, 11
**Location:** `CHANGELOG.md`
**Issue:** The `[Unreleased]` section ends at slice 8 (line 47). The
slice-9 swarm flagged this as HIGH; three more slices have shipped without
a changelog entry. Slice 9 had user-visible security changes (FIFO perm
hardening, dev-machine fallback dropped, `THIRD-PARTY-LICENSES.md`),
slice 10 added a new segment, slice 11 closed two CRITICAL render-path
holes. None of that is discoverable from CHANGELOG.
**Suggested fix:** Backfill three entries from the commit messages at
`d99a514`, `de0072c`, `e657779`. Slice 11's entry should call out the
behavioural change explicitly: "branch / cwd containing `%` now render
literally rather than triggering zsh prompt expansion."

### [HIGH] init.zsh:15-17 still promises slice-2 escaping that slice 11 actually delivered
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh`
**Issue:** Lines 14-17 still say "PROMPT_SUBST is left at the user's
setting; output is captured at assignment time, so `%` characters in
cwd would be re-interpreted by zsh. Slice 2 escapes them." Slice 2
never landed that escaping; slice 11's commit message identifies this
exact comment as the one that "promised slice-2 escaping; it never
landed", then ships the fix in `wrap_for_shell`. The comment is now a
direct lie: `%` doubling **is** in place. A future contributor reading
this in six months will assume the hole is open and either re-fix it
or, worse, conclude it's intentional and remove the doubling pass.
**Suggested fix:** Replace lines 14-17 with: "ANSI colors with `%{…%}`
bracketing land in slice 2 (`render_prompt`), `%`-doubling against
zsh PROMPT-expansion lands in slice 11 (`wrap_for_shell`), terminal-
escape stripping on segment input lands in slice 11
(`sanitize_for_terminal`). Instant prompt and transient prompt land
in slice 8 / a future slice." Then drop the file-header "slice 1"
on line 1 — by slice 11 this script is a much bigger artefact than
the slice-1 boot.

### [HIGH] RESUME.md is two slices stale
**Location:** `/home/seaburdz/.planning/powerlevel10k-rs/RESUME.md`
**Issue:** Header (line 3) says "HEAD: `c3034ec` — slice-9 review snapshot"
and "10 slices shipped" (line 6). HEAD is `e657779` and 11 slices have
shipped. § 6 still lists "CHANGELOG slice-9 + slice-10 entries (10 min)"
as a queued quick-win — that has compounded, not been done.
**Suggested fix:** Refresh § 1, § 3, § 6, and § 8's "1. Sanity" oneliner.
RESUME is the next-session hand-off; staleness here multiplies
elsewhere.

### [MEDIUM] README Workspace layout lists stub crates as if functional
**Location:** `README.md`
**Issue:** Lines 26-37 enumerate nine workspace members without flagging
which are stubs. Per RESUME § 2, `p10k-rs-config`, `p10k-rs-wizard`,
`p10k-rs-ai`, `p10k-rs-ipc` are stubs / "TOML schema (data only, not
yet wired)". A reader inferring capability from the layout
overestimates by ~half the workspace.
**Suggested fix:** Add `# stub` / `# data only — not yet wired` comments
to the four stub lines, mirroring RESUME § 2's annotations.

### [MEDIUM] README feature table doesn't match `default_layout`
**Location:** `README.md` (no feature table per se, but lines 16-19)
**Issue:** README claims four headline features (instant, transient,
show-on-command, wizard) plus "sub-millisecond git status." Of those,
only instant prompt and gitstatusd-fast vcs are implemented; transient,
show-on-command, and the wizard are unstarted (per RESUME § 6 deferred
list). `default_layout` at `crates/p10k-rs-segments/src/lib.rs:65-73`
ships five segments (`dir`, `vcs`, `command_execution_time`, `status`,
`prompt_char`). The README never enumerates the actual segment set.
**Suggested fix:** Add a "What's shipped today" subsection listing the
five default-layout segments, mark the four headline features as
"planned" vs "shipped". Keeps the marketing language but stops a reader
expecting working transient prompt today.

### [MEDIUM] CONTRIBUTING.md MSRV is the same 1.84 lie
**Location:** `CONTRIBUTING.md` (line 23)
**Issue:** "MSRV pinned and respected. stable - 2 (currently 1.84)."
Same defect as the README; same fix.
**Suggested fix:** Bump to 1.88 and reword to point at
`rust-toolchain.toml` as the source of truth. One sentence.

### [MEDIUM] Stale slice-number comments persist in seven files
**Location:** `crates/p10k-rs-segments/src/vcs.rs:5,26-27`,
`crates/p10k-rs-segments/src/command_execution_time.rs:5,50`,
`crates/p10k-rs-git/src/lib.rs:4,42`,
`crates/p10k-rs-segments/src/lib.rs:58`,
`crates/p10k-rs-shell/src/lib.rs:58`,
`crates/p10k-rs-core/src/lib.rs:336` (`TODO(adviser)` re config crate)
**Issue:** Slice-9's readability review flagged 19 stale slice-number
comments. A spot check confirms many remain — `segments/lib.rs:58` calls
`default_layout` "the slice-5 default layout" (now slice 11),
`vcs.rs:26-27` says "Daemon backend in slice 5+ flips this to true"
(it landed in slice 6/7). These mislead readers about what state the
code is currently in.
**Suggested fix:** Strip slice numbers wholesale. Where the historical
context matters (rare), replace with concrete language: "the daemon
backend (`Gitstatusd` impl) flips this to true."

### [MEDIUM] `wrap_for_shell` doc is excellent but the function is private
**Location:** `crates/p10k-rs-core/src/lib.rs:182-194` (the `///` block) +
`:195` (`fn wrap_for_shell`, no `pub`)
**Issue:** The `///` block on `wrap_for_shell` is the most precise piece
of doc in the workspace — it states the zsh `%`-doubling invariant, the
SGR-bracketing invariant, and the bash/fish pass-through contract. But
the function is private; the doc only renders for in-crate consumers and
`cargo doc` skips it. Given this is the load-bearing security boundary
slice 11 added, the invariants belong in a publicly-rendered location.
**Suggested fix:** Either promote `wrap_for_shell` to
`pub(crate) fn`/`pub fn` (it's a pure function, no contract risk) and let
`cargo doc --document-private-items` pick it up by default in CI; or move
the invariant text into the `safety` module-level doc, which is already
public, with a forward reference. The latter is cheaper.

### [LOW] `safety::sanitize_for_terminal` doc accuracy nit
**Location:** `crates/p10k-rs-core/src/safety.rs:38`
**Issue:** Doctest example says
`assert_eq!(sanitize_for_terminal("foo\x1b]0;evil\x07bar"), "foo]0;evilbar");`
which is correct (ESC and BEL stripped, payload preserved). The module
doc on lines 7-10 explains why intentionally — "the user sees that
something weird is in the input." The function-level `///` doesn't
forward that rationale, so a reader stopping at the function doc sees
the example without the "yes, leaving `]0;evil` visible is the
designed behaviour" justification.
**Suggested fix:** One line in the function-level doc: "The non-control
payload of stripped escape sequences is preserved intentionally so the
user can see suspicious content in their prompt rather than have it
silently disappear."

### [LOW] ADR-0001 line 60 references "this commit's parent" — git context lost in render
**Location:** `docs/adr/0001-git-backend.md:62-63`
**Issue:** "its source remains in git history at commit `<this commit's
parent>` for posterity." The `<>` placeholder was never substituted with
a real SHA. Anyone trying to find the spike crate has to grep history.
**Suggested fix:** Replace with the commit SHA where the spike crate
was deleted (slice 9, `de0072c`'s parent or de0072c itself; verify with
`git log --diff-filter=D --name-only -- crates/spike-gitstatus/`).

### [INFO] ADR-0001 Follow-ups all marked DONE — accurate
**Location:** `docs/adr/0001-git-backend.md:103-108`
**Issue:** None; this is a positive finding. All four follow-ups
(`THIRD-PARTY-LICENSES.md`, spike removal, gix-feature strip, plus the
two DEFERRED ROADMAP/ARCHITECTURE updates which live in the planning
bundle) reconcile against the repo state. ADR is the cleanest doc in
the tree.

## Things this review explicitly did NOT examine

- Rustdoc rendering of cross-crate links (would need `cargo doc` run).
- Whether the `.planning/powerlevel10k-rs/` bundle's other docs
  (MVP-SPEC, ARCHITECTURE, ROADMAP, the 01–10 numbered docs) reflect
  the slice-9 ADR-0001 closure — RESUME asserts they don't.
- Doc comments on private items inside non-`p10k-rs-core` crates beyond
  spot-checks for slice-number references.
- Whether `cargo doc --no-deps` builds clean (CONTRIBUTING claims it
  does in CI).

## Confidence

**High** on the README, CHANGELOG, init.zsh, and slice-11 safety doc
findings — those are direct file reads against HEAD. **Medium** on the
"19 stale slice comments" count carried forward from the slice-9
review; spot checks confirm many remain but I did not fully recount.
The slice-11 commit message itself is the load-bearing primary source
for the init.zsh:15-17 finding (it explicitly identifies that comment
as the broken promise).
