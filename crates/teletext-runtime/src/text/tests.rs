//! A page's text encoded by the server with the engine's encoder (REQ-2214,
//! REQ-2215, REQ-2216): read back through the engine's decoder to its
//! Unicode text, its triplets and designation as the engine gives them,
//! and every refusal named by path, rule and request.

extern crate std;

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::charset::{compose, to_unicode};
use neoteletext_core::consts::{COLUMNS, LOCAL_TRIPLETS, NO_SECOND_G0, ROWS, SPACE, STATUS_COLUMNS};
use neoteletext_core::level1::Glyph;
use neoteletext_core::packet::Triplet;
use neoteletext_formats::write_text::{Level, PageText, TextRun, write_runs};
use neoteletext_image::raster::grid_of_packets;
use serde_json::{Value, json};

use crate::channel::{Channel, ChannelConfig, Effect, InitialPage};
use crate::protocol::tests::{base64, line, with};
use crate::protocol::{Page, ProgramMessage, read};

/// A page with blank rows, no triplets and no packets, in `language`.
fn page_value(language: &str, text: &Value) -> Value {
    let rows: Vec<Value> = alloc::vec![Value::Null; ROWS];
    json!({
        "page": "100",
        "sub": "0000",
        "rows": rows,
        "links": [null, null, null, null, null, null],
        "display_row_24": false,
        "link_extension": {"keys": [], "numbers": [], "texts": []},
        "control": {
            "erase": false, "newsflash": false, "subtitle": false, "suppress_header": false,
            "update": false, "interrupted_sequence": false, "inhibit_display": false, "magazine_serial": false
        },
        "language": language,
        "enhancements": [],
        "packets": [],
        "cursor": null,
        "text": text
    })
}

fn page_message(page: &Value) -> Value {
    json!({"type": "page", "connections": "all", "page": page})
}

/// The page `value` as the server reads and encodes it.
fn encoded(value: &Value) -> Page {
    match read(&line(&page_message(value))).unwrap_or_else(|refusal| panic!("refused: {refusal}")) {
        ProgramMessage::Page(message) => message.page,
        other => panic!("{other:?}"),
    }
}

/// The page's rows as the engine's decoder shows them, trailing spaces
/// removed.
fn shown(page: &Page) -> Vec<String> {
    let packets = crate::page::encode(page).packets;
    let grid = grid_of_packets(&packets, crate::page::address(page.page.0), [b' '; STATUS_COLUMNS]).unwrap();
    grid.cells
        .iter()
        .map(|cells| {
            let mut line = String::new();
            for cell in cells {
                let Glyph::Char(character) = cell.glyph else {
                    line.push(' ');
                    continue;
                };
                let unicode = to_unicode(character);
                match unicode.combining.and_then(|mark| compose(unicode.base, mark)) {
                    Some(one) => line.push(one),
                    None => line.push(unicode.base),
                }
            }
            String::from(line.trim_end())
        })
        .collect()
}

/// The page's triplets, in order of designation code.
fn triplets(page: &Page) -> Vec<Triplet> {
    page.enhancements
        .iter()
        .flat_map(|packet| packet.triplets.iter().map(|triplet| Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data }))
        .collect()
}

