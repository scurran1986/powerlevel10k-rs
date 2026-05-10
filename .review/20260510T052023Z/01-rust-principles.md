# Rust Principles Review — 20260510T052023Z

## Summary
Slice 11's defenses are correctly placed and the new `safety` module is
small, well-documented, and `forbid(unsafe_code)`-clean. The strongest
Rust-principle gap is that "sanitised" is enforced by call-site
discipline rather than by the type system: a `SafeText` newtype on
`GitState::branch`/`commit` (and on the cwd display string) would make
the next slice's segments compile-fail-loud instead of audit-loud. A
handful of smaller items: avoidable allocations on the hot path
(`Cow<str>`-able), one `Path::display()` choice that drops information
silently, and a couple of byte-indexed slices in `wrap_for_shell` that
are correct but rely on invariants that aren't stated near the code.

## Findings

### [HIGH] No type-level guarantee that "sanitised" is enforced at every boundary
**Location:** `crates/p10k-rs-core/src/lib.rs:382-402` and
`crates/p10k-rs-core/src/safety.rs:16-55`
**Issue:** `GitState::branch` and `GitState::commit` are plain `String`.
The contract that those fields have already been run through
`sanitize_for_terminal` lives only in producer comments
(`gitstatusd.rs:185-188`, `lib.rs:103-106`). Any new producer or
segment that fills `GitState` from a different source — the planned
gitoxide backend, a future `stash`/`url` field, a feature segment that
takes a user-typed branch — will silently re-introduce the C2 class.
Today `vcs.rs:46` happily emits `git.branch` straight into the prompt
with no per-call-site sanitiser; correctness rests on every producer
remembering. That is the exact discipline the C1/C2 audit caught
slipping the first time.
**Suggested fix:** Introduce a newtype in `safety` —
`pub struct SafeText(String)` (or `SafePromptText`) with a single
constructor `SafeText::sanitise(&str) -> Self` that runs
`sanitize_for_terminal`, plus `Display` and `AsRef<str>`. Change
`GitState::branch`/`commit` (and the eventual cwd field) to
`SafeText`. Producers can only construct it through the ctor; segments
borrow `&str` for `format!`/`push_str`. This moves the C2 invariant
from a doc comment to the borrow checker, with zero runtime cost.

### [MEDIUM] `sanitize_for_terminal` always allocates on the no-change path
**Location:** `crates/p10k-rs-core/src/safety.rs:42-55`
**Issue:** The function returns `String` unconditionally. For the
overwhelmingly common case — a branch like `main` or
`feat/widget-1.2` — every prompt render allocates and copies a fresh
`String` of identical bytes. Three call sites fire per render
(`gitstatusd.rs:188`, `git/lib.rs:105`, `dir.rs:26`), so it's three
hot-path allocations whose only purpose is to satisfy the type. This
is also exactly the kind of "the type system forces an alloc"
situation worth fixing while the API is one slice old.
**Suggested fix:** Return `Cow<'_, str>`: scan once for any disallowed
char with `s.find(...)`; return `Cow::Borrowed(s)` when none is
found, build an owned `String` only on the rewrite path. Existing
callers that bind to `String` via `let raw: String = ...` change to
`let raw = ...; let raw = raw.as_ref()` or accept the `Cow`. If the
team prefers strict ownership, gate this behind the `SafeText` newtype
above and have its ctor do the same Cow-style fast path internally.

### [MEDIUM] `wrap_for_shell` byte-indexes a `&str` without stating the invariant
**Location:** `crates/p10k-rs-core/src/lib.rs:202-230`
**Issue:** The loop indexes `bytes[i]` where `bytes = s.as_bytes()`,
slices `&s[i..=j]` and `&s[i..ch_end]`, and depends on the fact that
every byte the scan compares against (`0x1b`, `b'['`, `b'm'`, `b'%'`)
is ASCII, so `i` is always on a UTF-8 boundary when the slice is
taken. That is true today, but the helper `next_char_boundary` exists
specifically because the author had to fix up the fallthrough — the
invariant is load-bearing yet not asserted. A future addition that
matches a non-ASCII byte will compile and then panic at runtime on
unicode branch names.
**Suggested fix:** Use `s.char_indices()` as the driving iterator and
match on `char` (`'\x1b'`, `'%'`) rather than raw bytes. The SGR scan
that needs to find `b'm'` can stay byte-level inside the matched ESC
arm — at that point we've already established the scan starts at a
char boundary. Drop `next_char_boundary`. If the byte form is kept for
perf, add a `// SAFETY-ish:` comment near line 206 stating "all
matched bytes are ASCII, so `i` is always a char boundary here".

