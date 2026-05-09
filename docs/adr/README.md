# Architecture Decision Records

This directory holds ADRs for `p10k-rs`. Each record captures one decision: the
context, the options considered, the choice, and the consequences. We keep them
short. If you need more than two pages, you probably need a design doc, not an ADR.

## Format

We use the lightweight [Michael Nygard format](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions):

- **Title** — short noun phrase, prefixed with the ADR number.
- **Status** — proposed | accepted | superseded by NNNN | deprecated.
- **Context** — the forces in play.
- **Decision** — what we chose.
- **Consequences** — what follows from it (positive, negative, neutral).

File names: `NNNN-kebab-case-title.md`, four-digit zero-padded.

## Index

| ADR | Title | Status |
|----:|-------|--------|
| [0001](0001-git-backend.md) | Git Status Backend | Accepted (2026-05-06) |

ADR 0001 records the day-1 spike's verdict: PIVOT to a gitstatusd subprocess
client, because pure-Rust paths (gix-only, gix+rustix hybrid) come in 16-35×
slower than long-lived gitstatusd on the linux kernel. Numbers and full
reasoning in `bench/results/SPIKE-VERDICT-20260506T184527Z.md`.