/// What the engine's encoder gives for `runs` on a blank English page.
fn engine(level: Level, runs: &[TextRun<'_>]) -> ([[u8; COLUMNS]; ROWS], Vec<Triplet>) {
    let mut grid = [[SPACE; COLUMNS]; ROWS];
    let text = PageText { language: Some("en"), designation: 0, level, runs };
    let written = write_runs(&mut grid, &text, &[]).unwrap();
    (grid, written.triplets)
}

/// The rows of `page` as codes.
fn codes(page: &Page) -> [[u8; COLUMNS]; ROWS] {
    let mut grid = [[SPACE; COLUMNS]; ROWS];
    for (slot, row) in grid.iter_mut().zip(&page.rows) {
        if let Some(row) = row {
            let mut bytes = [0; COLUMNS];
            assert_eq!(neoteletext_web::base64::decode(row.as_bytes(), &mut bytes).unwrap(), COLUMNS);
            *slot = bytes;
        }
    }
    grid
}

/// REQ-2214 verification: a Russian run on an English page and a Greek run
/// beside it encode to the engine's codes and row set triplets, and read
/// back to their text.
#[test]
fn russian_and_greek_runs_on_an_english_page_are_the_engines_encoding() {
    let text = json!([{"row": 2, "column": 0, "text": "Привет мир", "language": "ru"}, {"row": 2, "column": 12, "text": "Γειά σου", "language": "el"}]);
    let page = encoded(&page_value("en", &text));
    let runs =
        [TextRun { row: 2, column: 0, text: "Привет мир", language: Some("ru") }, TextRun { row: 2, column: 12, text: "Γειά σου", language: Some("el") }];
    let (grid, expected) = engine(Level::NeoTeletext, &runs);
    assert_eq!(codes(&page), grid);
    assert_eq!(triplets(&page)[..expected.len()], expected[..]);
    assert!(page.language.is_none() && page.text.is_none() && page.level.is_none());
    assert_eq!(page.character_set.map(|set| (set.region, set.national_option)), Some((Some(0), 0)));
    assert_eq!(shown(&page)[2], "Привет мир  Γειά σου");
}

/// REQ-2214 item 3: at Level 3.5 another set is a designation and a triplet
/// a character, over spaces.
#[test]
fn at_level_3_5_a_greek_run_is_written_by_triplets() {
    let text = json!([{"row": 4, "column": 3, "text": "Αθήνα", "language": "el"}]);
    let page = encoded(&with(&page_value("en", &text), "/level", json!("3.5")));
    assert_eq!(codes(&page)[4][3..8], [SPACE; 5]);
    assert_eq!(triplets(&page).iter().take_while(|triplet| triplet.mode != 0x1F).count(), 7, "address, designation, five letters");
    assert_eq!(shown(&page)[4], "   Αθήνα");
}

/// REQ-2214 verification: an Arabic run reads back in visual order.
#[test]
fn an_arabic_run_is_in_visual_order() {
    let text = json!([{"row": 6, "column": 0, "text": "مرحبا بالعالم", "language": "ar"}]);
    let page = encoded(&page_value("en", &text));
    let visual: String = "مرحبا بالعالم".chars().rev().collect();
    assert_eq!(shown(&page)[6], visual);
}

/// REQ-2214 item 7: the program's X/28/0 Format 1 keeps its colours and
/// gains the designation.
#[test]
fn the_programs_x28_keeps_its_colours_and_gains_the_designation() {
    let colour = json!({"address": 63, "mode": 31, "data": 127});
    let mut x28: Vec<Value> = (0..13).map(|_| colour.clone()).collect();
    x28[0] = json!({"address": 0, "mode": 0, "data": 0});
    let value = with(&page_value("ru", &Value::Null), "/packets", json!([{"packet": "x28", "designation": 0, "triplets": x28}]));
    let page = encoded(&value);
    let crate::protocol::PackedLine::X28(line) = &page.packets[0] else { panic!() };
    let values = line.triplets.map(|triplet| Some(Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data }.value()));
    let fields = neoteletext_core::packet::presentation_fields(&values);
    let russian = (0b0100 << neoteletext_core::consts::DESIGNATION_GROUP_SHIFT) | 0b100;
    assert_eq!(fields.designations.map(|designations| designations.default), Some(russian));
    assert_eq!(fields.designations.map(|designations| designations.second), Some(NO_SECOND_G0));
    for (index, triplet) in line.triplets.iter().enumerate().skip(2) {
        assert_eq!((triplet.address, triplet.mode, triplet.data), (63, 31, 127), "triplet {index} kept");
    }
    assert_eq!(page.character_set.map(|set| (set.region, set.national_option)), Some((Some(0b0100), 0b100)));
}

