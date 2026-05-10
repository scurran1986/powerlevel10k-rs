# Security Review — 20260510T054710Z

## Summary

SafeText successfully closes the "future segment re-introduces unsanitised text" gap (C2 class). The type's private inner `String`, combined with constructors that all funnel through `sanitize_for_terminal`, makes it impossible to bypass sanitisation without `unsafe` code — and the crate is `#![forbid(unsafe_code)]`. No new security defects introduced. Two pre-existing observations remain open from prior slices.

## Findings

### [INFO] SafeText invariant is sound — no bypass path
**Location:** `crates/p10k-rs-core/src/safety.rs:79`
**Issue:** Verified: `SafeText(String)` has a private field. The three public entry points — `from_untrusted` (line 87), `from_untrusted_bytes` (line 99), and `From<&str>` (line 136) — all call `sanitize_for_terminal`. `Default` produces empty string (safe). No `From<String>`, no `Deref<Target=String>`, no `DerefMut`, no `AsMut<str>`, no serde `Deserialize`. No `unsafe` in the crate. The invariant holds.
**Suggested fix:** None needed. Design is correct.

### [INFO] All GitState producers go through SafeText
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187`, `crates/p10k-rs-git/src/lib.rs:103-104`
**Issue:** Both the `Gitstatusd` backend (`untrusted_field` closure returning `SafeText::from_untrusted_bytes`) and `ShellOut` backend (`parse_branch_header` returning `SafeText::from_untrusted`) correctly construct `SafeText` at the ingestion boundary. A 3rd-party couldn't construct a `GitState` with unsanitised fields because `branch` and `commit` are typed as `SafeText`, and there's no way to produce a `SafeText` containing control bytes.
**Suggested fix:** None needed.

### [INFO] Single consumer verified
**Location:** `crates/p10k-rs-segments/src/vcs.rs:46`
**Issue:** `git.branch.as_str()` is the only production consumer reading the branch field into rendered output. The `format!` path for ahead/behind uses integer formatting only.
**Suggested fix:** None needed.

### [LOW] `RenderCtx::cwd` remains a raw `&Path` — deferred migration
**Location:** `crates/p10k-rs-core/src/lib.rs:106`
**Issue:** `cwd` is still `&Path`, not `SafeText`. The `Dir` segment sanitises inline (`dir.rs:26`), which is correct today. However, a future segment reading `ctx.cwd` could skip sanitisation because the type doesn't enforce it. The commit message acknowledges this deferral.
**Suggested fix:** Track as a follow-up. When a second cwd consumer appears, migrate `cwd` to a `SafePath` or pre-sanitised string in `RenderCtx`. Current risk is LOW because there's exactly one consumer and it does sanitise.

### [LOW] `GitState` fields are `pub` — weaker compile-time guarantee for out-of-crate producers
**Location:** `crates/p10k-rs-core/src/lib.rs:384-409`
**Issue:** `GitState` struct fields are `pub`. An out-of-crate producer (unlikely today but possible in a plugin model) could construct a `GitState` using struct-literal syntax. Because `branch: SafeText` and `commit: SafeText`, they'd still need a `SafeText` value — which always sanitises. So the type system holds even in this case. No actual bypass exists; this is purely a design-hardening note. A builder or `#[non_exhaustive]` could prevent future regressions if the struct gains a raw-string field.
**Suggested fix:** Consider `#[non_exhaustive]` on `GitState` when the API stabilises.

### [MEDIUM] IPC FIFO TOCTOU and symlink race (pre-existing, H3/H4 from slice-9)
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:229-240`
**Issue:** The `is_fifo` function uses `symlink_metadata` + UID check (good mitigations), but a TOCTOU window exists between the check and the `open()` call at line 96/114. An attacker with same-UID access could swap the FIFO between check and open. This was flagged in slice-9 as H3/H4 and remains open. Real-world exploitability requires local same-user access to `$XDG_RUNTIME_DIR`, making it a targeted-attack vector rather than a remote exploit.
**Suggested fix:** Open-then-fstat pattern: open the fd first, then `fstat` the *opened* fd to confirm FIFO type and ownership. This eliminates the TOCTOU window entirely. Still-open from prior audit.

### [LOW] `read_until_with_deadline` unbounded buffer growth
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:126-160`
**Issue:** The read loop accumulates into `record` until `RS` appears or timeout expires. A malicious daemon (or corrupted pipe) could send data without `RS` for up to 2 seconds, growing the buffer to ~GBs on a fast pipe. This is a local-only DoS vector (attacker controls a same-user process) with limited blast radius (one prompt render OOMs).
**Suggested fix:** Add a max-record-size cap (e.g. 1 MiB): `if record.len() > MAX_RECORD { return None; }`. Low priority because exploitability requires same-user local access.

## Things this review explicitly did NOT examine

- `install.sh` or shell init scripts (no changes in this slice)
- Full dependency audit (`cargo audit` unavailable in this env; `rustix 0.38.44` and `1.1.4` are recent)
- Performance characteristics of double-allocation in `SafeText::from_untrusted`
- Code style, naming, documentation quality (other reviewers' lanes)

## Confidence

**High.** The SafeText newtype is textbook "make illegal states unrepresentable." I verified every constructor, every trait impl, every producer, and the single consumer. The private inner field + `#![forbid(unsafe_code)]` makes it impossible to inject unsanitised text through the type without a breaking change to the module's public API.
