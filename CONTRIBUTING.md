# Contributing to p10k-rs

Thanks for the interest. The project is small and intentionally so; please
read this whole document before opening a PR.

## Ground rules

1. **Conservative dependencies.** Every line in a `Cargo.toml` is a long-term
   commitment. If a crate has fewer than three reverse-dependents on
   crates.io and you can write the equivalent in 50 lines, write it.
2. **No `unsafe` without justification.** A safety comment must explain why
   the safe alternative is unfit, what invariants the call site upholds, and
   what would have to change for the block to become unsound. Today only
   `p10k-rs-git` has the `unsafe` budget.
3. **Doc comments on every public item.** `///` on every `pub` thing, with
   one example for any non-obvious API.
4. **Typed errors.** `thiserror` on every error enum in libraries. `anyhow`
   only in the binary's `main` and binary-side glue. Libraries never panic.
5. **Boring code wins.** No clever macros, premature traits, or
   abstraction-for-its-own-sake. The next contributor reads this in six months.
6. **Tests where they matter.** Pure logic gets `#[test]`. I/O gets
   integration tests in `tests/`. Theatre tests get deleted.
7. **MSRV pinned and respected.** stable - 2 (currently 1.84). Don't reach
   for unstable features.
8. **No `tokio` in MVP.** The architecture is spawn-per-prompt synchronous.
   Async dependencies are a v0.2 conversation.

## Local checks before pushing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo doc --no-deps --workspace --locked
cargo deny check          # if you have cargo-deny installed
```

CI runs the same commands on Ubuntu and macOS.

## Commits

- One logical change per commit. The message explains the *why*, not the *what*.
- Conventional Commits is encouraged (`feat:`, `fix:`, `refactor:`, `docs:`,
  `chore:`, `ci:`, `test:`) but not enforced. Clarity over ceremony.
- Don't squash unrelated changes; rebase your PR before merge.

## Architecture decisions

Significant decisions go in `docs/adr/` as ADRs. See
[`docs/adr/README.md`](docs/adr/README.md) for the format. If your PR changes
how crates interact, the diff should land alongside an ADR — not after it.

## License

By submitting a contribution you agree to license it under the project's dual
MIT / Apache-2.0 terms. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
