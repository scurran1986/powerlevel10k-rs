//! Color and styling primitives.
//!
//! Single source of truth for prompt color emission, per `ARCHITECTURE.md`
//! § 3.1. All styling flows through [`anstyle`] codes; segments must not
//! concatenate ANSI escape strings by hand.
//!
//! The full color model — 8 / 256 / truecolor lowering, the Powerlevel9k
//! named-color compat table, and the per-state override pipeline — lands
//! with the segment-buildout phase. This module is the placeholder seam.

/// Color emission mode chosen at config-load time.
///
/// Determined by [`Config`](crate::Config); each [`Segment`](crate::Segment)
/// emits using whichever variant the renderer hands down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 8-color ANSI, the lowest common denominator. Forces dimming/underline
    /// to encode bright variants on terminals that don't support `bold`.
    Ansi8,
    /// 256-color ANSI, indexed.
    Ansi256,
    /// 24-bit truecolor.
    TrueColor,
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::Ansi256
    }
}
