#![no_main]
//! Fuzz `Config::from_toml` — the TOML config loader.
//!
//! Public entrypoint at `crates/p10k-rs-config/src/lib.rs` ~line 887.
//! We interpret libFuzzer's `&[u8]` as UTF-8 via `String::from_utf8_lossy`
//! because `from_toml` takes `&str`. Goal: prove the parser does not
//! panic on arbitrary input — malformed TOML must return `Err`, never
//! crash. A panic here is a denial-of-service surface (an attacker who
//! can plant a `~/.config/p10k-rs/config.toml` would break every prompt).

use libfuzzer_sys::fuzz_target;
use p10k_rs_config::Config;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    // Ignore the `Result` deliberately: success and failure are both
    // valid outcomes. The only invalid outcome is a panic.
    let _ = Config::from_toml(&s);
});
