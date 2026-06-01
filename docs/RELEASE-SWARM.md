# Release-swarm runbook

> Operational playbook for multi-lane Claude Code swarms on this
> project. Codified after the 2026-05-31 v1.0 swarm — 6 agents,
> 2 swarms, 0 truncations, 6 pre-existing defects surfaced
> post-tag. The pattern this doc captures is what *prevents* the
> 4 of those 6 that should have been caught pre-tag.

## When to swarm

Swarm only when **all three** are true:

1. **Multiple independent deliverables.** Lane outputs don't depend
   on each other; integration is a clean cherry-pick chain.
2. **Single-crate-primary per lane.** Each agent touches one logical
   surface. Multi-dep lanes truncate even with phase-commit prompting
   ([[feedback_swarm_recovery_2026_05_22]]).
3. **Each lane needs more than a Pomodoro.** If a lane fits in
   ~5 min of foreground work, the orchestration overhead outweighs
   the parallelism gain. Do it foreground.

Don't swarm what's actually sequential. The v1.0 packaging update
ran as 3 lanes (homebrew+flake, scoop, AUR×2) because the manifests
are independent files with disjoint hashes. The fuzz-dep fix and
miri config fix were foreground because they were single-file
single-logical-change edits.

## Before spawning anything: the gate audit lane

The v1.0 swarm's biggest finding: **gates that were already broken
on `main` cost three commits of release-engineering before they
surfaced.** Run the audit first.

```bash
./gates.sh           # fast gates, ~2 min
./gates.sh --slow    # + miri + semver-checks, ~5-10 min
```

If anything fails, **fix it before the swarm starts.** The fix
should be a separate slice on `main` ahead of the release work
(matches the v1.0 chain: `fix(rustdoc):` → swarm B/C/A → tag →
`fix(fuzz):` → packaging swarm). Don't bury a pre-existing fix
inside a release-engineering commit; the commits should be readable
later as "here's the prep, here's the release, here's the
packaging."

Why this matters: CI / local divergence has been the root cause
behind multiple v0.4.0 defects shipping. With `gates.sh` mirroring
CI exactly, the divergence is structurally closed.

## Worktree setup — manual only

**Do not use `isolation: "worktree"` in the Agent call.** The
harness pins the worktree to the session-start ref snapshot, not
current `main` ([[feedback_worktree_harness_stale_base_2026_05_24]]).
Two T1.8 agents on 2026-05-24 lost the v0.1.7 + slice-64 work to
phantom "merge conflicts" because of this bug.

Manual workaround:

```bash
git worktree add -b v1-lane-a /tmp/p10k-v1-lane-a main
git worktree add -b v1-lane-b /tmp/p10k-v1-lane-b main
git worktree add -b v1-lane-c /tmp/p10k-v1-lane-c main
git worktree list  # verify all three are at current main HEAD
```

Pass `/tmp/p10k-v1-lane-X` as the worktree path in the agent
prompt. Each lane operates on its own branch and is integrated
back to `main` via `git cherry-pick`.

## Lane prompt anatomy

Every lane prompt has these sections, in this order:

1. **You are LANE X of an N-agent swarm doing Y.** Sets context up
   front; agent isn't trying to derive purpose from the work.
2. **Worktree.** Absolute path + branch name + base commit. Start
   with `cd <path>` and stay there.
3. **Context.** What the swarm is for. Cite specific upstream
   artifacts (CHANGELOG entries, prior commits) for grounding.
4. **In scope (the only files you touch).** Explicit file list with
   the exact edits per file. Include hash values, version strings,
   etc. inline — don't ask the agent to compute or look up what you
   already have.
5. **Out of scope (DO NOT touch).** Explicit file list of what other
   lanes own. Each name should appear in exactly one lane's
   in-scope list.
6. **Commit ceremony.** Exact `git add` lines + exact commit message
   (HEREDOC). Sign-off line. Report contract.
7. **Anti-truncation guardrails.** Don't run tests if not needed.
   Don't over-research. Single commit. Do not push. State the
   bounded work.

## The Lane-B verification pattern

The v1.0 Lane B verified every claim in its prompt against actual
code and corrected the four wrong assertions I'd put there
(`Color` is NOT `#[non_exhaustive]`; `THREAT-MODEL.md` doesn't
exist; `RELEASE-CHECKLIST.md` is under `packaging/`; there's no
`--import-p9k` flag, the subcommand is `import <path>`).

Bake this into every lane that asserts facts about the codebase:

```
### Verification step

Before writing, verify these claims against actual code:
- Claim X: check by `grep ... crates/...`
- Claim Y: check by reading `crates/.../src/...`
- Claim Z: check by `find ... -name ...`

If a claim is wrong, document the actual state in your output AND
in your commit-message body. Don't propagate the prompt's wrong
assertion forward.
```

This costs one extra paragraph per lane prompt and one minute of
verification per lane. It prevents wrong assertions from
compounding into wrong documentation, wrong tests, and wrong
release notes.

## Integration: cherry-pick chain + gate sweep + tag

After all lanes report clean:

```bash
git log --oneline -1                              # confirm main HEAD
for lane in v1-lane-b v1-lane-c v1-lane-a; do     # order matters
    git cherry-pick "$lane"
done
git log --oneline -8                              # verify chain
./gates.sh                                        # full sweep before tag
```

