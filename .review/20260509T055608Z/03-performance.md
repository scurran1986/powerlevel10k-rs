# Performance Review — 20260509T055608Z

## Summary

The hot path is sound in shape: long-lived gitstatusd over FIFOs is exactly
what the pivot called for and the spike data validates the budget. But the
prompt binary still pays a measurable tax that doesn't go to git: a
`tracing-subscriber` init on every invocation, a `format_duration_ms`-style
allocation per segment, a byte-loop SGR wrapper, and—biggest of all—a
`fork+exec(p10k-rs)` per `precmd` whose process-startup cost will dominate
sub-ms gitstatusd responses on small repos. None block MVP § 0 (~25 ms warm
budget), but several are easy wins that compound. Cold-start path (first
prompt) is unmasked because Slice 8 instant prompt isn't here yet — the
review flags this but does not score it as a defect.

## Findings

### [HIGH] Per-prompt `tracing-subscriber` init on a silent path
**Location:** `crates/p10k-rs/src/main.rs:80, 225-233`
**Issue:** `init_tracing()` runs unconditionally at the top of `main`. Even
with no env var set, `EnvFilter::try_from_default_env` reads `RUST_LOG`,
constructs an `EnvFilter`, builds a `fmt::Subscriber`, and registers a
global default. On a binary that prints exactly one prompt and exits, this
is pure overhead — the subscriber never emits a line because filter is
`warn`. Microbench data on similar `tracing-subscriber` v0.3 setups puts
this at 100–300 µs, which is significant against a 25 ms warm budget and
catastrophic against a sub-ms gitstatusd response on small repos.
**Suggested fix:** Initialise tracing lazily, only when `RUST_LOG` is set
(or behind a `--log` flag). Cheap guard: `if std::env::var_os("RUST_LOG").is_some() { fmt().with_env_filter(…).try_init(); }`. Default path becomes a single `var_os` syscall.