/// REQ-2214 verification: each fault refused by its path and rule.
#[test]
fn each_fault_is_refused_by_path_and_rule() {
    use crate::protocol::Rule;
    let refused = |value: &Value| {
        let refusal = read(&line(&page_message(value))).unwrap_err();
        (refusal.path, refusal.rule)
    };
    let run = |row: u8, column: u8, text: &str, language: Value| json!({"row": row, "column": column, "text": text, "language": language});
    let page = |runs: Value| page_value("en", &runs);
    assert_eq!(refused(&page(json!([run(1, 0, "one", Value::Null), run(1, 2, "two", Value::Null)]))), ("page.text[1]".into(), Rule::TextOverlap));
    assert_eq!(refused(&page(json!([run(1, 35, "too long", Value::Null)]))), ("page.text[0].text".into(), Rule::TextOutOfColumns));
    assert_eq!(refused(&page(json!([run(2, 0, "中文", Value::Null)]))), ("page.text[0].text".into(), Rule::TextUnshown));
    // DEC-2239: Thai and Lao are no languages of NeoTeletext.
    assert_eq!(refused(&page(json!([run(1, 0, "สวัสดี", json!("th"))]))), ("page.text[0].language".into(), Rule::UnknownLanguage));
    assert_eq!(refused(&page(json!([run(1, 0, "ສະບາຍດີ", json!("lo"))]))), ("page.text[0].language".into(), Rule::UnknownLanguage));
    assert_eq!(refused(&page(json!([run(1, 0, "สวัสดี", Value::Null)]))), ("page.text[0].text".into(), Rule::TextUnshown));
    assert_eq!(refused(&page(json!([run(1, 0, "x", json!("tlh"))]))), ("page.text[0].language".into(), Rule::UnknownLanguage));
    assert_eq!(refused(&page_value("tlh", &Value::Null)), ("page.language".into(), Rule::UnknownLanguage));
    assert_eq!(refused(&page(json!([run(25, 0, "x", Value::Null)]))), ("page.text[0].row".into(), Rule::OutOfRange));
    assert_eq!(refused(&page(json!([run(1, 0, "", Value::Null)]))), ("page.text[0].text".into(), Rule::TooShort));
    assert_eq!(refused(&page(json!([run(1, 0, "a\u{7}", Value::Null)]))), ("page.text[0].text".into(), Rule::ForbiddenCharacter));
    assert_eq!(refused(&page(json!([run(1, 0, "e\u{301}", Value::Null)]))), ("page.text[0].text".into(), Rule::NotNfc));
    let both = with(&page_value("en", &Value::Null), "/character_set", json!({"region": null, "national_option": 0}));
    assert_eq!(refused(&both), ("page.character_set".into(), Rule::LanguageAndCharacterSet));
    let many: Vec<Value> = (0..65).map(|index| run(1 + index / 4, (index % 4) * 10, "x", Value::Null)).collect();
    assert_eq!(refused(&page(Value::Array(many))), ("page.text".into(), Rule::TooLong));
    // Six rows of forty Greek letters at Level 3.5: the fifth does not fit.
    let line_of = "αβγδεζηθικλμνξοπρστυφχψωαβγδεζηθικλμνξοπ";
    let greek: Vec<Value> = (1..=6).map(|row| run(row, 0, line_of, json!("el"))).collect();
    let over = with(&page(Value::Array(greek)), "/level", json!("3.5"));
    assert_eq!(refused(&over), ("page.text[4]".into(), Rule::TripletsOverBudget));
    // The program's own G2 character at a run's cell.
    let g2 = json!({"designation": 0, "triplets": [
        {"address": 42, "mode": 4, "data": 0}, {"address": 6, "mode": 15, "data": 65},
        {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7},
        {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7},
        {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7},
        {"address": 63, "mode": 31, "data": 7}, {"address": 63, "mode": 31, "data": 7}]});
    let taken = with(&page(json!([run(2, 5, "Мир", json!("ru"))])), "/enhancements", json!([g2]));
    assert_eq!(refused(&taken), ("page.text[0]".into(), Rule::TextCellTaken));
    let mut esc = [b' '; COLUMNS];
    esc[1] = 0x1B;
    let set_changed = with(&page(json!([run(2, 5, "Мир", json!("ru"))])), "/rows/2", json!(base64(&esc)));
    assert_eq!(refused(&set_changed), ("page.text[0]".into(), Rule::TextSetChanged));
}

/// A channel whose program has started, its effects taken.
fn channel() -> Channel {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        session_grace_ms: 0,
        wheel: false,
    };
    let mut channel = Channel::new(config);
    channel.program_started(0);
    channel
}

fn told(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ToProgram { line, .. } => Some(serde_json::from_slice(line).unwrap()),
            _ => None,
        })
        .collect()
}

/// REQ-2215 verification: `start` carries every language of the engine's
/// table with its direction, and the protocol's version; Thai and Lao are
/// not among them (DEC-2239).
#[test]
fn start_lists_every_language() {
    let mut channel = channel();
    let start = told(&channel.take_effects()).into_iter().find(|line| line["type"] == "start").unwrap();
    assert_eq!(start["protocol"], crate::protocol::PROTOCOL);
    let languages = start["languages"].as_array().unwrap();
    assert_eq!(languages.len(), neoteletext_core::charset::language::LANGUAGES.len());
    let find = |tag: &str| languages.iter().find(|language| language["tag"] == tag).unwrap().clone();
    assert_eq!(find("ar"), json!({"tag": "ar", "name": "Arabic", "direction": "rtl"}));
    assert!(!languages.iter().any(|language| language["tag"] == "th" || language["tag"] == "lo"), "no Thai or Lao");
    assert_eq!(find("sr-Latn")["name"], "Serbian (Latin script)");
}

