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
| 0001 | gitstatusd-class latency strategy | _to be written by the day-1 spike contractor_ |

ADR 0001 is the load-bearing decision for the project: whether `gix` plus a
`rustix`-based parent-fd walker can match `gitstatusd`'s latency on a clean
chromium repo. The spike crate (`crates/spike-gitstatus`) produces the data;
the spike contractor writes the ADR with the verdict. See `MVP-SPEC.md` § 0
and `07-gitstatus.md` for the full context.