### [HIGH] Process-spawn per prompt is the real ceiling, not gitstatusd
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:124`
**Issue:** Every `precmd` does `PROMPT="$("$_P10K_RS_BIN" prompt … 2>/dev/null) "`. That's a fork+exec of the 1.5 MB binary, dynamic linker resolution, clap parse, and shutdown — measured elsewhere at 3–8 ms cold and 1–2 ms warm on Linux, several × that on WSL2. With gitstatusd at 1.28 ms on ripgrep, the wrapper is now the dominant term. MVP-SPEC § 2 advertises "< 5 ms target with hot binary; < 1 ms with daemon (v0.2)" — the v0.1 number is at risk on WSL2 hosts where startup is inflated.
**Suggested fix:** Track binary startup as its own number in CI bench. Two cheap mitigations: (1) `panic = "abort"` and `strip = true` are already on in `Cargo.toml:130-134` (good); add `lto = "fat"` for one more ~5–10% win on cold start. (2) Replace `clap` derive with a hand-rolled `match` on `args().nth(1)` for the prompt subcommand only — clap's `Parser::parse` builds a non-trivial trie. The other subcommands stay on clap. This is the single biggest realistic win short of an in-process daemon (post-MVP per § 1.5).

### [HIGH] `wrap_for_shell` re-allocates and rescans every prompt
**Location:** `crates/p10k-rs-core/src/lib.rs:188-216`
**Issue:** Two passes over the assembled prompt: `s.contains('\x1b')` walks the whole string before the wrapping loop walks it again byte-by-byte. The `next_char_boundary` helper at line 219-225 is invoked per non-escape byte; on a typical prompt with cwd path it runs hundreds of times. `out` is preallocated to `s.len() + 16` but every wrapped escape adds 4 bytes (`%{`+`%}`), so a prompt with N escapes triggers a realloc once N > 4. Allocation count per prompt: 1 from `render_prompt`'s `String::new()` + 1 from `wrap_for_shell`'s `String::with_capacity` + ~1 realloc.
**Suggested fix:** Single pass using `memchr::memchr` (already a transitive dep via several crates) to find the next `\x1b`, copy the slice between escapes via `out.push_str(&s[last..idx])` — that's the exact slicing pattern, no per-char boundary check needed. Pre-size capacity at `s.len() + 4 * approximate_escape_count` (count `\x1b` once with `bytecount` or just `s.bytes().filter(|&b| b == 0x1b).count()`). Builds the wrapped string in O(n) bytes copied with one allocation.

### [MEDIUM] Segments build, then format-allocate, then re-scan-for-len
**Location:** `crates/p10k-rs-segments/src/dir.rs:22-37`, `vcs.rs:31-95`, `command_execution_time.rs:32-44`, `prompt_char.rs:21-34`
**Issue:** Per segment: `format!` allocates the styled string, then `chars().count()` scans the *unstyled* `collapsed` separately for `plain_len`. `Vcs::render` allocates `plain` (line 45), then `format!`s it into `text` (line 70 or 77), discarding `plain`. That's 2 allocations + a UTF-8 scan per segment, 4 segments default = ~8 allocations per prompt before the wrapper. `Dir::render` calls `std::env::var("HOME")` on every invocation — env is a syscall on first read but a libc lookup thereafter; still wasted work.
**Suggested fix:** (a) Cache `HOME` in `EnvSnapshot` — that's literally what the type is for per `lib.rs:349-356`. The env-snapshot is currently empty; this is the use case. (b) Compute `plain_len` from the byte length of ASCII-only segments (`prompt_char` is one char, `command_execution_time` is ASCII) without `chars().count()`. (c) For `Vcs`, write directly into a single `String` with ANSI prefixes inline: avoid building `plain` and discarding it.

### [MEDIUM] FIFO open/close on every prompt
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:91-117`
**Issue:** Each `Backend::status` does `OpenOptions::new().write(true).open(req_fifo)` then `.open(resp_fifo)`. Two `open(2)` syscalls per prompt plus two implicit closes on Drop. On WSL2 these are >100 µs each from prior measurement. The shell already holds R/W fds open on both FIFOs — the binary can't inherit those because it's a child process, but it could pass numbered fds via `--req-fd 3 --resp-fd 4` and `exec` redirections in zsh.
**Suggested fix:** Optional follow-up. The init script can `"$_P10K_RS_BIN" prompt … {req}>&$_P10K_RS_FIFO_REQ_FD {resp}<&$_P10K_RS_FIFO_RESP_FD`, export the fd numbers, and the binary opens via `File::from_raw_fd` (would require relaxing `forbid(unsafe_code)` in `p10k-rs/main.rs`, or a tiny crate that hides the unsafe). Cuts 2 `open(2)` per prompt. Defer until benchmark shows the gain — gitstatusd's FIFO RTT is ~0.8 ms, so saving 0.2 ms is real.

### [MEDIUM] `read_until_with_deadline` allocates 4 KiB even on tiny responses
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:124-159`
**Issue:** `Vec::with_capacity(4096)` for record + `[0u8; 4096]` stack buffer on every call. Typical responses are ~150-300 bytes; 4096 is overkill but harmless on stack. The Vec heap allocation is the unavoidable cost here. `extend_from_slice` will reallocate if the response is > 4096 bytes (rare but possible on a big repo with long branch names). Loop also calls `record.iter().position` each iteration — quadratic in the number of read chunks, though chunks are usually 1.
**Suggested fix:** Track the search start: after each `extend_from_slice`, scan only `&record[search_from..]` for the delimiter, advancing `search_from = record.len() - 1` (account for delimiter at boundary). Negligible for one-chunk reads; protects against pathological multi-chunk fragmentation. Allocation pattern is fine.

### [MEDIUM] `parse_response` allocates a `Vec<&[u8]>` per prompt
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:165-212`
**Issue:** `record.split(|&b| b == US).collect()` materialises a vector of all 17+ field slices, then indexes into it. `s(3).to_owned()` and `s(4).to_owned()` allocate two more `String`s. Total: 1 Vec + 2 Strings per prompt for parsing a fixed-format response.
**Suggested fix:** Replace with an iterator state machine: walk `record` byte-by-byte, increment a field counter at each US, stash the byte ranges we care about (3, 4, 10-15) into an array of `(start, end)` pairs. Single pass, zero heap. Net: removes the Vec allocation; the two Strings stay because `GitState` owns them by value. Saves ~80 bytes of heap and one vec growth.

