# Documentation Review — 20260510T054710Z

## Summary

Slice 12-b lands a polished `SafeText` newtype with module-grade
docstrings: the type-level invariant, both constructors, the
`From<&str>` ergonomic note, and a runnable doctest are all present
and accurate. `GitState::branch` / `GitState::commit` and
`parse_branch_header` were updated in lockstep and now point at the
invariant correctly. The HIGH doc-drift items from the prior swarm
landed in 12-a as advertised. The one defect this lane finds is the
expected one: **CHANGELOG was not updated as part of slice 12-b**, so
the type-system commitment is recorded in commit messages only. A few
smaller issues sit alongside it (RESUME header still says
"post-slice-11", residual `Slice N` comments survive in init.zsh
despite 12-a's comment-strip pass, and ADR-0001 has no entry recording
the new producer→render type-system contract).

## Findings

### [HIGH] CHANGELOG missing slice 12-b entry

**Location:** `CHANGELOG.md`
**Issue:** 12-a's own entry (`CHANGELOG.md:71-77`) advertises that the
backlog is closed; 12-b shipped `SafeText`, changed `GitState::branch`
and `GitState::commit` from `String` to `SafeText`, and is the
implementation of the consensus HIGH from the prior swarm. None of
that is in the changelog. The Keep-a-Changelog header at the top
explicitly promises that breaking pre-1.0 changes are documented when
they occur — `GitState`'s field types changing is exactly that.
**Suggested fix:** Add `### Slice 12-b: SafeText newtype (1c8f80b)`
under `[Unreleased]` after the 12-a entry. Three sub-bullets:
(1) new `SafeText` type in `core::safety` with `from_untrusted` /
`from_untrusted_bytes` / `AsRef<str>` / `Display` / `From<&str>`
constructors and an `assume_safe`-free design; (2) `GitState::branch`
and `GitState::commit` migrated `String` → `SafeText` (downstream
consumers must call `.as_str()` or rely on `Display`); (3) closes the
2-reviewer consensus HIGH from `.review/20260510T052023Z/SUMMARY.md`.

### [HIGH] RESUME.md still labelled "post-slice-11"

**Location:** `/home/seaburdz/.planning/powerlevel10k-rs/RESUME.md`
**Issue:** Header (line 1) reads `RESUME — p10k-rs (post-slice-11)`,
the HEAD line (3) still points at `7438a37` (the slice-11 review
commit), and the working-tree note (4) only mentions 12-a. The body
still describes 12-b as "in flight … ~1 day" (line 92) when it has
landed at `1c8f80b`. RESUME is the next-session entry point — stale
HEAD makes the resume sequence on line 138-152 give a misleading
sanity check.
**Suggested fix:** Bump header to "post-slice-12-b", update HEAD to
`1c8f80b`, mark 12-b done in the slice menu, and append a one-line
note that the SafeText invariant is now type-enforced.

### [MEDIUM] Residual `Slice N` comments in init.zsh contradict 12-a's strip pass

