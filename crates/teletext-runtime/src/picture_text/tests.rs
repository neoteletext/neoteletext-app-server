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
