# A3 — Local different-user attacker

**Capability summary:** another user on a multi-user box — a
classic shell account, a CI runner with multiple build users, a
container with shared `/tmp` mounted from the host, a
multiplexer's per-session uid. They can race `/tmp`, observe
predictable filenames, plant symlinks in shared dirs, and listen
on sockets in shared namespaces. They cannot read the user's
`$HOME` (the kernel says no) and cannot signal the user's
processes. Their goal: get the user's shell to interpret bytes
they planted, or pivot via a shared filesystem location.

## Threats

### T-A3.1 — Pre-plant a symlink at the FIFO path under `$TMPDIR`
- **State:** **done.** The per-shell IPC directory is created with
  `mktemp -d -- "$base/p10k-rs.XXXXXXXX"` (unpredictable suffix)
  and `chmod 0700` on the directory; the FIFOs are created with
  `umask 077 && mkfifo -m 0600`. The Rust-side `open_fifo_safely`
  additionally re-verifies `S_IFIFO` and owner uid on the fd. A
  different-user pre-plant in the parent dir (`/tmp` itself)
  cannot anticipate the mktemp suffix.
- **Cite:** `crates/p10k-rs-shell/shells/zsh/init.zsh:143-152`
  (`mktemp -d`, `umask 077`, `mkfifo -m 0600`),
  `crates/p10k-rs-git/src/gitstatusd.rs:242` (open helper),
  `crates/p10k-rs-git/src/gitstatusd.rs:259-270` (fd re-verify).

### T-A3.2 — Symlink-swap on the instant-prompt dump path
- **State:** **done.** `write_dump_tmp_atomic` uses
  `O_NOFOLLOW | O_CREAT | O_EXCL | 0600`; a symlink at the path
  fails the `create_new` open. The zsh side gates with `! -L` and
  `zstat +mode +uid`. A different-user symlink at
  `~/.cache/p10k-rs/dump-<user>-<term>.zsh` cannot resolve — the
  parent dir is under `$HOME`, which a different user cannot
  enter (assuming `~` mode 0700 / 0750). On a misconfigured
  `$HOME` mode 0755, the file open still fails because of
  `O_NOFOLLOW + create_new` on the *target*; the zsh source-time
  check additionally enforces ownership.
- **Cite:** `crates/p10k-rs/src/main.rs:1666-1681`,
  `crates/p10k-rs-shell/shells/zsh/init.zsh:98-106`.

### T-A3.3 — Race the `$XDG_RUNTIME_DIR` IPC dir on a shared host
- **State:** **done by systemd convention; partial otherwise.**
  `$XDG_RUNTIME_DIR` is per-uid mode 0700 on systemd hosts (the
  systemd-logind contract). On non-systemd hosts where the env var
  is set by hand, no Rust-side enforcement validates that. The
  per-shell `mktemp` directory inside it gets 0700 regardless; a
  different-user attacker cannot enter even if `$XDG_RUNTIME_DIR`
  happened to be mode 0755, because their lookup on the
  `mktemp`-suffixed child requires the mode bits we set.
- **Cite:** `crates/p10k-rs-shell/shells/bash/init.bash:131-134`
  ("tmpfs-cleaned at logout on systemd hosts"),
  `crates/p10k-rs-shell/shells/zsh/init.zsh:147`
  (base derivation).

### T-A3.4 — Symlink-swap on the config file
- **State:** **done.** `open_owned_mode_safely`-style open with
  `O_NOFOLLOW + fstat + owner-check + mode-mask`. A different-user
  symlink at `~/.config/p10k-rs/config.toml` fails at open
  (`ELOOP`), with the same caveat about `$HOME` mode as T-A3.2.
- **Cite:** `crates/p10k-rs-config/src/lib.rs:1029`.

