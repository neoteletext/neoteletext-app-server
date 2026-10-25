//! The showcase's channels, each a folder of the server.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::Application;

use crate::index::Index;
use crate::{adventure, audio, gallery, games, interactive, services};

/// A converted picture, from the host: its name (such as `e01`), its
/// caption with the attribution, and its pages' TTI at each level.
#[derive(Clone, Debug)]
pub struct Picture {
    /// The picture's name.
    pub name: String,
    /// Its caption: title, author, licence.
    pub caption: String,
    /// Its group: `e` your examples, `s` sci-fi sprites, `c` characters,
    /// `a` art, `p` people, `t` travel, `g` game art.
    pub group: char,
    /// TTI at Levels 1.5, 2.5 and 3.5, where converted.
    pub levels: Vec<(String, Vec<u8>)>,
}

/// A channel: its folder, its name and its applications.
pub struct ChannelSpec {
    /// The folder it is served at, such as `showcase`.
    pub folder: &'static str,
    /// Its name.
    pub name: &'static str,
    /// Its applications.
    pub applications: Vec<Box<dyn Application>>,
}

/// The showcase's channels, with `pictures` for the galleries: the
/// showcase itself, and a channel of the larger picture collections.
#[must_use]
pub fn channels(pictures: &[Picture]) -> Vec<ChannelSpec> {
    let mut sections = Vec::new();
    for area in [services::sections(), interactive::sections(), games::sections(), adventure::sections(), audio::sections(), gallery::sections()] {
        sections.extend(area);
    }
    let mut applications: Vec<Box<dyn Application>> = alloc::vec![Box::new(Index::new(sections))];
    for area in [
        services::applications(pictures),
        interactive::applications(pictures),
        games::applications(pictures),
        adventure::applications(pictures),
        audio::applications(pictures),
        gallery::applications(pictures),
    ] {
        applications.extend(area);
    }
    let mut channels = alloc::vec![ChannelSpec { folder: "showcase", name: "NeoTeletext showcase", applications }];
    channels.extend(gallery::channels(pictures));
    channels
}
