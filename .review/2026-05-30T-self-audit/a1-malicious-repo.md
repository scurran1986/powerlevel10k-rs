# A1 — Malicious repo author

**Capability summary:** an attacker who controls a git repository the
user has `cd`-ed into. They can set `.git/config` keys
(`core.fsmonitor`, `core.hooksPath`, …), name branches / tags / stash
entries / jj bookmarks arbitrarily, name files / directories
arbitrarily, configure submodule URLs, plant hook scripts, and
populate worktree state. Their reach ends at "things rendered into
the prompt or executed by the git or jj invocations the prompt
performs."

## Threats

### T-A1.1 — Branch / tag / stash name injection into the rendered prompt
- **State:** **done.** `vcs` and jj segments push branch / tag / stash
  / bookmark bytes through `SafeText` before they reach the rendered
  output. The fast path borrows when already-clean; the slow path
  strips C0/C1/DEL plus BiDi / zero-width / tag char / variation
  selectors and NFC-normalises.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:138-156`
  (`sanitize_for_terminal`), `crates/p10k-rs-core/src/safety.rs:83-101`
  (`is_unicode_unsafe` class list), `crates/p10k-rs-core/src/safety.rs:200-273`
  (`SafeText`).

### T-A1.2 — Branch name that triggers `PROMPT_SUBST` RCE on unpatched zsh
- **State:** **done.** Any `$`, backtick, or backslash that survives
  through the CSI / OSC arms of `wrap_for_shell` is escaped, so
  `$(rm -rf ~)` in a branch name cannot expand even on zsh <5.8.1
  (CVE-2021-45444).
- **Cite:** `crates/p10k-rs-core/src/lib.rs:903-924` (T1.12 / slice γ
  guard).

### T-A1.3 — Wedged `git status` (`.git/config` with hostile `core.fsmonitor`)
- **State:** **done.** `ShellOut::status_with_deadline` polls
  `try_wait` on a 25 ms cadence, kills the child with SIGKILL on
  timeout, and drains the pipes so post-kill `read(2)` returns EOF
  promptly. Hardened env (`GIT_CEILING_DIRECTORIES`, `LC_ALL=C`) is
  applied before spawn.
- **Cite:** `crates/p10k-rs-git/src/lib.rs:91-145` (T0.4 timeout),
  `crates/p10k-rs-git/src/lib.rs:148-170` (`apply_hardened_git_env`),
  `crates/p10k-rs-git/src/lib.rs:49` (`SHELLOUT_TIMEOUT = 2 s`).

### T-A1.4 — Oversize `gitstatusd` response from a pathological repo
- **State:** **done.** Response bytes are capped at 64 KiB; the read
  loop early-returns once the cap is reached.
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:70`
  (`MAX_RESPONSE_LEN = 64 * 1024`),
  `crates/p10k-rs-git/src/gitstatusd.rs:343` (cap enforcement).

### T-A1.5 — Hostile cwd path (control chars in dir component)
- **State:** **done.** The `dir` segment routes the working directory
  through `SafeText` after home-collapse; ANSI escapes and BiDi
  markers in a directory name cannot reach the terminal verbatim.
- **Cite:** `crates/p10k-rs-core/src/safety.rs:138-156`, and the
  segment-side wrap is documented as the chokepoint in
  `crates/p10k-rs-core/src/lib.rs:116-117`.

### T-A1.6 — Submodule URL / hook path embedded in `.git/config`
- **State:** **done (defence-in-depth).** Hardened env passed to
  every `git` spawn disables `GIT_OPTIONAL_LOCKS`, sets
  `GIT_CEILING_DIRECTORIES=` (empty), `LC_ALL=C`, etc. The deadline
  bounds the worst-case "hook hangs forever" outcome to
  `SHELLOUT_TIMEOUT`.
- **Cite:** `crates/p10k-rs-git/src/lib.rs:148-170`,
  `crates/p10k-rs-git/src/lib.rs:91-145`.

### T-A1.7 — In-progress action (rebase / merge / cherry-pick) reading
- **State:** **done.** `detect_action(&git_dir)` reads filesystem
  markers (e.g. `MERGE_HEAD`); paths come from `git rev-parse
  --git-dir`, not from arbitrary attacker-supplied input, and the
  rendered output flows through the same `SafeText` boundary.
- **Cite:** `crates/p10k-rs-git/src/lib.rs:108-113`.

### T-A1.8 — Tag / branch name containing oversize UTF-8 (DOS the prompt)
- **State:** **done.** `SafeText::from_untrusted_with_cap` truncates
  to `DEFAULT_SAFE_TEXT_CAP = 256` grapheme clusters with an `…`
  marker, on grapheme-cluster boundaries (no half-cluster cuts).
- **Cite:** `crates/p10k-rs-core/src/safety.rs:35-40`
  (`DEFAULT_SAFE_TEXT_CAP`), `crates/p10k-rs-core/src/safety.rs:163-176`
  (`truncate_to_graphemes`).

### T-A1.9 — `.git/HEAD` symlinked to a sensitive file
- **State:** **partial.** `git status` itself follows / does the
  resolution; `p10k-rs` does not open `.git/HEAD` directly. The
  filesystem-mode `detect_action` resolves through
  `git rev-parse --git-dir` (not raw walk) which inherits git's
  refusal to traverse out-of-repo (`safe.directory` machinery on
  modern git). No additional Rust-side `open_owned_*` is performed
  on `.git/*` content. Verified-by-absence rather than
  verified-by-citation.
- **Cite:** `crates/p10k-rs-git/src/lib.rs:108-113` (call site); no
  affirmative `open_*` gate on `.git/` content (intentional — git
  is the gate).

### T-A1.10 — Silent stderr discard hides "attack landed"
- **State:** **partial.** `crates/p10k-rs-shell/shells/zsh/init.zsh:470`
  redirects stderr to `${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs/diagnostics.log`,
  so genuine errors do reach a per-user log. There is no rate
  limit or audit-grade event log; a sufficiently noisy attacker
  could fill the log.
- **Cite:** `crates/p10k-rs-shell/shells/zsh/init.zsh:470`.

## Residual gaps (ranked, this attacker class)

1. **No Rust-side `safe.directory` enforcement** beyond what `git`
   itself does. If the user has set `safe.directory = *`, hostile
   `.git/config` keys reach a `git` we spawn unchanged. This is a
   user-config concern, not a `p10k-rs` defect, but the project
   does not document it.
2. **`gitstatusd` runs with the user's full env.** Hardened-env
   overrides apply to the `ShellOut` `git` spawn only. A
   `gitstatusd`-side env-derived RCE would be reachable. No
   evidence such a path exists in current `gitstatusd`, but the
   trust boundary is "the binary we pinned by sha256 + ownership-
   check" — see A4 for the supply-chain framing.
3. **No structured audit log** of `SafeText` strip events. A user
   inheriting a clone with a malicious branch name sees the
   stripped output silently; nothing tells them the byte sequence
   was hostile. Defensible (we don't want a noisy prompt) but
   worth surfacing as an opt-in diagnostic for power users.

## Conclusion

For the malicious-repo attacker, the render-path defences are
strong and citable. The shell-out path is timed-out, env-hardened,
and capped; the gitstatusd wire is owner-checked and bounded.
The principal residual risk is what is *outside* the boundary —
the user's own `safe.directory` config, and a hypothetical
gitstatusd-side env-derived bug we cannot eliminate without
running gitstatusd as a separate uid (which the architecture does
not require). Net: the A1 surface is well-defended; a competent
adversary needs a different attacker model to land.
