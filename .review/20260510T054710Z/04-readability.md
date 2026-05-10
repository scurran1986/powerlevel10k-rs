# Readability Review — 20260510T054710Z

## Summary

Slice 12-b lands the `SafeText` newtype cleanly. Naming is the main
thing left to argue with: `SafeText` is vaguer than the operation it
guarantees (sanitisation), and the doc on the type repeats what the
free function already says. The previous slice's stale-comment fix
held — `grep -rEn 'slice [0-9]|Slice [0-9]'` over `crates/` returns
zero, including the new file. `wrap_for_shell` continues to do three
jobs in one body and was not addressed by 12-b; the slice-11 finding
stands.

## Findings

### [MEDIUM] `SafeText` is named for a property, not the operation
**Location:** `crates/p10k-rs-core/src/safety.rs:78-79`
**Issue:** The codebase's verb is `sanitize_for_terminal`, the
constructors are `from_untrusted*`, and CHANGELOG/commit prose says
"sanitised". The type then breaks the pairing as `SafeText`. "Safe"
is softer than "sanitised" ("safe for what?" is left implicit) and
fails the grep-test: a reader chasing the invariant can't find type,
function, and tests with a single `sanitis*` search. `SanitisedText`
or `SanitisedForPrompt` reads more honestly; the generic `Safe-`
prefix is the weakest of the three.
**Suggested fix:** Rename to `SanitisedText` in a follow-up slice
before downstream crates pick it up. Keep constructor names. CASE
flag: do the churn once, now, rather than live with the mismatch
through the rest of the build-out.

### [MEDIUM] `wrap_for_shell` still does three jobs in one body
**Location:** `crates/p10k-rs-core/src/lib.rs:197-233`
**Issue:** Slice 11's swarm raised this and 12-b did not touch it.
The function is ~37 lines of body and a reader has to hold three
distinct concerns at once: SGR escape detection and `%{…%}`
wrapping (208-221), literal `%` doubling for zsh (222-226),
non-zsh passthrough plus `\x1b`-and-`%`-free fast path (198-203).
The state machine reads as one loop because byte `i` advances by
either an SGR span, two-byte `%%`, or one char-boundary, which
is fine — but the cognitive load comes from naming, not from the
bytes. There is no helper named `wrap_sgr_escape`, no helper named
`double_percent`, no helper named `next_char_boundary` paired with
the SGR scan. A reader chasing "where does `%` doubling live?" has
to scan the loop to find the right branch.
**Suggested fix:** Split into `fn wrap_for_zsh(s: &str) -> String`
plus two private helpers — `scan_sgr(bytes, i) -> Option<usize>`
returning the end index of an `\x1b[…m` span at `i`, and a
`copy_one_char(s, i, out) -> usize` that handles the unicode
boundary advance. The top-level `wrap_for_shell` then dispatches
to `wrap_for_zsh` or returns `s.to_owned()`. Same code, three
named pieces, each one tested in isolation.

### [LOW] `SafeText` doc-comment overlaps the module doc
**Location:** `crates/p10k-rs-core/src/safety.rs:57-77`
**Issue:** The type doc spends 21 lines re-stating threat-model
content (no control bytes, no DEL, why literals get sanitised, the
no-`assume_safe` policy, `From<&str>` rationale) that is already
covered in `safety.rs:1-14` (module) and the `sanitize_for_terminal`
doc above it (`safety.rs:16-40`). It earns its weight but it's
duplication: a maintainer who tweaks the threat model now has three
places to keep in sync. The runnable doctest (lines 68-73) is the
piece that justifies the length on its own — the prose around it
could shrink to two sentences plus a pointer to the module doc.
**Suggested fix:** Trim the type-level doc to: one sentence on the
invariant ("inner string has passed `sanitize_for_terminal`"), the
runnable doctest, one sentence on the `From<&str>` ergonomics. Move
the "no `assume_safe` by design" rationale to the module doc, where
it sits next to the rest of the threat model.

