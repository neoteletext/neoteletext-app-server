//! The program protocol's picture messages of renderer parity
//! (neoteletext-common `docs/renderer-parity.md` sections 7 to 10,
//! PAR-16 to PAR-26; REQ-6766 to REQ-6768): the audience `"kept"`, the
//! NAPLPS links and cursor, `forget_picture`, text parts, measuring
//! picture text, and a public page's picture.
//!
//! **Why program protocol 15 is not raised.** Every addition here is a
//! message a program sends, a value or an optional field of a message a
//! program sends, or an answer only to one of those (`measured_picture_text`
//! and the new refusal rules). A program written for protocol 15 before
//! them never sends any of them, so it never receives anything new
//! (PAR-16's introduction, DEC-6760 item 3).

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Serialize;

use super::animation::NaplpsObject;
use super::{Base64, Connections, LanguageTag, MOST_DRAWING_LINKS, PageNumber, SubCode};

/// The most parts one `regis` or `naplps` message holds (PAR-21).
pub const MOST_PICTURE_PARTS: usize = 64;
/// The most characters of one text part: the engine's bound
/// (`neoteletext_formats::picture_text`, PAR-3), used by name.
pub const MOST_PART_CHARACTERS: usize = neoteletext_formats::picture_text::MAX_PART_CHARACTERS;

/// Who a picture message is for (PAR-16): what [`Connections`] takes, or
/// `"kept"`, which keeps it for the page and sub-code named and sends it
/// to every connection showing that page now or later, for as long as it
/// is kept.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PictureAudience {
    /// `"kept"`.
    Kept(Kept),
    /// The connections, as every other message names them.
    Connections(Connections),
}

impl PictureAudience {
    /// The connections named, or none for `"kept"`.
    #[must_use]
    pub const fn connections(&self) -> Option<&Connections> {
        match self {
            Self::Kept(_) => None,
            Self::Connections(connections) => Some(connections),
        }
    }

    /// Whether it is `"kept"`.
    #[must_use]
    pub const fn is_kept(&self) -> bool {
        matches!(self, Self::Kept(_))
    }
}

/// `"kept"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kept {
    /// Kept for the page and sub-code, for every connection that shows it.
    Kept,
}

/// One part of a `regis` or `naplps` message's bytes (PAR-21): bytes as
/// `data` takes them, or Unicode text the server writes in the provided
/// character sets with the engine's encoder.
// Read by the reader, which chooses by the field it holds (`read.rs`), so
// that a fault keeps its path.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum PicturePart {
    /// Bytes.
    Data(DataPart),
    /// Text.
    Text(TextPart),
}

/// A part of bytes.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataPart {
    /// The bytes, exact base64, as `data` takes them.
    pub data: Base64,
}

/// A part of text (PAR-21): written in visual order in the language's own
/// provided sets and Latin English (PAR-3), as ReGIS alphabets 100 to 154
/// (PAR-4) or NAPLPS designations of the provided sets (PAR-5).
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextPart {
    /// 1 to 1,000 characters in Unicode normalisation form C, with no
    /// control or bidirectional formatting character.
    #[schemars(length(min = 1, max = MOST_PART_CHARACTERS))]
    pub text: String,
    /// Its language, a tag of `start.languages`; none for Latin English.
    #[serde(default)]
    pub language: Option<LanguageTag>,
    /// Whether the text came from outside the program: a character its
    /// language cannot show becomes the language's stand-in rather than
    /// refusing the message (DEC-2326). False when absent.
    #[serde(default)]
    pub outside: bool,
}

/// `naplps_links` (PAR-20): the objects of a sub-page's NAPLPS picture
/// that are links, written as a `DRAWING_LINKS` record (PAR-2). The list
/// replaces the sub-page's links, and an empty one removes them. An object
/// may be named before it is defined.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NaplpsLinks {
    /// Who it is for.
    pub connections: PictureAudience,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The links, each object named once.
    #[schemars(length(max = MOST_DRAWING_LINKS))]
    pub links: Vec<ObjectLink>,
}

/// One link of `naplps_links`.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectLink {
    /// The NAPLPS object that is the link (ANIM-11), 1 to 65,535.
    #[schemars(range(min = 1))]
    pub object: u16,
    /// Where it leads, in any form a text link's target takes, as
    /// `regis_links`'s.
    #[schemars(length(min = 1, max = neoteletext_formats::page::LINK_TARGET_BYTES))]
    pub target: String,
}

