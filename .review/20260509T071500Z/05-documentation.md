# Documentation Review — 20260509T071500Z

## Summary

Slice 9 made real progress: README and CHANGELOG actually reflect what
shipped through slice 8, ADR-0001 has had its GPL framing corrected, and the
new `THIRD-PARTY-LICENSES.md` is well-scoped. The biggest accuracy hit is
the **MSRV statement in README and CONTRIBUTING is wrong** — both still say
"stable - 2 (currently 1.84)" while `rust-toolchain.toml` and
`Cargo.toml` pin 1.88. Several smaller items: a stale "(post-MVP placeholder)"
descriptor on the IPC crate is fine, but the workspace member list, MSRV-policy
prose, the `gitstatusd` v1.5.4 pin disclosure, and the `RESUME.md` snapshot
have drifted versus reality. Coverage of the slice 9 work itself in the
CHANGELOG is **missing** — there is no slice-9 entry under `[Unreleased]`.

## Findings

### [HIGH] MSRV mismatch — README and CONTRIBUTING claim 1.84, toolchain pins 1.88
**Location:** `README.md:48`, `CONTRIBUTING.md:31` (rule 7), vs.
`rust-toolchain.toml:10` (`channel = "1.88.0"`) and `Cargo.toml:31`
(`rust-version = "1.88"`).
**Issue:** Both user-facing docs assert "stable - 2 (currently 1.84)." The
toolchain file and workspace metadata pin 1.88. `rust-toolchain.toml`
explicitly explains *why* 1.88 is the floor (clap_derive 4.6 → 1.85, `home`
0.5.12 → 1.88, gix 0.66 transitives → 1.85+). A new contributor following
README will install 1.84 and immediately fail `cargo build`. This is the
single most surface-visible accuracy bug in the doc set.
**Suggested fix:** Update README line 48 and CONTRIBUTING rule 7 to
"MSRV is **1.88** (pinned in `rust-toolchain.toml`; floor set by transitive
deps, not the stable-2 policy)." Cross-reference the comment block at
`rust-toolchain.toml:1-9` so the divergence from policy is visible to readers.

### [HIGH] CHANGELOG missing a Slice 9 entry
**Location:** `CHANGELOG.md:9-54`.
**Issue:** The `[Unreleased]` section enumerates slices 1-8 but stops there.
Slice 9 (HEAD `de0072c`) is what just shipped — ADR-0001 follow-ups closed,
spike crate removed, `gix.features = ["status"]` stripped, README/CHANGELOG
refreshed, `THIRD-PARTY-LICENSES.md` added, FIFO security hardening
(ownership + non-symlink checks at `crates/p10k-rs-git/src/gitstatusd.rs`).
The "### Added" subsection at lines 49-54 mixes bootstrap items with the
slice 9 ADR / install.sh additions in an undated way that doesn't follow
the same per-slice header pattern as the rest.
**Suggested fix:** Add a `### Slice 9: Triage / hardening (de0072c)` header
listing: spike crate removal, gix-status feature strip, FIFO ownership and
symlink defenses, `THIRD-PARTY-LICENSES.md`, ADR-0001 GPL framing fix,
README/CHANGELOG refresh. Move the existing "### Added" bullets either into
the appropriate slice section or a "Pre-slice-1 bootstrap" header.

### [MEDIUM] README "eight slices complete" wording is brittle
**Location:** `README.md:8-11`.
**Issue:** README says "Eight slices complete" but slice 9 is at HEAD. The
sentence enumerates slices 1-8 by feature; slice 9 is omitted because it is
"triage." That's defensible, but the count "Eight" will silently be wrong
the moment slice 10 lands and someone forgets to update the prose. The
phrasing also conflates "slices completed" with "user-visible features
shipped."
**Suggested fix:** Replace "Eight slices complete" with a feature-list
framing only: "Today's prompt: cwd, `$?`-aware chevron, vcs via gitstatusd
(with shell-out fallback, auto-respawn, 2 s timeout), command timing, and
sub-millisecond instant prompt." Drop the count. Bonus: link to CHANGELOG
for slice-by-slice history.

