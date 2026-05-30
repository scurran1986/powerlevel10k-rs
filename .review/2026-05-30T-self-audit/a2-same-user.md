# A2 — Local same-user attacker

**Capability summary:** another process running as the same uid as
the user — a compromised dev tool, a sandboxed-but-leaky CLI, a
malicious npm postinstall, a hostile editor extension. They can
read and write anywhere the user can, set environment variables for
shells they spawn, plant files in `$HOME`, `$TMPDIR`,
`$XDG_RUNTIME_DIR`, and `$XDG_CACHE_HOME`. They cannot escalate
privilege without a separate bug. Their goal: persistence via the
prompt path, or exfiltration via prompt-rendered output.

## Threats

### T-A2.1 — Plant a hostile `~/.config/p10k-rs/config.toml`
- **State:** **done (file open is mode + owner gated).** Config is
  opened via the inlined twin of `open_owned_mode_safely` — `O_NOFOLLOW`,
  `fstat` re-check for regular-file, owner-is-us-or-root, and a
  permission mask rejecting group / world write. A same-uid attacker
  who plants a 0644 config still wins the "set my own segments"
  game (they are us), but cannot abuse a less-restrictive symlink-
  swap or sneak in a config the user did not write.
- **Cite:** `crates/p10k-rs-config/src/lib.rs:972` and
  `crates/p10k-rs-config/src/lib.rs:1029` (inlined twin of
  `p10k_rs_core::safety::open_owned_mode_safely`).

### T-A2.2 — Plant a hostile `~/.cache/p10k-rs/dump-<user>-<term>.zsh`
- **State:** **done (zsh side: mode + owner gate); partial (Rust
  side: file is `O_CREAT | O_EXCL | O_NOFOLLOW | 0600`; parent
  dir is not owner-checked).** The zsh init refuses to source any
  dump that is not a regular file, owned by `EUID`, mode exactly
  0600. The Rust write side cannot be tricked into overwriting a
  symlink because of `O_NOFOLLOW + create_new`. A same-uid attacker
  can pre-empt the dump path with a 0600 file they own; that file
  meets the gate, so the next shell sources it. The dump has no
  content signature today.
- **Cite:** `crates/p10k-rs-shell/shells/zsh/init.zsh:98-106`
  (`! -L`, `uid == EUID`, `mode & 0777 == 0600`),
  `crates/p10k-rs/src/main.rs:1666-1681` (`write_dump_tmp_atomic`:
  `O_NOFOLLOW + create_new + mode 0600 + fsync`).
- **Residual:** see T-A2.6 (no content signature).

### T-A2.3 — Hijack `gitstatusd` via `$P10K_RS_GITSTATUSD_BIN`
- **State:** **done.** Override is honoured but routed through
  `check_candidate` → `open_owned_safely`: `O_NOFOLLOW`, fstat,
  owner-is-us-or-root. A same-uid attacker who plants a binary they
  own technically passes the check (they are us); the gate prevents
  the *foreign-owned* substitution, not the same-uid one. The
  sha256-pin (T0.5) is the second line of defence at install time;
  runtime ownership re-check is by-design "us-or-root" because
  `~/.local/bin/gitstatusd` is a normal install path.
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:666-696`
  (`locate_binary_checked`), `crates/p10k-rs-git/src/gitstatusd.rs:705-732`
  (`check_candidate`), `crates/p10k-rs-core/src/safety.rs:425-526`
  (`open_owned_safely` body).

### T-A2.4 — Race the FIFO open between `mkfifo` and the read
- **State:** **done.** Both `req` and `resp` FIFOs are opened via
  `open_fifo_safely`: `O_NOFOLLOW` on the open, `fstat` on the fd,
  re-verify `S_IFIFO` and owner uid. The `mktemp -d` directory the
  FIFOs live in is created with `umask 077` and mode 0700, so a
  pre-plant of the path requires already-winning the directory.
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:242` (function
  signature), `crates/p10k-rs-git/src/gitstatusd.rs:259-270` (S_IFMT
  / S_IFIFO check), `crates/p10k-rs-shell/shells/zsh/init.zsh:143-152`
  (`umask 077 && mkfifo -m 0600`).

