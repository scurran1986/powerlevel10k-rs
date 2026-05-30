#![no_main]
//! Fuzz `import::import_p10k_zsh` — the Powerlevel10k zsh-config importer.
//!
//! Public entrypoint at `crates/p10k-rs-config/src/import.rs` ~line 84.
//! The importer parses zsh as text (it does NOT execute it — see the
//! module docstring). Input is decoded via `String::from_utf8_lossy`.
//! Goal: prove any garbage `~/.p10k.zsh` lands as a warnings-only
//! `ImportOutcome` rather than panicking.

use libfuzzer_sys::fuzz_target;
use p10k_rs_config::import::import_p10k_zsh;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let _ = import_p10k_zsh(&s);
});