### [MEDIUM] Workspace layout in README omits `p10k-rs-config`
**Location:** `README.md:24-37`.
**Issue:** The workspace layout block lists nine crates: `p10k-rs`, `-core`,
`-config`, `-segments`, `-git`, `-shell`, `-wizard`, `-ai`, `-ipc`. That
matches `Cargo.toml:15-25` exactly — good. However, the description
`p10k-rs-config: TOML schema + Powerlevel9k import` overstates the current
state: `crates/p10k-rs-config/src/lib.rs` defines the schema types but
`p10k-rs import` is a CLI stub (`crates/p10k-rs/src/main.rs:75-78` declares
the subcommand; nothing wires it). Same for `p10k-rs-wizard` — README says
"`configure` TUI" but the wizard crate is single-file with no TUI present.
**Suggested fix:** Either annotate stub crates ("(scaffold; lights up in
slice N)") or add a short "Status by crate" table. The current presentation
implies more shipped than has.

### [MEDIUM] ADR-0001 "Follow-ups" line about ROADMAP.md is misleading
**Location:** `docs/adr/0001-git-backend.md:104-105`.
**Issue:** Two follow-ups are flagged "DEFERRED (planning bundle outside
repo)." That's true — `ROADMAP.md` and `ARCHITECTURE.md` live in
`/home/seaburdz/.planning/powerlevel10k-rs/`, not the repo. But "DEFERRED"
without a forwarding pointer reads like the work was dropped. A reader
without access to Sean's `~/.planning/` cannot tell whether the planning
bundle has actually been updated. (RESUME.md at
`/home/seaburdz/.planning/powerlevel10k-rs/RESUME.md` is itself stale —
"last updated 2026-05-07" describing the repo as "post-spike, pre-pivot-
execution" while slices 1-9 have all shipped.)
**Suggested fix:** Either change "DEFERRED" to "OUT OF SCOPE — tracked in
private planning bundle; updated alongside slice X" with the date the
planning doc was actually refreshed, or, if the planning bundle hasn't been
refreshed, change to "OPEN" so it stays on the radar. Independently: bump
RESUME.md so its narrative matches HEAD.

### [MEDIUM] `THIRD-PARTY-LICENSES.md` claims a v1.5.4 pin that is not enforced in the build
**Location:** `THIRD-PARTY-LICENSES.md:6-7,28`.
**Issue:** The doc says `gitstatusd` is "pinned tag v1.5.4." Searching the
repo, no build artifact, install script, or daemon-spawn code path pins or
verifies that version. `install.sh:125-129` symlinks any locally-discovered
`gitstatusd-linux-x86_64` (typically from `~/github/powerlevel10k/gitstatus/`)
or PATH; no SHA, no version probe. The legal narrative is sound — but the
"pinned tag" claim is asserted, not enforced. RESUME.md note at line 41-43
says the dev-host binary is v1.5.4; that's incidental, not a release
guarantee.
**Suggested fix:** Either (a) wire a pin in `install.sh` / a release script
that downloads the v1.5.4 binary from upstream releases with sha256
verification (matches ADR-0001 § Decision bullet 4), or (b) soften the doc
to "pinned at release time to v1.5.4 (today: whatever `gitstatusd` the user
or distro provides; pinning lands with the release tooling)." Without one
of these the GPL-§ 6 source-offer points at a tag that may not match what
shipped.

### [MEDIUM] `install.sh` hard-codes a maintainer-specific gitstatusd path
**Location:** `install.sh:126`.
**Issue:** The first `GITSTATUSD_CANDIDATES` entry is
`"$HOME/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64"` —
literally Sean's checkout layout. The script's header comment claims the
fallback was dropped "for security reasons" (line 121-122), but the path is
still right there. Cosmetically and operationally, anyone else running
`install.sh` will silently miss the first candidate and (if no other entry
matches) fall through to the warning at line 138-142.
**Suggested fix:** Drop the maintainer-specific path entirely and keep only
distro paths (`/opt/homebrew/bin/gitstatusd`, `/usr/local/bin/gitstatusd`),
or make the comment block at lines 118-124 honest about the
dev-machine bias. Either is fine, but the current state is half-and-half.

