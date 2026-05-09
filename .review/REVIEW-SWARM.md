# Review Swarm — methodology

After every slice commit, six review agents run in parallel against `HEAD`.
Each agent owns one quality dimension and writes findings to
`.review/<utc-stamp>/<NN>-<area>.md`. A synthesis pass produces a
`SUMMARY.md` that ranks findings and feeds back into the next slice.

This is **always-on**. Don't ship a slice without running the swarm.

## Cadence

```
slice N committed → fire swarm in background → start slice N+1 design
                                               ↑              │
                                               │              ↓
                                  synthesis when agents finish
                                               │              │
                                               └──────────────┘
                                       findings inform slice N+1
```

## The six agents

| # | Area | Owner brief |
|---|---|---|
| 01 | Rust principles | idiomatic Rust, ownership/borrowing, error handling, trait design, type safety, lints |
| 02 | Security | unsafe blocks, command injection, path traversal, env var handling, FIFO/IPC isolation, privilege boundaries |
| 03 | Performance | allocations, syscalls/prompt, hot-path overhead, latency budget vs MVP-SPEC § 0 |
| 04 | Readability | naming, comment quality, function length, complexity, structural clarity |
| 05 | Documentation | module/fn docs, ADR alignment, RESUME.md staleness, planning bundle accuracy, README |
| 06 | Architecture & other | does code match ADR-0001? Slice boundary cleanliness, tech debt accumulation, test discipline, anything the other five miss |

## Severity rubric (used by all agents)

- **CRITICAL** — security or data-loss bug; ship-blocker.
- **HIGH** — correctness or major-perf issue; fix before next two slices.
- **MEDIUM** — quality issue with concrete fix; queue for a maintenance slice.
- **LOW** — nit, style, or speculative; mention but don't track.
- **INFO** — observation that warrants attention but isn't a defect.

## Per-agent prompt (template)

```
You are the {{AREA}} reviewer in the p10k-rs review swarm.

Repo: /home/seaburdz/github/powerlevel10k-rs
HEAD: {{git rev-parse HEAD}}
Methodology: .review/REVIEW-SWARM.md

Read these orientation files first:
  - README.md
  - docs/adr/0001-git-backend.md
  - .planning/powerlevel10k-rs/MVP-SPEC.md (if visible at /home/seaburdz/.planning/...)

Then review the workspace through the lens of {{AREA}}. Areas the other
reviewers will own (don't duplicate):
{{LIST OF OTHER AREA NAMES}}

Output one markdown report at exactly this path:
  /home/seaburdz/github/powerlevel10k-rs/.review/{{STAMP}}/{{NN}}-{{area-kebab}}.md

Schema:
```
# {{Area}} Review — {{STAMP}}

## Summary
2-4 sentence overall verdict.

## Findings

### [SEVERITY] short title
**Location:** path:line (or "workspace-wide")
**Issue:** one paragraph.
**Suggested fix:** one paragraph or code sketch.

(Repeat per finding. Use the severity rubric above.)

## Things this review explicitly did NOT examine
- bullet list

## Confidence
high / medium / low — and why.
```

Constraints:
- Read code, don't run it. Don't modify any file outside `.review/`.
- Limit to ≤ 12 findings; rank by severity then by remediation cost.
- Be specific. "function X is too long" → cite `path:line range`.
- Cite line numbers, not just file names.
- Word budget for the report: 1200 words max.

Return a one-paragraph (≤ 100 words) chat summary listing the highest-
severity findings.
```

## Synthesis pass

After all six agents finish:

1. Concatenate the per-area files.
2. Group findings by severity across files.
3. Write `.review/<stamp>/SUMMARY.md` with:
   - Top 3 CRITICAL/HIGH findings with one-line action items.
   - Aggregate count by severity.
   - Cross-cutting themes (issues two or more reviewers raised).
   - "Suggested next slice" — concrete deliverable to address the worst.

## What to commit

- `REVIEW-SWARM.md` (this file) — methodology, committed once.
- `.review/<stamp>/` — full snapshot per slice; committed as part of the
  slice's commit (or in a follow-up commit when fixes ship).
- Reviews are **not** authoritative — Sean is. Reviews surface, Sean decides.

## When to skip

- Trivial commits (typo, doc backtick) don't need a swarm.
- A slice that is 100% inside `.review/` doesn't review itself.
- Otherwise: run.
