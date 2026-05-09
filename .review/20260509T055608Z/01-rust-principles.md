# Rust Principles Review — 20260509T055608Z

## Summary

The codebase demonstrates strong Rust fundamentals: `forbid(unsafe_code)` on every crate, workspace-level clippy pedantic enforcement, and disciplined use of `#[non_exhaustive]` on config types. Three structural issues stand out: (1) three types are duplicated across crates instead of being re-exported from a canonical source, (2) the `Backend` trait erases the distinction between "not a repo" and "I/O error", and (3) `GitState` and `SegmentOutput` lack `#[non_exhaustive]` while neighboring types in the same file have it, creating a semver inconsistency. Overall quality is high for an MVP-stage project.

## Findings

### [HIGH] Backend::status conflates "not a repo" with I/O errors
**Location:** `crates/p10k-rs-git/src/lib.rs:37`
**Issue:** `fn status(&self, path: &Path) -> Option<GitState>` returns `None` for both "not inside a git repository" (the normal case) and "FIFO read timed out" / "daemon crashed" / "UTF-8 decode failed". Callers (`main.rs:130`) silently fall through to `ShellOut` on daemon errors, which is the right behaviour for *that* caller, but the trait itself prevents any consumer from distinguishing a legitimate absence from a failure. When observability or retry logic arrives, this will force a breaking change.
**Suggested fix:** Return `Result<Option<GitState>, BackendError>` where `BackendError` is a `thiserror` enum. `Ok(None)` = not a repo, `Err(_)` = something went wrong. The `Gitstatusd` implementation already has distinct failure modes (timeout, parse error, FIFO missing) that map cleanly onto variants.

### [MEDIUM] Duplicate Shell, ColorMode, and HostKind types across crates
**Location:** `crates/p10k-rs-core/src/lib.rs:290` vs `crates/p10k-rs-shell/src/lib.rs:17`; `crates/p10k-rs-core/src/style.rs:16` vs `crates/p10k-rs-config/src/lib.rs:71`; `crates/p10k-rs-core/src/lib.rs:306` vs `crates/p10k-rs-ai/src/lib.rs:22`
**Issue:** Three enums (`Shell`, `ColorMode`, `HostKind`) are defined independently in two crates each with identical variant names but no shared type identity. The `main.rs` binary already imports both `Shell` types and aliases them (`Shell as CoreShell`, `Shell as ShellInit`). This will silently diverge as variants are added to one side but not the other, and forces `match`-based conversion at every boundary.
**Suggested fix:** Pick one canonical owner per type (e.g. `Shell` in `-core`, `ColorMode` in `-core::style`) and have the other crate `pub use` or depend on it. The `-core` TODO comment at line 282 acknowledges this for `Config`; extend the same plan to the other three types.

### [MEDIUM] GitState and SegmentOutput missing #[non_exhaustive]
**Location:** `crates/p10k-rs-core/src/lib.rs:327` (`GitState`), `crates/p10k-rs-core/src/lib.rs:125` (`SegmentOutput`)
**Issue:** `Config`, `EnvSnapshot`, and `HostKind` in the same file are `#[non_exhaustive]`, and `RenderCtx` has a doc comment explaining why it deliberately *isn't*. But `GitState` and `SegmentOutput` have neither the annotation nor the rationale. Both are public structs with `pub` fields that will grow (stash count, tag, submodule status for `GitState`; `tooltip`, `priority` for `SegmentOutput`). Adding a field without `#[non_exhaustive]` is a semver-major break for any external segment crate constructing them.
**Suggested fix:** Add `#[non_exhaustive]` to both and provide builder or `Default`-based construction patterns (already natural since `GitState` derives `Default`). If the `RenderCtx` rationale applies here too, document it the same way.

