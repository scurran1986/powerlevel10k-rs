#![no_main]
//! Fuzz the zsh prompt-wrapping path.
//!
//! The literal `wrap_for_shell` function in
//! `crates/p10k-rs-core/src/lib.rs` (~line 837) is a private `fn`.
//! Exposing it just for the fuzz crate would violate the slice
//! charter; the public reachable entrypoint is `render_prompt`, which
//! requires a full `RenderCtx` + segment list to set up — too heavy
//! for an untrusted-byte boundary fuzz target.
//!
//! As the next-best public surrogate, we fuzz
//! `p10k_rs_core::safety::SafeText::from_untrusted_with_cap`, which
//! sits on the same render path immediately UPSTREAM of
//! `wrap_for_shell`: every untrusted string that ends up wrapped for
//! the shell has been passed through `SafeText`'s sanitiser first.
//! Bytes that would have survived to `wrap_for_shell` are exactly the
//! bytes this target's outputs would be.
//!
//! Once `wrap_for_shell` is exposed (e.g. behind a `#[doc(hidden)]`
//! `pub fn wrap_for_shell` or a dedicated `pub mod render::wrap`),
//! switch this target to call it directly with `Shell::Zsh`. See
//! `fuzz/README.md` § Deferred.

use libfuzzer_sys::fuzz_target;
use p10k_rs_core::safety::SafeText;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    // Cap matches the `SafeText::from_untrusted_with_cap` cap used by
    // render-path callers — picked to exercise the truncation branch
    // alongside the sanitiser. Both Some / None caps deserve coverage.
    let _ = SafeText::from_untrusted_with_cap(&s, 256);
    let _ = SafeText::from_untrusted(&s);
});
