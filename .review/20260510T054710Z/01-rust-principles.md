# Rust Principles Review — 20260510T054710Z

## Summary

Slice 12-b is a textbook newtype-as-invariant: a private `String`,
sanitising constructors only, no `unsafe`, no `assume_safe` escape
hatch. The producer migration in `gitstatusd::parse_response` and
`git::parse_branch_header` cleanly removes the "remember to call
`sanitize_for_terminal`" foot-cannon. A few trait choices are slightly
asymmetric and there's a missed allocation on the no-op fast path,
but nothing here is a ship-blocker. The critical concern from a
type-theoretic angle is the `From<&str>` impl: it is the right
ergonomic call, but it makes `SafeText` a *lossy* `From`, which
violates the conventional "`From` is total and information-preserving"
expectation. Worth a docs nudge, not a redesign.

## Findings

### [MEDIUM] `From<&str>` is silently lossy and contradicts the trait's de-facto contract
**Location:** `crates/p10k-rs-core/src/safety.rs:135-139`
**Issue:** Rust convention (and the std lib's own docs on
`From`/`Into`) treats `From` as cheap, total, and *information
preserving*. `SafeText::from("main\rEVIL")` silently drops bytes —
that is not what most reviewers reading `let t: SafeText = s.into();`
will expect. It also makes the conversion lossy for any code path
that does generic `T: Into<SafeText>` (e.g. test fixtures, future
config deserialization). The current docs (lines 75-77) call it
"sugar" but don't warn that the round-trip is not the identity.
**Suggested fix:** Either (a) keep the impl and add a one-line "this
is a sanitising constructor; the conversion is lossy" remark in the
`From` impl's own doc-comment, or (b) drop `From<&str>` and require
explicit `SafeText::from_untrusted(...)` everywhere. (a) is the
pragmatic call; the existing "sugar" framing is what makes
`branch: "main".into()` readable in tests. The 60-test suite leans
on this and removing it doubles test churn. Just document the
loss-of-bytes property where the impl lives.

### [MEDIUM] `from_untrusted` always allocates, even on the no-op path
**Location:** `crates/p10k-rs-core/src/safety.rs:42-55`, `87-89`
**Issue:** `sanitize_for_terminal` unconditionally builds a fresh
`String` via `String::with_capacity(s.len())` and pushes char-by-char,
even when the input is already entirely safe — which is the common
case (`"main"`, `"HEAD"`, every well-behaved branch name, every commit
OID). For a per-prompt hot path that ADR-0001 budgets in milliseconds
this is a real allocation per `SafeText` constructed (two for
`gitstatusd::parse_response` per prompt, more once `dir` migrates).
The type system does not force this — it is an implementation choice.
**Suggested fix:** Bytes-scan first, allocate only on first unsafe
codepoint. Sketch:
```rust
pub fn sanitize_for_terminal(s: &str) -> Cow<'_, str> { … }
```
plus a `SafeText::from_untrusted` that does
`Self(sanitize_for_terminal(s).into_owned())`. The cost is a single
linear scan (already paying it) and a one-byte branch. Performance
agent will likely flag the same; coordinate.

### [MEDIUM] `PartialEq` impls are asymmetric — only `SafeText == &str`, not `&str == SafeText`
**Location:** `crates/p10k-rs-core/src/safety.rs:141-162`
**Issue:** The reverse-direction comparison is intentionally not
provided (line 143 comment). That is a defensible scope choice, but
it leaves a real ergonomic crater: `assert_eq!("main", git.branch)`
fails to compile while `assert_eq!(git.branch, "main")` works. Test
authors hit this the moment they swap argument order out of habit.
The cost of providing the reverse impls is three trivial
`impl PartialEq<SafeText> for str/&str/String` blocks; the "foreign
impl footprint" rationale is undersold (these are local impls on
local-or-fundamental types, all permitted by orphan rules in this
crate).
**Suggested fix:** Add the reverse impls. Six lines, zero risk,
removes a class of "why won't this compile" surprise. If the concern
is symbol bloat, a `macro_rules!` keeps it to one block.

### [MEDIUM] No `Borrow<str>`; can't key a `HashMap<SafeText, _>` by `&str`
**Location:** `crates/p10k-rs-core/src/safety.rs:123-127`
**Issue:** `AsRef<str>` is implemented but `Borrow<str>` is not. They
look similar but mean different things: `Borrow<str>` requires that
`x.borrow() == y.borrow()` and `hash` agree on equivalent forms,
which lets `HashMap<SafeText, V>::get("main")` work. With only
`AsRef`, callers must clone or wrap to query. The slice doesn't
exercise this yet, but config keys, segment-state lookups, and any
caching layer (`HashMap<SafeText, …>` for resolved branches, for
instance) are obvious near-future consumers.
**Suggested fix:** Add `impl Borrow<str> for SafeText { fn borrow(&self) -> &str { &self.0 } }`.
Verify `Hash` derives produce the same value as `str::hash` on the
inner contents (they do, because `String: Hash` delegates to its
`str` slice). Three lines.