### [MEDIUM] unimplemented!() in public #[must_use] functions
**Location:** `crates/p10k-rs-ai/src/lib.rs:62,73,79,103`; `crates/p10k-rs-wizard/src/lib.rs:45`
**Issue:** Five public functions use `unimplemented!()` which panics at runtime. The workspace lints warn on `panic` and `todo` but `unimplemented` is not covered. Since these functions are `#[must_use]` and have return types, a caller can reach the panic through normal control flow. The `--host` CLI subcommand would panic if invoked today.
**Suggested fix:** Either (a) add `unimplemented = "warn"` to `[workspace.lints.clippy]` and gate these functions behind a `cfg` feature so they don't compile into the binary, or (b) return typed errors (`Err(Unimplemented)`) instead of panicking. The `main.rs` `bail!` pattern on `Command::Configure` / `Command::Statusline` already does the right thing for the CLI layer; the library functions should follow suit.

### [MEDIUM] Dir segment bypasses EnvSnapshot for HOME
**Location:** `crates/p10k-rs-segments/src/dir.rs:24`
**Issue:** `Dir::render` calls `std::env::var("HOME")` directly, contradicting the architecture documented at `crates/p10k-rs-core/src/lib.rs:352` ("Segments read through [EnvSnapshot] rather than calling `std::env::var`"). This makes the segment untestable without mutating process-global state (which is `unsafe` since Rust 1.85), and it introduces a hidden dependency on the process environment that other segments avoid.
**Suggested fix:** Add a `home` field (or method) to `EnvSnapshot` and read it from `ctx.env` in `Dir::render`. The private `home_collapse` function already accepts `home: Option<&str>` specifically to enable this.

### [LOW] Hardcoded vendored path in locate_binary
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:234-236`
**Issue:** `locate_binary()` hardcodes `/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64` as a fallback path. This is developer-machine-specific and will never resolve for any other user or in CI.
**Suggested fix:** Remove the hardcoded path and rely on `$P10K_RS_GITSTATUSD_BIN` and `$PATH` lookup only. If a development convenience path is needed, gate it behind `#[cfg(debug_assertions)]` or an env var.

### [LOW] Backend trait is not object-safe for future dynamic dispatch
**Location:** `crates/p10k-rs-git/src/lib.rs:35`
**Issue:** The `Backend` trait *is* currently object-safe (`&self`, no generics, no `Self: Sized`), which is good. However, it lacks a `Send + Sync` bound, which means `Box<dyn Backend>` can't be sent across threads. If the post-MVP daemon dispatches backend calls to a worker thread, this becomes a breaking change to the trait.
**Suggested fix:** Add `Send + Sync` as supertraits now (both `ShellOut` and `Gitstatusd` already satisfy them), matching the `Segment` trait design at `crates/p10k-rs-core/src/lib.rs:51`.

### [INFO] Segment trait render() could benefit from fallible return
**Location:** `crates/p10k-rs-core/src/lib.rs:62`
**Issue:** `Segment::render` returns `SegmentOutput` infallibly. The `Vcs` segment handles the "no git" case by returning an empty `SegmentOutput` (line 33-38) even though `enabled()` should have prevented the call. If a segment encounters a genuine error mid-render (e.g. filesystem probe fails), it has no way to signal this without panicking or returning garbage. This is acceptable for MVP but worth noting for the trait's next evolution.
**Suggested fix:** Consider `Result<SegmentOutput, SegmentError>` in a future breaking revision, or add an `Option<SegmentOutput>` variant where `None` means "skip me."

## Things this review explicitly did NOT examine
- Security implications of FIFO handling and command injection (reviewer #02)
- Performance characteristics of `wrap_for_shell` byte scanning or `read_until_with_deadline` (reviewer #03)
- Naming conventions and comment quality (reviewer #04)
- Doc completeness and ADR alignment (reviewer #05)
- Architecture conformance to ADR-0001 and slice boundaries (reviewer #06)
- `spike-gitstatus` crate internals (separate contractor-owned code)

## Confidence
**High.** Read every non-spike `.rs` file in the workspace, all `Cargo.toml` manifests, `clippy.toml`, and workspace lint config. The codebase is small enough (~1200 LOC across the production crates) that full coverage is feasible. The spike crate was scanned for cross-cutting patterns only.
