//! Picture galleries (pages 600 to 799 of the showcase, and the pictures channel): your examples, the sci-fi sprites, game art, characters, art, people and travel, each at Levels 1.5, 2.5 and 3.5 as sub-pages.

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

/// The picture collections' own channels (characters, art, people,
/// travel); none when there are no pictures.
#[must_use]
pub fn channels(_pictures: &[Picture]) -> Vec<crate::channels::ChannelSpec> {
    Vec::new()
}
