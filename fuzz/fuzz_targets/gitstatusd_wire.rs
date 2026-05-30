#![no_main]
//! Fuzz the gitstatusd wire-format sanitiser path.
//!
//! The literal `parse_response` function in
//! `crates/p10k-rs-git/src/gitstatusd.rs` (~line 359) is a private
//! `fn` — the scaffold does not expose it because (a) the slice
//! charter forbids adding `pub` for fuzz convenience and (b) the
//! function takes `&[u8]` already so its untrusted-byte boundary is
//! its own concern, not the public API's.
//!
//! As the next-best public surrogate, we fuzz
//! `p10k_rs_core::safety::SafeText::from_untrusted_bytes`, which is
//! *exactly* the function `parse_response` itself calls (via the
//! `untrusted_field` closure) on every byte slice it pulls out of the
//! gitstatusd record. Any panic that `parse_response` could trigger
//! through that downstream sanitiser is reachable here.
//!
//! Once `parse_response` is exposed (e.g. behind a `#[doc(hidden)]`
//! `pub fn` or a dedicated `pub mod wire`), this target should be
//! switched to call it directly. See `fuzz/README.md` § Deferred.

use libfuzzer_sys::fuzz_target;
use p10k_rs_core::safety::SafeText;

fuzz_target!(|data: &[u8]| {
    let _ = SafeText::from_untrusted_bytes(data);
});
