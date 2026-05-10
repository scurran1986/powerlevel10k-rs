# Readability Review — 20260510T052023Z

## Summary
Slice 11 lands as the most readable security slice so far: the new
`safety` module is small, well-named, and exemplary in the way it ties
its rationale to a single function. The two real readability regressions
are inside `wrap_for_shell` (now ~37 lines mixing three concerns inside
one byte-walking loop) and the new convention of leaving auditor
finding codes (`C1`, `C2`, `M2`) as anonymous markers in source
comments. The previously flagged stale-slice-comment problem did not
get worse but did not get better either; one new slice-numbered comment
landed.

## Findings

### [MEDIUM] `wrap_for_shell` now has three concerns in one loop
**Location:** `crates/p10k-rs-core/src/lib.rs:195-231`
**Issue:** Slice 11 added the literal-`%`-doubling pass inline, on top
of the existing SGR-bracketing pass and the unicode-boundary fallback.
The function is now a 37-line `while i < bytes.len()` loop with three
interleaved branches (SGR scan and wrap, `%` doubling, unrecognised-
byte advance) plus a quick-exit guard at line 199. The control flow
relies on the reader noticing that the SGR branch `continue`s past the
`%` branch — so `%` inside an SGR body is *not* doubled. That is
correct (SGR bodies contain only `0-9`, `;`, `m`), but the proof is
implicit. The doc comment from line 182-194 carries the entire load.
**Suggested fix:** Split into two passes. A `wrap_zsh_sgrs(s) -> String`
that handles only the bracketing (already most of the existing loop),
followed by a `double_zsh_percents(s) -> String` that walks the result
and doubles `%` outside `%{…%}` regions. The two passes can be fused
later if a profiler ever flags the extra allocation; today the prompt
runs once per Enter press and clarity dominates. Bonus: each pass is
trivially unit-testable in isolation (the existing tests already split
along this seam).

### [MEDIUM] Auditor finding codes leak into source as anonymous markers
**Location:** `crates/p10k-rs-segments/src/dir.rs:25,101,113`,
`crates/p10k-rs-core/src/lib.rs:285`,
`crates/p10k-rs-git/src/gitstatusd.rs:185-186,290,320`
**Issue:** Seven new comments now reference `C1`, `C2`, `M2` with no
in-repo glossary. These are slice-9 review-finding IDs that live in
`.review/20260509T130000Z/EXTRACTED-FINDINGS.md`. A reader six months
from now hitting `// C2 reproducer:` has no way to discover what `C2`
is unless they happen to know the review-swarm convention. The
commit message defines them; the code does not.
**Suggested fix:** Inline the meaning at first use per file, then drop
the codes. E.g., `// Prevent terminal-escape injection via untrusted
cwd (slice-9 audit C2)` once at the top of the relevant function;
later mentions in the same file can say `// see above`. Or, equivalent,
add a one-line link in `safety.rs`'s module doc to the audit doc by
relative path so a grep for `C2` lands somewhere informative.

