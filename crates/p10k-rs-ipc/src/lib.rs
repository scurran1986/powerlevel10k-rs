//! Daemon IPC protocol — placeholder for the v0.2 daemon mode.
//!
//! See `ARCHITECTURE.md` § 2.7 for the wire format sketch (length-prefixed
//! `postcard` or CBOR over an abstract Unix domain socket, session-keyed,
//! version-tagged with deny-on-unknown).
//!
//! This crate is **intentionally empty** in v0.1. It exists so v0.2 can land
//! a real protocol without a workspace reshuffle. Don't depend on anything
//! here from `-core` / `-segments`; the spawn-per-prompt MVP must compile
//! without it.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Schema version of the IPC wire protocol.
///
/// Bumped any time a request or response type changes shape. Daemons reject
/// requests with an unrecognised version with a typed error rather than
/// silently coercing.
pub const PROTOCOL_VERSION: u32 = 0;
