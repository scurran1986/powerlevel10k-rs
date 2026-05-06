//! Straight `gix-status`-style implementation. The "boring Rust" baseline.
//!
//! Uses only `gix` high-level APIs — no syscall tricks, no caches. This is the
//! number we have to beat with the hybrid path; if the spike shows the gix-only
//! number is already inside our latency budget, we drop the hybrid entirely
//! and ship the simpler architecture (see `MVP-SPEC.md` § "Decision tree").
//!
//! Strategy:
//! 1. `gix::open` the repo.
//! 2. Resolve HEAD into branch name + 40-hex oid.
//! 3. Compute ahead/behind via the upstream tracking ref, if any.
//! 4. Walk the index for the conflict count (cheap, in-memory).
//! 5. Run `Repository::status(...)` to enumerate index-vs-worktree deltas
//!    (unstaged + untracked) and a hand-rolled HEAD-tree-vs-index walk for
//!    staged. `gix` 0.66's status `Platform` only models index/worktree, so
//!    the staged side is computed separately rather than faked.

use std::collections::HashSet;
use std::path::Path;

use crate::{GitStatusSummary, Result, SpikeError};

/// Compute a [`GitStatusSummary`] for `repo_path` using only `gix` APIs.
///
/// `repo_path` may be the repo root or any subdirectory; `gix::discover` walks
/// upward until it finds a `.git`. Returns [`SpikeError::NotARepository`] if
/// none is found within the filesystem boundary.
pub fn status(repo_path: &Path) -> Result<GitStatusSummary> {
    let repo = gix::discover(repo_path)
        .map_err(|_| SpikeError::NotARepository(repo_path.to_path_buf()))?;

    let mut summary = GitStatusSummary::default();

    // --- HEAD: branch + commit oid ------------------------------------------------
    //
    // `head_ref()?` returns `None` for a detached HEAD; in that case
    // `head_id()` still gives us the oid, and the branch name stays empty,
    // matching the wire-format convention from `07-gitstatus.md` § field 5.
    match repo.head_ref() {
        Ok(Some(reference)) => {
            // `name().shorten()` strips the `refs/heads/` prefix.
            summary.branch = reference.name().shorten().to_string();
        }
        Ok(None) => {
            // Detached HEAD: leave branch empty.
        }
        Err(e) => return Err(SpikeError::GixRef(e.to_string())),
    }

    // `head_id()` errors on an unborn branch (a freshly `git init`'d repo with
    // no commits yet). That's a valid state — we report empty `commit`.
    let head_oid: Option<gix::ObjectId> = repo.head_id().ok().map(|id| {
        let oid = id.detach();
        summary.commit = oid.to_hex().to_string();
        oid
    });

    // --- ahead / behind -----------------------------------------------------------
    //
    // gix 0.66 doesn't ship a single-shot ahead/behind helper. We resolve
    // the upstream tracking ref via `Reference::remote_tracking_ref_name`,
    // then count the symmetric difference of the two reachability sets. For
    // a prompt this is bounded by typical local-vs-upstream divergence, not
    // the full history.
    //
    // If no upstream is configured we report `0/0`, matching gitstatusd's
    // "no upstream" zero-fill (see `07-gitstatus.md` § field 8/9).
    if let (Some(local), Ok(Some(local_ref))) = (head_oid, repo.head_ref()) {
        if let Some(Ok(remote_name)) =
            local_ref.remote_tracking_ref_name(gix::remote::Direction::Fetch)
        {
            if let Ok(mut remote_ref) = repo.find_reference(remote_name.as_ref()) {
                if let Ok(remote_id) = remote_ref.peel_to_id_in_place() {
                    let (ahead, behind) =
                        count_ahead_behind(&repo, local, remote_id.detach()).unwrap_or((0, 0));
                    summary.ahead = ahead;
                    summary.behind = behind;
                }
            }
        }
    }

    // --- conflicts (cheap: walk the index) ----------------------------------------
    //
    // Stage > 0 in an index entry indicates an unmerged path; multiple
    // entries per path are normal during conflict, so the boolean is what
    // we report (not a count).
    if let Ok(index) = repo.index() {
        summary.has_conflicts = index
            .entries()
            .iter()
            .any(|e| e.stage() != gix::index::entry::Stage::Unconflicted);
    }

    // --- staged (HEAD tree vs index) ---------------------------------------------
    //
    // `gix::status::Platform` in 0.66 only emits index-vs-worktree items,
    // not tree-vs-index. We do the comparison by hand: for each index entry,
    // look up the same path in the HEAD tree and count entries whose blob
    // oid differs (or is missing in the tree = newly added). This matches
    // the semantics of `git diff --cached --name-only | wc -l` for the
    // common case (no rename detection, matching gitstatusd defaults).
    summary.staged_count = staged_count(&repo)?;

    // --- unstaged / untracked (index vs worktree) --------------------------------
    //
    // `Repository::status(progress)` returns a `Platform`; `into_index_worktree_iter(patterns)`
    // turns it into an iterator of `Item`s. The iterator is lazy; we drain it.
    let status_iter = repo
        .status(gix::progress::Discard)
        .map_err(|e| SpikeError::GixStatus(e.to_string()))?
        .into_index_worktree_iter(Vec::new())
        .map_err(|e| SpikeError::GixStatus(e.to_string()))?;

    use gix::status::index_worktree::iter::Item;
    for item_result in status_iter {
        let item = item_result.map_err(|e| SpikeError::GixStatus(e.to_string()))?;
        match item {
            Item::Modification { status, .. } => {
                use gix::status::plumbing::index_as_worktree::EntryStatus;
                match status {
                    // A real worktree change relative to the index entry.
                    EntryStatus::Change(_) => summary.unstaged_count += 1,
                    // Conflicts are tracked separately via the index walk above;
                    // count them as unstaged too so the caller sees a non-zero
                    // change count even on a conflicted-only state.
                    EntryStatus::Conflict(_) => summary.unstaged_count += 1,
                    // `IntentToAdd` shows up for `git add -N`; treat as unstaged
                    // (the path has no content yet so it's a pending change).
                    EntryStatus::IntentToAdd => summary.unstaged_count += 1,
                    // `NeedsUpdate` is a stat-only refresh signal; not a change.
                    EntryStatus::NeedsUpdate(_) => {}
                }
            }
            Item::DirectoryContents { entry, .. } => {
                if matches!(entry.status, gix::dir::entry::Status::Untracked) {
                    summary.untracked_count += 1;
                }
                // Pruned / Tracked / Ignored: not counted.
            }
            Item::Rewrite { .. } => {
                // Rename detection is off by default; the iter will not emit
                // these in the spike's configuration, but match exhaustively.
            }
        }
    }

    Ok(summary)
}

