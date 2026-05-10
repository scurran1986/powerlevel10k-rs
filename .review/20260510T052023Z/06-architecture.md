# Architecture & Other Review — 20260510T052023Z

## Summary

Slice 11 lands the C1/C2 hardening with disciplined surgery: a new
`p10k-rs-core::safety` module, three-boundary application
(`gitstatusd::parse_response`, `parse_branch_header`,
`Dir::render`), and 20 new tests pinned to the audit's reproducer
payloads. The crate placement is right and ADR-0001 is unaffected.
Two architectural concerns: the defence is data-plane only — the
`%`-doubling lives in `wrap_for_shell` while sanitisation lives at
the producer, and that split is undocumented; and the **CHANGELOG
has no slice-9, slice-10, or slice-11 entry**, repeating the HIGH
the previous swarm flagged. Test discipline is good but unit-only;
no integration test covers the full producer→render pipeline that
slice 11 just hardened. Tech-debt accumulation is otherwise flat.

## Findings

### [HIGH] CHANGELOG drift unfixed across three slices
**Location:** `CHANGELOG.md` (last entry: slice 8, line 38).
**Issue:** The previous swarm (`.review/20260509T071500Z/SUMMARY.md`)
flagged this as HIGH for missing slice 9. Slice 9 is still absent;
slice 10 (instant-prompt persistence work referenced in the commit
message as H1) is absent; slice 11 (a security-class fix users will
care about) is absent. Three consecutive slices of drift on a
user-facing artefact that explicitly documents pre-1.0 breakage —
a security hotfix is exactly the entry that must not be silent.
**Suggested fix:** Backfill three entries in this slice (or the
next maintenance one). Slice 11's bullet should explicitly call
out the C1/C2 closure with severity, since downstream packagers
will scan CHANGELOG for security mentions.

### [MEDIUM] No integration test for the producer→render pipeline
**Location:** workspace-wide; nearest concrete pointer
`crates/p10k-rs-segments/src/dir.rs:65-130` and
`crates/p10k-rs-core/src/lib.rs:284-321`.
**Issue:** All 20 new tests are unit tests inside the modules they
exercise. Each link in the chain is verified in isolation — but the
chain is `Dir::render` (sanitises) → `render_prompt` (concatenates)
→ `wrap_for_shell` (doubles `%`). There is no test that asserts an
attacker-controlled cwd carrying *both* a control byte *and* a `%n`
payload survives the full pipeline as expected. The defence depends
on producer-side sanitisation and renderer-side doubling composing
correctly; that composition is currently only verified by hand
(commit message's `/tmp/p10ktest-*` reproducers). On a future
refactor that reorders the passes, every unit test still passes.
**Suggested fix:** Add an integration test in
`crates/p10k-rs-segments/tests/render_pipeline.rs` (or
`crates/p10k-rs/tests/`) that drives `render_prompt` with a
`Vcs`+`Dir` layout, a `RenderCtx` carrying a hostile `cwd` and
`GitState { branch: "%n@%m\x1b]0;evil\x07", … }`, and asserts the
final string contains `%%n@%%m`, no `\x1b`, no `\x07`, no bare `%`.
This is the only test that would actually fail if the defence were
silently broken end-to-end.

### [MEDIUM] `safety` doc undersells where the second half lives
**Location:** `crates/p10k-rs-core/src/safety.rs:1-14`.
**Issue:** The module doc says per-shell `%`-escapes are applied
"later in `render_prompt`'s `wrap_for_shell` pass". Correct, but
the threat model — and therefore the invariant a future contributor
must preserve — is split across two files with no test or compile-time
link. If a v0.2 segment author calls `wrap_for_shell` early, or
adds a new shell variant that forgets `%` doubling, sanitisation
alone won't catch a `%n` payload (sanitiser explicitly leaves `%`
alone, see `safety.rs:122-127`). The two-pass design is load-bearing
and currently held together by a doc-comment chain.
**Suggested fix:** In `safety.rs`'s module doc, add an explicit
"Defence is two-pass" bullet listing the boundaries (producers call
`sanitize_for_terminal`; renderer calls `wrap_for_shell`); mirror
the same prose in `wrap_for_shell`'s doc; consider a `#[doc =]`
or test-only assertion in `render_prompt` that fails if a non-zsh
shell ever sees a literal `%n` in input. The architectural
invariant deserves enforcement, not just narration.