**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:39`,
`crates/p10k-rs-shell/shells/zsh/init.zsh:56`,
`crates/p10k-rs-shell/shells/zsh/init.zsh:61`,
`crates/p10k-rs-shell/shells/zsh/init.zsh:79`,
`crates/p10k-rs-shell/shells/zsh/init.zsh:117`
**Issue:** 12-a's CHANGELOG entry promises "Stripped 21 stale
slice-number comments from source." Five `Slice 6/8/9` markers (and
one parenthetical "earlier slice 5") survived in init.zsh. Same
"forecasts and history that rot" rationale the readability + docs
lanes flagged at slice 11. Not a defect per se, but the changelog
overstates the scope of the cleanup.
**Suggested fix:** Either rewrite each comment present-tense
("instant-prompt cache" / "FIFO security: unpredictable directory
name…" / "FIFO teardown: refuse rm -rf on anything outside our
template"), or — at minimum — make the 12-a CHANGELOG entry honest
about what stripping covered (Rust source only).

### [MEDIUM] ADR-0001 has no entry for the producer→render type-system contract

**Location:** `docs/adr/0001-git-backend.md`
**Issue:** Slice 12-b promotes the sanitisation invariant from a
documentation convention to a load-bearing API contract: every
`GitState` field that flows untrusted bytes is `SafeText`, and the
private inner field plus absence of `assume_safe` make it impossible
for a future producer to short-circuit. RESUME.md § 9 already calls
this out as "the render-path safety boundary held together by doc
comments today" — that boundary just got teeth, and ADR-0001 is the
only place ADRs live, so it's the right home or it deserves its own.
ADR-0001's `Consequences` and `Follow-ups` sections were the natural
place; both are silent.
**Suggested fix:** Either add a short ADR-0002 ("Render-path safety:
`SafeText` newtype enforces sanitisation at the type level") with the
producer-emits / segment-consumes contract spelled out, or append a
one-paragraph "Render-path safety" subsection to ADR-0001 §
Consequences > Architectural pointing at `core::safety`. ADR-0002 is
cleaner — ADR-0001 is about the backend, not safety.

### [LOW] `SafeText` doctest doesn't exercise `from_untrusted_bytes`

**Location:** `crates/p10k-rs-core/src/safety.rs:68-73`
**Issue:** The module-level doctest covers `from_untrusted` and
`Display`. `from_untrusted_bytes` is the more interesting constructor
(it's what gitstatusd uses, and `String::from_utf8_lossy` semantics
are the part downstream readers will be uncertain about) — it has
unit tests but no doctest. Doc-comment on the function itself
(line 92-97) is correct but doesn't show output.
**Suggested fix:** Add one extra line in the existing doctest, e.g.
`assert_eq!(SafeText::from_untrusted_bytes(b"main\xff").as_str(),
"main\u{fffd}");`. Two-minute change.

### [LOW] `SafeText` docstring overstates the literal cost

**Location:** `crates/p10k-rs-core/src/safety.rs:62-63`
**Issue:** "Literals like `\"HEAD\"` go through sanitisation too;
the cost is one no-op pass over a tiny string." Slightly misleading:
the no-op path *also* allocates a fresh `String`, which the
`from_untrusted` doc on line 84-85 correctly notes. Module-level
docstring should match.
**Suggested fix:** Tweak to "the cost is one no-op scan plus a small
fresh allocation" (or drop the cost claim — `from_untrusted`'s own
docstring is the right place for it).

### [LOW] `parse_branch_header` doctest opportunity

**Location:** `crates/p10k-rs-git/src/lib.rs:96-105`
**Issue:** The function comment is now accurate (it returns `SafeText`,
the rationale about hand-written refs survives 12-b cleanly). But the
function is `fn` (private), so it has no public-API doc surface.
Consumers see only `Backend::status` and `GitState`. The
`SafeText`-flowing-out-of-the-shell-out-backend story is therefore
told only in the changelog and in `parse_branch_header`'s private
comment. Mostly fine — flagging as INFO-grade in case future readers
look at `Backend::status` and want a pointer to where the sanitisation
boundary actually fires.
**Suggested fix:** Optional. Add a one-line note on the `Backend`
trait (`crates/p10k-rs-git/src/lib.rs:36`) that returned `GitState`
fields satisfy the `SafeText` invariant.

### [INFO] gitstatusd untrusted-field comment is the new bar

**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:182-187`
**Issue:** Positive finding. The comment block on the
`untrusted_field` closure cleanly explains both the bytes-in pipeline
and the type-system payoff in 4 lines. It's the model for how
producer-side comments should read post-12-b.

### [INFO] README is not stale

**Location:** `README.md`
**Issue:** Verified line by line against `1c8f80b`. The "11 slices
shipped" list and the feature table reflect what's actually in the
repo. 12-a/12-b are correctly absent — they're hygiene + an internal
type-system change, neither of which moves the user-visible
slice-list. CHANGELOG is the right place for them, not README.

### [INFO] `safety` module-level docstring still aligned

**Location:** `crates/p10k-rs-core/src/safety.rs:1-14`
**Issue:** The producer-side / shell-aware-wrapper-side split
described on lines 7-13 still matches the implementation: producers
construct `SafeText` (now mandatorily via the new constructors),
`wrap_for_shell` does the `%`-doubling pass. No drift.

## Things this review explicitly did NOT examine

- Rust idiom of the `PartialEq<str>` / `PartialEq<&str>` /
  `PartialEq<String>` triad (rust-principles lane).
- Whether `SafeText` should be `Cow`-backed (performance lane).
- IPC protocol versioning or the H3/H4 carryovers (architecture +
  security lanes).
- Public API stability obligations of the `String` → `SafeText`
  field-type change for downstream segment crates outside this
  workspace (architecture lane).
- Code that was not touched in 12-b (style.rs, render_prompt,
  wrap_for_shell — all untouched since the slice-11 review).

## Confidence

**High.** The 12-b diff is small (188 lines, 5 files); I read it
end-to-end against the prior swarm's recommendations and verified
12-a's claims against the current tree. The CHANGELOG omission is
mechanical and unambiguous. RESUME staleness is a freshness call,
not a judgement call. The init.zsh and ADR findings are softer —
both are downstream consequences of slice 12-a/12-b's narrow scope,
not regressions in 12-b itself.
