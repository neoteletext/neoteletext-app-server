//! Text parts and measuring (neoteletext-common `docs/renderer-parity.md`
//! PAR-21, PAR-22; REQ-6767): a part encodes to exactly the bytes the
//! engine's encoder gives (PAR-4, PAR-5), in every language `start`
//! lists; a mark, Arabic and Hebrew; `outside`; each refusal with its
//! path; and `measure_picture_text` gives the encoder's figures.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_formats::picture_text;
use serde_json::json;

use crate::protocol::tests::{base64, line};
use crate::protocol::{ProgramMessage, Rule, read, read_reporting};

/// The bytes `kind` (`regis` or `naplps`) gives for one text part.
fn written(kind: &str, text: &str, language: Option<&str>) -> Result<Vec<u8>, crate::protocol::Refusal> {
    let message = json!({"type": kind, "connections": "kept", "page": "100", "sub": "0001", "parts": [{"text": text, "language": language}]});
    let data = match read(&line(&message))? {
        ProgramMessage::Regis(message) => message.data,
        ProgramMessage::Naplps(message) => message.data,
        other => panic!("{other:?}"),
    };
    Ok(crate::protocol::check::bytes_of(&data.unwrap()))
}

/// The engine's own bytes for the same text.
fn engine(kind: &str, text: &str, language: Option<&str>) -> Vec<u8> {
    let stretches = picture_text::stretches(text, language, false).unwrap();
    let mut out = Vec::new();
    if kind == "regis" { picture_text::regis(&stretches, &mut out) } else { picture_text::naplps(&stretches, &mut out) }.unwrap();
    out
}

/// A text each language shows: its name in English, which Latin English
/// writes where the language's own set lacks a letter (PAR-3).
#[test]
fn a_part_in_every_language_is_the_engines_bytes() {
    for language in crate::protocol::languages() {
        let tag = language.tag.0.as_str();
        for kind in ["regis", "naplps"] {
            assert_eq!(written(kind, "Hello 1", Some(tag)).unwrap(), engine(kind, "Hello 1", Some(tag)), "{kind} {tag}");
        }
    }
}

/// Greek, Arabic, Hebrew and a mark, in both languages; ReGIS byte-exact
/// for a mark (PAR-4's example).
#[test]
fn greek_arabic_hebrew_and_a_mark_are_the_engines_bytes() {
    for (text, language) in [("Καλημέρα", Some("el")), ("مرحبا", Some("ar")), ("שלום", Some("he")), ("éa", None)] {
        for kind in ["regis", "naplps"] {
            assert_eq!(written(kind, text, language).unwrap(), engine(kind, text, language), "{kind} {text}");
        }
    }
    assert_eq!(written("regis", "éa", None).unwrap(), b"T(A101)\"e\"T(A120)T(W(V))\"\x08\x42\"T(A101)\"a\"T(A0)");
}

/// Renderer parity PAR-44: each text part of the shared wire vectors,
/// given as its meaning gives it to a message of the picture language its
/// name begins with, is that row's bytes.
#[test]
fn the_shared_text_parts_are_the_wire_vectors() {
    let mut names = Vec::new();
    for vector in neoteletext_web::wire_vectors::vectors().filter(|vector| vector.kind == "picture_text") {
        let (part, _) = vector.meaning.rsplit_once(" as ").unwrap();
        let part: serde_json::Value = serde_json::from_str(part).unwrap();
        let kind = vector.name.split('_').next().unwrap();
        let message = json!({"type": kind, "connections": "kept", "page": "101", "sub": "0001", "parts": [part]});
        let data = match read(&line(&message)).unwrap() {
            ProgramMessage::Regis(message) => message.data,
            ProgramMessage::Naplps(message) => message.data,
            other => panic!("{other:?}"),
        };
        let (expected, length) = neoteletext_web::wire_vectors::bytes_of(vector.value).unwrap();
        assert_eq!(crate::protocol::check::bytes_of(&data.unwrap()), expected[..length], "{}", vector.name);
        names.push(vector.name);
    }
    // A Greek and an Arabic part, in each picture language.
    assert_eq!(names, ["regis_greek", "regis_arabic", "naplps_greek", "naplps_arabic"]);
}

/// `outside`: a character the language cannot show becomes the stand-in,
/// and is reported for the log; without it the part is refused at its path.
#[test]
fn outside_text_shows_a_stand_in_and_is_reported() {
    let message = json!({"type": "naplps", "connections": [1], "page": "100", "sub": "0001",
                         "parts": [{"data": base64(b"\x0e")}, {"text": "a\u{4e00}", "outside": true}]});
    let (_, replaced) = read_reporting(&line(&message)).unwrap();
    assert_eq!(replaced.len(), 1);
    assert_eq!((replaced[0].character, replaced[0].stand_in), ('\u{4e00}', '?'));
    let refused = read(&line(&crate::protocol::tests::with(&message, "/parts/1/outside", json!(false)))).unwrap_err();
    assert_eq!((refused.path.as_str(), refused.rule), ("parts[1].text", Rule::TextUnshown));
}

