# Performance Review — 20260509T071500Z

## Summary

Slice 9 was a triage slice; the lazy-tracing win promised in the dispatch
brief is real and verifiable in `main.rs:318-329`. That removes the prior
review's top HIGH. Everything else on the prior list is still standing —
process spawn per `precmd` remains the dominant warm-prompt term,
`wrap_for_shell` still does the two-pass byte loop, segments still
allocate-then-discard, and the FIFO open/close + per-prompt `kill -0`
are unchanged. None of this blocks MVP § 0's ~25 ms warm budget, but the
ceiling on a sub-ms gitstatusd RTT now lives almost entirely in the wrapper
shell+binary cost. New observations below; prior findings are referenced
but not duplicated.

## Findings

### [INFO] Slice-9 lazy tracing init — verified
**Location:** `crates/p10k-rs/src/main.rs:318-329`
**Issue:** The dispatch asked us to confirm the win. `init_tracing()` now
returns immediately when `RUST_LOG` is unset, after one `var_os` syscall:
no `EnvFilter` build, no global subscriber registration. The previous
review's HIGH finding (per-prompt 100–300 µs subscriber init on the silent
path) is closed. The doc comment at `main.rs:314-317` correctly captures
the rationale.
**Suggested fix:** None. Keep the `var_os` short-circuit shape if the
filter ever grows complexity — the silent-path budget here is the entire
point.

### [HIGH] Process spawn per `precmd` is the warm-prompt ceiling (still open)
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:147`; `crates/p10k-rs/src/main.rs:87-89`
**Issue:** Every prompt forks+execs the binary; clap derive parses argv
on each call. With gitstatusd pinned at ~1.3 ms warm and tracing now free,
fork+exec+clap is the largest remaining term in the warm path (1–2 ms
Linux native, several × that on WSL2 — the host this repo is being
developed on). MVP-SPEC § 2's "< 5 ms target" is realistic on Linux but
pressured on WSL2.
**Suggested fix:** Two cheap, additive moves while the post-MVP in-process
daemon is out of scope: (1) bypass clap for the prompt subcommand —
`std::env::args_os().nth(1)` peek, hand-rolled flag walk, fall through to
`Cli::parse()` for everything else; clap's argv-trie build measures at
~200–400 µs on a six-flag command. (2) Bump `Cargo.toml` release profile
to `lto = "fat"` (currently `"thin"` per the prior review's LOW); pairs
well with `strip` + `panic = "abort"` already set.

### [HIGH] `wrap_for_shell` two-pass byte loop (still open)
**Location:** `crates/p10k-rs-core/src/lib.rs:188-216`
**Issue:** Unchanged from prior review: `s.contains('\x1b')` walk
followed by a per-byte loop with `next_char_boundary` invoked once per
non-escape byte. For a typical 60-char prompt with 6 SGR escapes the inner
loop runs ~60 times and reallocates once when `with_capacity(s.len()+16)`
fills.
**Suggested fix:** Single pass with `memchr` (already a transitive dep
via `clap`/`anstyle`); pre-size the output capacity from a one-shot
`bytes().filter(|&b| b == 0x1b).count() * 4 + s.len()`. Worth ~30-60 µs on
warm prompt and removes a realloc.

### [MEDIUM] `default_layout` rebuilds 4 `Box<dyn Segment>` per prompt (still open)
**Location:** `crates/p10k-rs-segments/src/lib.rs:64-71`; consumed at
`crates/p10k-rs/src/main.rs:168-169`
**Issue:** Slice 9 didn't touch this. Four heap allocations per prompt
for ZSTs, plus the `Vec<Box<…>>` allocation, plus dyn dispatch defeating
inlining for `enabled()`/`render()` on the hottest call site. Until
config drives layout, this is pure waste — segments are statically known.
**Suggested fix:** Replace with `&'static [&'static dyn Segment]` over
`static` instances (`static DIR: Dir = Dir; …`). Drops 5 allocations per
prompt; restores monomorphisable dispatch where LLVM can devirtualise.
When config-driven layout lands, swap back to owned `Vec<Box<…>>` only on
config-change paths.

### [MEDIUM] Segment render: format-then-rescan-then-discard (still open)
**Location:** `crates/p10k-rs-segments/src/dir.rs:22-37`,
`vcs.rs:31-95`, `command_execution_time.rs:32-44`,
`prompt_char.rs:21-34`
**Issue:** Pattern unchanged: each segment allocates with `format!`, then
runs `chars().count()` on a separate string for `plain_len`. `Vcs::render`
allocates `plain` (line 45) only to format-discard it into `text` (line
70/77). Four segments × ~2 allocations each = ~8 allocations per prompt
inside segments alone. `Dir::render` still calls `std::env::var("HOME")`
on every prompt despite `EnvSnapshot` being literally designed for this
(see `core/src/lib.rs:349-356`).
**Suggested fix:** (a) Populate `EnvSnapshot` with `HOME` once in
`cmd_prompt` and read through the snapshot in `Dir`. (b) For ASCII-only
segments (`prompt_char` is single-codepoint, `command_execution_time`
output is ASCII) compute `plain_len = formatted.len() as u16` and skip
the `chars().count()` scan. (c) For `Vcs`, write ANSI prefix → branch →
counters → marker directly into one `String` with `write!`, deriving
`plain_len` by tracking it as we go.

### [MEDIUM] FIFO open(2) + close(2) per prompt (still open)
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:91-117`
**Issue:** Unchanged. Two `open(2)` syscalls per prompt for FIFOs the
parent shell already holds R/W on. WSL2 inflates each above 100 µs;
native Linux is cheaper but non-zero. Prior review proposed fd-passing
via numbered redirections from the zsh init.
**Suggested fix:** Defer the implementation but plumb the measurement —
the bench harness should record per-prompt syscall counts so the win is
visible when the change ships. The change requires relaxing
`forbid(unsafe_code)` in the `p10k-rs` binary (or wrapping `from_raw_fd`
in a tiny crate) — call that out in the slice that takes it.