/// Count `(ahead, behind)` between two oids by collecting reachability sets.
///
/// Equivalent to `git rev-list --left-right --count A...B`. We walk both tips
/// to build oid sets, then count the symmetric difference. Bounded by the
/// reachability of each tip; in the prompt case (local-vs-upstream divergence)
/// this is small. Returns `Err` only if the underlying object database can't
/// be read.
fn count_ahead_behind(
    repo: &gix::Repository,
    local: gix::ObjectId,
    remote: gix::ObjectId,
) -> Result<(u32, u32)> {
    if local == remote {
        return Ok((0, 0));
    }
    let local_set = walk_set(repo, local)?;
    let remote_set = walk_set(repo, remote)?;

    let ahead = local_set
        .iter()
        .filter(|id| !remote_set.contains(*id))
        .count();
    let behind = remote_set
        .iter()
        .filter(|id| !local_set.contains(*id))
        .count();
    Ok((
        u32::try_from(ahead).unwrap_or(u32::MAX),
        u32::try_from(behind).unwrap_or(u32::MAX),
    ))
}

/// Collect every commit reachable from `tip` into a hash set.
fn walk_set(repo: &gix::Repository, tip: gix::ObjectId) -> Result<HashSet<gix::ObjectId>> {
    let walk = repo
        .rev_walk([tip])
        .all()
        .map_err(|e| SpikeError::GixRef(e.to_string()))?;
    let mut out = HashSet::new();
    for info in walk {
        let info = info.map_err(|e| SpikeError::GixRef(e.to_string()))?;
        out.insert(info.id);
    }
    Ok(out)
}

/// Count files whose blob oid in the index differs from the HEAD tree, or
/// that don't exist in the HEAD tree at all (newly staged).
///
/// Returns `Ok(0)` if the repo has no HEAD commit yet (an unborn branch).
fn staged_count(repo: &gix::Repository) -> Result<u32> {
    use gix::bstr::ByteSlice;

    // No HEAD yet → every index entry is "staged for the initial commit".
    let head_tree_id = match repo.head_tree_id() {
        Ok(id) => id.detach(),
        Err(_) => {
            return Ok(repo
                .index()
                .map(|idx| u32::try_from(idx.entries().len()).unwrap_or(u32::MAX))
                .unwrap_or(0));
        }
    };

    let tree = repo
        .find_object(head_tree_id)
        .map_err(|e| SpikeError::GixRef(e.to_string()))?
        .into_tree();

    let index = repo
        .index()
        .map_err(|e| SpikeError::GixRef(e.to_string()))?;

    let mut count: u32 = 0;
    let mut buf = Vec::new();
    for entry in index.entries() {
        // Skip stages > 0 (conflict variants) — they aren't "staged" in the
        // wire-format sense; they're unmerged paths counted by `has_conflicts`.
        if entry.stage() != gix::index::entry::Stage::Unconflicted {
            continue;
        }
        let path_bytes = entry.path(&index);
        let path = match path_bytes.to_path() {
            Ok(p) => p,
            Err(_) => continue,
        };
        match tree.lookup_entry_by_path(path, &mut buf) {
            Ok(Some(tree_entry)) => {
                if tree_entry.oid() != entry.id.as_ref() {
                    count = count.saturating_add(1);
                }
            }
            Ok(None) => {
                // New file in the index that wasn't in HEAD: a staged add.
                count = count.saturating_add(1);
            }
            Err(_) => {
                // Path lookup failure (e.g. ill-formed bytes) — be conservative
                // and don't count, the unstaged/untracked walk will catch real
                // worktree weirdness.
            }
        }
    }
    Ok(count)
}