/// PAR-22: the answer is the encoder's measure, its sets by name; a
/// refusal carries the request.
#[test]
fn measure_picture_text_answers_the_encoders_figures() {
    let message = json!({"type": "measure_picture_text", "request": 17, "text": "مرحبا", "language": "ar", "outside": false});
    let ProgramMessage::MeasurePictureText(measure) = read(&line(&message)).unwrap() else { panic!() };
    let answer = super::measure(&measure).unwrap();
    let expected = picture_text::measure(&picture_text::stretches("مرحبا", Some("ar"), false).unwrap());
    assert_eq!(
        (answer.request, answer.cells, answer.regis_bytes, answer.naplps_bytes, answer.stand_ins),
        (17, expected.cells, expected.regis_bytes, expected.naplps_bytes, 0)
    );
    assert_eq!(answer.regis_bytes as usize, engine("regis", "مرحبا", Some("ar")).len());
    assert_eq!(answer.sets, ["G0-Arabic-Plain"]);
    let answer = serde_json::to_value(&answer).unwrap();
    let keys: Vec<String> = answer.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["cells", "naplps_bytes", "regis_bytes", "request", "sets", "stand_ins"]);

    let unshown = crate::protocol::tests::with(&message, "/text", json!("\u{4e00}"));
    let ProgramMessage::MeasurePictureText(measure) = read(&line(&unshown)).unwrap() else { panic!() };
    let refused = super::measure(&measure).unwrap_err();
    assert_eq!((refused.path.as_str(), refused.rule), ("text", Rule::TextUnshown));
    assert_eq!(crate::protocol::request_of(&line(&unshown)), Some(17), "the refusal carries the request");
}

/// RAS-8 item 4 (DEC-6857, DEC-6856 item 8): a NAPLPS text part is bytes of
/// characters only, the set's designation, SI, the codes and the primary
/// set designated back, with no SO, so no TEXT field or point of the
/// server's own can be among them, in any language `start` lists.
#[test]
fn a_naplps_text_part_writes_no_field_or_point_of_its_own() {
    /// SO, which a picture description instruction needs before it.
    const SHIFT_OUT: u8 = 0x0E;
    for language in crate::protocol::languages() {
        let tag = language.tag.0.as_str();
        let bytes = written("naplps", "Hello 1", Some(tag)).unwrap();
        assert!(!bytes.contains(&SHIFT_OUT), "{tag}: {bytes:02x?}");
        assert_eq!(bytes, engine("naplps", "Hello 1", Some(tag)), "{tag}: the engine's characters and nothing else");
    }
}