### [MEDIUM] `Path::display()` silently lossy where a value already exists
**Location:** `crates/p10k-rs-segments/src/dir.rs:26`
**Issue:** `ctx.cwd.display().to_string()` produces a `String` that
silently substitutes U+FFFD on non-UTF-8 path components. That is
intentional for *display*, but the function name doesn't make it
obvious to a reader that the type system has erased the
non-UTF-8-ness. Compare with `gitstatusd.rs:188` which spells
`String::from_utf8_lossy(...)` — same conversion, different ergonomics
and far clearer about what's happening.
**Suggested fix:** Use `ctx.cwd.to_string_lossy()` and pass the `Cow`
straight into `sanitize_for_terminal` (after the Cow refactor above).
That makes the lossy step a typed `Cow<'_, str>` value, mirrors the
other two boundary call sites, and skips one allocation when the path
is valid UTF-8 (the common case).

### [MEDIUM] `parse_response` numeric fields silently coerce non-ASCII to zero
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:180-206`
**Issue:** `parse_u` does `from_utf8(...).unwrap_or("").parse().unwrap_or(0)`.
That's three failure modes (non-UTF-8, empty, non-numeric) all
collapsed to `0`. For ahead/behind/staged this masks daemon-protocol
drift behind a clean repo. The branch/commit fields got the M2 fix
(lossy + visible U+FFFD); the numeric fields stayed silent. If a
future daemon ever returns a payload with a stray byte in a count
field, the user sees "clean" and shrugs — defeating the point of the
audit-driven hardening.
**Suggested fix:** Make `parse_response` return
`Result<GitState, ParseError>` with a `thiserror`-style enum. Internal
helpers return `Result<u32, _>`; the function `?`-propagates. Keep
the existing `Option` semantics by mapping `Err` → `None` at the
single call site in `Backend::status`, and log the parse error at
`tracing::debug!` so an operator can see drift. Also gives a hook for
slice-12's "request/response correlation" find (H4 deferred).

### [LOW] `untrusted_field` closure captures `fields` and re-allocates per call
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187-188`
**Issue:** A closure `|i: usize| -> String` is fine at this scale, but
it (a) hides that each invocation runs `from_utf8_lossy` which itself
sometimes allocates, and (b) when combined with `String::from_utf8_lossy`
followed by `sanitize_for_terminal`, can allocate up to two strings
per untrusted field. Mostly a perf concern (other lane) but a
principle-level smell: the type signature obscures the cost.
**Suggested fix:** Replace the closure with a free function
`fn sanitised_field(bytes: &[u8]) -> SafeText` (using the proposed
newtype) so the cost is visible at the call site and the boundary is
a single named function. Once `sanitize_for_terminal` returns `Cow`,
this collapses to one alloc only on the rewrite path.

### [LOW] `wrap_for_shell` returns `String` even on the no-op path
**Location:** `crates/p10k-rs-core/src/lib.rs:196-201`
**Issue:** `if shell != Shell::Zsh { return s.to_owned(); }` and the
`!contains('\x1b') && !contains('%')` branch both clone the input. For
non-zsh shells this is one extra allocation per render with no
purpose; the caller already owns the input string.
**Suggested fix:** Have `render_prompt` accept the assembled `String`
by value and pass it through unchanged when no work is needed, or
return `Cow<'_, str>` from `wrap_for_shell` and let `render_prompt`
finalise into `Prompt.left`. The `s.contains('%')` early-out check is
also doing two passes over the string in the worst case (`contains`
twice + the loop); fold it into the single rewrite loop or drop it.