### [MEDIUM] Stale slice-number comments — net +1 since last review
**Location:** workspace-wide; 21 occurrences (was 19 at slice 9)
**Issue:** The previous swarm flagged 19 stale `[Ss]lice [0-9]`
references. Slice 11 added two new ones (`gitstatusd.rs:56` "Slice 7
adds a `poll(2)`-based deadline …", `gitstatusd.rs:247` "Slice 9
dropped the dev-machine fallback …") and removed zero. The new ones
are *historical* annotations rather than future-tense promises, which
is slightly less misleading, but they still anchor the comment to a
release-engineering concept (slice numbering) instead of the code's
own behaviour. A reader doesn't need to know which slice added the
deadline; they need to know there *is* a deadline. Findings 04-MEDIUM
in `.review/20260509T071500Z/04-readability.md` stands.
**Suggested fix:** Same as last time — convert each to present-tense
behaviour-only prose. "A `poll(2)`-based deadline ensures a wedged
daemon falls back to `ShellOut` instead of hanging the prompt
indefinitely." No slice number needed. A maintenance slice could sweep
all 21 in one commit.

### [LOW] `read_until_with_deadline` — clamp comment overstates its effect
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:136-138`
**Issue:** Comment reads "poll's i32 ms argument: clamp to i32::MAX
(~24 days). Way past any reasonable timeout." The `unwrap_or(i32::MAX)`
fallback only fires if `as_millis()` returns `> i32::MAX` (~24 days),
which can only happen if `timeout` itself was set to ~24+ days — the
code is correct, but the comment is reassuring the reader about a path
that's effectively dead. Reading the function I had to stop and verify
that `remaining` couldn't somehow exceed `timeout`. It can't (deadline
is computed once; `remaining = deadline - now`).
**Suggested fix:** Replace the two-line comment with one line:
`// poll(2) takes i32 ms; saturate at i32::MAX for absurdly large timeouts.`
Less reassurance, more precision.

### [LOW] `next_char_boundary` is unnecessarily named
**Location:** `crates/p10k-rs-core/src/lib.rs:233-240`
**Issue:** The helper exists because the SGR-scanning loop indexes
into `bytes` and may land mid-codepoint when fall-through hits a
non-ASCII byte. That's fine, but the name "next char boundary" hides
the intent. It's only ever called from one site (line 226) and only
with `i` already known to be at a valid boundary — what it really
returns is the *length of the char starting at `i`*, plus `i`. Inline
or rename to `char_end(s, i)` and the call site reads "advance past
this char".
**Suggested fix:** Rename to `char_end` and add a one-line `# Panics`
note (it doesn't panic; it just relies on `i` being a boundary). Even
simpler: `s[i..].chars().next().map_or(s.len(), |c| i + c.len_utf8())`
inline at the single call site removes the helper.

### [LOW] `safety` module is well-named but the `crate::style` neighbour invites confusion
**Location:** `crates/p10k-rs-core/src/lib.rs:24-25`
**Issue:** `pub mod safety;` and `pub mod style;` sit next to each
other. Both are about output presentation. `style` is the obvious home
for ANSI color helpers; `safety` is the chokepoint for control-byte
stripping. The names are correct individually but together they hint
that the module split is by concern (presentation vs. defence) rather
than by abstraction. A new reader scanning the public API of `-core`
needs the module-doc on `safety` to disambiguate — and that doc
delivers (lines 1-14 are crisp). Pure observation, not a defect: the
naming holds up.
**Suggested fix:** None required. Worth calling out only because the
module name was an explicit review item. `sanitize_for_terminal` is
self-documenting in isolation (no leaked abstraction), and the
function-level doc at lines 16-40 explains the *threat model* (zsh `%`
expansion is delegated; control bytes are this function's job),
preventing a caller from over-trusting it.

### [LOW] `wrap_for_shell` doc comment buries the `%` rationale
**Location:** `crates/p10k-rs-core/src/lib.rs:182-194`
**Issue:** The doc opens with "Per-shell escape-wrapping for the
assembled prompt string" — accurate but generic. The interesting
content (why `%` doubling, why only zsh, why segments don't trigger it)
starts mid-paragraph at line 187. A reader skimming for the dangerous
operation has to read three lines of bullet-list framing before
reaching the threat description.
**Suggested fix:** Lead with the threat. "Wrap the assembled prompt
for the target shell. For zsh this does two things: bracket SGR
escapes in `%{…%}` so prompt-width math is correct, and double every
literal `%` so attacker-controlled text can't trigger zsh PROMPT
expansion (the `%n@%m` injection vector). Bash and fish: pass-through."
Then the bullet list as detail.

### [LOW] `untrusted_field` closure name is the only naming win — exploit it
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187-188`
**Issue:** This closure is the slice's clearest piece of teaching code:
the *name itself* tells you the field came from outside the trust
boundary, and the body shows exactly what defence applies. Compare to
the `s` and `parse_u` closures right above, which are bland accessors.
The asymmetry is correct but inconsistent. A reader wonders whether
fields read via `s` or `parse_u` are also untrusted (they are — `parse_u`
falls back to 0 on garbage, which is silent rather than safe in some
contexts).
**Suggested fix:** Either rename `s` to `trusted_str` (it's only used
for fixed sentinels at fields[1] = "1") and document the trust
asymmetry, or, simpler, drop the `s` helper and inline the two uses.
The closure-trio currently reads as if all three are equivalent
accessors when only one of them carries security responsibility.

### [LOW] `parse_branch_header` has a non-obvious whitespace-cuts-CR side effect
**Location:** `crates/p10k-rs-git/src/lib.rs:103-120` plus test at
lines 174-186
**Issue:** The test comment at line 179-182 explains that `\r` is
Unicode `White_Space`, so `split_whitespace` truncates the branch name
at the CR before sanitisation runs. That's correct behaviour for this
parser but it's load-bearing reasoning living *only* in the test. The
parser itself (`parse_branch_header_raw`) doesn't mention that
`split_whitespace` is doing double duty as a defence-in-depth.
**Suggested fix:** Move the rationale up to `parse_branch_header_raw`
as a `// Note:` or `// Defence-in-depth:` line. The test can then say
"see parser comment".

### [INFO] Module doc on `safety` is a model for the codebase
**Location:** `crates/p10k-rs-core/src/safety.rs:1-14`
**Issue:** Not a defect. The `//!` block answers four questions in 14
lines: what the module is for, what threats it addresses, where the
boundary is, and what's *not* its job (zsh `%` handling). The "applied
later in `wrap_for_shell`" sentence is exactly the kind of cross-module
pointer the rest of the codebase needs. Use this as the template when
documenting future `-core` modules.

### [INFO] Test names in `safety.rs` and `gitstatusd.rs` are documentation
**Location:** `crates/p10k-rs-core/src/safety.rs:60-127`,
`crates/p10k-rs-git/src/gitstatusd.rs:288-360`
**Issue:** Not a defect. `passes_plain_ascii_through`,
`strips_carriage_return`, `parses_non_utf8_branch_lossily_rather_than_dropping`
— each test name is a one-line spec. A reader who only skims the
`#[test]` headers gets a tour of the security contract. Worth
preserving as a convention.

## Things this review explicitly did NOT examine
- Trait design, ownership, error-handling idiom (rust-principles lane)
- Whether `sanitize_for_terminal` is sufficient vs. allowlist (security
  lane)
- Allocation cost of the new `String::with_capacity(s.len())` paths
  (performance lane)
- Public-API doc comment completeness against `missing_docs` (docs lane)
- Module boundary appropriateness, cross-crate cycles (architecture lane)

## Confidence
High — all five files in the slice 11 diff were read in full; the
slice-comment count was reverified against `git grep`; no behavioural
claims are made (this is a structure / wording review).
