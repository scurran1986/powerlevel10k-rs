# Segments

The runtime-authoritative list lives in
[`crates/p10k-rs-segments/src/lib.rs::segment_names()`][src]. Every
entry below resolves through `build()` in the same file — advertisement
matches reality, enforced by a unit test.

Each segment is a `pub struct` implementing the `Segment` trait from
`p10k-rs-core`. Per-segment styling routes through `p10k_rs_core::style`
so `[segment.<name>].foreground` / `.background` / `.states.<tag>`
overrides all reach the rendered prompt.

## Always-on (MVP-SPEC § 1.2)

| Name | Description |
|---|---|
| `dir` | Current working directory. Cwd painted black-on-blue, `$HOME` collapsed to `~`. |
| `prompt_char` | Trailing chevron. Green `❯` on success, red `❯` on failure. |
| `status` | Last command exit code, hidden on success. Shows `✘<code>` red on non-zero `$?`. |
| `command_execution_time` | Duration of the last foreground command. Black-on-yellow when past the 3-second threshold. |
| `background_jobs` | Count of suspended/running background jobs. Hidden when the shell has no jobs. |
| `time` | Current wall-clock time in `HH:MM:SS`, white. |
| `context` | `user@host` with privilege/SSH awareness. Gated by identity render rules. |
| `vi_mode` | Vi keymap indicator: `INSERT` / `NORMAL` / `VISUAL` / `OPER`. _TODO: confirm wire-up in non-zsh shells._ |
| `root_indicator` | Single red lightning glyph when EUID is 0. |
| `vcs` | Branch name black-on-green with a trailing dirty marker. Powered by `gitstatusd` on the hot path. |

## Auto-detected

| Name | Description |
|---|---|
| `virtualenv` | `(<basename>)` yellow when `$VIRTUAL_ENV` is set. |
| `anaconda` | `conda:<name>` green when `$CONDA_DEFAULT_ENV` is set. |
| `pyenv` | `py:<version>` yellow when `$PYENV_VERSION` is set. |
| `nodenv` | `node:<version>` green when `$NODENV_VERSION` is set. |
| `kubecontext` | `k8s:<context>` cyan when a kubeconfig file is readable. |
| `terraform` | `tf:<workspace>` magenta when a Terraform workspace can be resolved. |
| `aws` | `aws:<profile>` yellow when standard AWS env vars are set. |
| `os_icon` | Always-on glyph identifying the host OS. |

## Useful enough to bundle

| Name | Description |
|---|---|
| `node_version` | `node:<version>` (strips leading `v`) when the cwd contains a Node project. |
| `python_version` | `py:<version>` yellow when the cwd resolves a Python interpreter. |
| `rust_version` | `rust:<version>` red when the cwd sits inside a Rust workspace. |

## AI host (slice 46)

| Name | Description |
|---|---|
| `ai_host` | Surfaces the active AI host (Claude Code, Aider, Cursor, …). Visible only inside a detected AI shell. |

## Modern version managers + container context (slices 48–50)

| Name | Description |
|---|---|
| `mise` | Cross-language version manager (formerly `rtx`). `rtx` is accepted as a deprecated alias. |
| `fnm` | `fnm:<version>` green when `$FNM_NODE_VERSION` is set. |
| `pixi` | `pixi:<name>` green when `$PIXI_PROJECT_NAME` is set. |
| `docker_context` | `docker:<context>` cyan when a non-default Docker context is active. |

[src]: https://github.com/scurran1986/powerlevel10k-rs/blob/main/crates/p10k-rs-segments/src/lib.rs