### [MEDIUM] `parse_response` materialises a 17-element `Vec<&[u8]>` (still open)
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:165-212`
**Issue:** `record.split(...).collect()` per prompt is a fresh `Vec` plus
17 slice descriptors. Two `to_owned` `String`s for `branch` and `commit`
are unavoidable (owned by `GitState`); the `Vec` is not.
**Suggested fix:** Replace with a state machine: walk bytes once,
increment a field counter on each US (0x1F), capture the (start, end)
ranges for indices 3, 4, 10-15 into a `[(usize, usize); 8]` on the stack.
Saves the heap allocation and the indirect deref through `Vec`.

### [MEDIUM] `read_until_with_deadline` re-scans the whole record each iteration (still open)
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:124-159`
**Issue:** `record.iter().position(|&b| b == delim)` runs over the entire
accumulated buffer on every read. For typical single-chunk responses
this is one scan and fine; for multi-chunk responses on a slow daemon
this is quadratic in chunk count.
**Suggested fix:** Track a `search_from` cursor: scan only
`&record[search_from..]`, set `search_from = record.len()` after each
`extend_from_slice` (minus 0; delim won't be split across reads on a
streaming pipe but conservative is fine). 4 lines of code.

### [MEDIUM] `kill -0` health probe in every `precmd` (still open)
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:143-146`
**Issue:** Comment self-reports "~1ms cost per prompt". For sessions
where the daemon never dies (the overwhelming case), this is pure tax.
1 ms × thousands of prompts is a measurable session-cumulative drag.
**Suggested fix:** Sample, don't poll: `(( ++_p10k_rs_health_tick % 10
== 0 )) && kill -0 …`. Or better: detect death lazily — if the binary
returns failure (FIFO write `EPIPE`), the next `precmd` runs the probe
once and respawns. Today's code respawns proactively but pays the cost
on every healthy prompt.

### [LOW] `current_dir()` syscall when `$PWD` is in scope (still open)
**Location:** `crates/p10k-rs/src/main.rs:145`
**Issue:** `getcwd(2)` per prompt; zsh holds `$PWD` already.
**Suggested fix:** Have the init script pass `--cwd "$PWD"`; fall back
to `std::env::current_dir()` only when the flag is absent. Saves one
syscall (~5-20 µs).

### [LOW] Slice-9 dump path: writes a fresh tempfile + rename per prompt
**Location:** `crates/p10k-rs/src/main.rs:193-213`; called from
`main.rs:178-182` on every prompt
**Issue:** New in slice 8/9. Every prompt now does `write(tmp)` +
`rename(tmp, dump)`. Net: two extra syscalls plus an inode churn that
defeats most filesystem caches' write-coalescing. The dump only changes
when the rendered prompt bytes change (cwd, status, branch, …); on a
prompt loop in the same dir the bytes are often identical.
**Suggested fix:** Skip the write when the new content equals the
existing dump. Cheapest form: keep an in-process `OnceLock<String>` of
the last-written content for the lifetime of *this* invocation — but
since each invocation is a fresh process, that's useless. Better:
hash the rendered string with `ahash`/`fxhash` (cheap), write the hash
in a tiny sidecar file (`dump.zsh.hash`), and skip the rename when the
hash matches. Or just `read(dump)` first and `memcmp` — one read syscall
trades against two writes + a rename, net ~zero on a no-change prompt.
Worth doing; not urgent.

### [LOW] `git_status` re-stats both FIFOs every prompt
**Location:** `crates/p10k-rs/src/main.rs:287-299`;
`crates/p10k-rs-git/src/gitstatusd.rs:222-233`
**Issue:** `Gitstatusd::from_env_paths` calls `is_fifo` on both paths,
which `lstat`s each. Slice-9 security correctly hardened this with a
UID check (good — see security review). For the binary that runs once
and exits, two `lstat(2)`s per prompt is the cost of admission. Trust
boundary lives at the env-var pair (req/resp paths); after the first
successful prompt, the FIFO identity won't change within the shell's
lifetime.
**Suggested fix:** None for the per-process binary. Document for the
post-MVP daemon: cache the validated FIFO identity (dev/inode pair)
across requests so the lstats happen once per shell, not once per
prompt.

## Things this review explicitly did NOT examine
- Rust idioms / trait design (lane #01)
- Security / FIFO permissions / unsafe (lane #02 — only referenced where
  it intersects performance)
- Naming / readability (lane #04)
- Documentation freshness (lane #05)
- Architecture / ADR-0001 alignment (lane #06)
- Live benchmarks (read-only constraint; no `cargo` runs)

## Confidence

Medium-high. The slice-9 tracing win is mechanically verifiable from the
diff and the closure on the prior review's top finding is firm. Open
findings carried forward are unchanged in code shape; their severity
ranking is unchanged. Process-spawn as the new ceiling is supported by
both the spike data already in `bench/results/` and the elementary cost
of fork+exec+dynamic-linking on Linux/WSL2; exact ms numbers would need
a slice-N+1 microbench to lock in.