### [LOW] `Display` bypasses width formatting — fine here, but worth a one-liner
**Location:** `crates/p10k-rs-core/src/safety.rs:129-133`
**Issue:** `f.write_str(&self.0)` skips the formatter's `width`,
`fill`, `align`, and `precision` handling. `format!("{branch:>10}")`
silently right-aligns *nothing*; the user sees the raw string. For
this codebase that is the correct behaviour — segments compute their
own width via `plain.chars().count()` (`vcs.rs:65`) and width-padding
should never apply to user-supplied content that may contain
multi-cell glyphs. But a future contributor may try `format!("{x:>5}")`
and hit a surprising no-op.
**Suggested fix:** One sentence in the `Display` impl's doc comment:
"Width/precision flags are ignored — segment width must be computed
upstream because grapheme cells, not bytes, drive prompt geometry."
Alternatively, route through `f.pad(&self.0)`, which honours width
in *byte* terms — which would be wrong here, so the current write_str
is the right call. Document, don't change.

### [LOW] `parse_branch_header_raw` survived as a private split that no longer earns its keep
**Location:** `crates/p10k-rs-git/src/lib.rs:103-119`
**Issue:** `parse_branch_header` is now a one-liner that delegates
to `parse_branch_header_raw`; the split made sense when sanitisation
was a separate post-processing step. With `SafeText` owning the
invariant, the indirection is dead weight. The compiler will inline
it, but a human reader has to chase one more hop.
**Suggested fix:** Inline `parse_branch_header_raw` into
`parse_branch_header` and wrap with `SafeText::from_untrusted` at
the return. Saves one function, one stack frame, and a small mental
load. Tests (`lib.rs:172-185`) already use the public wrapper, so no
test churn.

### [LOW] Helper closure in `parse_response` collapses three calls — readability win, no semantic change
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187`
**Issue:** The change from `|i| sanitize_for_terminal(&from_utf8_lossy(fields[i]))`
to `|i| SafeText::from_untrusted_bytes(fields[i])` is a clean win:
fewer composed calls at call sites, the type carries the invariant,
no behaviour drift. Worth flagging as a deliberate good — keeps the
pattern visible for the other side-quests (untrusted commit-OID,
upstream remote URL, etc. once those land).
**Suggested fix:** None. Continue the pattern as new untrusted
fields are added.

### [LOW] `is_empty` and `len` make the API feel `String`-like — confirm or trim
**Location:** `crates/p10k-rs-core/src/safety.rs:111-120`
**Issue:** `len()` returns bytes (per its own doc). For a type whose
whole purpose is terminal display, the more-useful metric is column
width (`UnicodeWidthStr::width`) or grapheme count. Exposing byte-len
invites the same mistake `vcs.rs:45` makes (`git.branch.len() + 16`
for capacity — fine for capacity, wrong for visual layout). Keep
`is_empty` (it's unambiguous), but consider whether `len` belongs
or should be renamed `byte_len` to discourage misuse.
**Suggested fix:** Either rename to `byte_len` for clarity or add a
doc warning that this is not column width. Low priority — `String`
itself has the same trap.

### [INFO] `RenderCtx::cwd` deferral is the right call
**Location:** `crates/p10k-rs-core/src/lib.rs:106`, `crates/p10k-rs-segments/src/dir.rs:26`
**Issue:** Cwd is a `&Path`, not a string; sanitising it requires a
`display().to_string()` round-trip that already happens inside
`Dir::render` (line 26). Migrating to `SafeText` would either force
an eager allocation in `RenderCtx` construction (every prompt, even
when no segment reads cwd) or invent a `SafePath` newtype with the
same invariant on `OsStr`. Neither is justified by the slice-11
finding.
**Suggested fix:** None. The deferral is consistent with the
"newtype where bytes flow from outside, but only at the leaf" rule.

### [INFO] No `unsafe`, `#![forbid(unsafe_code)]` upheld
**Location:** `crates/p10k-rs-core/src/lib.rs:18`
**Issue:** `safety.rs` is pure-safe — chars-iter, push, no
`from_utf8_unchecked`. Crate-level `forbid(unsafe_code)` would have
caught any regression.
**Suggested fix:** None.

### [INFO] Test coverage for `SafeText` is right-sized
**Location:** `crates/p10k-rs-core/src/safety.rs:236-274`
**Issue:** Seven targeted tests covering: control-strip on
construction, lossy bytes, default, Display, From sugar. No
property-test (proptest) for "sanitised output never contains
controls" — would be a small addition with `proptest = { version =
"1", ... }`, but the deterministic cases here are the real defenders.
**Suggested fix:** Optional follow-up: add a single proptest that
asserts `sanitize_for_terminal(arbitrary_string).chars().all(|c|
c == '\t' || (!c.is_control() && c != '\u{7F}'))`. Belt-and-braces.

## Things this review explicitly did NOT examine

- Security threat model beyond Rust-trait shape (see lane 02).
- Allocation budget vs MVP-SPEC § 0 milliseconds (see lane 03).
- Naming, comment voice, function length (see lane 04).
- Doc-comment completeness vs ADR-0001 narrative (see lane 05).
- Cross-slice ADR alignment, slice-boundary cleanliness (see lane 06).
- Whether `dir.rs` should migrate — covered as INFO only.
- The `gitstatusd` wire protocol itself.

## Confidence

**High.** The diff is small (147 lines added, mostly tests), the
newtype pattern is canonical Rust, and the call sites are all on the
producer side where the invariant lives. The findings above are
correctness-of-design nits, not behavioural risks. Low confidence on
the `Borrow<str>` recommendation only because no current consumer
needs it — that one is genuinely speculative.
