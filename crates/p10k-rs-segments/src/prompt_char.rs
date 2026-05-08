//! `prompt_char` — the trailing chevron the user types after.
//!
//! Slice 1: a fixed `❯`. Last-status coloring (red on non-zero) and vi-mode
//! variants land in later slices.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Trailing prompt character.
#[derive(Debug, Default)]
pub struct PromptChar;

impl Segment for PromptChar {
    fn name(&self) -> &'static str {
        "prompt_char"
    }

    fn render(&self, _ctx: &RenderCtx<'_>) -> SegmentOutput {
        SegmentOutput {
            text: "❯".to_owned(),
            plain_len: 1,
            state: None,
            icon: None,
        }
    }
}