### T-A3.5 — Symlink-swap on the `gitstatusd` binary
- **State:** **done.** `check_candidate` → `open_owned_safely`:
  `O_NOFOLLOW`, fstat for regular-file, owner-is-us-or-root. A
  different-user symlink in any candidate path fails the open;
  a different-user pre-plant of a real binary fails the owner
  check.
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:705-732`,
  `crates/p10k-rs-core/src/safety.rs:425-526`.

### T-A3.6 — Observe FIFO request/response bytes on `/tmp`
- **State:** **done.** FIFOs are mode 0600 in a mode-0700 parent
  dir under `mktemp`. Different-user `open(O_RDONLY)` on the
  FIFO requires `r` perm; the kernel denies it.
- **Cite:** `crates/p10k-rs-shell/shells/zsh/init.zsh:152`
  (`mkfifo -m 0600`), `:148` (`chmod 0700` on dir).

### T-A3.7 — Plant a `gitstatusd` candidate earlier in `$PATH`
- **State:** **done via owner check.** `locate_binary_checked`
  walks every candidate in `$PATH`; each must pass `check_candidate`
  (owner-is-us-or-root). A different-user planted binary fails
  the owner check; the loop falls through to the next candidate.
  If no candidate passes, the binary falls back to `ShellOut`
  (`git status`).
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:666-696`
  (loop + `first_unsafe` short-circuit), `:644-654`
  (`locate_binary` collapse → `Option`).

### T-A3.8 — Fork-leak of an open fd to a hostile binary
- **State:** **open.** `crates/p10k-rs-git/src/gitstatusd.rs`'s
  FIFO opens do not set `O_CLOEXEC`. The dump-write tempfile
  open in `crates/p10k-rs/src/main.rs:1666-1681` likewise does
  not set `O_CLOEXEC`. Today the binary does not spawn a child
  while holding these fds open, so the leak is hypothetical;
  the lint posture is the gap. Tracked in lane A of this swarm.
- **Cite:** `crates/p10k-rs-git/src/gitstatusd.rs:242-280`
  (no `O_CLOEXEC` in the custom_flags),
  `crates/p10k-rs/src/main.rs:1670-1675` (no `O_CLOEXEC`).

### T-A3.9 — Observe `/proc/<pid>/cmdline` to derive secrets
- **State:** **out of scope.** Different-user `/proc/<pid>`
  visibility is governed by `hidepid=` mount option; the prompt
  itself does not write secrets to argv. The dump path passed via
  `--dump` is a cache path under the user's `$HOME`, not a
  secret.

## Residual gaps (ranked, this attacker class)

1. **No `O_CLOEXEC` on FIFO / dump fd opens.** Today's binary
   does not `exec` while holding them, so the leak is latent.
   `O_CLOEXEC` belongs on every fd open we make as a matter of
   posture, not just where the leak is currently reachable.
   Tracked in lane A; may close in this swarm.
2. **Instant-prompt dump parent dir not owner-checked** before
   `create_dir_all`. A different-user attacker who can write to
   `$HOME/.cache` (which requires `$HOME` perms allowing the
   write — unusual but possible on hardened-server `/home` setups
   with group-write) could pre-plant the parent dir to be
   mode 0777. The dump file open still succeeds with the right
   owner + mode, but the dir's directory entries are then
   readable / writable by anyone the attacker grants. Tracked
   in lane A.
3. **`$XDG_RUNTIME_DIR` env value not validated** for mode 0700 /
   owner-is-us before use. Mitigated by per-open `O_NOFOLLOW`
   + owner-check on the actual FIFOs; not type-enforced at the
   env-read site.

## Conclusion

The different-user surface is the easiest of the five to reason
about because every cross-uid boundary in the architecture goes
through an fd-time owner check. The `mktemp`-suffixed per-shell
IPC dir defeats the predictable-path race; the `O_NOFOLLOW + fstat`
discipline on every file open defeats symlink-swap; the owner
check defeats foreign-owned-binary substitution. The two genuine
residuals (`O_CLOEXEC`, parent-dir ownership) are latent — the
binary's current spawn / open pattern does not expose them — but
should still close for posture. A real fix is in flight; the
audit records state as of HEAD `aef6a0b`, which still has the
gap.
