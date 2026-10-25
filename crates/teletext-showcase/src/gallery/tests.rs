use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::types::SubCode;
use neoteletext_formats::page::PageFile;
use teletext_runtime::{Application, Channel, ChannelConfig};

use super::levels::subcode;
use super::text::{TITLE_CHARACTERS, contents_title, credit_lines, cut, fold, wrap};
use super::*;

/// A level's TTI: one page whose rows 0, 1 and 24 name the level, as the
/// conversion's files fill rows 0 to 24.
fn tti(level: &str) -> Vec<u8> {
    format!(
        "DE,Picture\r\nPN,10000\r\nSC,0000\r\nPS,8000\r\nFL,100,100,100,100,100,100\r\nOL,0,{level} top\r\nOL,1,{level} picture\r\nOL,24,{level} bottom\r\n"
    )
    .into_bytes()
}

fn picture(name: &str, group: char, caption: &str) -> Picture {
    let levels = ["l15", "l25", "l35"].iter().map(|level| (level.to_string(), tti(level))).collect();
    Picture { name: name.to_string(), caption: caption.to_string(), group, levels }
}

/// A long Wikimedia caption, as `captions.tsv` holds some.
const LONG: &str = "Kiril Lazarov 06.jpg \u{B7} English: Kuebi = Armin K\u{FC}belbeck. In case of questions regarding usage of my work, please see here \u{2014} after you have read the license below.Fran\u{E7}ais: Kuebi = Armin K\u{FC}belbeck. Pour toute question concernant l'usage de mes travaux, s.v.p voyez vous r\u{E9}f\u{E9}rer ici \u{2014} apr\u{E8}s avoir lu la licence pr\u{E9}c\u{E9}demment cit\u{E9}e. \u{B7} CC BY-SA 3.0";

/// The pictures of the real folder's shape: 10 examples, 20 sprites, 221
/// characters and 133 art pictures with long captions.
fn folder() -> Vec<Picture> {
    let mut pictures = Vec::new();
    for number in 1..=10 {
        pictures.push(picture(&format!("e{number:02}"), 'e', &format!("example {number:02}")));
    }
    for number in 1..=20 {
        pictures.push(picture(&format!("s{number:02}"), 's', &format!("{number:02}_razor_crawler (sci_fi_enemy_sprites_individual)")));
    }
    for number in 1..=221 {
        pictures.push(picture(&format!("c{number:03}"), 'c', &format!("source-{number:02}-character-1 (transparent background/all-characters)")));
    }
    for number in 1..=133 {
        pictures.push(picture(&format!("a{number:03}"), 'a', LONG));
    }
    pictures
}

fn by_number(gallery: &Gallery) -> BTreeMap<u16, Vec<PageFile>> {
    gallery.published().into_iter().collect()
}

/// A row's text, its attributes shown as the spaces they take.
fn row(page: &PageFile, row: usize) -> String {
    page.rows[row].iter().map(|&code| if code < 0x20 { ' ' } else { char::from(code) }).collect()
}

