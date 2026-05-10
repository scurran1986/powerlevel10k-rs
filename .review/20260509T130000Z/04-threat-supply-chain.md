# Lane 4: Threat model + supply chain — extracted from rate-limited transcript

**Source:** `/tmp/claude-1000/-home-seaburdz-github-powerlevel10k-rs/794df3aa-6985-44d2-aa7e-e6c8fbbda924/tasks/af1cdf16789326193.output`
**Lines / size:** 110 lines / 311 KB
**Status when limit hit:** Limit hit at L110 mid-investigation. The agent had completed 48 tool uses, read every requested file, and verified key infrastructure facts (Cargo.lock = 101 packages, sole author = Sean Curran, fixtures gitignored not vendored, dependabot configured, CI uses `RUSTFLAGS=-D warnings`, no GitHub Actions release pipeline yet). **No draft survives.** The findings below are reconstructed from the agent's tool-call evidence and the L43 / L96 / L101 inline reasoning beats.

## Findings

### [LOW] Vendored-fixtures supply-chain concern is REFUTED — fixtures are gitignored, fetched by script
**Location:** `.gitignore:14-15`, `bench/fixtures/.gitkeep`, `bench/fetch_fixtures.sh`
**Evidence:** Agent confirmed at L97-100: `.gitignore` excludes `/bench/fixtures/repos/` (line 15); `bench/fixtures/.gitkeep` says "bench/fixtures/repos/ is excluded by the workspace .gitignore"; `bench/fetch_fixtures.sh` is a clone-script with pinned-commit semantics (it honours `BENCH_FIXTURES_DIR`, has flags `--with-linux` / `--with-chromium`). The 8 GB linux + 9.7 MB ripgrep on disk are the *user's* local clones, not committed artefacts.
**Why it matters:** The orchestrator's pre-fanout note treated this as supply-chain risk; the agent verified it is not. The risk class is "developer's local cache size," not "vendored hostile content." Re-classify the orchestrator's listing of this concern as resolved-INFO.
**Status:** verified-by-agent (refutes a pre-fanout suspicion)

