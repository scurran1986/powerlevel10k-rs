# Privacy

`p10k-rs` is **local-only by design.** Nothing leaves your
machine. There is nothing to opt out of because there is nothing
collected.

## No outbound network traffic

The prompt binary and every crate in this workspace make **zero**
outbound connections. Not telemetry, not analytics, not error
reporting, not crash dumps, not auto-update checks, not anonymous
"usage" pings. The prompt path renders, writes to stdout, exits.

You can verify this:

```bash
strings $(which p10k-rs) | grep -iE 'https?://|telemetry|analytics' || echo "no network strings"
```

Or watch the binary at runtime — pick your favourite eBPF /
`strace` tool. Any outbound socket call is a bug. File it.

## The one network call

`get.sh` (the install bootstrap) does **one** network call:
`git clone` of this repository into
`~/.local/share/powerlevel10k-rs`. That's it. After that, you
own the source.

If you prefer to skip the bootstrap entirely:

```bash
git clone https://github.com/scurran1986/powerlevel10k-rs.git ~/.local/share/powerlevel10k-rs
~/.local/share/powerlevel10k-rs/install.sh
```

Same outcome. The `install.sh` itself does no network calls
(the v0.1.5 gitstatusd download is one external curl to the
upstream `romkatv/gitstatus` GitHub release — fully documented
in [SECURITY.md](SECURITY.md) under the T0.5 pinning section).

## What runs on your machine

The prompt binary reads:

- Your working directory (for the `dir` segment).
- Your git/jj working tree state (via the long-lived `gitstatusd`
  daemon over FIFOs, plus the `git` shell-out fallback). Nothing
  is sent anywhere; values feed the render path.
- A handful of environment variables (per the segment
  implementations).
- Your TOML config (`~/.config/p10k-rs/config.toml` or
  `$P10K_RS_CONFIG`).

The prompt binary writes:

- Stdout — the rendered prompt string.
- A diagnostics log at `$XDG_STATE_HOME/p10k-rs/diagnostics.log.<date>`
  (mode `0o600`, daily-rotating). Filter by setting
  `P10K_RS_LOG=warn` (or `debug` / `trace`); the default level
  records nothing routine.
- An instant-prompt cache at
  `$XDG_CACHE_HOME/p10k-rs/dump-<user>-<term>.zsh` (mode
  `0o600`). Used for sub-millisecond first-prompt rendering.
- A per-host AI runtime cache at
  `$XDG_RUNTIME_DIR/p10k-rs/ai/<host>.json` when an AI host is
  detected (claude-code, cursor, …).

That's it. All paths are user-owned, mode `0o600` where
possible, and never leave the machine.

## Data-flow boundaries

Documented in detail in [SECURITY.md](SECURITY.md) under the
threat-model section. Short version: the daemon wire is treated
as attacker-controlled, the cwd is treated as attacker-influenced,
and every byte that crosses the render path flows through
`SafeText` sanitisation. The threat model is local-trust,
not network-trust — because there is no network surface.

## If you find a privacy leak

That's a bug, and an important one. Report it via the GitHub
Security Advisories channel (see [SECURITY.md](SECURITY.md)) —
private vulnerability reporting is enabled.
