# Critical Security Audit — Resume Notes

**Started:** 2026-05-09 ~01:28 local
**Status:** All four agents rate-limited mid-run. Resumes after limit resets (≈05:50 America/Chicago).
**Triggered by:** User asked for a 180-IQ critical audit of this brand-new repo, with multiple parallel agents being deliberately tough on the codebase. Prior swarm review (`.review/20260509T071500Z/`) declared "0 CRITICAL, 2 HIGH (both doc drift)" — user wanted that conclusion stress-tested.

---

## Audit Plan (4 parallel angles, deliberately non-overlapping)

| # | Angle | Agent type | Status | Transcript |
|---|---|---|---|---|
| 1 | IPC + process lifecycle (TOCTOU, daemon spawn, FIFO races, $PATH trust, signal/cleanup) | general-purpose (write-capable) | rate-limited at 8 tool uses — barely started | `/tmp/claude-1000/-home-seaburdz-github-powerlevel10k-rs/794df3aa-6985-44d2-aa7e-e6c8fbbda924/tasks/af3c4dc9dd44335cf.output` |
| 2 | Render-path injection (branch/dir/path → terminal/zsh PROMPT, ANSI/`%`/`%{}` escapes, install.sh argv) | security-reviewer (read-only) | rate-limited at 31 tool uses — substantial investigation | `tasks/a40de06fdfe73a78b.output` |
| 3 | Meta-critique of prior review (severity calibration, scope omissions, drift between two prior runs, methodology gaps) | critic (read-only) | rate-limited at 48 tool uses — heavy investigation | `tasks/a04cb877f3feb22e4.output` |
| 4 | Threat model + supply chain (`p10k-rs-ai` crate, `cargo audit`, hostile-repo + hostile-host scenarios, vendored fixtures) | architect (read-only) | rate-limited at 48 tool uses — heavy investigation | `tasks/af1cdf16789326193.output` |

**Important:** Three of the four agents were read-only and were instructed to write to disk anyway — they cannot write. Their findings, if recoverable, are in the JSONL transcripts above. Only agent #1 had Write capability.

**Output target dir for the new audit:** `.review/20260509T130000Z/`

---

## Initial findings I (orchestrator) noticed myself before fanout

Read pre-fanout: `install.sh`, `crates/p10k-rs-git/src/gitstatusd.rs`, `.review/20260509T071500Z/02-security.md`, `.review/20260509T071500Z/SUMMARY.md`. These are observations from that pass — they need verification but are not from the rate-limited agents.

### [LIKELY HIGH] FIFO request framing is byte-injectable via directory name
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:99–105`
The request is framed `id\x1F<dir>\x1E`. The `<dir>` value is `path.as_os_str().as_encoded_bytes()` written verbatim — no escaping. If a directory name contains `\x1F` (US) or `\x1E` (RS), the framing breaks. An attacker who can convince the user to `cd` into a path containing `$'\x1F'` or `$'\x1E'` can inject extra fields or a second request into the daemon. Not flagged by the prior review. Severity depends on what the daemon does with malformed framing — needs tomorrow's verification.

### [LIKELY HIGH] No request/response correlation
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:91–115` + parser at 165–212
Request id is the literal constant `b"p10k-rs-prompt"`. The parser at 165 doesn't validate the response id — it just splits and reads `fields[1]`. With concurrent prompts (split panes, two zsh subshells inheriting the same `_P10K_RS_GITSTATUSD_REQ/RESP`), responses can cross-talk. Worse: a half-timed-out request (line 132 returns `None` on timeout) leaves the daemon's response queued; the *next* prompt reads a stale response and renders state from a different repo. Not flagged by prior review.

### [LIKELY CRITICAL] Branch/commit fields rendered to terminal without sanitization
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:191–192` → flows into `crates/p10k-rs-segments/src/vcs.rs`
`s(3).to_owned()` (commit) and `s(4).to_owned()` (branch) are owned strings carried unsanitized into `GitState`. Git permits branch names containing escape sequences (`git branch $'\e[2J\e[H'` works on most setups). Once rendered into a zsh PROMPT, two attack vectors: (a) raw ANSI escapes hit the terminal, enabling title-spoofing/screen-clearing; (b) `%` characters in the rendered string are interpreted by zsh's PROMPT expansion — `%(?.x.y)` etc. — which is a code-execution surface in the worst case. **The prior review did not examine this path.** This is the kind of finding the render-injection agent (#2) was asked to confirm; awaiting its transcript.

### [MEDIUM] `from_utf8(...).unwrap_or("")` silently empties non-UTF-8 fields
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:179`
A repo with non-UTF-8 branch bytes shows as empty. Not exploitable, but a correctness/UX issue and can mask the real branch in security-relevant audit logs that consume the prompt.