### T-A2.5 — Hijack via env (`$XDG_RUNTIME_DIR`, `$XDG_CACHE_HOME`,
`$TMPDIR`, `$HOME`)
- **State:** **partial.** The shell init prefers
  `$XDG_RUNTIME_DIR` and falls back to `$TMPDIR`/`/tmp`. There is
  no Rust-side `validate_path_is_owned_by_us` step on the runtime
  base. However, the FIFO opens themselves are owner-checked, so
  pointing `$XDG_RUNTIME_DIR` at a hostile dir still fails at FIFO
  open. `$XDG_CACHE_HOME` is read at instant-dump path derivation;
  again the file open is `O_NOFOLLOW + O_CREAT | O_EXCL`, so a
  symlink at the redirected location does not survive. The
  *parent directory* is not validated, which is the residual gap.
- **Cite:** `crates/p10k-rs/src/main.rs:1524-1551`
  (`instant_dump_path`), `crates/p10k-rs/src/main.rs:1587-1589`
  (parent `create_dir_all`).

### T-A2.6 — Drop a `dump.zsh` that meets the owner-and-mode gate
- **State:** **open.** This is the "same-uid attacker writes a valid
  dump" attack. The dump has no content signature; the zsh gate is
  ownership + mode + regular-file only. **Mitigation deferred** —
  it needs a coordinated change: Rust embeds a BLAKE3 of
  `(config-hash || schema-version || hostname || uid)` into the
  dump, zsh refuses to source unless the embedded hash matches a
  freshly-derived one. The shell-side derivation is non-trivial
  (cannot run a Rust binary to derive it — that's the whole point
  of the dump). Tracked as Phase 3 of the v1.0 plan.
- **Cite:** `crates/p10k-rs/src/main.rs:1688-1701` (`zsh_dump_line`
  — content is plain `PROMPT='...'`, no signature line).

### T-A2.7 — Plant `~/.local/state/p10k-rs/diagnostics.log` as a hostile log
- **State:** **partial.** The zsh init appends stderr with `2>>`,
  which `open(2)` opens `O_WRONLY | O_CREAT | O_APPEND`. A
  same-uid attacker who pre-plants the log as a 0644 file gets a
  file they can read (they are us — already true) but cannot
  redirect the log destination without also winning the
  `$XDG_STATE_HOME` env vector. No symlink-followed gate.
- **Cite:** `crates/p10k-rs-shell/shells/zsh/init.zsh:470`.

### T-A2.8 — Read the rendered PROMPT bytes (exfiltrate cwd / branch)
- **State:** **out of scope.** A same-uid attacker can already
  `ptrace` the user's shell or read its `/proc/<pid>/cmdline`. The
  prompt does not introduce a new exfiltration primitive a
  same-uid attacker did not already have.

## Residual gaps (ranked, this attacker class)

1. **Dump file has no content signature.** Same-uid pre-plant
   succeeds. (T-A2.6.) Deferred to Phase 3 of the v1.0 plan;
   needs Rust + zsh coordinated change.
2. **Dump parent dir not owner-checked.** `create_dir_all` on
   `$XDG_CACHE_HOME/p10k-rs/` does not fail if the dir already
   exists as a same-uid attacker's mode-0777 directory. The
   *file* open then succeeds (it's still owned by us), so the
   dump itself is fine — but the file-listing in that dir is now
   visible to anyone the attacker grants access via the dir
   perms. Tracked in lane A; may be closed in this swarm.
3. **No `O_CLOEXEC` on FIFO / dump fd opens.** A future
   `posix_spawn` from a helper would inherit them. Currently the
   binary does not `posix_spawn` anything with the fds held open,
   but the lint posture is missing the belt-and-braces.
4. **`$XDG_RUNTIME_DIR` / `$XDG_CACHE_HOME` / `$TMPDIR` not
   path-validated** before use. Mitigated by per-open `O_NOFOLLOW`
   + owner check on every fd we open under them; not type-enforced
   at the variable-read site.

## Conclusion

The same-uid surface is the hardest to defend because we cannot
distinguish "the user typed this" from "the user's process typed
this" without crypto-anchored identity. The architecture leans
hard on per-open `O_NOFOLLOW` + `fstat` + owner-check, which
defeats the symlink-swap and foreign-owner classes. The unresolved
hole is the dump-file content signature (T-A2.6) — a same-uid
attacker who can write a 0600 file they own at the dump path gets
arbitrary zsh sourced on next shell start. The other residual
items (parent-dir ownership, `O_CLOEXEC`) reduce blast radius
rather than close net-new vectors and are in the v1.0 plan.