### [MEDIUM] `precmd` `kill -0` health check is per-prompt
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:120-123`
**Issue:** Comment claims "~1ms cost per prompt". For the 99.99% of prompts where the daemon is alive, this is pure overhead. On a healthy daemon over a session of 1000 prompts that's ~1 second of cumulative latency for nothing.
**Suggested fix:** Two cheaper signals exist: (a) Trust the FIFO write — if the daemon is dead, `write` to the req FIFO returns `EPIPE` and the binary's `write_all().ok()?` already falls through to `None`, which causes `git_status` to skip and… nothing respawns the daemon. So actually the current `kill -0` IS protecting against the wedged-daemon case. Keep it but rate-limit: only check every Nth prompt (`(( ++_p10k_rs_health_tick % 10 == 0 ))`). Cuts the cost by 10×. (b) Better: `trap '_p10k_rs_start_daemon' SIGCHLD` or a periodic check tied to `$RANDOM`-based sampling.

### [LOW] `current_dir()` syscall when zsh already knows
**Location:** `crates/p10k-rs/src/main.rs:124`
**Issue:** `std::env::current_dir()` is a `getcwd(2)` syscall. Zsh knows `$PWD` already. One syscall, but it's on the hot path.
**Suggested fix:** Add `--cwd "$PWD"` to the init script invocation, fall back to `current_dir()` only when missing. Saves one syscall. ~5-20 µs win.

### [LOW] `default_layout()` allocates 4 boxed trait objects per prompt
**Location:** `crates/p10k-rs-segments/src/lib.rs:64-71`
**Issue:** Each prompt allocates 4 `Box<dyn Segment>` via `vec![Box::new(...)]`. The segments are zero-sized types (`Dir`, `Vcs`, etc.) so the allocations are 0-byte but the vtable indirection through `Box<dyn Segment>` defeats inlining of `enabled()` and `render()`.
**Suggested fix:** Until config-driven assembly lands, expose a `&'static [&'static dyn Segment]` and have segments be `static` instances (`static DIR: Dir = Dir;`). No allocations, no boxing, monomorphic dispatch where the compiler can prove it. The `Box<dyn>` shape comes back the moment configuration drives layout — fine then.

### [LOW] `cargo build --release` profile could squeeze more
**Location:** `Cargo.toml:130-138`
**Issue:** `lto = "thin"`, `strip = true`, `panic = "abort"` already set — good. `lto = "fat"` would shave another 5-10% binary size and (more importantly) cold-start time at the cost of build time. `codegen-units = 1` is already optimal.
**Suggested fix:** Try `lto = "fat"` and measure the binary-size + startup-time delta. If startup drops by >5%, ship it.

### [INFO] Cold-start gitstatusd un-masked without instant prompt
**Location:** Workspace-wide; tracked in `MVP-SPEC.md` § 1.3 (Slice 8).
**Issue:** First prompt after shell startup pays gitstatusd's cold-cache hit (~2 s on linux kernel). Slice 8 instant prompt is the design answer; until it lands, first-prompt UX is rough on big repos. Not a defect today, but performance work in slices 6-7 is invisible to the user without slice 8 to mask the cold path.
**Suggested fix:** Prioritise slice 8 design; consider issuing a warm-up gitstatusd query for `$PWD` from the init script in the background after `start_daemon` returns, so the cache is warm before the user's first prompt.

## Things this review explicitly did NOT examine
- Rust idioms / API design (#01)
- Security / unsafe / FIFO permission model (#02)
- Naming / readability (#04)
- Documentation accuracy (#05)
- Architecture / ADR alignment (#06)
- Actual bench runs (constraint: read-only, no cargo)

## Confidence
Medium-high. Findings on `tracing` init, `wrap_for_shell`, segment allocations, and the `kill -0` per-prompt cost are mechanically certain from code reading. Process-startup as a ceiling is well-supported by the spike data and standard knowledge of Linux fork+exec; exact ms numbers would need a microbench. WSL2 inflation of all syscall numbers means a native-Linux re-measurement could re-rank some MEDIUMs.