### [LOW] `untrusted_field` closure name is fine
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187`
**Issue:** Brief asked. Closure names by *input* (untrusted wire
bytes) not *output* (`SafeText`). Either reading works; the name
matches the constructor (`from_untrusted_bytes`). Sibling closures
on the same lines (`s`, `parse_u`) name by output, so `safe_field`
would be the symmetric choice.
**Suggested fix:** Leave it. If touched, rename to `safe_field`.
Not worth a slice.

### [LOW] `parse_branch_header_raw` is a private helper with no doc
**Location:** `crates/p10k-rs-git/src/lib.rs:107-119`
**Issue:** The pre-12-b `parse_branch_header` returned `String`;
12-b kept the parsing logic in `parse_branch_header_raw` (no doc)
and made `parse_branch_header` (`:103-105`) a one-line wrapper that
sanitises. The split is fine but the `_raw` suffix is a code smell:
a reader sees two functions with near-identical names and has to
diff them to see which one is safe to call. `_raw` says "this one
is dangerous" but doesn't say *why*.
**Suggested fix:** Either inline `parse_branch_header_raw` into
`parse_branch_header` (it's 12 lines, one allocation either way), or
rename it `parse_branch_header_inner` and add a one-line `///`
saying it returns the unsanitised local-branch substring and is
only callable through the sanitising `parse_branch_header`. The
inline option is cleaner — there is no other caller of the `_raw`
variant.

### [LOW] `vcs.rs` `plain.len() - marker.len()` mixes byte and char widths
**Location:** `crates/p10k-rs-segments/src/vcs.rs:65, 74`
**Issue:** Not introduced by 12-b but adjacent. Line 65 computes
`plain_len` from `chars().count()` (display columns). Line 74
recomputes a split point with `plain.len()` (bytes). For ASCII
markers (`*`, `!`, ``) the two coincide; the day someone localises
the marker to a non-ASCII glyph the split point silently slices
mid-codepoint. A reader has to notice that the `&plain[..split]`
on line 76 is byte-indexed, which the surrounding `chars()` math
disguises.
**Suggested fix:** Document the ASCII assumption next to line 74
or split with `plain.rfind(marker)`. Keeping it byte-indexed is
fine — make the "marker is ASCII" precondition visible.

### [INFO] Stale slice-comment fix held — no new regressions
**Location:** workspace-wide
**Issue:** Per-brief: ran `grep -rEn 'slice [0-9]|slice-[0-9]|Slice
[0-9]|Slice-[0-9]' --include="*.rs" -- crates`; the result is empty.
The new `safety.rs` did not reintroduce slice numbers (the commit
message and CHANGELOG carry that context, the source does not).
Slice 11's regression is closed and slice 12-b respected the
discipline.
**Suggested fix:** None. Worth keeping as a smoke test in CI: the
grep is one line and stops the comment-rot from coming back.

### [INFO] `SafeText` doctest using `format!` is the right kind of doc
**Location:** `crates/p10k-rs-core/src/safety.rs:68-73`
**Issue:** Observation, not a defect. The runnable example proves
both `as_str()` and `Display` work as advertised in eight lines.
This is exactly the kind of weight a long doc-comment earns.
**Suggested fix:** Keep this idiom for `Sanitised…`'s eventual
replacements.

### [INFO] `is_empty()`/`len()` pair on `SafeText` is the polite default
**Location:** `crates/p10k-rs-core/src/safety.rs:111-120`
**Issue:** Clippy's `clippy::len_without_is_empty` would yell if
`is_empty()` weren't there. Both have one-line `///`. Cheap, correct,
no concerns.
**Suggested fix:** None.

## Things this review explicitly did NOT examine

- Idiomatic Rust ownership/borrowing/error handling (rust-principles lane).
- Threat-model adequacy of `sanitize_for_terminal` (security lane).
- Allocation cost of `from_untrusted` on hot paths (performance lane).
- `CHANGELOG.md` / `RESUME.md` / planning bundle currency (documentation lane).
- ADR alignment, slice-boundary cleanliness, test discipline (architecture lane).
- The IPC-carryover slice (12-c) — out of scope for 12-b.

## Confidence

High. Read-only inspection of the five source files touched by 12-b
plus the prior slice's commit. The naming finding (SafeText →
SanitisedText) is opinionated — surfaced as MEDIUM not HIGH because
the current name isn't *wrong*, just less honest than the codebase's
own vocabulary suggests it should be. The `wrap_for_shell` re-flag
is mechanical: same function body slice 11 already objected to,
unchanged in 12-b.