/// The pages' text as one line, each run of spaces one space, so text
/// wrapped over rows reads on.
fn text_of(pages: &[PageFile]) -> String {
    let rows: Vec<String> = pages.iter().flat_map(|page| (0..25).map(|number| row(page, number))).collect();
    rows.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn page_numbers_are_decimal_digits_as_the_runtime_numbers_them() {
    assert_eq!(page_number(100), 0x100);
    assert_eq!(page_number(601), 0x601);
    assert_eq!(page_number(630), 0x630);
    assert_eq!(page_number(899), 0x899);
    assert_eq!(subcode(1), SubCode::new(0x0001));
    assert_eq!(subcode(40), SubCode::new(0x0040));
}

#[test]
fn the_showcase_numbers_examples_from_601_and_sprites_from_611() {
    let pages = by_number(&showcase(&folder()));
    let numbers: Vec<u16> = pages.keys().copied().collect();
    let mut expected = alloc::vec![0x600];
    expected.extend((601..=630).map(page_number));
    expected.extend([0x690, 0x695]);
    assert_eq!(numbers, expected);
}

#[test]
fn each_picture_is_one_page_of_its_three_levels_in_order() {
    let pages = by_number(&showcase(&folder()));
    for number in (601..=630).map(page_number) {
        let subpages = &pages[&number];
        assert_eq!(subpages.len(), 3, "page {number:X}");
        for (subpage, level) in subpages.iter().zip(["l15", "l25", "l35"]) {
            assert_eq!(subpage.address.mpp(), number);
            assert!(row(subpage, 1).starts_with(&format!("{level} picture")), "page {number:X}: {}", row(subpage, 1));
            assert!(row(subpage, 24).starts_with(&format!("{level} bottom")), "the picture keeps row 24");
        }
        let numbering: Vec<(u8, SubCode)> = subpages.iter().map(|page| (page.subpage, page.subcode)).collect();
        assert_eq!(numbering, [(1, subcode(1)), (2, subcode(2)), (3, subcode(3))]);
    }
}

#[test]
fn a_pictures_fastext_is_previous_next_menu_credit_and_index() {
    let gallery = showcase(&folder());
    let pages = by_number(&gallery);
    let links = |number: u16| pages[&number][0].fastext.map(|link| link.map(|link| (link.address.mpp(), link.subcode)));
    let any = SubCode::ANY;
    // The first wraps to the last; every level carries the same links.
    assert_eq!(links(0x601), [Some((0x630, any)), Some((0x602, any)), Some((0x600, any)), Some((0x695, subcode(1))), Some((0x100, any)), None]);
    assert_eq!(links(0x630)[1], Some((0x601, any)));
    for subpage in &pages[&0x615] {
        assert_eq!(subpage.fastext[0].map(|link| link.address.mpp()), Some(0x614));
    }
}

#[test]
fn every_credit_names_its_picture_on_the_sub_page_its_link_names() {
    let pictures = folder();
    let art = collection(&pictures, COLLECTIONS[1]).unwrap();
    let pages = by_number(&art);
    for number in (101..=233).map(page_number) {
        let credit = pages[&number][0].fastext[3].unwrap();
        let credits = &pages[&credit.address.mpp()];
        let shown = credits.iter().find(|page| page.subcode == credit.subcode).unwrap();
        let text = text_of(core::slice::from_ref(shown));
        assert!(text.contains(&format!("{number:03X} Kiril Lazarov 06, by English: Kuebi = Armin Kubelbeck")), "page {number:X}:\n{text}");
        assert!(text.contains("CC BY-SA 3.0"), "the licence is kept whole:\n{text}");
    }
}

#[test]
fn the_credits_hold_every_caption() {
    let pictures = folder();
    let pages = by_number(&showcase(&pictures));
    let credits = text_of(&pages[&0x695]);
    for number in 1..=10 {
        assert!(credits.contains(&format!("example {number:02}")));
    }
    assert!(credits.contains("01 razor crawler (sci fi enemy sprites individual)"), "{credits}");
}

#[test]
fn every_channel_stays_within_the_limits() {
    let pictures = folder();
    let mut galleries = alloc::vec![showcase(&pictures)];
    galleries.extend(COLLECTIONS.iter().filter_map(|&spec| collection(&pictures, spec)));
    assert_eq!(galleries.len(), 3, "the showcase, characters and art");
    for gallery in &galleries {
        let entries = gallery.pages();
        assert!(entries.len() <= 800, "at most 800 pages a channel");
        for entry in &entries {
            let digits = format!("{:03X}", entry.number);
            assert!(digits.bytes().all(|digit| digit.is_ascii_digit()), "contents names decimal pages: {digits}");
            assert!((0x100..=0x899).contains(&entry.number));
            assert!(!entry.title.is_empty() && entry.title.chars().count() <= TITLE_CHARACTERS, "{:?}", entry.title);
            assert!(entry.title.chars().all(|character| (' '..='~').contains(&character)), "{:?}", entry.title);
        }
        let published = gallery.published();
        let numbers: BTreeSet<u16> = published.iter().map(|(number, _)| *number).collect();
        let listed: BTreeSet<u16> = entries.iter().map(|entry| entry.number).collect();
        assert_eq!(numbers.len(), published.len(), "each page published once");
        assert_eq!(numbers, listed, "every page published is in the contents");
        assert!(published.iter().all(|(_, subpages)| subpages.len() <= 40), "at most 40 sub-pages a page");
        let stored: usize = published.iter().map(|(_, subpages)| subpages.len()).sum();
        assert!(stored <= 1024, "{stored} pages and sub-pages");
    }
}

#[test]
fn pictures_beyond_the_plan_are_left_out() {
    let pictures: Vec<Picture> = (1..=120).map(|number| picture(&format!("e{number}"), 'e', "example")).collect();
    let gallery = showcase(&pictures);
    let numbers: Vec<u16> = gallery.pages().iter().map(|entry| entry.number).collect();
    assert_eq!(numbers.iter().filter(|&&number| (0x601..=0x689).contains(&number)).count(), 89);
    assert!(!numbers.contains(&0x68A) && numbers.contains(&0x690) && numbers.contains(&0x695));
}

#[test]
fn a_picture_without_levels_is_left_out_and_one_that_does_not_read_says_so() {
    let mut pictures = alloc::vec![picture("e01", 'e', "first"), picture("e02", 'e', "second"), picture("e03", 'e', "third")];
    pictures[1].levels.clear();
    pictures[2].levels = alloc::vec![("l15".to_string(), b"no page here".to_vec())];
    let pages = by_number(&showcase(&pictures));
    assert!(pages.contains_key(&0x601) && pages.contains_key(&0x602) && !pages.contains_key(&0x603));
    assert!(text_of(&pages[&0x602]).contains("could not be read"));
}

#[test]
fn with_no_pictures_the_menu_says_why_and_there_are_no_picture_pages() {
    let gallery = showcase(&[]);
    let pages = by_number(&gallery);
    assert_eq!(pages.keys().copied().collect::<Vec<_>>(), [0x600, 0x690]);
    assert!(text_of(&pages[&0x600]).contains("No pictures are installed here"));
    assert!(channels(&[]).is_empty());
}

#[test]
fn the_collections_have_channels_of_their_own_with_an_index_at_100() {
    let pictures = folder();
    let specs = channels(&pictures);
    let folders: Vec<&str> = specs.iter().map(|spec| spec.folder).collect();
    assert_eq!(folders, ["characters", "art"]);
    let characters = collection(&pictures, COLLECTIONS[0]).unwrap();
    let pages = by_number(&characters);
    let index = text_of(&pages[&0x100]);
    assert!(index.contains("221 pictures, on pages 101 to 321."), "{index}");
    assert_eq!(pages[&0x100][0].fastext[0].map(|link| link.address.mpp()), Some(0x101));
    assert!(pages.contains_key(&0x321) && !pages.contains_key(&0x322) && pages.contains_key(&0x800));
    let menu = text_of(&by_number(&showcase(&pictures))[&0x600]);
    assert!(menu.contains("Characters") && menu.contains("221 pictures") && menu.contains("characters"), "{menu}");
}

#[test]
fn a_channel_publishes_the_pages_once_at_start() {
    let pictures = folder();
    let spec = channels(&pictures).into_iter().find(|spec| spec.folder == "characters").unwrap();
    let config =
        ChannelConfig { name: spec.name.to_string(), push_address: "ws://127.0.0.1:1/characters/push".to_string(), initial_page: 0x100, epoch_at_start_ms: 0 };
    let mut channel = Channel::new(config, spec.applications, 1);
    channel.start(0);
    let shared: Vec<u16> = channel.shared_frames().map(|(number, _)| number).collect();
    assert_eq!(shared.len(), 1 + 221 + 1);
    assert!(channel.shared_frames().all(|(_, frame)| !frame.is_empty() && frame.len() % 42 == 0));
    let contents = String::from_utf8(channel.resource("contents.txt").unwrap().body).unwrap();
    assert!(contents.contains("p\t101\tsource-01-character-1\n"), "{contents}");
    assert!(contents.contains("p\t800\tPicture credits\n"));
}

#[test]
fn captions_fold_to_what_the_english_set_shows() {
    assert_eq!(fold("Armin K\u{FC}belbeck \u{B7} CC"), "Armin Kubelbeck - CC");
    assert_eq!(fold("\u{41C}\u{430}\u{43A}\u{441}\u{430}\u{442}79"), "Maksat79");
    assert_eq!(fold("\u{41C}\u{438}\u{445}\u{430}\u{439}\u{43B}\u{44E}\u{43A}"), "Mikhaylyuk");
    assert_eq!(fold("sci_fi [a] {b} a|b ~"), "sci fi (a) (b) a/b -");
    assert_eq!(fold("\u{5D0} (Aleph)"), "? (Aleph)");
    assert_eq!(fold("\u{9AD}\u{9B0}\u{9A4} x  \t y "), "? x y");
}

#[test]
fn titles_fit_the_contents_and_keep_the_author() {
    assert_eq!(contents_title("Prairie dog Theodore Roosevelt NP ND1.jpg \u{B7} Acroterion \u{B7} CC BY-SA 4.0"), "Prairie dog Theodo.., Acroterion");
    assert_eq!(contents_title("01_razor_crawler (sci_fi_enemy_sprites_individual)"), "01 razor crawler");
    assert_eq!(contents_title("Kenney tiny-dungeon (CC0), sheet crop of tilemap.png at 1:1"), "Kenney tiny-dungeon (CC0), she..");
    assert!(contents_title(LONG).chars().count() <= TITLE_CHARACTERS);
}

#[test]
fn long_captions_are_shortened_but_keep_their_licence() {
    let lines = credit_lines(LONG, 35, 6);
    assert!(lines.len() <= 6 && lines.iter().all(|line| line.chars().count() <= 35), "{lines:?}");
    assert!(lines.join(" ").ends_with("CC BY-SA 3.0"), "{lines:?}");
    assert_eq!(credit_lines("example 01", 35, 6), ["example 01"]);
    assert_eq!(
        credit_lines("Monitor lizard swallows Uromastyx in Kuwait.jpg \u{B7} Mfalhajji \u{B7} CC BY-SA 4.0", 35, 6).last().unwrap(),
        "CC BY-SA 4.0",
        "a licence is never broken"
    );
    assert_eq!(wrap("a bb ccc", 4), ["a bb", "ccc"]);
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(cut("abcdef", 4), "ab..");
}

#[test]
fn the_index_entry_names_the_gallery() {
    assert_eq!(sections().first().map(|section| section.page), Some(MENU));
}
