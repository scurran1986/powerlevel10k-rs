#![no_main]
//! Fuzz `safety::sanitize_for_terminal` — the render-path control-byte
//! stripper that every untrusted string passes through before it can
//! reach the terminal.
//!
//! Public entrypoint at `crates/p10k-rs-core/src/safety.rs` ~line 138.
//! Input is decoded via `String::from_utf8_lossy` (`sanitize_for_terminal`
//! takes `&str`). Trojan-Source bidi overrides, C0/C1 controls, and
//! zero-width characters are all in-scope inputs the seed corpus covers.

use libfuzzer_sys::fuzz_target;
use p10k_rs_core::safety::sanitize_for_terminal;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let _ = sanitize_for_terminal(&s);
});
