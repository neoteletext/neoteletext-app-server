//! Picture galleries: on the showcase channel (pages 600 to 699) your
//! examples and the sci-fi sprites, a page that compares the levels and
//! their credits; and a channel of its own for each larger collection
//! (characters, art, people, travel, game art). Every picture is one page
//! whose sub-pages are its Levels 1.5, 2.5 and 3.5, and every caption,
//! with its attribution, is on the gallery's credits pages (decision 6).
//! The pages are shared and static: they are published once, at start.

use alloc::boxed::Box;
use alloc::vec::Vec;

use teletext_runtime::Application;

use crate::channels::{ChannelSpec, Picture};
use crate::index::Section;

mod application;
mod levels;
mod text;

#[cfg(test)]
mod tests;

pub use application::{Gallery, Plan, page_number};

/// A group of pictures, as the levels page names it.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    /// Its letter in `captions.tsv`.
    pub letter: char,
    /// Its name.
    pub name: &'static str,
    /// Its name as a Fastext label: at most 9 characters.
    pub short: &'static str,
}

/// The groups the showcase channel shows itself, in page order.
pub const SHOWCASE_GROUPS: [Group; 2] =
    [Group { letter: 'e', name: "Your examples", short: "Examples" }, Group { letter: 's', name: "Sci-fi sprites", short: "Sprites" }];

/// A collection served as a channel of its own: its folder, the
/// channel's name and its group.
#[derive(Clone, Copy, Debug)]
pub struct Collection {
    /// The folder the channel is served at.
    pub folder: &'static str,
    /// The channel's name.
    pub channel: &'static str,
    /// Its pictures.
    pub group: Group,
}

/// The collections with channels of their own: each is kept to a few
/// hundred pages and sub-pages, so that it opens quickly in a browser
/// whose page store starts at 256.
pub const COLLECTIONS: [Collection; 5] = [
    Collection { folder: "characters", channel: "NeoTeletext characters", group: Group { letter: 'c', name: "Characters", short: "First" } },
    Collection { folder: "art", channel: "NeoTeletext art", group: Group { letter: 'a', name: "Art", short: "First" } },
    Collection { folder: "people", channel: "NeoTeletext people", group: Group { letter: 'p', name: "People", short: "First" } },
    Collection { folder: "travel", channel: "NeoTeletext travel", group: Group { letter: 't', name: "Travel", short: "First" } },
    Collection { folder: "gameart", channel: "NeoTeletext game art", group: Group { letter: 'g', name: "Game art", short: "First" } },
];

/// The showcase gallery's menu page.
pub const MENU: u16 = 0x600;

/// The pictures of `group`.
fn of_group(pictures: &[Picture], group: Group) -> Vec<&Picture> {
    pictures.iter().filter(|picture| picture.group == group.letter).collect()
}

/// The showcase's own gallery, of [`SHOWCASE_GROUPS`], naming the
/// collections that have channels of their own.
#[must_use]
pub fn showcase(pictures: &[Picture]) -> Gallery {
    let shown: Vec<(Group, Vec<&Picture>)> = SHOWCASE_GROUPS.iter().map(|&group| (group, of_group(pictures, group))).collect();
    let elsewhere: Vec<(Collection, usize)> =
        COLLECTIONS.iter().map(|&collection| (collection, of_group(pictures, collection.group).len())).filter(|&(_, count)| count > 0).collect();
    Gallery::new("Picture galleries", Plan::SHOWCASE, &shown, elsewhere)
}

/// A collection's gallery, with its index at page 100, or none when it
/// has no pictures.
#[must_use]
pub fn collection(pictures: &[Picture], collection: Collection) -> Option<Gallery> {
    let members = of_group(pictures, collection.group);
    (!members.is_empty()).then(|| Gallery::new(collection.group.name, Plan::COLLECTION, &[(collection.group, members)], Vec::new()))
}

/// The showcase's gallery: one application for pages 600 to 699.
#[must_use]
pub fn applications(pictures: &[Picture]) -> Vec<Box<dyn Application>> {
    alloc::vec![Box::new(showcase(pictures))]
}

/// This area's entry in the showcase's index.
#[must_use]
pub fn sections() -> Vec<Section> {
    alloc::vec![Section { page: MENU, name: "Picture galleries", about: "Levels 1.5, 2.5 and 3.5 compared" }]
}

/// The collections' own channels, each with its index at page 100: one
/// for every collection with pictures, so none when there are none.
#[must_use]
pub fn channels(pictures: &[Picture]) -> Vec<ChannelSpec> {
    COLLECTIONS
        .iter()
        .filter_map(|&spec| {
            let gallery = collection(pictures, spec)?;
            let applications: Vec<Box<dyn Application>> = alloc::vec![Box::new(gallery)];
            Some(ChannelSpec { folder: spec.folder, name: spec.channel, applications })
        })
        .collect()
}
