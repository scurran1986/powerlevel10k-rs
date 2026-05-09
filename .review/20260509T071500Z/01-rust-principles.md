# Rust Principles Review — 20260509T071500Z

## Summary

The codebase is well-structured idiomatic Rust with strong fundamentals: `forbid(unsafe_code)` everywhere, clean trait design, good ownership discipline, and thorough workspace-level lint configuration. The main concerns are three parallel type hierarchies (`Shell`, `Config`, `ColorMode`) duplicated across crates creating a unification debt, one segment violating the `EnvSnapshot` contract by calling `std::env::var` directly, and the `Backend` trait returning owned data where a lifetime-bound design would reduce allocations on the hot path.

## Findings

### [MEDIUM] Duplicate `Shell` enum — two identical definitions
**Location:** `crates/p10k-rs-core/src/lib.rs:290` and `crates/p10k-rs-shell/src/lib.rs:17`
**Issue:** Two structurally identical `Shell` enums exist with no shared derivation. `p10k-rs-core::Shell` lacks `FromStr`; `p10k-rs-shell::Shell` has it. The binary imports both under aliases (`CoreShell`, `ShellInit`). This will drift — adding a variant to one and forgetting the other is a compile-silent bug if the match arms use `_` wildcards.
**Suggested fix:** Have `p10k-rs-shell` re-export `p10k_rs_core::Shell` and add the `FromStr` impl there (or via an extension trait). One canonical enum, one source of truth.

### [MEDIUM] Duplicate `Config` struct — placeholder shadows real type
**Location:** `crates/p10k-rs-core/src/lib.rs:286` and `crates/p10k-rs-config/src/lib.rs:27`
**Issue:** `p10k-rs-core` defines an empty `Config {}` while `p10k-rs-config` defines the real one with 8+ fields. The TODO at line 282 acknowledges this. The binary uses `p10k_rs_core::Config::default()`, meaning all config fields are ignored. This is tracked debt but it blocks config-driven rendering.
**Suggested fix:** Add `p10k-rs-config` as a dependency of `p10k-rs-core` (or invert: have `-core` define a trait and `-config` implement it). Resolve the TODO before the next feature slice.

### [MEDIUM] Duplicate `ColorMode` enum
**Location:** `crates/p10k-rs-core/src/style.rs:16` and `crates/p10k-rs-config/src/lib.rs:71`
**Issue:** Same pattern as `Shell` and `Config`. Two identical enums, no shared import. The `-config` version has serde derives; the `-core` version does not. They will diverge.
**Suggested fix:** Canonicalise in one crate and re-export.

### [MEDIUM] `dir` segment bypasses `EnvSnapshot` contract
**Location:** `crates/p10k-rs-segments/src/dir.rs:24`
**Issue:** Calls `std::env::var("HOME")` directly despite `RenderCtx` carrying an `EnvSnapshot` field specifically designed to avoid this (documented at `crates/p10k-rs-core/src/lib.rs:350-354`). This makes `Dir::render` untestable without process-global env mutation, which the doc comment at line 43 acknowledges is `unsafe` since Rust 1.85.
**Suggested fix:** Add a `home: Option<PathBuf>` field to `EnvSnapshot`. Read `$HOME` once in the binary when constructing the snapshot. `Dir::render` reads `ctx.env.home`.

### [MEDIUM] `Backend::status` returns owned `GitState` — no lifetime option
**Location:** `crates/p10k-rs-git/src/lib.rs:37`
**Issue:** `fn status(&self, path: &Path) -> Option<GitState>` forces an allocation per prompt for all backends. The `Gitstatusd` backend already owns the parsed data internally; returning a borrow (`Option<&GitState>` with a stored cache) would eliminate the clone on the hot path. This is a design-level concern for a trait that will be called every prompt render.
**Suggested fix:** Consider `fn status(&mut self, path: &Path) -> Option<&GitState>` with the backend caching its last result internally, or keep the current signature and accept the allocation cost as acceptable for MVP.

### [LOW] `plain_len` uses `chars().count()` — not grapheme-cluster width
**Location:** `crates/p10k-rs-segments/src/vcs.rs:65`, `crates/p10k-rs-segments/src/dir.rs:26`
**Issue:** `SegmentOutput` docs say `plain_len` is "visual width in columns" and segments must "count grapheme clusters correctly" (core lib.rs:129). But `chars().count()` counts Unicode scalar values, not display width. CJK characters and emoji are 2 columns wide; combining marks are 0. This will misalign the ruler/frame on non-ASCII branch names or paths.
**Suggested fix:** Use `unicode-width` crate's `UnicodeWidthStr::width()` for correct terminal column counting.

### [LOW] `Segment` trait is not object-safe for `Clone`
**Location:** `crates/p10k-rs-core/src/lib.rs:51`
**Issue:** `Segment: Send + Sync` is used as `Box<dyn Segment>`. This works, but prevents cloning segment vectors (e.g., for caching or async dispatch). Not a bug today, but worth noting if post-MVP daemon mode needs to clone segment sets across threads.
**Suggested fix:** No action needed now. If cloning is needed later, add a `clone_box` method or use `dyn-clone`.

### [LOW] `parse_response` closures shadow field access pattern
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:179-180`
**Issue:** The closures `let s = |i: usize| -> &str { ... }` and `let parse_u = |i: usize| -> u32 { ... }` capture `fields` by reference and silently swallow UTF-8 and parse errors via `unwrap_or`. This is acceptable for a wire-protocol parser where the daemon is trusted, but a malformed response will produce silently wrong data rather than a clean `None`. The workspace warns on `unwrap_used` but not on silent `unwrap_or` misuse.
**Suggested fix:** Acceptable for MVP. Consider logging a `tracing::debug!` on parse failures for future debugging.

### [INFO] Lint configuration is thorough and well-considered
**Location:** workspace-wide (`Cargo.toml:84-104`, `clippy.toml`)
**Issue:** Not an issue — this is a positive observation. `clippy::pedantic` at warn, `unwrap_used`/`expect_used`/`panic`/`todo`/`dbg_macro` all warned, `forbid(unsafe_code)` per-crate, and a raised cognitive-complexity threshold with rationale. The `doc-valid-idents` list prevents false positives on domain terms. This is textbook workspace lint setup.

### [INFO] Trait design (`Segment`, `Backend`) is clean and extensible
**Location:** `crates/p10k-rs-core/src/lib.rs:51-83`, `crates/p10k-rs-git/src/lib.rs:35-38`
**Issue:** Positive observation. `Segment` has sensible defaults for `enabled()` and `is_fast()`, clear doc contracts, and the `Send + Sync` bound is forward-looking for async dispatch. `Backend` is minimal with a single method — easy to implement, easy to mock. Both traits avoid over-specification.

## Things this review explicitly did NOT examine
- Security of FIFO handling and path validation (lane #02)
- Allocation counts and syscall overhead on hot path (lane #03)
- Naming, comment quality, function length (lane #04)
- Module docs, ADR alignment, README accuracy (lane #05)
- Architectural slice boundaries, test discipline (lane #06)

## Confidence
High — read all production `.rs` files in the workspace (12 files across 9 crates), all lint/toolchain config, and the workspace manifest. The codebase is small enough for complete coverage in this lane.
