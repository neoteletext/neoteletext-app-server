//! The showcase's pictures from a folder on the host (decision 6): the
//! engine's levels conversion, `NAME_l15.tti`, `NAME_l25.tti` and
//! `NAME_l35.tti`, with `captions.tsv` giving each name's group and
//! caption.

use std::path::Path;

use teletext_showcase::Picture;

/// The levels each picture is shown at, as sub-pages.
const LEVELS: [&str; 3] = ["l15", "l25", "l35"];

/// Reads the pictures in `folder`; a picture without its files is left
/// out.
#[must_use]
pub fn load(folder: &Path) -> Vec<Picture> {
    let Ok(captions) = std::fs::read_to_string(folder.join("captions.tsv")) else { return Vec::new() };
    let mut pictures = Vec::new();
    for line in captions.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(name), Some(group), Some(caption)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let levels: Vec<(String, Vec<u8>)> = LEVELS
            .iter()
            .filter_map(|level| std::fs::read(folder.join(format!("{name}_{level}.tti"))).ok().map(|bytes| ((*level).to_owned(), bytes)))
            .collect();
        if levels.is_empty() {
            continue;
        }
        pictures.push(Picture { name: name.to_owned(), caption: caption.to_owned(), group: group.chars().next().unwrap_or('a'), levels });
    }
    pictures
}