### [MEDIUM] Sanitisation gate at the producer leaks responsibility
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:185-189`,
`crates/p10k-rs-git/src/lib.rs:103-105`,
`crates/p10k-rs-segments/src/dir.rs:24-26`.
**Issue:** Three call-sites in three crates each remember to call
`sanitize_for_terminal` on the right field. Slice 12 will likely
add `hostname`, `user`, `kubecontext`, `aws_profile`, etc. — all
read from outside the process. If any future segment forgets the
call, slice 11's defence is partially defeated for that producer.
The architecturally cleanest fix is to make sanitisation a property
of the type the renderer accepts, not a discipline producers
must remember. Today, `SegmentOutput::text` is a plain `String`;
nothing distinguishes "raw" from "sanitised".
**Suggested fix:** Two paths, surface both for Sean. (a) Cheap:
sanitise inside `render_prompt` itself, *before* `wrap_for_shell`,
on `out.text` for every segment — moves the discipline from N
producers to one consumer. Risk: re-sanitising the SGR-bearing
text needs the function to be idempotent, which it currently is
(SGR's `\x1b` would be stripped — bad). (b) Right: introduce a
`SafeText` newtype wrapping `String`, constructable only via
`SafeText::sanitise(&str)` or `SafeText::trusted_sgr(&str)`;
`SegmentOutput::text` becomes `SafeText`. Then forgetting to
sanitise is a compile error. Slice 12 is the natural time.

### [LOW] `safety` module placement is correct, with a caveat
**Location:** `crates/p10k-rs-core/src/safety.rs`.
**Issue:** The brief asks whether `sanitize_for_terminal` belongs in
`p10k-rs-segments` instead. No — `core` is the I/O-free contract
crate (`crates/p10k-rs-core/src/lib.rs:1-16`), `safety` is pure
function over `&str`, and the function is already consumed by both
`p10k-rs-git` (a non-segments crate) and `p10k-rs-segments`. Putting
it in segments would force `p10k-rs-git` to depend on segments,
which inverts the dep arrow. Caveat: `p10k-rs-config` (slice-12
TOML loader) and `p10k-rs-ai` (OSC emitter, even if mostly stub)
will both want it. `core` is the right home; the placement scales.
**Suggested fix:** None. Documenting this finding so the synthesis
pass records the placement was reviewed and confirmed.

### [LOW] `String::from_utf8_lossy` allocation pattern is wasteful
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:188`.
**Issue:** `sanitize_for_terminal(&String::from_utf8_lossy(fields[i]))`
materialises a `Cow<str>`, dereferences it as `&str`, then
`sanitize_for_terminal` allocates a fresh `String`. For the common
case (valid UTF-8, no control bytes), that's two allocations where
zero suffice. Performance lane will likely catch this; flagging
from architecture because the producer pattern will be replicated
across slice-12 segments.
**Suggested fix:** Have `sanitize_for_terminal` accept `&[u8]`
(or `Cow<str>`) and short-circuit returning `Cow::Borrowed` when
the input is already safe. Defer to perf-lane verdict.

### [LOW] Test boundary — unit-inline is correct here
**Location:** `crates/p10k-rs-core/src/safety.rs:55-128`,
`crates/p10k-rs-git/src/gitstatusd.rs:283-360`,
`crates/p10k-rs-git/src/lib.rs:170-186`,
`crates/p10k-rs-segments/src/dir.rs:65-130`.
**Issue:** All 20 new tests are inline `#[cfg(test)]` modules
calling private functions (`parse_response`, `parse_branch_header`,
`wrap_for_shell`, `sanitize_for_terminal`). That is the right
boundary for pure-function defence-in-depth tests — they pin
specific byte sequences against specific outputs, and they need
access to private parsing helpers. The missing piece is the
integration story (see [MEDIUM] above), not these tests' location.
**Suggested fix:** None to existing tests; add the integration
layer separately rather than relocating these.

