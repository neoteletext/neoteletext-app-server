//! Text in pictures (renderer parity PAR-21, PAR-22; REQ-6767): the text
//! parts of a `regis` or `naplps` message written by the engine's one
//! picture text encoder (`neoteletext_formats::picture_text`, PAR-3 to
//! PAR-5) in the 55 provided sets only (DEC-6669), joined in order with the
//! message's byte parts into its `data`, after which the message is
//! treated exactly as one that gave `data`; and `measure_picture_text`'s
//! answer (PAR-6). The message has been checked
//! ([`crate::protocol::check`]) before it gets here.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::glyphs::conformance::FONT_SETS;
use neoteletext_formats::picture_text::{self, PictureTextError, STAND_INS, Stretch};
use neoteletext_web::base64;

use crate::protocol::{self, Base64, MeasurePictureText, MeasuredPictureText, MessageType, PageNumber, PicturePart, ProgramMessage, Refusal, Replaced, Rule};

/// Which encoding a message's text parts take.
#[derive(Clone, Copy)]
enum Language {
    Regis,
    Naplps,
}

/// The language of a part that names none: Latin English.
const ENGLISH: &str = "en";

/// The refusal of an encoder's fault in the part at `at` (`parts[3]`, or
/// the root for `measure_picture_text`), by the rule PAR-21 gives it.
fn refusal(kind: MessageType, at: &str, error: &PictureTextError) -> Refusal {
    let field = |name: &str| if at.is_empty() { name.to_string() } else { format!("{at}.{name}") };
    let (path, rule) = match error {
        PictureTextError::UnknownLanguage(_) => (field("language"), Rule::UnknownLanguage),
        PictureTextError::TooLong { .. } => (field("text"), Rule::TooLong),
        PictureTextError::Forbidden { .. } => (field("text"), Rule::ForbiddenCharacter),
        // A code ReGIS strings cannot carry (PAR-4) is a character this
        // picture language cannot show.
        _ => (field("text"), Rule::TextUnshown),
    };
    Refusal { message: Some(kind), path, rule, detail: error.to_string() }
}

/// The characters of `text` that a part from outside showed as a stand-in,
/// each once, with the stand-in: those no set of the language shows alone
/// (DEC-2326), as the teletext encoder's runs report them.
fn stand_ins(page: PageNumber, text: &str, language: Option<&str>) -> Vec<Replaced> {
    let shows = |character: char| {
        let mut buffer = [0_u8; 4];
        picture_text::stretches(character.encode_utf8(&mut buffer), language, false).is_ok()
    };
    let mut replaced: Vec<Replaced> = Vec::new();
    for character in text.chars() {
        if replaced.iter().any(|seen| seen.character == character) || shows(character) {
            continue;
        }
        let Some(stand_in) = STAND_INS.into_iter().find(|&candidate| shows(candidate)) else { continue };
        replaced.push(Replaced { page, character, stand_in, language: language.unwrap_or(ENGLISH).to_string() });
    }
    replaced
}

/// One text part's stretches.
fn stretches_of(part: &protocol::TextPart) -> Result<Vec<Stretch>, PictureTextError> {
    picture_text::stretches(&part.text, part.language.as_ref().map(|tag| tag.0.as_str()), part.outside)
}

/// The bytes of `parts`, joined in order.
fn joined(kind: MessageType, page: PageNumber, language: Language, parts: &[PicturePart], replaced: &mut Vec<Replaced>) -> Result<Vec<u8>, Refusal> {
    let mut bytes = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let at = format!("parts[{index}]");
        match part {
            PicturePart::Data(part) => bytes.extend_from_slice(&protocol::check::bytes_of(&part.data)),
            PicturePart::Text(part) => {
                let stretches = stretches_of(part).map_err(|error| refusal(kind, &at, &error))?;
                if part.outside {
                    replaced.extend(stand_ins(page, &part.text, part.language.as_ref().map(|tag| tag.0.as_str())));
                }
                let written = match language {
                    Language::Regis => picture_text::regis(&stretches, &mut bytes),
                    Language::Naplps => picture_text::naplps(&stretches, &mut bytes),
                };
                written.map_err(|error| refusal(kind, &at, &error))?;
            }
        }
    }
    if bytes.len() > protocol::check::MOST_REGIS_BYTES {
        return Err(Refusal {
            message: Some(kind),
            path: "parts".to_string(),
            rule: Rule::TooLong,
            detail: format!("{} bytes once written, at most {}", bytes.len(), protocol::check::MOST_REGIS_BYTES),
        });
    }
    Ok(bytes)
}

/// Bytes as exact base64.
fn base64_of(bytes: &[u8]) -> Base64 {
    let mut out = alloc::vec![0_u8; base64::encoded_len(bytes.len())];
    let written = base64::encode(bytes, &mut out).unwrap_or(0);
    out.truncate(written);
    Base64(String::from_utf8(out).unwrap_or_default())
}

/// A `regis` or `naplps` message's `parts` written into its `data`
/// (PAR-21), with the characters its parts from outside showed as a
/// stand-in, for the log. Any other message is left as it is.
///
/// # Errors
/// The encoder's refusal, at the path of the part at fault.
pub(crate) fn encode_parts(message: &mut ProgramMessage) -> Result<Vec<Replaced>, Refusal> {
    let kind = message.kind();
    let (page, data, parts, language) = match message {
        ProgramMessage::Regis(message) => (message.page, &mut message.data, &mut message.parts, Language::Regis),
        ProgramMessage::Naplps(message) => (message.page, &mut message.data, &mut message.parts, Language::Naplps),
        _ => return Ok(Vec::new()),
    };
    let Some(given) = parts.take() else { return Ok(Vec::new()) };
    let mut replaced = Vec::new();
    *data = Some(base64_of(&joined(kind, page, language, &given, &mut replaced)?));
    Ok(replaced)
}

/// `measure_picture_text`'s answer (PAR-22): the encoder's own figures,
/// each set by its `FONT_SETS` name.
///
/// # Errors
/// The encoder's refusal, at the field at fault.
pub(crate) fn measure(message: &MeasurePictureText) -> Result<MeasuredPictureText, Refusal> {
    let kind = MessageType::MeasurePictureText;
    let stretches = picture_text::stretches(&message.text, message.language.as_ref().map(|tag| tag.0.as_str()), message.outside)
        .map_err(|error| refusal(kind, "", &error))?;
    let measured = picture_text::measure(&stretches);
    let sets = measured.sets.iter().filter_map(|&set| FONT_SETS.get(usize::from(set)).map(|(name, _)| (*name).to_string())).collect();
    Ok(MeasuredPictureText {
        request: message.request,
        cells: measured.cells,
        regis_bytes: measured.regis_bytes,
        naplps_bytes: measured.naplps_bytes,
        sets,
        stand_ins: measured.stand_ins,
    })
}

#[cfg(test)]
mod tests;