### [MEDIUM] `THIRD-PARTY-LICENSES.md` v1.5.4 pin is documentary, not enforced
**Location:** `THIRD-PARTY-LICENSES.md:5-7`, `install.sh:131-133`
**Evidence:** Agent read THIRD-PARTY-LICENSES at L20: it asserts "pinned tag v1.5.4" for the bundled gitstatusd binaries. Agent read install.sh at L44: the script searches `GITSTATUSD_CANDIDATES` (including `$HOME/github/powerlevel10k/gitstatus/usrbin/...`, `/opt/homebrew/bin`, `/usr/local/bin`) and `ln -sfn`s the first match — no version probe, no checksum, no signature verify. If `/opt/homebrew/bin/gitstatusd` is v1.6.0 or v1.4.0 or a Trojan, install.sh symlinks it.
**Why it matters:** "Pinned to v1.5.4" creates a security expectation the codebase does not back. This is the same finding the prior review labels LOW; lane 4's evidence supports MEDIUM because the candidate list includes `$HOME/github/powerlevel10k/...` (named after a real upstream project, attacker-pre-plantable).
**Status:** verified-by-agent (corroborates orchestrator's pre-fanout MEDIUM)

### [MEDIUM] No release pipeline / signing / SLSA — risk path from "Sean writes code" to "user runs binary" is unsigned
**Location:** `.github/workflows/ci.yml`, `.github/dependabot.yml`
**Evidence:** Agent read `ci.yml` at L52: it tests, lints, builds with `RUSTFLAGS=-D warnings`, but does NOT publish, sign, or attest a release. There is no `release.yml`, no goreleaser/cargo-dist, no GPG signing of tags, no SBOM generation. The user's only install path today is `cargo install --path crates/p10k-rs` from a local clone. Repository has 18 commits, sole author "Sean Curran" (L31).
**Why it matters:** The prior review confidently said "supply chain audited" because deny.toml is strict — but distribution is also part of supply chain. The current model says "trust Sean's commits, build from source, run on your machine." That's defensible for early-alpha but the README says "Daily-driver-grade for the maintainer" — the maintainer is one cargo build away from running unaudited code on their personal shell. No one is enforcing what reaches `~/.cargo/bin/p10k-rs`.
**Status:** verified-by-agent (no release pipeline exists in `.github/`)

### [MEDIUM] 101 transitive packages unaudited; `cargo audit` not on host, prior review's exclusion stands
**Location:** `Cargo.lock`
**Evidence:** Agent confirmed at L82 that `Cargo.lock` has 101 packages (`grep -c '^\[\[package\]\]'`). At L62 / L75, the agent attempted `cargo audit` and was denied by the sandbox. `cargo` itself is on the host (lane 3 verified `/home/seaburdz/.cargo/bin/cargo`); `cargo-audit` is not installed, requires `cargo install --locked cargo-audit` first.
**Why it matters:** 101-package transitive graph in a tool that runs every prompt is a non-trivial advisory surface. `deny.toml` is strict on policy but does not run advisories on its own — it needs `cargo deny check` + advisory db. None of that has been run.
**Status:** preliminary (audit attempted but blocked; required for tomorrow's verification)

### [LOW] `p10k-rs-ai` crate exists but does NOT call out to LLMs — pre-fanout suspicion was wrong
**Location:** `crates/p10k-rs-ai/src/lib.rs`
**Evidence:** Agent read the crate's docstring at L13 / L58. Three responsibilities: (1) detect AI host (Claude Code, Cursor, Aider, Warp) from env-var heuristics, (2) emit OSC 7 / OSC 133 sequences for prompt-boundary semantics, (3) render a `--host claude-code` statusline payload. **No network, no API keys, no LLM call.** "AI" here is "AI-host-aware shell prompt", not "the prompt asks an LLM things." The crate is documented but largely unimplemented (text-only — agent only got the truncated preamble).
**Why it matters:** Defuses the orchestrator's pre-fanout concern that "p10k-rs-ai may exfiltrate data to an LLM." That risk is not present in slice 10. **However:** OSC 7 emits cwd, OSC 133 emits prompt boundaries — these are sent to the *terminal*, which can be a TTY-recording AI host. So the crate increases the surface of "prompt content seen by an outside watcher" by design. That's intentional, not malicious, but worth noting.
**Status:** verified-by-agent (refutes a pre-fanout suspicion)

### [INFO] Single-author repo; no review-before-merge is documented
**Location:** Repository
**Evidence:** Agent's L31: `git shortlog -sn --all` shows `18 Sean Curran` — sole author. CONTRIBUTING.md exists but the workflow assumes external contributors; no branch-protection rule enforces that, and there is no record of any PR merging external code.
**Why it matters:** Reduces some supply-chain risk (no external code injected) but increases another (single point of failure / single key compromise = full project compromise). For an early-alpha project with the maintainer as the only user, this is acceptable.
**Status:** verified-by-agent

### [INFO] CI uses `RUSTFLAGS=-D warnings` — strong warning hygiene, no advisory hygiene
**Location:** `.github/workflows/ci.yml`
**Evidence:** Agent read CI at L52: `CARGO_TERM_COLOR=always`, `RUST_BACKTRACE=short`, `CARGO_INCREMENTAL=0`, `RUSTFLAGS=-D warnings`. CI runs tests + lints. No `cargo audit` step. No `cargo deny check` step despite `deny.toml` being present and strict.
**Why it matters:** The deny.toml is the strongest part of the supply-chain story; not running it in CI means policy drift goes unnoticed. Cheap fix.
**Status:** verified-by-agent

## Investigation in flight (incomplete)

The agent's planned-but-unreached investigations:
- A trust-boundary table mapping every input (env vars, config files, git directory contents, gitstatusd response, argv, FIFO contents) to trust level + validator location.
- The hostile-repo step-by-step exploit chain (a developer clones a malicious project, what happens). Lane 2 covers the C1/C2 part of this; lane 4 was going to cover the .git/config / hooks / large-tree DoS angles.
- The hostile-environment scenario for shared hosts / CI runners / dev containers / multi-user lab boxes — what env vars an attacker user could set to escalate via this prompt.
- A privacy / data-flow inventory (`tracing` subscriber default behaviour, instant-prompt dump file content + permissions).
- A direct read of `crates/p10k-rs-ai/src/lib.rs` body (only got truncated preamble — the OSC emission code itself was not inspected line-by-line).
- TOML-parser attack surface for `p10k-rs-config` (the agent confirmed at L97 that the crate is currently a documented stub — `from_toml` is mentioned in docs but not implemented yet, so the surface is empty *today*).

## Confidence + caveats

This lane is mostly evidence-grounded but the **threat-model output document was never built.** I have rebuilt the seven supply-chain / distribution findings above from the agent's tool calls; I have NOT rebuilt the planned trust-boundary table or the three concrete attack scenarios because the agent never even started them.

The strongest single signal: the agent *defused* two concerns from the orchestrator's pre-fanout notes (vendored fixtures, AI-crate-as-LLM-exfil) and *corroborated* one (the gitstatusd v1.5.4 pin is documentary, not enforced). The 101-package transitive surface remains unmeasured — that's the highest-value tomorrow-action.

The orchestrator's pre-fanout note about `p10k-rs-ai` is refuted by code-reading; it does not call an LLM. But `p10k-rs-config`'s future TOML loader will be a new untrusted-input surface and is currently un-implemented; the prior review's exclusion stands.