### [LOW] ADR-0001 follow-ups verified still closed
**Location:** `docs/adr/0001-git-backend.md:103-107`.
**Issue:** Spike crate absent from `Cargo.toml`. `gix` absent
from `[workspace.dependencies]` (the comment at line 56-58 confirms
the strip and lists the resurrection conditions). `THIRD-PARTY-LICENSES.md`
present at repo root. Zero regressions in slice 11.
**Suggested fix:** None. ARCHITECTURE.md § 2.4 is still DEFERRED
(planning bundle outside repo); not a slice-11 regression.

### [LOW] No phantom-imports of `core::safety`
**Location:** workspace-wide (verified by grep on
`p10k_rs_core::safety` and `sanitize_for_terminal`).
**Issue:** Five hits across three crates, all in code paths that
genuinely consume the function. No `pub use` re-exports, no
test-only imports leaking. Dep graph for `safety` is exactly
`core` (defines) → `git` + `segments` (consume). Boundary is
clean.
**Suggested fix:** None.

### [INFO] M1 from prior review is closed by accident
**Location:** `crates/p10k-rs-core/src/lib.rs:222-226`
(percent-doubling pass).
**Issue:** Slice-9's M1 ("`wrap_for_shell` does not handle `%{`/`%}`
in untrusted text") is closed transitively: doubling every `%`
including those inside attacker-controlled `%{` payloads makes
the prior bracket-injection vector inert. Worth re-rating in the
synthesis index — it's not deferred, it's done.
**Suggested fix:** Mark M1 closed in the synthesis pass.

### [INFO] M3, M4 deferral is legitimate; H3, H4 remain real risk
**Location:** commit message "Deferred to the next swarm pass".
**Issue:** H3 (FIFO byte-injection via `\x1F`/`\x1E` in dirname)
and H4 (no req/resp correlation) are deferred to slice 12. The
decision is defensible — slice 11 is scoped to render-path defence,
not IPC framing — but H3 has the same root cause as C2 (unfiltered
external bytes flowing into a parser). If slice 12 doesn't land
within two weeks, this becomes tech-debt, because the pattern of
"sanitise-at-producer" we just established only covers display,
not framing.
**Suggested fix:** Track in synthesis as a slice-12 P0; consider
extending `safety` with a `frame_safe_for_us_rs` companion.

### [INFO] Test count credibility check passes
**Location:** workspace-wide.
**Issue:** Commit message claims 53 tests up from 33. The new tests
counted: 5 in `wrap_for_shell` (4 added, 1 existing reframed),
9 in `safety`, 2 in `gitstatusd::parse_response`, 1 in
`parse_branch_header`, 2 in `Dir`. That's ~19 new test functions —
within rounding of the +20 claim. No suspicious test-count inflation.
**Suggested fix:** None.

## Things this review explicitly did NOT examine

- Idiomatic Rust, ownership/borrowing patterns (lane 01).
- Privilege boundaries beyond the producer/consumer split (lane 02).
- Allocation counts inside `sanitize_for_terminal` beyond noting the
  double-alloc in `gitstatusd.rs:188` (lane 03).
- Naming, function length, comment quality of new code (lane 04).
- Module-level doc-comment grammar / ADR cross-refs in narrative
  prose (lane 05).
- IPC/FIFO H3/H4 — those are explicitly deferred and lanes 02/03 own
  the re-run.
- Wizard, config, ai, ipc placeholder crates — unchanged in slice 11.

## Confidence

High on the architectural verdict: read every changed file in the
slice, the surrounding callers, the prior two swarm summaries, the
ADR, the workspace manifest, the changelog, and the dep graph (via
`Cargo.toml` grep, no compilation). Medium on the
"what should slice 12 prioritise" claim — the C2-vs-H3 root-cause
similarity is a judgment call, not a measurement.