/// `naplps_cursor` (PAR-20): the object of a sub-page's NAPLPS picture
/// that is its cursor, or 0 for none, written as a `REGIS_CURSOR` record
/// (PAR-2).
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NaplpsCursor {
    /// Who it is for.
    pub connections: PictureAudience,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The object, or 0 for no cursor.
    pub object: u16,
}

/// `forget_picture` (PAR-19): drops the kept picture of a page and
/// sub-code; every connection taking it is sent an empty picture.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgetPicture {
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
}

/// `measure_picture_text` (PAR-22): what a text part costs, sending
/// nothing to any viewer.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasurePictureText {
    /// A number the program chooses, which the answer carries back.
    #[schemars(range(max = u32::MAX))]
    pub request: u32,
    /// The text, as a text part's.
    #[schemars(length(min = 1, max = MOST_PART_CHARACTERS))]
    pub text: String,
    /// Its language, a tag of `start.languages`; none for Latin English.
    #[serde(default)]
    pub language: Option<LanguageTag>,
    /// As a text part's `outside`.
    #[serde(default)]
    pub outside: bool,
}

/// `measured_picture_text` (PAR-22): the answer to `measure_picture_text`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasuredPictureText {
    /// `measure_picture_text`'s request number.
    #[schemars(range(max = u32::MAX))]
    pub request: u32,
    /// The character cells the text advances; a mark takes none. Its width
    /// is this many times the character cell the picture sets.
    pub cells: u32,
    /// The bytes of its ReGIS (PAR-4).
    pub regis_bytes: u32,
    /// The bytes of its NAPLPS (PAR-5).
    pub naplps_bytes: u32,
    /// The provided sets it uses, by the engine's names
    /// (`FONT_SETS`), in order of first use.
    pub sets: Vec<String>,
    /// The characters that became stand-ins.
    pub stand_ins: u32,
}

/// A picture language (PAR-26).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PictureLanguageName {
    /// ReGIS, 800 by 480.
    Regis,
    /// NAPLPS on the 480 by 500 teletext raster.
    Naplps,
}

/// The most bytes of a picture `description.picture` carries, decoded
/// (PAR-26).
pub const MOST_DESCRIPTION_PICTURE_BYTES: usize = 262_144;

/// `description.picture` (PAR-26): the public page's picture, drawn by
/// the server as the browser draws it.
// Read by the reader, which chooses by the field it holds (`read.rs`).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum DescriptionPicture {
    /// The kept picture of the described page with this sub-code.
    Kept(KeptPictureName),
    /// These bytes.
    Bytes(PictureBytes),
}

/// `{"kept": "0001"}`.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeptPictureName {
    /// The sub-code of the described page whose kept picture is drawn.
    pub kept: SubCode,
}

/// A picture's bytes for a public page.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PictureBytes {
    /// Its language.
    pub language: PictureLanguageName,
    /// Its bytes, exact base64, at most 262,144 decoded.
    pub data: Base64,
    /// For NAPLPS, its objects, of which the shown ones are composed; null
    /// for none.
    #[serde(default)]
    #[schemars(length(max = super::MOST_NAPLPS_OBJECTS))]
    pub objects: Option<Vec<NaplpsObject>>,
}

/// The schema of a picture message's bytes: exactly one of `data` and
/// `parts` (PAR-21), stated as the reader enforces it.
pub(crate) fn data_or_parts(schema: &mut Schema) {
    let one_of = serde_json::json!([{ "required": ["data"] }, { "required": ["parts"] }]);
    schema.insert(String::from("oneOf"), one_of);
}

/// A `parts` list's schema: 1 to [`MOST_PICTURE_PARTS`] parts.
pub(crate) struct Parts;

impl JsonSchema for Parts {
    fn schema_name() -> Cow<'static, str> {
        "PictureParts".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let part = generator.subschema_for::<PicturePart>();
        json_schema!({
            "description": "The message's bytes as parts joined in order: bytes, or text the server writes in the provided character sets (renderer parity PAR-21).",
            "type": "array",
            "items": part,
            "minItems": 1,
            "maxItems": MOST_PICTURE_PARTS
        })
    }
}
