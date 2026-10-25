//! Action games (pages 300 to 399): invaders with the sci-fi sprites, snake, breakout and more, each viewer's game its own.

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::Application;

use crate::channels::Picture;
use crate::index::Section;

/// This area's applications.
#[must_use]
pub fn applications(_pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    Vec::new()
}

/// This area's entries in the index.
#[must_use]
pub fn sections() -> Vec<Section> {
    Vec::new()
}