/// REQ-2216 verification: `measure` reaches no viewer and answers with the
/// engine's figures; a refusal carries the request it answers.
#[test]
fn measure_answers_with_the_encoders_figures_and_sends_nothing() {
    let mut channel = channel();
    channel.opened(1, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    let text = json!([{"row": 2, "column": 0, "text": "Привет", "language": "ru"}, {"row": 3, "column": 0, "text": "café"}]);
    let measure = json!({"type": "measure", "request": 7, "page": page_value("en", &text)});
    channel.from_program(&line(&measure), 1);
    let effects = channel.take_effects();
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Send { .. })), "nothing to viewers");
    let answer = told(&effects).into_iter().find(|line| line["type"] == "measured").unwrap();
    // Russian: an address and a row set triplet; café: an address and its
    // mark; the end.
    assert_eq!(
        answer,
        json!({"type": "measured", "request": 7, "triplets": 5, "limit": LOCAL_TRIPLETS, "designation": {"default": 0, "second": null},
            "runs": [{"cells": 6, "triplets": 3}, {"cells": 4, "triplets": 2}]})
    );
    let bad = json!({"type": "measure", "request": 9, "page": page_value("en", &json!([{"row": 2, "column": 0, "text": "中"}]))});
    channel.from_program(&line(&bad), 2);
    let refused = told(&channel.take_effects()).into_iter().find(|line| line["type"] == "refused").unwrap();
    assert_eq!(
        (refused["message"].clone(), refused["path"].clone(), refused["rule"].clone(), refused["request"].clone()),
        (json!("measure"), json!("page.text[0].text"), json!("text_unshown"), json!(9))
    );
    // A message with no request number is refused with none.
    channel.from_program(&line(&json!({"type": "log", "level": "info", "event": "x", "text": "\u{7}"})), 3);
    let refused = told(&channel.take_effects()).into_iter().find(|line| line["type"] == "refused").unwrap();
    assert_eq!(refused["request"], Value::Null);
}

/// REQ-2214 item 9: the same page always encodes to the same bytes.
#[test]
fn the_same_page_encodes_to_the_same_bytes() {
    let text = json!([{"row": 2, "column": 0, "text": "Привет", "language": "ru"}, {"row": 8, "column": 0, "text": "Tiếng Việt", "language": "vi"}]);
    let value = page_value("en", &text);
    assert_eq!(crate::page::encode(&encoded(&value)).packets, crate::page::encode(&encoded(&value)).packets);
}

/// DEC-2326, REQ-2150 item 2: a run from outside the program shows a
/// stand-in for each character its language cannot show, where the same
/// run of the program's own is refused (above): the question mark, or the
/// Arabic script's own where its sets lack ASCII's.
#[test]
fn a_run_from_outside_shows_a_stand_in_for_what_its_language_cannot() {
    let run = |row: u8, text: &str, language: &str| json!({"row": row, "column": 0, "text": text, "language": language, "outside": true});
    let page = encoded(&page_value("en", &json!([run(2, "Tea 中文 ok", "en"), run(3, "Привет 😀", "ru"), run(4, "سلام 中", "ar")])));
    let rows = shown(&page);
    assert_eq!(rows[2], "Tea ?? ok");
    assert_eq!(rows[3], "Привет ?");
    assert!(rows[4].contains('\u{061F}') && !rows[4].contains('中'), "Arabic's own question mark: {:?}", rows[4]);
    // Marked false, or not marked, the program's own text is refused.
    let own = json!([{"row": 2, "column": 0, "text": "中", "outside": false}]);
    let refusal = read(&line(&page_message(&page_value("en", &own)))).unwrap_err();
    assert_eq!(refusal.rule, crate::protocol::Rule::TextUnshown);
}

/// REQ-2150 item 2: each replacement is reported in the server's log, once
/// a page message and character, and the page is sent.
#[test]
fn each_stand_in_is_logged_once_a_page_and_character() {
    let mut channel = channel();
    channel.opened(1, 1, Vec::new(), 0);
    let _ = channel.take_effects();
    let text = json!([{"row": 2, "column": 0, "text": "中中文", "outside": true}, {"row": 3, "column": 0, "text": "文", "outside": true}]);
    channel.from_program(&line(&page_message(&page_value("en", &text))), 1);
    let effects = channel.take_effects();
    let logged: Vec<&String> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Log { event: "stand_in", text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(logged.len(), 2, "{logged:?}");
    assert!(logged[0].contains("U+4E2D") && logged[1].contains("U+6587"), "{logged:?}");
    assert!(!told(&effects).iter().any(|line| line["type"] == "refused"), "the page is not refused");
}
