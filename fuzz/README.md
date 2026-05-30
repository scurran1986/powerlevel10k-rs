# `fuzz/` — cargo-fuzz harness for powerlevel10k-rs

Self-hosted [cargo-fuzz][cf] scaffold targeting the highest-attacker-input
surfaces of the prompt: gitstatusd wire-format sanitisation, TOML config
parsing, render-path control-byte stripping, zsh prompt wrapping, and the
Powerlevel10k zsh-config importer.

[cf]: https://rust-fuzz.github.io/book/cargo-fuzz.html

## Layout

```
fuzz/
  Cargo.toml                       # standalone crate (NOT a workspace member)
  fuzz_targets/<target>.rs         # one libFuzzer entrypoint per target
  corpus/<target>/seed-*           # checked-in seed inputs
  .gitignore                       # ignores target/, artifacts/, libFuzzer-added corpus
```

The fuzz crate is its own one-package workspace (`[workspace] members = []`
in `Cargo.toml`) so the root workspace's explicit `members = [...]` list
stays untouched.

## Toolchains

`cargo-fuzz` itself builds on **stable**:

```sh
cargo install cargo-fuzz --locked
```

Running the targets needs **nightly** for the libFuzzer compiler-rt
runtime and the sanitiser flags cargo-fuzz injects:

```sh
cargo +nightly fuzz check                              # compile-check all
cargo +nightly fuzz run <target>                       # run forever
cargo +nightly fuzz run <target> -- -max_total_time=60 # 60s smoke
```

## Targets

| target                    | what it fuzzes                                          | seed corpus                                       |
| ------------------------- | ------------------------------------------------------- | ------------------------------------------------- |
| `gitstatusd_wire`         | `SafeText::from_untrusted_bytes` (proxy — see Deferred) | gitstatusd record fixtures, branch with `\x1b`    |
| `toml_config`             | `p10k_rs_config::Config::from_toml`                     | scraped `themes/*.toml` + minimal/empty configs   |
| `sanitize_for_terminal`   | `p10k_rs_core::safety::sanitize_for_terminal`           | Trojan-Source bidi, C0/C1 controls, zero-width    |
| `wrap_for_shell_zsh`      | `SafeText::from_untrusted_{with_cap,…}` (proxy)         | ANSI SGR/OSC/CSI, `%`/`$`/backtick/`\\` strings   |
| `p9k_importer`            | `p10k_rs_config::import::import_p10k_zsh`               | real `.p10k.zsh`-style scalar/array assignments   |

## Deferred targets

Two targets currently fuzz the closest **public** surrogate of their
intended entrypoint because the underlying function is private and the
slice charter forbids adding `pub` for fuzz convenience:

- **`gitstatusd_wire`** wants `crates/p10k-rs-git/src/gitstatusd.rs`'s
  `fn parse_response(&[u8]) -> Option<GitState>`. Today it fuzzes
  `SafeText::from_untrusted_bytes`, which `parse_response` itself calls
  on every untrusted byte slice it pulls out of the record. Same
  panic-surface, narrower than the full parser. Switch this target to a
  direct `parse_response` call once the function is exposed (e.g.
  `#[doc(hidden)] pub fn parse_response` or a dedicated `pub mod wire`).

- **`wrap_for_shell_zsh`** wants `crates/p10k-rs-core/src/lib.rs`'s
  `fn wrap_for_shell(s: &str, shell: Shell) -> String`. The only public
  reachable path today is `render_prompt`, which requires a full
  `RenderCtx` + segment list — too heavy for an untrusted-byte boundary
  fuzz target. The current scaffold fuzzes the upstream
  `SafeText::from_untrusted{,_with_cap}` instead. Switch to a direct
  `wrap_for_shell(_, Shell::Zsh)` call once exposed.

Neither deferred target is silently wrong — both exercise a real
public-API panic surface that's on the path to the desired function.

## Adding a target

1. Create `fuzz_targets/<name>.rs` with a `fuzz_target!(|data: &[u8]| { … })`
   body. Keep it thin — no assertions; ANY panic crashes libFuzzer.
2. Register the new `[[bin]]` in `fuzz/Cargo.toml`.
3. Drop a few seed inputs under `corpus/<name>/seed-<short-name>`.
   Real-world fixtures beat hand-rolled bytes.
4. `cargo +nightly fuzz check` to confirm wiring.
5. Add the target to `.github/workflows/fuzz.yml` so the smoke job
   covers it.

## What to do if fuzz finds a crash

libFuzzer writes the failing input to `artifacts/<target>/crash-<sha>`.

- Treat it as a real bug, not a fuzzer false-positive — the targets
  call public APIs through the same lossy-UTF-8 boundary the prompt
  itself uses at run time.
- Reproduce locally:
  `cargo +nightly fuzz run <target> artifacts/<target>/crash-<sha>`
- Open an issue with the artifact attached (base64 it — they're
  small) and a one-line `parse_response`/`Config::from_toml`/etc.
  repro. Fix in a separate slice; do not couple a hotfix to a fuzz
  scaffold PR.

## CI

`.github/workflows/fuzz.yml` runs every target for `-max_total_time=60`
on PRs and `-max_total_time=600` on the weekly schedule. The job is
ubuntu-only because libFuzzer on macOS needs extra dance steps that
aren't worth the CI time for a smoke job.