### [LOW] `is_fifo` mixes inline `use` with module-level imports
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:230-241`
**Issue:** The function pulls in `std::os::unix::fs::FileTypeExt` and
`MetadataExt` inside the function body. With `#![forbid(unsafe_code)]`
this is fine, but it's the only function in the crate doing
function-local `use` of OS extension traits, which makes the
cross-platform story implicit (the file silently won't compile on
Windows). Other crates import OS traits at module top.
**Suggested fix:** Move both `use` lines to the module's top with a
`#[cfg(unix)]` guard, and add a sibling `#[cfg(not(unix))]` stub that
returns `false`. Same behaviour, but the platform contract becomes
local-grep-able and `cargo check --target x86_64-pc-windows-gnu`
gives a coherent error if anyone tries.

### [LOW] `parse_branch_header_raw` allocates twice for the common case
**Location:** `crates/p10k-rs-git/src/lib.rs:108-120`
**Issue:** `local.split_whitespace().next().unwrap_or("").to_owned()`
allocates a `String`, which is then handed to `sanitize_for_terminal`
which allocates again. The function is `String`-returning by signature,
so the first alloc is forced. Together with the always-allocate
behaviour of the sanitiser, it's two allocs per render even for the
all-ASCII `## main\n` case.
**Suggested fix:** Change `parse_branch_header_raw` to return
`&str` borrowing from `header`, and have `parse_branch_header` do the
single `sanitize_for_terminal(...).into()` (Cow form) at the end.
Combined with the `Cow` refactor above this becomes zero alloc on the
fast path.

### [LOW] `next_char_boundary` is a re-implementation of `str::ceil_char_boundary`
**Location:** `crates/p10k-rs-core/src/lib.rs:234-240`
**Issue:** `str::ceil_char_boundary` was stabilised in 1.79; the
crate's MSRV is 1.88 (`clippy.toml:3`). The hand-rolled version is a
small thing, but it's a maintenance item that the standard library
already owns.
**Suggested fix:** Delete `next_char_boundary` and call
`s.ceil_char_boundary(i + 1)` from line 226. Or, better, drop the
helper entirely as part of the `char_indices` refactor in the
`wrap_for_shell` finding above.

### [INFO] `safety` module is the right shape for a type-system upgrade
**Location:** `crates/p10k-rs-core/src/safety.rs:1-128`
**Issue:** Not a defect — an observation. The module currently exposes
one free function. It's the natural home for the `SafeText` newtype
finding above, plus a future `SafeAnsi` / `SafeOsc` if segments ever
need to round-trip an SGR through validation. Keeping the type-level
contract centralised here, instead of sprinkling `assert!(!_.contains('\x1b'))`
across segments, will pay off as more segments land. The existing
docs (lines 1-14) already frame the module that way; the next step is
giving it teeth.
**Suggested fix:** None for slice 11. Plumb the `SafeText` newtype
through in slice 12 alongside the deferred IPC findings.

## Things this review explicitly did NOT examine
- Threat-model completeness or new injection vectors (security lane).
- Allocation/syscall budget against MVP-SPEC § 0 — flagged hot-path
  allocs as principle issues, not as perf-budget violations.
- Naming, comment quality, function length (readability lane).
- Module-doc and ADR alignment (documentation lane).
- ADR-0001 conformance and slice boundary cleanliness (architecture
  lane).
- The `gitstatus/` C++ subtree, `bench/` fixtures, `target/` artefacts.
- IPC-lifecycle / FIFO byte-injection findings deferred by the slice
  message (H3, H4) — those are owned by the next swarm pass.
- Test discipline beyond noting that the new tests cover the C1/C2
  reproducer payloads.

## Confidence
**High** for the typed-boundary finding, the `Cow`/allocation findings,
and the byte-indexing observation — those follow directly from reading
the touched files. **Medium** on the `parse_u` "silent zero" framing:
it's a real principle gap, but the right `Result` shape depends on
slice-12's wire-protocol-correlation work, which I deliberately did
not read. **Low/Info** items are unsurprising nits.