### [LOW] CHANGELOG header style mixes "Slice N: title (sha)" with subsection headers
**Location:** `CHANGELOG.md:11-54`.
**Issue:** Slices 1-8 use H3 headers `### Slice N: ...`. The trailing
`### Added` block (lines 49-54) breaks the pattern and lists items that
chronologically span multiple slices (workspace scaffold = bootstrap; CI =
bootstrap; ADR = post-spike; install.sh = post-slice-2). Keep-a-Changelog
style usually segregates by Added/Changed/Fixed/etc. *within* a release;
this hybrid is confusing.
**Suggested fix:** Pick one. Either (a) per-slice subsections with their
own Added/Changed/Fixed when it matters, or (b) collapse all of
`[Unreleased]` to a single "0.1.0 — pre-release" with standard KaC
subsections and put the slice-by-slice narrative in a ROADMAP / `notes/`
file. (a) is closer to current shape; (b) is closer to the linked spec.

### [LOW] CONTRIBUTING.md "No `tokio` in MVP" is correct but understates current state
**Location:** `CONTRIBUTING.md:38` (rule 8).
**Issue:** True today, but the rule reads as a guard against future
contributors rather than describing intent. With the gitstatusd subprocess
+ FIFO architecture (slice 6+) the sync model is now load-bearing, not
provisional. A reader would benefit from one sentence on *why* sync wins:
prompt rendering is one-shot, the daemon is the long-lived component,
async runtime cost is non-zero per shell-spawn.
**Suggested fix:** Append: "Each `p10k-rs prompt` invocation is one-shot
synchronous; the long-lived component is the gitstatusd daemon, not our
Rust code. Adding `tokio` would buy nothing on the hot path and would
inflate cold-start. See ADR-0001."

### [LOW] `docs/adr/README.md` not verified by this reviewer
**Location:** `docs/adr/README.md` (referenced from `CONTRIBUTING.md:50`).
**Issue:** CONTRIBUTING points contributors at this file as the ADR
template / index. I deferred reading it to stay within the word budget —
flagging so the next swarm pass can confirm it actually exists with current
content (it does exist on disk; freshness vs. ADR-0001 not checked).
**Suggested fix:** Spot-check during the next slice's review that the ADR
index lists 0001 with the correct status and date.

### [INFO] Doc comments on public items — sampled, broadly compliant
**Location:** workspace-wide spot check (`crates/*/src/lib.rs`,
`crates/p10k-rs/src/main.rs`).
**Issue:** CONTRIBUTING rule 3 demands `///` on every `pub` item.
Sampling `p10k-rs-config/src/lib.rs`, `p10k-rs-git/src/lib.rs`,
`p10k-rs-git/src/gitstatusd.rs`, and `p10k-rs/src/main.rs` shows the rule
is being honoured. `missing_docs = "warn"` at `Cargo.toml:85` enforces it.
No defect; recording as positive signal.
**Suggested fix:** None.

## Things this review explicitly did NOT examine

- Doc-comment density inside individual segment crates beyond a sample.
- Correctness of ADR-0001's benchmark numbers (lane 03 owns perf).
- Whether the FIFO security claims in slice 9 actually hold (lane 02).
- The `install.sh` rc-edit logic vs. real-world `~/.zshrc` shapes (lane 06).
- `bench/` README and methodology docs.
- Doc-test compilation status (`cargo test --doc`).
- `docs/adr/README.md` body content (referenced but not opened).

## Confidence

**Medium-high.** The MSRV mismatch and missing slice-9 CHANGELOG entry are
black-and-white — direct file comparisons confirm both. The workspace-layout
and "eight slices" findings are judgment calls about precision of
user-facing prose, not bugs. The `gitstatusd` v1.5.4 pin finding is
high-confidence as a doc/build divergence; whether it's a defect depends on
whether release tooling (not yet in tree) will close the gap. RESUME.md
staleness is high-confidence but lives outside the repo — flagged because
ADR-0001 § Follow-ups points at the planning bundle.