### [CONFIRMS PRIOR REVIEW BUT WORSE] install.sh:126 re-introduces the dev-machine path
**Location:** `install.sh:125–129`
The prior review's slice-9 commit (`d99a514`) explicitly **removed** the `$HOME/github/powerlevel10k/...` fallback from the Rust binary as a security fix. `install.sh` *still has it*, at install-time, as the **first** candidate in `GITSTATUSD_CANDIDATES`. The prior SUMMARY notes this but classifies it MEDIUM ("install-time, Sean-machine"). Re-rate: anyone who clones the repo and runs install.sh on a host where they happen to have *any* directory at `$HOME/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64` will silently install whatever binary lives there. No checksum. No signature. The path is named after a real upstream project, increasing the chance of an attacker pre-planting it.

### [LOW–MEDIUM] No version/checksum enforcement on gitstatusd
**Location:** `install.sh:131–133` and `THIRD-PARTY-LICENSES.md`
`THIRD-PARTY-LICENSES.md` asserts a v1.5.4 pin. Nothing enforces it: the install script symlinks whatever first match it finds. If `/opt/homebrew/bin/gitstatusd` is a different version, the binary still uses it; if a downstream package has a vulnerability, no detection.

---

## What the prior review explicitly excluded (and tomorrow should attack)

From `.review/20260509T071500Z/02-security.md` § "Things this review explicitly did NOT examine":
- `cargo audit` — not run; "toolchain not available in review sandbox" — **but it can run on this host; needs to be run tomorrow**
- `bench/fixtures/` vendored code — full ripgrep + linux kernel sources are tracked in git, ~unknown size — supply-chain footprint
- Performance, style, docs, architecture lanes — out of scope for the security review
- `p10k-rs-ai` crate was not specifically examined for data exfiltration / network behavior — that crate is 112 lines and needs a line-by-line read

## Prior review's confidence claim to challenge

> "**High.** The security surface is small (CLI tool, no network listeners, no auth, no user-facing input beyond shell env vars and filesystem paths)."

This is the line to attack tomorrow. The prior review labels its scope "small" but **branch names and directory paths from cloned repos are user-facing untrusted input** — the highest-frequency untrusted input on a developer's machine. Calling that "small" is a category error, and the "no CRITICAL" verdict probably rests on it.

---

## Resume checklist for tomorrow (in order)

1. **Try mining the rate-limited transcripts.** They contain ~127 tool-use rounds of investigation across the three deep agents. Spawn a fresh general-purpose agent with prompt: "Read the JSONL files at the four `tasks/*.output` paths above, extract every finding the agents identified before the rate limit, and write each to `.review/20260509T130000Z/0X-<topic>.md`." Be cautious: those JSONL files can be large; agent should grep / chunk them rather than reading whole.
2. **Run cargo audit and cargo deny check** — both should work in this environment.
   ```
   cargo install --locked cargo-audit cargo-deny  # if missing
   cargo audit
   cargo deny check
   ```
3. **Verify the [LIKELY HIGH] / [LIKELY CRITICAL] preliminary findings above** with concrete reproducers:
   - Create a branch named `$'\e[2J'` and observe rendered prompt
   - Create a directory whose name contains literal `\x1F`, `cd` into it, observe daemon request framing
   - Open two zsh subshells, fire prompts concurrently (cross-talk test)
4. **Re-launch agents** using write-capable types only (general-purpose / executor / scientist-with-Bash). The original prompts are good; just swap the subagent_type.
5. **Synthesize** into `.review/20260509T130000Z/SUMMARY.md` with re-rated severities and a sharper challenge to the "0 CRITICAL" verdict.

---

## Files already touched in this run

- Created `.review/20260509T130000Z/` (this dir, currently only this file)
- No source code modified
- No commits made

## What NOT to do tomorrow

- Don't `cat` the JSONL transcripts directly — they will overflow main context. Use a sub-agent to extract.
- Don't trust the prior review's MEDIUM ratings without re-checking environmental assumptions (TMPDIR, multi-user, NFS).
- Don't re-launch the read-only specialist agents (`security-reviewer`, `critic`, `architect`) for tasks that require writing — pick general-purpose or executor.