/// RAS-8's full-page test through the server's text parts: the 23 rows 1
/// to 23 of 40 characters, row r column c the G0 Latin English code
/// 0x21 + ((r + c) mod 94), each row a text part after the program's own
/// bytes setting the field (3,276 by 4,452 steps of 1/131072) and the
/// point of the dot (0, 20r + 19) rounded down, as the kits' `text()`
/// writes them (neoteletext-common's example `naplps-text-page`). The
/// message's stream is the program's bytes and the engine's characters
/// with nothing between, and drawn as the receiver draws it, on 480 by 460
/// with the engine's Noto Teletext glyphs, every cell's 12 by 20 dots are
/// exactly its glyph.
#[test]
fn a_page_of_naplps_text_parts_puts_every_character_on_its_noto_cell() {
    use naplps::{Rounding, dot_steps, multi_value_operand, unit_steps};
    use neoteletext_core::charset::{CharRef, to_unicode};
    use neoteletext_core::consts::{CELL_HEIGHT, CELL_WIDTH, COLUMNS};
    use neoteletext_core::glyphs::GlyphSet;
    use neoteletext_core::glyphs::conformance::FONT_SETS;

    /// The provided set of G0 Latin English (REQ-6668).
    const LATIN_ENGLISH: usize = 1;
    /// Six-byte multi-value operands (RAS-6).
    const BYTES: u8 = 6;
    /// The teletext rows NAPLPS text is written on (DEC-6857).
    const ROWS: core::ops::RangeInclusive<usize> = 1..=23;
    /// The codes of a provided set's printing characters cycle through 94.
    const PRINTING: usize = 94;
    /// The first printing code, 2/1.
    const FIRST_PRINTING: usize = 0x21;
    let set = FONT_SETS[LATIN_ENGLISH].1;
    assert_eq!(FONT_SETS[LATIN_ENGLISH].0, "G0-Latin-English");
    let code = |row: usize, column: usize| u8::try_from(FIRST_PRINTING + (row + column) % PRINTING).unwrap();

    // The kits' preamble: SO, RESET, DOMAIN of six-byte operands, CLEAR
    // (ANSI X3.110-1983 sections 5.3.2.1, 5.3.2.2, 5.2.5).
    let mut preamble = alloc::vec![0x0E, 0x20, 0x7F, 0x7F, 0x21, 0x54];
    multi_value_operand(0, 0, BYTES, &mut preamble);
    preamble.push(0x0C);
    let mut parts = Vec::new();
    let mut program = Vec::new();
    for row in ROWS {
        let mut data = if row == *ROWS.start() { preamble.clone() } else { alloc::vec![0x0E] };
        // RAS-8 items 1 and 2: TEXT's field, then POINT SET ABSOLUTE.
        data.extend_from_slice(&[0x22, 0x40, 0x40]);
        multi_value_operand(unit_steps(1, 40, BYTES, Rounding::Down), unit_steps(20 * 25, 32 * 460, BYTES, Rounding::Down), BYTES, &mut data);
        data.push(0x24);
        let (across, up) = dot_steps(0, i64::try_from(CELL_HEIGHT * row + CELL_HEIGHT - 1).unwrap(), 1, BYTES, Rounding::Down);
        multi_value_operand(across, up, BYTES, &mut data);
        let text: String = (0..COLUMNS).map(|column| to_unicode(CharRef::new(set, code(row, column))).base).collect();
        parts.push(json!({"data": base64(&data)}));
        parts.push(json!({"text": text, "language": "en"}));
        program.push((data, text));
    }
    // The shared example's first part, at neoteletext-common d24ff33.
    assert_eq!(parts[0]["data"], "DiB/fyFUQEBAQEBADCJAQEBBcF1MZCRCR0dCQ0M=");
    let message = json!({"type": "naplps", "connections": "kept", "page": "100", "sub": "0001", "parts": parts});
    let ProgramMessage::Naplps(read) = read(&line(&message)).unwrap() else { panic!() };
    let stream = crate::protocol::check::bytes_of(&read.data.unwrap());
    let mut expected = Vec::new();
    for (data, text) in &program {
        expected.extend_from_slice(data);
        expected.extend_from_slice(&engine("naplps", text, Some("en")));
    }
    assert_eq!(stream, expected, "the program's bytes and the engine's characters, nothing between");

    let (mut terminal, mut canvas) = crate::fallback::naplps_terminal().unwrap();
    assert_eq!((canvas.width(), canvas.height()), (480, 460));
    let _ = terminal.feed(&mut canvas, &stream);
    let _ = terminal.finish(&mut canvas);
    let width = canvas.width() as usize;
    let pixels = canvas.pixels();
    // Canvas line 0 is raster line 20, teletext row 1's first (RAS-11).
    let at = |dot: usize, line: usize| pixels[(line - naplps::limits::DISPLAY_FIRST_LINE as usize) * width + dot];
    let glyphs = GlyphSet::noto_teletext();
    let ink = |rows: &[u16; CELL_HEIGHT], dot: usize, line: usize| rows[line] >> (CELL_WIDTH - 1 - dot) & 1 != 0;
    let first = glyphs.glyph(CharRef::new(set, code(1, 0)));
    let cell = || (0..CELL_HEIGHT).flat_map(|line| (0..CELL_WIDTH).map(move |dot| (dot, line)));
    let inked = cell().find(|&(dot, line)| ink(&first, dot, line)).unwrap();
    let bare = cell().find(|&(dot, line)| !ink(&first, dot, line)).unwrap();
    let (drawing, cleared) = (at(inked.0, CELL_HEIGHT + inked.1), at(bare.0, CELL_HEIGHT + bare.1));
    assert_ne!(drawing, cleared);
    let mut wrong = Vec::new();
    for row in ROWS {
        for column in 0..COLUMNS {
            let glyph = glyphs.glyph(CharRef::new(set, code(row, column)));
            let (left, top) = (CELL_WIDTH * column, CELL_HEIGHT * row);
            if !cell().all(|(dot, line)| at(left + dot, top + line) == if ink(&glyph, dot, line) { drawing } else { cleared }) {
                wrong.push((row, column));
            }
        }
    }
    assert_eq!(wrong, [], "every one of the 920 cells is exactly its glyph");
}
