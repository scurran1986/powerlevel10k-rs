//! `prompt_char` — the trailing chevron the user types after.
//!
//! Slice 2: green `❯`. Last-status coloring (red on non-zero `$?`) and
//! vi-mode variants land in later slices. ANSI escapes raw; renderer
//! post-processes for the target shell.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Trailing prompt character.
#[derive(Debug, Default)]
pub struct PromptChar;

impl Segment for PromptChar {
    fn name(&self) -> &'static str {
        "prompt_char"
    }

    fn render(&self, _ctx: &RenderCtx<'_>) -> SegmentOutput {
        // 32 = ANSI green. Red-on-error in slice 3 once `--last-status` lands.
        SegmentOutput {
            text: "\x1b[32m❯\x1b[39m".to_owned(),
            plain_len: 1,
            state: None,
            icon: None,
        }
    }
}
