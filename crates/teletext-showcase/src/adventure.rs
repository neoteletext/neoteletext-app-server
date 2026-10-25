//! Adventure games (pages 400 to 499): a typed text adventure and a Fastext adventure, each viewer's own.

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