**Lane order matters.** Put the lane whose commit should sit at
HEAD last — that's where the tag will land. For a release swarm
the order is typically: stability docs → user-facing narrative →
release engineering. The `chore(release): vX.Y.Z` commit sits at
HEAD and the tag points there.

If `gates.sh` fails after the cherry-pick chain, **don't** push the
tag. The chain is recoverable by `git reset --hard <main-pre-swarm>`
and the lane branches still hold the commits.

If `gates.sh` passes:

```bash
git tag vX.Y.Z -m "p10k-rs vX.Y.Z — <theme>"
git push origin main
git push origin vX.Y.Z
```

## Post-tag: packaging follow-up

Packaging manifests (Homebrew, Scoop, AUR ×2, Nix flake) need:

- Version bump to the new tag
- Real sha256 from release artifacts (not available until the
  release workflow builds binaries)

This is **always** a separate swarm after the tag has fired the
release workflow. Manually fetch the per-artifact hashes first:

```bash
mkdir -p /tmp/p10k-vX-hashes && cd /tmp/p10k-vX-hashes
for f in p10k-rs-X.Y.Z-{aarch64-apple-darwin,x86_64-apple-darwin,\
x86_64-unknown-linux-gnu,aarch64-unknown-linux-gnu,\
x86_64-pc-windows-msvc,aarch64-pc-windows-msvc}.tar.gz.sha256 \
     p10k-rs-X.Y.Z-*.zip.sha256 ; do
    gh release download vX.Y.Z -p "$f" -R scurran1986/powerlevel10k-rs
done
curl -sL https://github.com/scurran1986/powerlevel10k-rs/archive/vX.Y.Z.tar.gz \
    | sha256sum  # source tarball sha for AUR source PKGBUILD
```

Inline the hashes into the lane prompts directly. Don't have agents
fetch their own — eliminates a failure mode where the agent does
the wrong `curl` and computes a wrong hash.

## Cleanup

After integration + push:

```bash
git worktree remove /tmp/p10k-vX-lane-a
git worktree remove /tmp/p10k-vX-lane-b
git worktree remove /tmp/p10k-vX-lane-c
git branch -D v1-lane-a v1-lane-b v1-lane-c
git worktree list  # should show only main
```

Stale agent worktrees from prior sessions (e.g. `swarm/v030-*`,
`worktree-agent-*` branches) are not this swarm's mess. Leave them.

## Known failure modes

- **Multi-dep slices truncate** regardless of phase-commit prompting.
  Surface a multi-dep slice as foreground or split it further.
  ([[feedback_swarm_pattern_2026_05_19]])
- **Auto-isolation harness bug** — covered above; manual worktrees only.
- **Shell `cwd` silently shifts** into an agent worktree mid-session.
  Always `cd ~/github/powerlevel10k-rs` before integration ops.
  ([[project_slice_64_closure_2026_05_23]])
- **Agent `Write` with absolute path** can leave orphan files in
  main. Recover by moving them; don't `git reset --hard`.
  ([[feedback_swarm_recovery_2026_05_22]])
- **SendMessage can't recover a truncated worktree agent.** Once an
  agent's context is gone, the work is integrate-or-redo, not
  resume. ([[feedback_swarm_recovery_2026_05_22]])

## The v1.0 ship — canonical worked example

Two swarms across one session:

**Swarm 1 — release prep (3 lanes):**

- Lane A (release engineering): `Cargo.toml`s + `CHANGELOG.md` +
  `.github/release-notes/v1.0.0.md`
- Lane B (stability docs): `STABILITY.md` + `docs/src/stability.md`
  + `docs/src/SUMMARY.md`
- Lane C (user-facing): `README.md` + `docs/src/migration.md`

Integration order: B → C → A. Tag fires on A's commit. Pre-tag
foreground patch: `fix(rustdoc):` for an unresolved intra-doc link
on v0.4.0 that the doc gate caught.

**Swarm 2 — packaging (3 lanes):**

- Lane D1: Homebrew formula (`.rb`) + Nix flake (`flake.nix`)
- Lane D2: Scoop manifest (`.json`)
- Lane D3: Both AUR PKGBUILDs (`.tar.gz` source + `.tar.gz` binary)

All five manifests had been pinned at v0.2.7 — they had sat through
v0.3 + v0.4 cycles untouched. The swarm jumped them straight to
v1.0.0 with hashes inlined.

**Post-tag foreground patches:**

- `fix(fuzz):` drop unused dep (CI cargo-machete caught it)
- `ci(miri):` force `+nightly` (toolchain.toml was pinning miri off)
- `fix(config)!:` reject non-ASCII bytes in hex colour Visitor
  (cargo-fuzz `toml_config` found a real panic on the first
  v1.0-cycle main push)
- `docs(packaging):` refresh crates.io note for the v1.0 stance

All six post-tag patches are defects that **existed on v0.4.0**.
The v1.0-cycle CI surfaced them because CI's gate matrix was
broader than the maintainer's local sweep had been. `gates.sh`
exists so that gap closes.

## See also

- [`gates.sh`](../gates.sh) — the script this runbook orchestrates
- [`CLAUDE.md`](../CLAUDE.md) — project rules
- [`packaging/RELEASE-CHECKLIST.md`](../packaging/RELEASE-CHECKLIST.md) — release-time operational checklist
- [`STABILITY.md`](../STABILITY.md) — what v1.0 commits to
- Auto-memory at `~/.claude/projects/-home-seaburdz-github-powerlevel10k-rs/memory/` — operational gotchas indexed by trigger phrase
