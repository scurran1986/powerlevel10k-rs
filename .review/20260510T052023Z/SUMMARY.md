# Review Swarm Summary — slice 11 (HEAD `e657779`)

**Aggregate:** **0 CRITICAL.** Slice 11 successfully closes C1 (zsh
`%`-expansion) and C2 (ANSI/control-char injection) per the security
lane's own verification. **5 HIGH** — none are new code defects;
they are doc drift, an architectural improvement (SafeText newtype),
and the pre-fanout IPC findings (H3/H4) carrying over from the prior
audit cycle.

## All HIGH

| # | Reviewer | Finding | Action |
|---|---|---|---|
| 1 | rust + arch | Sanitisation invariant lives only in doc-comments — `vcs.rs:46` happily emits `git.branch` raw, the next backend or feature segment can silently re-introduce the C2 class | Introduce `SafeText` newtype in `core::safety`; producers return `SafeText`, segments accept only `SafeText`. **Two reviewers independently recommended this.** |
| 2 | docs + arch | CHANGELOG missing slices 9, 10, 11 entries — three slices, including the security-class fix users will scan for first | Backfill in a doc slice |
| 3 | docs | CONTRIBUTING.md still says MSRV 1.84; code pins 1.88 (same drift slice-9 review flagged as HIGH) | 1-line edit |
| 4 | docs | `init.zsh:15-17` still promises "Slice 2 escapes them" — the exact comment slice 11 just satisfied | Update the comment to reference the live impl in `wrap_for_shell` |
| 5 | sec (carryover) | H3 (FIFO framing byte-injection via `\x1F`/`\x1E` in dirname) + H4 (no request/response correlation, constant request id) — both still preliminary, the IPC lanes were empty in the slice-9 audit cycle | Re-run an IPC-focused audit; verify with strace + two-subshell repros |

## Cross-cutting themes (≥ 2 reviewers raised)

1. **`SafeText` newtype** (rust + architecture) — encode sanitisation
   in the type system. Strongest consensus signal of the swarm.
2. **CHANGELOG drift** (documentation + architecture) — three slices
   unrecorded.
3. **`wrap_for_shell` is now multi-purpose** (readability — split into
   `wrap_zsh_sgrs` + `double_zsh_percents`; performance — three-branch
   byte loop makes a `memchr` rewrite more attractive).
4. **`sanitize_for_terminal` always allocates** (rust + performance) —
   even on the no-change path. Easy win: return `Cow<str>` and only
   allocate when stripping actually fires.
5. **Stale slice-number comments** (readability + documentation) —
   went from 19 → 21 in slice 11. Two new ones added; none cleaned.
6. **No integration test exercising producer → render chain end-to-end**
   (architecture). The unit tests cover each boundary in isolation;
   nothing exercises "malicious branch flows from gitstatusd through
   vcs through wrap_for_shell to stdout in a single test".

## Severity distribution

- **CRITICAL:** 0 (slice 11 closed both prior CRITICALs).
- **HIGH:** 5 (1 SafeText newtype, 2 CHANGELOG/docs, 1 init.zsh comment,
  1 IPC carryover representing 2 sub-findings).
- **MEDIUM:** ~22 (allocations, function split, integration tests,
  install.sh validation, dump-file umask, `Backend::status` Result
  carryover, stale comments, etc.).
- **LOW:** ~10 (style, docs nits, perf nits).
- **INFO:** ~6 (positive findings, observations).

## Per-area headlines

- **01 Rust principles:** safety module is exemplary; SafeText newtype
  is the sole HIGH.
- **02 Security:** slice 11 closes C1 + C2 verified; H3/H4 IPC
  carryover; new MEDIUM on dump-file umask + install.sh `case`-validation.
- **03 Performance:** slice 11 is "correctness wins paid in µs of
  allocator pressure"; doesn't threaten MVP-SPEC § 0's < 5ms warm
  budget. Process-spawn ceiling re-rated unchanged HIGH.
- **04 Readability:** net win — `safety` module sets the new bar.
  Regression: stale slice-number comments 19 → 21, `wrap_for_shell` now
  too multi-purpose.
- **05 Documentation:** four HIGH doc-drift items. **One HIGH was a
  false alarm** — the README MSRV/slice-list finding race-conditioned
  with the in-session README rewrite (the file on disk is correct).
  Real HIGHs: CONTRIBUTING, CHANGELOG, init.zsh comment.
- **06 Architecture:** `safety` placement in `core` is correct
  (consumed by both `git` and `segments` — moving it to `segments`
  would invert the dep arrow). CHANGELOG repeat HIGH. SafeText repeat
  consensus.

## Suggested slice 12 menu

Three credible next slices, ranked by leverage:

1. **slice 12-a: doc/changelog hygiene** (~30-60 min). Close 4 of the
   5 HIGHs in one mechanical commit:
   - CHANGELOG entries for slices 9, 10, 11.
   - CONTRIBUTING.md MSRV → 1.88.
   - `init.zsh:15-17` comment update.
   - Strip the 21 stale slice-number comments (the original parked
     slice-11 punchlist).
   - Update `RESUME.md` (two slices behind).

2. **slice 12-b: SafeText newtype** (~1 day). Architectural improvement
   that encodes the sanitisation invariant in the type system. Closes
   the 2-reviewer consensus HIGH. Prevents future segments from
   silently re-introducing the C2 class. Bigger diff: change
   `GitState::branch` from `String` to `SafeText`, ripple through
   producers and consumers, add `From`/`AsRef` so segments can format
   them transparently.

3. **slice 12-c: IPC verification** (~half-day). Re-run the empty-from-
   prior-cycle IPC lane: verify H3 (FIFO framing byte-injection)
   reproduces with `\x1F`/`\x1E` in a dir name; verify H4 (cross-talk)
   with two zsh subshells; fix whichever land. Closes the carryover.

**TARS lean:** slice 12-a first. ~1 hour, closes 4 HIGHs, doc-hygiene
backlog goes to zero. Then 12-b (SafeText) for the architectural win.
Then 12-c (IPC) for the residual carryovers.

**CASE counterpoint:** 12-b first. The doc HIGHs are paper cuts; the
type-system fix is the load-bearing improvement. Doc cleanup as 12.5.

## What this review confirmed

- **Slice 11 closes C1 and C2.** Lane 02 traced the data plane end-to-
  end (gitstatusd parser → segment → wrap_for_shell → stdout) and
  confirmed every untrusted-input boundary applies `sanitize_for_terminal`,
  every text portion of the zsh sink doubles `%`, and the instant-prompt
  dump preserves the doubled `%%` across shell restarts.
- **The new `safety` module is the new readability + documentation bar.**
  Two lanes called it out as exemplary.
- **`safety` placement in `core` is correct** per the architecture lane.
- **MVP-SPEC § 0 perf budget intact.** Slice-11 fixes cost µs of
  allocator pressure, not ms.

## Confidence

**High** on all six lanes. The one false alarm (README documentation
HIGH) is a race-condition artefact, not an analytical defect — the
agent did its job, it just read the file before the in-session
rewrite landed.

The IPC carryovers (H3, H4) are still preliminary because no agent
on this swarm or the previous one has actually run a reproducer.
Slice 12-c is the right place to close that loop.
