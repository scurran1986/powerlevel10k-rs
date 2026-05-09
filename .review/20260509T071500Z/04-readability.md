# Readability Review — 20260509T071500Z

## Summary
Codebase readability is strong overall. Module docs, function docs, and naming are consistently clear. The main issues are stale slice-number comments that reference past slices as if they are current/future work, a duplicated test-context builder pattern across three segment crates, and one function (`cmd_prompt`) that could benefit from extraction. No critical or high findings.

## Findings

### [MEDIUM] Stale slice-number comments throughout codebase
**Location:** workspace-wide (19 occurrences across 7 files)
**Issue:** Many comments reference slice numbers as future work ("slice 5+ flips this to true", "slice 6+ exposes it via TOML config") that have already shipped. At HEAD (slice 9), `vcs.rs:27` still says "Daemon backend in slice 5+ flips this to true" — but the daemon backend shipped in slice 6. `command_execution_time.rs:5` says "slice 6+ exposes it via TOML config" — config crate exists but the threshold is still hardcoded. These comments now mislead rather than inform: a reader cannot tell whether the referenced work landed or is still pending.
**Suggested fix:** Audit all `[Ss]lice [0-9]` references. For completed work, rewrite as present-tense statements ("The daemon backend handles this"). For genuinely pending work, use TODO with a tracking reference, not a slice number that will go stale again.

### [MEDIUM] Duplicated `RenderCtx` builder in test modules
**Location:** `crates/p10k-rs-segments/src/vcs.rs:107-125`, `crates/p10k-rs-segments/src/prompt_char.rs:50-68`, `crates/p10k-rs-segments/src/command_execution_time.rs:73-86`
**Issue:** Three segment test modules each define a near-identical helper to construct a `RenderCtx` for tests. The differences are minor (which fields are parameterized). This is a cognitive load multiplier — a contributor modifying `RenderCtx` (e.g. adding a field) must update all three independently, and the slight signature differences make it unclear which is "canonical."
**Suggested fix:** Add a `#[cfg(test)] pub mod test_fixtures` to `p10k-rs-core` exporting a single builder or `Default`-like factory. Each segment test calls it and overrides only the fields it cares about.

### [MEDIUM] `cmd_prompt` in main.rs does too many things
**Location:** `crates/p10k-rs/src/main.rs:138-184`
**Issue:** `cmd_prompt` (46 lines) handles shell parsing, cwd resolution, git backend selection, config/env/context construction, segment rendering, stdout printing, and instant-prompt dumping. Each concern is straightforward, but the function reads as a sequential checklist rather than a pipeline. The inline `// Slice 6:` and `// Slice 8:` comments are load-bearing section headers — a sign the function wants to be broken up.
**Suggested fix:** Extract `build_render_ctx` and keep `cmd_prompt` as the orchestrator that calls it, prints, and dumps. This also makes the render context construction independently testable.

### [LOW] `vcs.rs` render method has split concerns
**Location:** `crates/p10k-rs-segments/src/vcs.rs:31-96`
**Issue:** The `render` method builds the plain text, computes the marker, assembles ANSI escapes, and determines the state tag — all inline. At 65 lines it is not excessive, but the ANSI color logic (lines 70-78) is entangled with the display-text logic. Extracting a `format_vcs_text` helper would make the color application pattern reusable as more segments adopt multi-color output.
**Suggested fix:** Extract the "plain text + marker" construction into a helper returning `(String, &'static str)` (text, marker). Keep ANSI wrapping in `render`.

### [LOW] Module doc headers reference planning docs that aren't visible in-repo
**Location:** `crates/p10k-rs-config/src/lib.rs:10` (`05-config-parameters.md`), `crates/p10k-rs-git/src/gitstatusd.rs:24` (`07-gitstatus.md`), `crates/p10k-rs-ai/src/lib.rs:12` (`10-ai-integration.md`)
**Issue:** These doc comments cite planning documents by filename (e.g. `05-config-parameters.md in the planning bundle`) but the planning bundle lives at `~/.planning/powerlevel10k-rs/`, outside the repo. A new contributor reading the source has no way to find these documents. Not a blocker, but it creates dead-reference noise.
**Suggested fix:** Either commit the planning docs into `docs/planning/` or replace the references with `ARCHITECTURE.md` section pointers that are in-repo.

### [LOW] Naming inconsistency: `locate_binary` vs `locate_gitstatusd`
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:244` (defined as `locate_binary`), `crates/p10k-rs-git/src/lib.rs:28` (re-exported as `locate_gitstatusd`)
**Issue:** The function is named `locate_binary` at its definition site but re-exported as `locate_gitstatusd`. Both names are reasonable, but the inconsistency means `grep locate_binary` and `grep locate_gitstatusd` each only find half the picture.
**Suggested fix:** Rename the definition to `locate_gitstatusd` and keep the re-export as a simple `pub use`.

### [LOW] Magic ANSI numbers without named constants
**Location:** `crates/p10k-rs-segments/src/dir.rs:28-29`, `crates/p10k-rs-segments/src/vcs.rs:71-77`, `crates/p10k-rs-segments/src/prompt_char.rs:23-26`, `crates/p10k-rs-segments/src/command_execution_time.rs:37`
**Issue:** ANSI color codes (31, 32, 33, 34, 36, 39) are inlined as raw escape strings with explanatory comments. The comments help, but they are duplicated across files. When the style module (`p10k-rs-core/src/style.rs`) ships real color helpers, these will all need manual replacement. Named constants or a thin helper now would reduce that churn.
**Suggested fix:** Add `const ANSI_RED: &str = "\x1b[31m"` etc. to `style.rs` and import them. Low urgency — the `style.rs` placeholder already exists and is the natural home.

### [INFO] Comment quality is generally excellent
**Location:** workspace-wide
**Issue:** Not a defect. Module-level doc comments consistently explain the "why" and cross-reference architecture docs. Function docs state contracts, error semantics, and caller responsibilities. The `is_fifo` doc comment in `gitstatusd.rs:216-222` is a standout example of documenting security rationale inline.

## Things this review explicitly did NOT examine
- Correctness of ANSI escape sequences (performance lane)
- Security of FIFO handling (security lane)
- Trait design and ownership patterns (Rust principles lane)
- ADR alignment and architecture boundaries (architecture lane)
- Documentation completeness of public API surface (documentation lane)

## Confidence
High — all source files were read in full; slice-comment staleness verified against git log.
