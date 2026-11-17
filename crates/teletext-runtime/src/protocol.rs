//! The protocol between the server and a channel's program (DEC-1482,
//! REQ-1475 to REQ-1479; DOC-1968 and DOC-1970): every
//! message in both directions as one exact Rust type, read serde-strict
//! (an unknown `type` or field refused, never skipped), then checked
//! against every bound, from the engine's `neoteletext_core::limits`
//! where the standard has one ([`check`]). A message that breaks any rule
//! is refused whole, with the path of its first fault and a rule from the
//! closed list [`Rule`]. The protocol's JSON Schema (draft 2020-12) is
//! generated from these same types by [`schema`], and published as
//! `protocol/schema-<PROTOCOL>.json`, so a program in any language checks
//! against exactly what the server enforces.
//!
//! The wire is one JSON object a line (REQ-1479).

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::limits;
use neoteletext_input::keys;
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod check;
mod read;

pub use check::Refusal;
pub use read::{read, read_reporting, request_of};

/// The protocol's version, in `start` and in the schema's name. Version 1
/// was the protocol before DEC-1482: declared pages, TTI and
/// publishing for everyone. Version 2 had `goto`, which `redirect` replaced
/// (DEC-1683), and which could name no other channel. Version 6 numbers
/// each selection (DEC-1759 lesson 13), passes a program its cookies, adds
/// `denied`, lets a redirect keep a query, and adds `describe` and
/// `description` (REQ-1728). Version 7 gives input every field a receiver
/// sends: `mods` on a key, the mouse and the wheel, the dot `px` and `py` on
/// the mouse and the wheel, and `compose` (REQ-1347). Version 8 lets a page
/// carry Unicode text runs with their languages, which the server encodes
/// with the engine's encoder (DEC-2199, REQ-2214), lists the languages in
/// `start` (REQ-2215), adds `measure` and `measured`, and names the request
/// a refusal answers (REQ-2216). Version 9 removes what existed only for
/// Thai and Lao, which NeoTeletext no longer supports (the engine's
/// DEC-2239): `mark_rows` from `start.languages` and from `measured`'s
/// runs, and the refusal `mark_rows_out_of_range`. Version 10 marks a
/// text run as from outside the program (`outside`), whose characters no
/// set of its language shows become a stand-in rather than a refusal, and
/// bounds `measure`'s and `measured`'s request numbers above in the schema
/// (DEC-2326, REQ-2150).
pub const PROTOCOL: u32 = 11;
/// The most text runs a page carries (REQ-2214 item 4).
pub const MOST_TEXT_RUNS: usize = 64;
/// The longest text run, in characters: a row (REQ-2214 item 4).
pub const TEXT_RUN_CHARACTERS: usize = COLUMNS;
/// The triplets a page's local enhancement data holds: sixteen packets X/26
/// of thirteen (EN 300 706 clause 9.4.1; REQ-2149).
pub const TRIPLET_LIMIT: u16 = neoteletext_core::consts::LOCAL_TRIPLETS;
/// The first row a text run may be on: row 0 is the header.
const FIRST_TEXT_ROW: u8 = neoteletext_formats::write_text::FIRST_TEXT_ROW;
/// The longest line either side may send: 16 MiB. A stored sound's two
/// minutes of 16-bit samples in base64 are about 15 MB, the largest
/// message there is; a page of the standard's 8,190 sub-pages of whole
/// rows is about 12 MB.
pub const LONGEST_LINE: usize = 16 * 1024 * 1024;
/// The longest page title, magazine name or section heading: the
/// engine's contents rule (its REQ-WEB-025, decision E.120).
pub const TITLE_CHARACTERS: usize = neoteletext_web::contents::MAX_TITLE;
/// The longest text of a page's description, in characters: a server's own
/// budget, so that one answer stays small. A sub-page holds 960 characters;
/// this holds a long article of some 2,500 words, and a program cuts a longer
/// one. Provisional, not yet measured against search engines.
pub const DESCRIPTION_TEXT_CHARACTERS: usize = 16_000;
/// The connections one message may name. Provisional, a budget of the
/// server's own: more viewers than this are reached with `"all"`.
pub const MOST_NAMED_CONNECTIONS: usize = 4_096;
/// The longest operator's log line a program may send, in characters.
pub const OPERATOR_LOG_CHARACTERS: usize = 1_000;
/// The longest event name of an operator's log line.
pub const EVENT_NAME_CHARACTERS: usize = 64;
/// The longest cookie name and value, in bytes: RFC 6265 section 6.1's
/// least a user agent must keep, 4,096 bytes for name, value and
/// attributes together.
pub const COOKIE_BYTES: usize = 4_096;
/// The cookies a `connected` event carries at most: RFC 6265 section 6.1's
/// least a user agent keeps for one domain.
pub const MOST_COOKIES: usize = 50;
/// The largest integer every language's JSON reader holds exactly, 2^53
/// less one (IEEE 754 binary64): connections and sessions are at most it.
pub const SAFE_INTEGER_LIMIT: u64 = crate::session_cookie::SESSION_LIMIT - 1;
/// The longest a sound may be: two minutes at 48 kHz, the browser's own
/// limit for one sound.
pub const MOST_SAMPLES: usize = 48_000 * 120;
/// The sample rate of the instrument codecs, the SID chip's and the DX7's
/// (the engine's REQ-6411, DEC-6410).
pub const INSTRUMENT_RATE: u32 = 48_000;
/// The blocks of an instrument's sound in a second: a block is 20 ms.
pub const INSTRUMENT_BLOCKS_A_SECOND: u32 = 50;
/// The samples a block of an instrument's sound plays for: 960.
pub const INSTRUMENT_BLOCK_SAMPLES: u32 = INSTRUMENT_RATE / INSTRUMENT_BLOCKS_A_SECOND;
/// The most blocks of a stored instrument sound: two minutes, 6,000 blocks,
/// the receivers' limit on a stored sound.
pub const MOST_INSTRUMENT_BLOCKS: usize = MOST_SAMPLES / INSTRUMENT_BLOCK_SAMPLES as usize;
/// The most blocks one message adds to a live instrument sound: two
/// seconds, far more than a program keeps ahead of what is heard.
pub const MOST_LIVE_INSTRUMENT_BLOCKS: usize = 100;
/// The SID chip's writable registers, one image of them: 25 bytes.
pub const SID_IMAGE_BYTES: u16 = 25;
/// How many images of the SID chip's registers a block may hold: one every
/// 20, 10 or 5 ms.
pub const SID_IMAGES_PER_BLOCK: [u8; 3] = [1, 2, 4];
/// One slot of a DX7 block: a time, a MIDI status byte and its two data
/// bytes.
pub const DX7_SLOT_BYTES: u16 = 4;
/// The most slots a DX7 block holds.
pub const DX7_MOST_SLOTS: u8 = 16;
/// One DX7 patch in the instrument's packed form: 128 bytes.
pub const DX7_PATCH_BYTES: u16 = 128;
/// The most patches a DX7 bank holds: the instrument's 32 voices.
pub const DX7_MOST_PATCHES: usize = 32;
/// The bytes of one Juno patch: the Juno-106's sixteen sliders and two
/// bytes of switches (its owner's manual, MIDI Implementation 3.1), the
/// layout of the engine's `juno::Patch` (DEC-6429).
pub const JUNO_PATCH_BYTES: u16 = 18;
/// The bytes of one Minimoog patch, the layout of the engine's
/// `minimoog::Patch`: the panel's knobs and switches, the engine's own
/// layout since the instrument has no patch memory.
pub const MINIMOOG_PATCH_BYTES: u16 = 36;
/// The bytes of one Prophet-5 program: the 24 bytes of its memory
/// (Prophet-5 Operation Manual, section 6-7).
pub const PROPHET5_PATCH_BYTES: u16 = 24;
/// The bytes of one Jupiter-8 patch, the layout of the engine's
/// `jupiter8::Patch`, after Roland's Jupiter-8/System-8 Parameter
/// Correspondence Table.
pub const JUPITER8_PATCH_BYTES: u16 = 43;
/// The bytes of one CS-80 patch: two tones of 26 controls and 33 controls
/// of the panel outside them, the layout of the engine's `cs80::Patch`.
pub const CS80_PATCH_BYTES: u16 = 85;
/// The most patches a bank of an analogue synthesizer holds: as many as a
/// DX7 bank, so that a program change means the same in every instrument.
pub const SYNTH_MOST_PATCHES: usize = DX7_MOST_PATCHES;
/// The largest MIDI key: a data byte's seven bits (MIDI 1.0), the highest
/// split point a Jupiter-8 sound may name.
pub const LAST_MIDI_KEY: u8 = 0x7F;
/// The sample rates of the audio profile (the engine's `audio.md`).
pub const SAMPLE_RATES: [u32; 8] = [8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000];
/// The highest duck step (`audio.md` section 4.4: 0 to 6).
pub const MOST_DUCK: u8 = 6;
/// The six links of packet X/27/0 (EN 300 706 clause 9.6.1).
pub const LINKS: usize = neoteletext_formats::page::FASTEXT_LINKS;
/// A row's character codes: 40 (EN 300 706 clause 9.4).
pub const COLUMNS: usize = neoteletext_core::consts::COLUMNS;
/// The rows a page carries: 0, the header, to 24, the Fastext prompts.
pub const ROWS: usize = neoteletext_core::consts::ROWS;
/// The values a packed line of X/27 designation codes 1 to 3 carries:
/// thirty-nine of four bits (the engine's `RawBody::Nibbles`).
pub const NIBBLES: usize = 39;

// The bit widths of the standard's fields, as JSON Schema bounds.
/// A triplet's address: 6 bits (EN 300 706 clause 12.3.1).
const TRIPLET_ADDRESS_MAX: u8 = 63;
/// A triplet's mode: 5 bits.
const TRIPLET_MODE_MAX: u8 = 31;
/// A triplet's data: 7 bits.
const TRIPLET_DATA_MAX: u8 = 127;
/// A designation code: 4 bits (clause 9.4.2).
const DESIGNATION_MAX: u8 = 15;
/// The region of the default character set: bits 3 to 6 of its
/// designation, 4 bits (clause 9.4.2.2, table 32).
const REGION_MAX: u8 = 15;
/// The national option, C12 to C14: 3 bits (clause 9.3.1.3, table 32).
const NATIONAL_OPTION_MAX: u8 = 7;
/// The last row, 24.
const LAST_ROW: u8 = 24;
/// The last column, 39.
const LAST_COLUMN: u8 = 39;
/// The last audio channel, 3 (`audio.md`).
const LAST_AUDIO_CHANNEL: u8 = 3;
/// The highest gain, 100.
const MOST_GAIN: u8 = neoteletext_audio::MAX_GAIN;
/// The last mouse button: 2, the right.
const LAST_BUTTON: u8 = 2;
/// The longest Opus packet the profile's constant rate allows: 40 bytes
/// (decision 21; the engine's E.123).
pub const OPUS_MOST_PACKET_BYTES: u16 = 40;

/// A connection, which the server numbers, from 1, and never reuses
/// while it runs.
pub type ConnectionId = u64;
/// A session, which outlives its connections (requirement 5.5).
pub type SessionId = u64;
/// A selection's number on its connection, rising from 1 (DEC-1759 lesson
/// 13). A program's answer carries it, so an answer that comes after the
/// viewer moved on cannot replace the page on screen. The kits copy it.
pub type RequestNumber = u64;

// ---------------------------------------------------------------- scalars

/// A page number: three hexadecimal digits in capitals, the magazine 1 to
/// 8 first (EN 300 706 clause 7.1.2), such as `"100"` or `"1A0"`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct PageNumber(pub u16);

impl PageNumber {
    /// The magazine, 1 to 8.
    #[must_use]
    pub const fn magazine(self) -> u8 {
        self.0.to_be_bytes()[0]
    }

    /// Whether the standard reserves it, never a page: mFD, mFE and mFF
    /// in every magazine, 1BE and 1F0 (EN 300 706 Annex A.10).
    #[must_use]
    pub fn is_reserved(self) -> bool {
        let [magazine, page] = self.0.to_be_bytes();
        limits::is_reserved_page(magazine, page)
    }
}

impl Serialize for PageNumber {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:03X}", self.0))
    }
}

/// Reads hexadecimal digits in capitals, exactly `digits` of them.
fn upper_hex(text: &str, digits: usize) -> Option<u16> {
    if text.len() != digits || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)) {
        return None;
    }
    u16::from_str_radix(text, 16).ok()
}

struct PageVisitor;

impl Visitor<'_> for PageVisitor {
    type Value = PageNumber;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a page: three hexadecimal digits in capitals, magazine 1 to 8 first, such as \"100\"")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<PageNumber, E> {
        upper_hex(text, 3)
            .filter(|&number| (limits::FIRST_PAGE_ADDRESS..=limits::LAST_PAGE_ADDRESS).contains(&number))
            .map(PageNumber)
            .ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
    }
}

impl<'de> Deserialize<'de> for PageNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(PageVisitor)
    }
}

impl JsonSchema for PageNumber {
    fn schema_name() -> Cow<'static, str> {
        "PageNumber".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A page number: three hexadecimal digits in capitals, the magazine 1 to 8 first (EN 300 706 clause 7.1.2).",
            "type": "string",
            "pattern": "^[1-8][0-9A-F]{2}$"
        })
    }
}

/// A sub-code: four hexadecimal digits in capitals, 0000 to 3F7F, with no
/// bit set outside the thirteen a sub-code has (EN 300 706 clause
/// 9.3.1.2). 3F7F means any sub-page.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SubCode(pub u16);

/// "Any sub-page", the page chosen and none of its sub-pages (EN 300 706
/// clause 9.6.1).
pub const ANY_SUBPAGE: SubCode = SubCode(limits::SUBCODE_ANY);

impl Serialize for SubCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:04X}", self.0))
    }
}

struct SubCodeVisitor;

impl Visitor<'_> for SubCodeVisitor {
    type Value = SubCode;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a sub-code: four hexadecimal digits in capitals, 0000 to 3F7F, such as \"0001\"")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<SubCode, E> {
        upper_hex(text, limits::SUBCODE_DIGITS)
            .filter(|&value| limits::is_sub_code(value))
            .map(SubCode)
            .ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
    }
}

impl<'de> Deserialize<'de> for SubCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(SubCodeVisitor)
    }
}

impl JsonSchema for SubCode {
    fn schema_name() -> Cow<'static, str> {
        "SubCode".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A sub-code: four hexadecimal digits in capitals, 0000 to 3F7F, no bit outside the thirteen (EN 300 706 clause 9.3.1.2); 3F7F means any sub-page.",
            "type": "string",
            "pattern": "^[0-3][0-9A-F][0-7][0-9A-F]$"
        })
    }
}

/// A key's name: a W3C `KeyboardEvent.key` name from the engine's table
/// (`neoteletext_input::keys`, DOC-1936), or the one
/// character of a character key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(transparent)]
pub struct KeyName(pub String);

/// Every key name the engine's table has, in its order.
fn key_names() -> Vec<&'static str> {
    let one_byte = (0..keys::FIRST_NAMED).filter_map(keys::name);
    let later = (keys::FIRST_NAMED..).map_while(keys::name);
    let mut names: Vec<&'static str> = one_byte.chain(later).filter(|name| name.chars().count() > 1).collect();
    // What the server writes for a code the table does not name.
    names.push("Unidentified");
    names.sort_unstable();
    names.dedup();
    names
}

impl JsonSchema for KeyName {
    fn schema_name() -> Cow<'static, str> {
        "KeyName".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A W3C KeyboardEvent.key name, from the engine's table (DOC-1936), or the one character of a character key.",
            "anyOf": [
                { "type": "string", "enum": key_names() },
                { "type": "string", "minLength": 1, "maxLength": 1 }
            ]
        })
    }
}

/// Bytes as exact base64 (RFC 4648 section 4, padded, no whitespace),
/// checked where it is used for its decoded length ([`check`]).
pub type Base64 = String;

/// Who a message is for: every open connection of the channel, or the
/// connections named. A connection named that has closed is passed over
/// (REQ-1473).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Connections {
    /// `"all"`: every connection open.
    All(Everyone),
    /// The connections named, at most [`MOST_NAMED_CONNECTIONS`].
    Listed(#[schemars(length(min = 1, max = MOST_NAMED_CONNECTIONS), inner(range(min = 1, max = SAFE_INTEGER_LIMIT)))] Vec<ConnectionId>),
}

/// `"all"`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Everyone {
    /// Every open connection.
    All,
}

// ------------------------------------------------------- server to program

/// A message from the server to a program.
#[derive(Clone, PartialEq, Debug, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The program is to serve its channel: sent once, when it starts.
    Start(Start),
    /// A viewer's connection opened (REQ-1471); answer with `reply`.
    Connected(Connected),
    /// A connection selected a page or a sub-page (REQ-1472); answer with
    /// `page`.
    Selected(Selected),
    /// Input on a page (DOC-1969).
    Input(InputMessage),
    /// A connection's selection left a page, or the connection closed.
    Left(Left),
    /// A session ended, five minutes after its last connection closed.
    Closed(Closed),
    /// The server's and the channel's figures, every ten seconds.
    Stats(Box<Stats>),
    /// A message the program sent was refused (REQ-1475).
    Refused(Refused),
    /// A web visitor or a search engine is at a page's address: answer with
    /// `description` (REQ-1728).
    Describe(Describe),
    /// The answer to `measure` (REQ-2216).
    Measured(Measured),
}

impl ServerMessage {
    /// The connection the message is about, if it is about one.
    #[must_use]
    pub const fn connection(&self) -> Option<ConnectionId> {
        match self {
            Self::Connected(Connected { connection, .. })
            | Self::Selected(Selected { connection, .. })
            | Self::Input(InputMessage { connection, .. })
            | Self::Left(Left { connection, .. }) => Some(*connection),
            Self::Start(_) | Self::Closed(_) | Self::Stats(_) | Self::Refused(_) | Self::Describe(_) | Self::Measured(_) => None,
        }
    }
}

/// `describe`: the page number and nothing else. It carries no connection,
/// session, cookie or query, so a description cannot hold one viewer's data
/// (REQ-1728 item 7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Describe {
    /// The page.
    pub page: PageNumber,
}

/// `start`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Start {
    /// The protocol's version.
    #[schemars(extend("const" = PROTOCOL))]
    pub protocol: u32,
    /// The time, in milliseconds since the Unix epoch (UTC).
    pub epoch_ms: u64,
    /// The standard's maxima, from the engine's `neoteletext_core::limits`.
    pub standard: Standard,
    /// The server's own bounds.
    pub budgets: Budgets,
    /// The languages a page or text run may name: the engine's table, in
    /// its order (REQ-2215, REQ-2181).
    pub languages: Vec<LanguageEntry>,
}

/// One language a page or text run may name (REQ-2215).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LanguageEntry {
    /// The BCP 47 tag a page or run names it by.
    pub tag: LanguageTag,
    /// Its English name.
    pub name: String,
    /// Which way it is written.
    pub direction: Direction,
}

/// Which way a language is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Left to right.
    Ltr,
    /// Right to left: the server puts its text in visual order.
    Rtl,
}

/// The server's closed list of languages, from the engine (REQ-2215).
#[must_use]
pub fn languages() -> Vec<LanguageEntry> {
    neoteletext_formats::write_text::languages()
        .into_iter()
        .map(|language| LanguageEntry {
            tag: LanguageTag(language.tag),
            name: language.name.to_string(),
            direction: match language.direction {
                neoteletext_formats::write_text::Direction::Ltr => Direction::Ltr,
                neoteletext_formats::write_text::Direction::Rtl => Direction::Rtl,
            },
        })
        .collect()
}

/// Every tag of [`languages`].
#[must_use]
pub fn language_tags() -> Vec<String> {
    languages().into_iter().map(|language| language.tag.0).collect()
}

/// The standard's maxima (REQ-1031, `STANDARD-LIMITS.md` section 3).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Standard {
    /// Magazines 1 to 8 (EN 300 706 clause 7.1.2).
    pub magazines: u64,
    /// Page numbers that may carry a page: 00 to FF in each magazine, less
    /// those the standard reserves (Annex A.10).
    pub pages: u64,
    /// Pages a magazine may have: 253.
    pub pages_per_magazine: u64,
    /// Pages magazine 1 may have, 1BE and 1F0 reserved too: 251.
    pub pages_in_magazine_1: u64,
    /// Those a viewer can key, 100 to 899.
    pub keyable_pages: u64,
    /// Sub-pages a page may hold, 0001 to 3F7E.
    pub subpages: u64,
    /// Sub-codes a viewer can key, 0001 to 3979.
    pub decimal_subpages: u64,
    /// Sub-pages automatic numbering can number: those above the set's last
    /// decimal sub-code (the engine's `SUBCODES_ABOVE_SET`, EN 300 706 Annex
    /// A.1), so a kit numbering a page given whole stops there (TODO-2056).
    pub numbered_subpages: u64,
    /// The longest link target, in bytes (the engine's `LINK_TARGET_BYTES`).
    pub link_target_bytes: u64,
}

/// The server's own bounds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// The longest line, in bytes.
    pub line_bytes: u64,
    /// The longest title, magazine name or section, in characters.
    pub title_characters: u64,
    /// The longest author's log line, in bytes of UTF-8.
    pub author_log_bytes: u64,
    /// The longest operator's log line, in characters.
    pub log_characters: u64,
    /// The connections one message may name.
    pub named_connections: u64,
    /// How long the server waits for an answer to `connected` or
    /// `selected`, in milliseconds (REQ-1481).
    pub reply_limit_ms: u64,
    /// The longest text of a `description`, in characters.
    pub description_text_characters: u64,
    /// How long the server waits for a `description`, in milliseconds.
    pub description_limit_ms: u64,
}

/// `connected` (REQ-1471).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connected {
    /// The connection: the program addresses it by this while it is open.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The viewer's session, from its signed cookie (requirement 5.5).
    #[schemars(range(max = SAFE_INTEGER_LIMIT))]
    pub session: SessionId,
    /// The number of this selection: `reply` carries it back.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: RequestNumber,
    /// The cookies the connection opened with, within [`MOST_COOKIES`],
    /// without the server's own, whose names begin `_`.
    #[schemars(length(max = MOST_COOKIES))]
    pub cookies: Vec<CookiePair>,
    /// What the connection selected at its start, or none: the channel's
    /// initial page. A connection is one browser tab with one selection
    /// (DEC-1527).
    pub selection: Option<Selection>,
    /// `selection`'s place, as an address names it: `302` or `302/0002`.
    pub path: Option<String>,
    /// The query of the address, without its `?`, or none (DEC-1730). It goes
    /// with the selection (see [`Selected`]).
    #[schemars(length(min = 1, max = PARAMETERS_BYTES), regex(pattern = PARAMETERS_PATTERN))]
    pub query: Option<String>,
}

/// A cookie as a request carries it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CookiePair {
    /// Its name.
    #[schemars(length(min = 1, max = COOKIE_BYTES))]
    pub name: String,
    /// Its value.
    #[schemars(length(max = COOKIE_BYTES))]
    pub value: String,
}

/// A page and sub-code selected.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// The page.
    pub page: PageNumber,
    /// The sub-code, or 3F7F for the page and none of its sub-pages.
    pub sub: SubCode,
}

impl Selection {
    /// The place an address names, the text SELECT carries: `302`, or
    /// `302/0002` for one sub-page.
    #[must_use]
    pub fn path(self) -> String {
        if self.sub.0 == limits::SUBCODE_ANY { format!("{:03X}", self.page.0) } else { format!("{:03X}/{:04X}", self.page.0, self.sub.0) }
    }
}

/// `selected` (REQ-1472).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selected {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The number of this selection: the answer carries it back.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: RequestNumber,
    /// The place selected, as an address names it: `302` or `302/0002`.
    pub path: String,
    /// The page.
    pub page: PageNumber,
    /// The sub-code, or 3F7F for the page and none of its sub-pages.
    pub sub: SubCode,
    /// The query of the address selected, without its `?`, exactly as
    /// written, or none (DEC-1730). It belongs to this one selection: the
    /// next one that has none says so here. The server does not interpret
    /// it: it is untrusted text from the viewer.
    #[schemars(length(min = 1, max = PARAMETERS_BYTES), regex(pattern = PARAMETERS_PATTERN))]
    pub query: Option<String>,
}

/// `input`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputMessage {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page on screen when it happened: the event's stamp, which is the
    /// connection's selection, or for a release the page that held it
    /// (REQ-1557). Focus and raw text carry no stamp, and are for the
    /// selection as it stands.
    pub page: PageNumber,
    /// The sub-code on screen when it happened.
    pub sub: SubCode,
    /// A release the page lost: the key or button did not come up, the page
    /// lost the input or the connection went (REQ-207). A game can tell
    /// "the player let go" from "the player left".
    pub lost: bool,
    /// What happened.
    pub event: InputEvent,
}

/// An input event (DOC-1969), by `kind`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputEvent {
    /// A key pressed, repeating or released: every one is sent.
    Key {
        /// What it did.
        action: KeyAction,
        /// Which key.
        key: KeyName,
        /// The modifier keys held, a bit each as DOC-1935 numbers them.
        mods: u8,
        /// The text the key produced, if any.
        #[schemars(length(min = 1, max = neoteletext_input::MAX_KEY_TEXT))]
        text: Option<String>,
        /// The United States layout's character of the same key, when it
        /// differs from `key`.
        #[schemars(length(min = 1, max = 1))]
        base: Option<String>,
    },
    /// A mouse button pressed or released, or the pointer moved, over the
    /// page.
    Mouse {
        /// What it did.
        action: MouseAction,
        /// The button pressed or released, 0 left, 1 middle, 2 right; none
        /// for a move.
        #[schemars(range(max = LAST_BUTTON))]
        button: Option<u8>,
        /// The buttons held during a move; none for a press or release.
        buttons: Option<Buttons>,
        /// The modifier keys held, a bit each.
        mods: u8,
        /// The cell's column, 0 to 39.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The cell's row, 0 to 24.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
        /// The dot of the page image across, when the receiver knows it.
        px: Option<u32>,
        /// The dot down.
        py: Option<u32>,
    },
    /// The wheel turned over a cell, in 1/120ths of a line, positive right
    /// and down.
    Wheel {
        /// Across.
        dx: i16,
        /// Down.
        dy: i16,
        /// The modifier keys held, a bit each.
        mods: u8,
        /// The cell's column.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The cell's row.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
        /// The dot of the page image across, when the receiver knows it.
        px: Option<u32>,
        /// The dot down.
        py: Option<u32>,
    },
    /// A contact after the first (the first goes as `mouse`), down, up,
    /// moved or gone, over the page (REQ-1366, DOC-1938). Sent only on a
    /// connection that agreed TOUCH.
    Touch {
        /// What it did: `press` down, `release` up, `move`, or `gone` when
        /// the receiver lost it.
        action: MouseAction,
        /// The contact's number, the same from its beginning to its end.
        #[schemars(range(min = 1, max = neoteletext_input::MAX_TOUCH_ID))]
        contact: u8,
        /// The cell's column, 0 to 39.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The cell's row, 0 to 24.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
        /// The dot of the page image across, when the receiver knows it.
        px: Option<u32>,
        /// The dot down.
        py: Option<u32>,
    },
    /// On a page drawn in ReGIS, what lay under the pointer of the mouse,
    /// touch or wheel event before this one (PICK, DEC-6335): the receiver
    /// looks it up in the picture it is showing.
    Pick {
        /// The numbered drawing picked, or 0 for none.
        drawing: u16,
        /// The place within that drawing, from its origin; 0 for none.
        x: i32,
        /// See `x`.
        y: i32,
        /// The place in the picture, in its pixels from the top left.
        picture_x: u32,
        /// See `picture_x`.
        picture_y: u32,
        /// The commits the receiver had applied to the picture on screen.
        commits: u32,
    },
    /// On a page drawn in ReGIS, the pointer came onto a detectable
    /// numbered drawing, or went off it (the engine's REQ-6369).
    DrawingHover {
        /// The numbered drawing, 1 to 65,535.
        #[schemars(range(min = 1))]
        drawing: u16,
        /// It went off the drawing; false when it came onto it.
        leave: bool,
        /// The commits the receiver had applied to the picture on screen.
        commits: u32,
    },
    /// The pointer came onto one of the page's controls, or went off it.
    Hover {
        /// Which.
        action: HoverAction,
        /// The control's first column.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The control's top row.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
    },
    /// Text that came from no key: an input method's, or a phone's.
    Text {
        /// Whole characters.
        #[schemars(length(min = 1, max = neoteletext_input::MAX_COMPOSE))]
        text: String,
    },
    /// An input method's text as it stands while it is being composed: what
    /// it commits comes as `text`.
    Compose {
        /// Where the caret is, in code points from the text's start.
        #[schemars(range(max = neoteletext_input::MAX_COMPOSE))]
        cursor: u32,
        /// The text; empty when composing ended.
        #[schemars(length(max = neoteletext_input::MAX_COMPOSE))]
        text: String,
    },
    /// A paste: data, never keystrokes.
    Paste {
        /// What was pasted.
        #[schemars(length(min = 1, max = neoteletext_input::MAX_PASTE))]
        text: String,
    },
    /// The keys came to the page.
    Focus,
    /// The keys left it: let go of anything held.
    Blur,
}

/// What a key did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    /// Pressed.
    Press,
    /// Repeating while held.
    Repeat,
    /// Released.
    Release,
}

/// What the mouse did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MouseAction {
    /// A button pressed.
    Press,
    /// A button released.
    Release,
    /// The pointer moved.
    Move,
    /// The pointer left the page, or the receiver lost it, so that a
    /// channel can stop showing what follows it (REQ-1355, REQ-1367). A
    /// button held is released first, marked `lost`; `gone` itself names no
    /// button.
    Gone,
}

/// The pointer came onto a control or went off it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HoverAction {
    /// Came onto it.
    Enter,
    /// Went off it.
    Leave,
}

/// The mouse buttons held.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Buttons {
    /// The left button.
    pub left: bool,
    /// The middle button.
    pub middle: bool,
    /// The right button.
    pub right: bool,
}

/// `left`: the connection's selection left a page, or the connection
/// closed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Left {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The page it left.
    pub page: PageNumber,
}

/// `closed`: the session ended; the program may forget it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Closed {
    /// The session.
    #[schemars(range(max = SAFE_INTEGER_LIMIT))]
    pub session: SessionId,
}

/// `stats`: the figures of DOC-1968, every one named and unsigned
/// (TODO-1525 item 1). Counts only: nothing that identifies a viewer.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    /// The server's figures.
    pub server: ServerFigures,
    /// The channel's figures.
    pub channel: ChannelCounts,
}

/// The server's figures since it started.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerFigures {
    /// Seconds since it started.
    pub uptime_s: u64,
    /// WebSocket connections open now.
    pub connections: u64,
    /// The most open at once.
    pub connections_peak: u64,
    /// Connections made.
    pub connections_total: u64,
    /// WebSocket messages sent.
    pub messages_sent: u64,
    /// Bytes sent in them.
    pub bytes_sent: u64,
    /// HTTP requests answered.
    pub requests: u64,
    /// Programs started again after they ended.
    pub restarts: u64,
    /// Its resident memory in kilobytes, or none where the system does not
    /// say.
    pub memory_kb: Option<u64>,
    /// Its channels.
    pub channels: u64,
    /// Each channel's viewers now, by the channel's folder.
    pub viewers: BTreeMap<String, u64>,
    /// Its totals since its state was first kept.
    pub all_time: ServerTotals,
}

/// The server's figures accumulated across restarts.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerTotals {
    /// Connections made.
    pub connections_total: u64,
    /// The most open at once.
    pub connections_peak: u64,
    /// WebSocket messages sent.
    pub messages_sent: u64,
    /// Bytes sent in them.
    pub bytes_sent: u64,
    /// HTTP requests answered.
    pub requests: u64,
    /// Programs started again after they ended.
    pub restarts: u64,
}

/// A channel's figures since the server started.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelCounts {
    /// Viewers connected now.
    pub viewers: u64,
    /// The most connected at once.
    pub viewers_peak: u64,
    /// Viewers that connected.
    pub viewers_total: u64,
    /// Pages opened.
    pub opens: u64,
    /// Input events from viewers.
    pub inputs: u64,
    /// Key presses among them.
    pub keys: u64,
    /// Sub-pages sent to connections, each copy counted.
    pub pages_sent: u64,
    /// Sub-pages the program sent to connections it named that did not
    /// show them (REQ-1473).
    pub subpages_not_forwarded: u64,
    /// Lines from the program refused.
    pub refused: u64,
    /// Packets not sent because they failed the stream's check.
    pub invalid_packets: u64,
    /// Input dropped for a page no longer selected.
    pub stale_input: u64,
    /// Answers dropped for a selection already replaced.
    pub stale_answers: u64,
    /// Viewers the program redirected.
    pub redirects: u64,
    /// Audio records sent.
    pub sound_records: u64,
    /// Lines sent to the program.
    pub lines_to_application: u64,
    /// Lines from it.
    pub lines_from_application: u64,
    /// Lines for it dropped because it fell behind.
    pub lines_dropped: u64,
    /// Sessions waiting for their viewer to come back.
    pub sessions_waiting: u64,
    /// Sessions a viewer came back to.
    pub sessions_resumed: u64,
    /// The pages opened most, the most opened first, at most ten.
    pub most_opened: Vec<PageOpens>,
    /// Its totals since the server's state was first kept.
    pub all_time: ChannelTotals,
}

/// A page and how often it was opened.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageOpens {
    /// The page.
    pub page: PageNumber,
    /// Its opens.
    pub opens: u64,
}

/// A channel's figures accumulated across restarts.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelTotals {
    /// Viewers that connected.
    pub viewers_total: u64,
    /// The most connected at once.
    pub viewers_peak: u64,
    /// Pages opened.
    pub opens: u64,
    /// Input events from viewers.
    pub inputs: u64,
    /// Key presses among them.
    pub keys: u64,
    /// Sub-pages sent to connections.
    pub pages_sent: u64,
    /// Sub-pages sent to connections that did not show them.
    pub subpages_not_forwarded: u64,
    /// Lines from the program refused.
    pub refused: u64,
    /// Input dropped for a page no longer selected.
    pub stale_input: u64,
    /// Answers dropped for a selection already replaced.
    pub stale_answers: u64,
    /// Viewers the program redirected.
    pub redirects: u64,
    /// Audio records sent.
    pub sound_records: u64,
    /// Lines sent to the program.
    pub lines_to_application: u64,
    /// Lines from it.
    pub lines_from_application: u64,
    /// Lines for it dropped.
    pub lines_dropped: u64,
    /// Sessions a viewer came back to.
    pub sessions_resumed: u64,
    /// Each page's opens, by its number.
    pub pages_opened: BTreeMap<String, u64>,
}

/// `refused` (REQ-1475).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Refused {
    /// The type of the message refused, or none when it had none the
    /// protocol knows.
    pub message: Option<MessageType>,
    /// The path to the first fault, such as `magazines[2].pages[14].title`;
    /// empty for the message as a whole.
    pub path: String,
    /// The rule broken.
    pub rule: Rule,
    /// The fault in words.
    pub detail: String,
    /// The request number of the message refused, when it carried one
    /// (`reply`, `page`, `subpages`, `redirect`, `denied`, `measure`), and
    /// none otherwise (REQ-2216 item 3).
    #[schemars(range(max = SAFE_INTEGER_LIMIT))]
    pub request: Option<u64>,
}

/// `measured`: what the server's encoding of a page `measure` sent costs,
/// sent to no viewer (REQ-2216 item 2).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Measured {
    /// `measure`'s request number.
    #[schemars(range(max = u32::MAX))]
    pub request: u32,
    /// The X/26 triplets the page uses, the program's and the encoder's
    /// together, its Termination Marker included.
    #[schemars(range(max = TRIPLET_LIMIT))]
    pub triplets: u16,
    /// The most it may use: 208.
    #[schemars(extend("const" = TRIPLET_LIMIT))]
    pub limit: u16,
    /// The page's designation.
    pub designation: PageDesignation,
    /// Each text run's cost, in the order of the page's `text`.
    #[schemars(length(max = MOST_TEXT_RUNS))]
    pub runs: Vec<RunCost>,
}

/// A page's default G0 and G2 designation and its second G0 designation
/// (EN 300 706 Tables 32 and 33), as the header's C12 to C14 and X/28/0
/// Format 1 carry them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageDesignation {
    /// The default G0 and G2 designation, a Table 32 value: the region in
    /// bits 4 to 7, the national option in bits 1 to 3.
    #[schemars(range(max = TRIPLET_DATA_MAX))]
    pub default: u8,
    /// The second G0 designation, a Table 33 value, or none.
    #[schemars(range(max = TRIPLET_DATA_MAX))]
    pub second: Option<u8>,
}

/// What one text run cost.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunCost {
    /// The cells it takes on its row.
    #[schemars(range(min = 1, max = TEXT_RUN_CHARACTERS))]
    pub cells: u8,
    /// The triplets it added, its share of row addresses and the
    /// Termination Marker included.
    #[schemars(range(max = TRIPLET_LIMIT))]
    pub triplets: u16,
}

/// The types of message a program sends.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "each names a message of ProgramMessage")]
pub enum MessageType {
    Reply,
    Page,
    Subpages,
    Index,
    Redirect,
    Sound,
    Cookie,
    AuthorLog,
    Log,
    Packets,
    Denied,
    Description,
    Measure,
    Regis,
    RegisLinks,
    RegisCursor,
    Naplps,
}

/// Why a message was refused: a closed list (REQ-1475).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Not one JSON object on one line.
    Syntax,
    /// The line is longer than `budgets.line_bytes`.
    LineTooLong,
    /// A field the message's type does not have.
    UnknownField,
    /// A field the message's type needs is missing.
    MissingField,
    /// A field is given twice.
    DuplicateField,
    /// A value of the wrong JSON type, or not in its format.
    WrongType,
    /// A value outside its closed list, a message's `type` among them.
    NotListed,
    /// A number outside its range.
    OutOfRange,
    /// Text or a list longer than its bound.
    TooLong,
    /// Text or a list shorter than its bound, or empty.
    TooShort,
    /// A list not in ascending order.
    NotAscending,
    /// The same number or code twice.
    Duplicate,
    /// A page number the standard reserves.
    Reserved,
    /// A page listed under another magazine.
    WrongMagazine,
    /// A character text may not have: a control or a bidirectional
    /// formatting character.
    ForbiddenCharacter,
    /// Text not in Unicode normalisation form C.
    NotNfc,
    /// Not exact base64 (RFC 4648 section 4, padded, no whitespace).
    NotBase64,
    /// Bytes of the wrong length once decoded.
    WrongLength,
    /// A selected page not in the index.
    NotInIndex,
    /// A connection that is not open.
    UnknownConnection,
    /// An answer to nothing the server asked: a second `reply`.
    NotAwaited,
    /// A sound outside the profile the receivers decode.
    OutsideProfile,
    /// A raw packet the stream may not carry (REQ-1462).
    InvalidPacket,
    /// ReGIS whose commit a receiver would refuse (DEC-6335).
    InvalidRegis,
    /// Drawing links a receiver would refuse (REQ-6372).
    InvalidDrawingLinks,
    /// A selection's number given with other than one connection: a number
    /// belongs to one connection.
    OneConnection,
    /// A page other than the one the message is about.
    WrongPage,
    /// A page names both a `language` and a `character_set` (REQ-2214
    /// item 2).
    LanguageAndCharacterSet,
    /// A language not in `start.languages`, or one no set writes at the
    /// page's level (REQ-2215).
    UnknownLanguage,
    /// A text run's cell at which the page's own triplets select a
    /// character (REQ-2214 item 6).
    TextCellTaken,
    /// The page's own codes or triplets change the character set in force
    /// before or within a text run on its row: an ESC, or a triplet of
    /// column mode 01000 or 01010.
    TextSetChanged,
    /// The page's triplets, its own and its text's, are more than 208
    /// (REQ-2214 item 8).
    TripletsOverBudget,
    /// A character of a text run the page cannot show in its language.
    TextUnshown,
    /// A text run takes a cell another takes.
    TextOverlap,
    /// A text run does not end by column 39.
    TextOutOfColumns,
    /// The page's own enhancement data holds objects, which text runs
    /// cannot be merged into.
    TextAndObjects,
}

// ------------------------------------------------------- program to server

/// A message from a program to the server.
// Read by the reader, which follows `type` to the message's own type
// (`read.rs`).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgramMessage {
    /// The answer to `connected` (REQ-1471).
    Reply(Reply),
    /// One sub-page, to the connections named (REQ-1472, REQ-1473).
    Page(Box<PageMessage>),
    /// A page's sub-page list (REQ-1503).
    Subpages(SubPages),
    /// A change to the magazines and their pages.
    Index(Index),
    /// Sends a connection to a page.
    Redirect(Redirect),
    /// A sound, its data, or a command.
    Sound(Sound),
    /// Sets or clears one of the program's cookies for a connection.
    Cookie(Cookie),
    /// A line for the developer console of the viewer's browser (the
    /// engine's REQ-INP-026).
    AuthorLog(AuthorLog),
    /// A line for the server operator's log.
    Log(Log),
    /// Raw teletext packets (DOC-1970).
    Packets(Packets),
    /// A selection the program will not answer with a page.
    Denied(Denied),
    /// The answer to `describe` (REQ-1728).
    Description(Description),
    /// Asks what a page's encoding costs, sending it to no viewer
    /// (REQ-2216).
    Measure(Box<Measure>),
    /// Bytes of a sub-page's ReGIS, drawn by a receiver's ReGIS renderer in
    /// place of the sub-page's teletext page (DEC-6335).
    Regis(Regis),
    /// The numbered drawings of a sub-page's ReGIS picture that are links
    /// (REQ-6372).
    RegisLinks(RegisLinks),
    /// The numbered drawing that is a ReGIS sub-page's cursor (REQ-6372).
    RegisCursor(RegisCursor),
    /// Bytes of a sub-page's NAPLPS, drawn by a receiver's NAPLPS renderer
    /// in place of the sub-page's teletext page (the NAPLPS renderer's
    /// REQ-6408 item 3).
    Naplps(Naplps),
}

impl ProgramMessage {
    /// Its type.
    #[must_use]
    pub const fn kind(&self) -> MessageType {
        match self {
            Self::Reply(_) => MessageType::Reply,
            Self::Page(_) => MessageType::Page,
            Self::Subpages(_) => MessageType::Subpages,
            Self::Index(_) => MessageType::Index,
            Self::Redirect(_) => MessageType::Redirect,
            Self::Sound(_) => MessageType::Sound,
            Self::Cookie(_) => MessageType::Cookie,
            Self::AuthorLog(_) => MessageType::AuthorLog,
            Self::Log(_) => MessageType::Log,
            Self::Packets(_) => MessageType::Packets,
            Self::Denied(_) => MessageType::Denied,
            Self::Description(_) => MessageType::Description,
            Self::Measure(_) => MessageType::Measure,
            Self::Regis(_) => MessageType::Regis,
            Self::RegisLinks(_) => MessageType::RegisLinks,
            Self::RegisCursor(_) => MessageType::RegisCursor,
            Self::Naplps(_) => MessageType::Naplps,
        }
    }

    /// Whether it answers something the server asked, which only a program
    /// reading its lines can do (requirement 8.4).
    #[must_use]
    pub const fn is_answer(&self) -> bool {
        matches!(self, Self::Reply(_) | Self::Denied(_) | Self::Description(_))
    }
}

/// `measure`: a page encoded as for sending and sent to no viewer; the
/// server answers `measured` or `refused` (REQ-2216 item 1).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Measure {
    /// A number the program chooses, which the answer carries back.
    #[schemars(range(max = u32::MAX))]
    pub request: u32,
    /// The page, as `page` carries one.
    pub page: Page,
}

/// `reply` (REQ-1471).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    /// The connection whose `connected` this answers.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// `connected`'s number. A reply to a selection already replaced still
    /// gives the index, and its sub-page is dropped.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: RequestNumber,
    /// The channel's magazines for this viewer (REQ-1477).
    #[schemars(length(max = limits::MAGAZINES))]
    pub magazines: Vec<Magazine>,
    /// The sub-page the connection's selection shows: the one it selected;
    /// for a page selected with 3F7F, the page's first sub-page in
    /// ascending sub-code order (0000 when the page has it); for none, the
    /// initial page's first. None when the program has nothing to show.
    pub selected: Option<Box<Page>>,
}

/// A magazine of the index (REQ-1477).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Magazine {
    /// Its number, 1 to 8.
    #[schemars(range(min = 1, max = limits::MAGAZINES))]
    pub magazine: u8,
    /// Its name, which a receiver shows.
    #[schemars(length(max = TITLE_CHARACTERS))]
    pub name: String,
    /// Its pages, in ascending order: at most 253, 251 in magazine 1.
    #[schemars(length(min = 1, max = limits::PAGE_NUMBERS_PER_MAGAZINE))]
    pub pages: Vec<IndexEntry>,
}

/// A page of the index.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    /// Its number, in its magazine, never a reserved one.
    pub page: PageNumber,
    /// Its title, shown in a receiver's lists and overview.
    #[schemars(length(max = TITLE_CHARACTERS))]
    pub title: String,
    /// The heading a receiver shows it under, or none.
    #[schemars(length(max = TITLE_CHARACTERS))]
    pub section: Option<String>,
    /// How many sub-pages it has, 1 to 8,190.
    #[schemars(range(min = 1, max = limits::MAX_SUBPAGES))]
    pub subpages: u16,
}

/// `page`: one sub-page. It reaches only those of the connections named
/// whose selection shows it: the connections that selected it, and those
/// that selected its page with 3F7F and are waiting for its first
/// sub-page, which then becomes their selection (DEC-1500, DEC-1526).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The selection this answers, naming one connection; none for a change
    /// to what connections already show, which waits for no answer.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: Option<RequestNumber>,
    /// The sub-page.
    pub page: Page,
}

/// `subpages`: a page's list of sub-pages (DEC-1500 item 3, REQ-1503),
/// sent with the answer to a page selection and whenever the list changes;
/// relayed as the SUBPAGES record to the connections named whose
/// selection shows the page and that do not hold this version.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubPages {
    /// Who it is for.
    pub connections: Connections,
    /// The selection this answers, naming one connection; none for a change
    /// to the list.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: Option<RequestNumber>,
    /// The page.
    pub page: PageNumber,
    /// The list's version, which the program changes whenever the list
    /// changes.
    pub version: u16,
    /// The sub-pages in the page's order, each sub-code at most once.
    #[schemars(length(min = 1, max = limits::MAX_SUBPAGES))]
    pub subpages: Vec<SubPageEntry>,
}

/// One entry of a page's sub-page list.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubPageEntry {
    /// Its sub-code, 0000 to 3F7E.
    pub sub: SubCode,
    /// Its title, at most a row's 40 characters, or none.
    #[schemars(length(max = SUBPAGE_TITLE_CHARACTERS))]
    pub title: Option<String>,
}

/// The longest sub-page title: a teletext row (REQ-1503).
pub const SUBPAGE_TITLE_CHARACTERS: usize = COLUMNS;

/// `index`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Index {
    /// Who it is for.
    pub connections: Connections,
    /// The magazines, as a reply gives them.
    #[schemars(length(max = limits::MAGAZINES))]
    pub magazines: Vec<Magazine>,
}

/// `redirect`: sends the connection's viewer to a page of this channel,
/// which is itself a selection (DEC-1526 item 4), or to another channel's
/// address, which changes no selection (REQ-1688). It names `page` and `sub`
/// or `address`, never both (`check`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Redirect {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The selection it moves. A redirect for a selection already replaced
    /// is dropped: the viewer's own choice overrides it (DOC-1960).
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: RequestNumber,
    /// The page of this channel.
    #[serde(default)]
    pub page: Option<PageNumber>,
    /// The sub-code, or 3F7F for the page.
    #[serde(default)]
    pub sub: Option<SubCode>,
    /// The query the page keeps, without its `?`, or none: a redirect to a
    /// page is a new selection, and has only what it is given.
    #[serde(default)]
    #[schemars(length(min = 1, max = PARAMETERS_BYTES), regex(pattern = PARAMETERS_PATTERN))]
    pub query: Option<String>,
    /// Another channel's address, at most the engine's
    /// [`MAX_ADDRESS`](neoteletext_web::records::redirect::MAX_ADDRESS) bytes:
    /// the most a REDIRECT record carries.
    #[serde(default)]
    #[schemars(length(max = neoteletext_web::records::redirect::MAX_ADDRESS))]
    pub address: Option<String>,
}

/// `denied`: a selection ends with a status and no page (DOC-1961),
/// so the viewer is told at once and not at the deadline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Denied {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The selection it ends.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub request: RequestNumber,
    /// A real HTTP status number: 403, 404, 429 or 503.
    #[schemars(extend("enum" = DENIED_STATUSES))]
    pub status: u16,
}

/// `description`: the answer to `describe` (REQ-1728). A page is public only
/// when this says so, and everything it holds is then readable by anyone. A
/// page that is not public gives nothing but that, so that nothing of a
/// private page can be kept or sent.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Description {
    /// The page `describe` named.
    pub page: PageNumber,
    /// Whether the page may be shown to anyone on the web.
    pub public: bool,
    /// Its title, plain text, for a public page.
    #[serde(default)]
    #[schemars(length(min = 1, max = TITLE_CHARACTERS))]
    pub title: Option<String>,
    /// Its text, plain text on one line, for a public page.
    #[serde(default)]
    #[schemars(length(min = 1, max = DESCRIPTION_TEXT_CHARACTERS))]
    pub text: Option<String>,
    /// The sub-page its picture shows, for a public page.
    #[serde(default)]
    pub content: Option<Box<Page>>,
}

/// What DENIED carries (DOC-1961).
pub const DENIED_STATUSES: [u16; 4] = [
    neoteletext_web::records::denied::STATUS_FORBIDDEN,
    neoteletext_web::records::denied::STATUS_NOT_FOUND,
    neoteletext_web::records::denied::STATUS_TOO_MANY,
    neoteletext_web::records::denied::STATUS_UNAVAILABLE,
];

/// One sub-page of a page, as the engine's page model (`PageFile`), field
/// by field (REQ-1478). A sub-page travels alone, never with the others of
/// its page (DEC-1500, REQ-1501): a page of one sub-page is a page whose
/// list has one entry.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// The page's number, never a reserved one.
    pub page: PageNumber,
    /// The sub-page's sub-code, 0000 to 3F7E.
    pub sub: SubCode,
    /// Rows 0 to 24, each 40 character codes of seven bits as exact
    /// base64 of 40 bytes; none for a blank row. Row 0's columns 0 to 7
    /// are the receiver's (EN 300 706 clause 9.3.1.4).
    pub rows: [Option<Base64>; ROWS],
    /// The six links of packet X/27/0 (EN 300 706 clause 9.6.1): red,
    /// green, yellow, cyan, the fifth and the index; none for a null link.
    pub links: [Option<Link>; LINKS],
    /// Packet X/27/0's link control: row 24 is displayed.
    pub display_row_24: bool,
    /// The TTI Link Extension's links that packets cannot carry (the
    /// engine's Appendix E; REQ-1494), sent as the sub-page's LINKS record.
    pub link_extension: LinkExtension,
    /// The control bits C4 to C11 (clause 9.3.1.3).
    pub control: Control,
    /// The default character set by its region and national option; none
    /// with `language`, which sets it, or for the defaults (region none,
    /// national option 0).
    pub character_set: Option<CharacterSet>,
    /// The page's default language, a tag of `start.languages`: it sets the
    /// default G0 and G2 designation and is the language of each text run
    /// that names none (REQ-2214 item 2). Never with `character_set`.
    pub language: Option<LanguageTag>,
    /// The presentation level the page is written for; none for
    /// `neoteletext` (REQ-2214 item 3, REQ-2151).
    pub level: Option<TextLevel>,
    /// Unicode text the server writes over `rows` with the engine's encoder
    /// (REQ-2214 item 4), at most 64 runs; none for no text.
    #[schemars(length(max = MOST_TEXT_RUNS))]
    pub text: Option<Vec<TextRun>>,
    /// Packets X/26, at most one of each designation code.
    #[schemars(length(max = neoteletext_core::consts::DESIGNATION_CODES))]
    pub enhancements: Vec<Enhancement>,
    /// Packets X/27 of designation codes 1 to 15, X/28 and M/29.
    #[schemars(length(max = neoteletext_formats::page::RAW_PACKETS))]
    pub packets: Vec<PackedLine>,
    /// The input cursor (DOC-1954), or none.
    pub cursor: Option<Cursor>,
}

/// The presentation level a page's text is written for (REQ-2151).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize, JsonSchema)]
pub enum TextLevel {
    /// NeoTeletext's own receivers: Level 3.5 with NeoTeletext's own
    /// extensions, the row set triplet among them.
    #[default]
    #[serde(rename = "neoteletext")]
    NeoTeletext,
    /// Level 3.5 as EN 300 706 defines it: another set is reached by
    /// Modified Designation, one triplet a character.
    #[serde(rename = "3.5")]
    Level35,
}

/// A run of Unicode text at its place on the page (REQ-2214 item 4). It
/// takes a cell for each character it shows, from its column rightwards,
/// once combining marks join their letters, right-to-left text is in
/// visual order and Arabic letters take their joined forms.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextRun {
    /// Its row, 1 to 24.
    #[schemars(range(min = FIRST_TEXT_ROW, max = LAST_ROW))]
    pub row: u8,
    /// Its first column, 0 to 39.
    #[schemars(range(max = LAST_COLUMN))]
    pub column: u8,
    /// Its text: 1 to 40 characters in Unicode normalisation form C, no
    /// control or bidirectional formatting character.
    #[schemars(length(min = 1, max = TEXT_RUN_CHARACTERS))]
    pub text: String,
    /// Its language, a tag of `start.languages`; none for the page's, or
    /// Latin English when the page names none.
    pub language: Option<LanguageTag>,
    /// Whether the text came from outside the program: what a viewer typed,
    /// a feed's data (REQ-2150 item 3). A character of such a run that its
    /// language cannot show becomes the language's stand-in and is
    /// reported in the log; in any other run it refuses the page
    /// (`text_unshown`). False when absent (DEC-2326).
    #[serde(default)]
    pub outside: bool,
}

/// A character a run from outside the program showed as a stand-in,
/// because its language cannot show it (REQ-2150 item 2, DEC-2326): what
/// the server reports in its log, once a page message and character.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replaced {
    /// The page whose run it was.
    pub page: PageNumber,
    /// The character the run's language cannot show.
    pub character: char,
    /// What was shown in its place.
    pub stand_in: char,
    /// The run's language: its own, else the page's, else Latin English.
    pub language: String,
}

/// A language's BCP 47 tag, one of `start.languages` (REQ-2215); any other
/// is refused with `unknown_language`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageTag(pub String);

impl JsonSchema for LanguageTag {
    fn schema_name() -> Cow<'static, str> {
        "LanguageTag".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A language's BCP 47 tag, one of start.languages: the engine's table of languages (REQ-2215).",
            "type": "string",
            "enum": language_tags()
        })
    }
}

/// A link: a page and a sub-code.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// The page.
    pub page: PageNumber,
    /// The sub-code, or 3F7F for any.
    pub sub: SubCode,
}

/// A sub-page's Link Extension links (the engine's Appendix E; REQ-1478
/// version 3, REQ-1494).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkExtension {
    /// The keys' links, as TTI's `LN` lines: each key at most once.
    #[schemars(length(max = neoteletext_formats::page::LINK_LINES))]
    pub keys: Vec<KeyLink>,
    /// Page numbers on the page that lead elsewhere, as TTI's `LK` lines.
    #[schemars(length(max = neoteletext_formats::page::NUMBER_LINKS))]
    pub numbers: Vec<NumberLink>,
    /// Runs of text on the page that lead elsewhere, as TTI's `LT` lines.
    #[schemars(length(max = neoteletext_formats::page::TEXT_LINKS))]
    pub texts: Vec<TextLink>,
}

/// A key of the Link Extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "the four coloured keys and Index")]
pub enum LinkKey {
    Red,
    Green,
    Yellow,
    Cyan,
    Index,
}

/// A key's link (`LN`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyLink {
    /// The key.
    pub key: LinkKey,
    /// Where it leads: `mpp`, `mpp:ssss`, `name:mpp`, `name:mpp:ssss`, an
    /// `http` or `https` address, or `web:` and one (Appendix E.1).
    #[schemars(length(min = 1, max = LINK_TARGET_BYTES), regex(pattern = "^[!-+\\--~]+$"))]
    pub target: String,
    /// The prompt: at most 9 characters for this service, 8 for another;
    /// none for the Index key.
    #[schemars(length(min = 1, max = LINK_LABEL_CHARACTERS))]
    pub label: Option<String>,
}

/// A page number on the page that leads elsewhere (`LK`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumberLink {
    /// The page number as the page shows it.
    pub number: PageNumber,
    /// Where it leads, in a form of Appendix E.1, never `web:`.
    #[schemars(length(min = 1, max = LINK_TARGET_BYTES), regex(pattern = "^[!-+\\--~]+$"))]
    pub target: String,
}

/// A run of text on the page that leads elsewhere (`LT`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextLink {
    /// Its row, 1 to 23.
    #[schemars(range(min = 1, max = LAST_TEXT_LINK_ROW))]
    pub row: u8,
    /// Its first column, 0 to 39.
    #[schemars(range(max = LAST_COLUMN))]
    pub column: u8,
    /// Its characters, keeping the run within the row's 40 columns.
    #[schemars(range(min = 1, max = COLUMNS))]
    pub length: u8,
    /// Where it leads, in a form of Appendix E.1.
    #[schemars(length(min = 1, max = LINK_TARGET_BYTES), regex(pattern = "^[!-+\\--~]+$"))]
    pub target: String,
}

/// The longest link target: the engine's, the longest address of REQ-1661
/// and a `web:` mark.
pub const LINK_TARGET_BYTES: usize = neoteletext_formats::page::LINK_TARGET_BYTES;
/// The longest page parameters of a selection (DOC-1753).
pub const PARAMETERS_BYTES: usize = neoteletext_input::MAX_PAGE_PARAMETERS;
/// Page parameters are printable ASCII with no `#` (DOC-1753);
/// the engine's encoder of a redirect holds them to the map's pairs too.
const PARAMETERS_PATTERN: &str = "^[!-\"$-~]+$";
/// The longest label of a key's link to this service (Appendix E.1).
pub const LINK_LABEL_CHARACTERS: usize = 9;
/// The longest label of a key's link to another service: one fewer, for
/// the mark a receiver shows of another service (Appendix E.1).
pub const OTHER_SERVICE_LABEL_CHARACTERS: usize = 8;
/// The last row a text link may be on: rows 1 to 23, never the header or
/// the Fastext row.
const LAST_TEXT_LINK_ROW: u8 = 23;

/// The control bits C4 to C11 of a page header (EN 300 706 clause
/// 9.3.1.3). The server sets C4 and C11 itself on every page it sends, so
/// that each replaces the last and stands alone (DOC-1949).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools, reason = "the standard's eight control bits, each a named flag (REQ-1478)")]
pub struct Control {
    /// C4, erase page.
    pub erase: bool,
    /// C5, newsflash.
    pub newsflash: bool,
    /// C6, subtitle.
    pub subtitle: bool,
    /// C7, suppress header.
    pub suppress_header: bool,
    /// C8, update indicator.
    pub update: bool,
    /// C9, interrupted sequence.
    pub interrupted_sequence: bool,
    /// C10, inhibit display.
    pub inhibit_display: bool,
    /// C11, magazine serial.
    pub magazine_serial: bool,
}

/// The page's default character set (EN 300 706 clause 15, table 32).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CharacterSet {
    /// The region, bits 3 to 6 of the default set's designation, carried
    /// in packet X/28/0; none for no packet X/28/0.
    #[schemars(range(max = REGION_MAX))]
    pub region: Option<u8>,
    /// The national option, C12 to C14 of the header, 0 to 7.
    #[schemars(range(max = NATIONAL_OPTION_MAX))]
    pub national_option: u8,
}

/// A packet X/26 (EN 300 706 clause 12.3).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Enhancement {
    /// Its designation code, 0 to 15.
    #[schemars(range(max = DESIGNATION_MAX))]
    pub designation: u8,
    /// Its thirteen triplets.
    pub triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
}

/// A triplet, by its fields (EN 300 706 clause 12.3.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Triplet {
    /// The address, 6 bits.
    #[schemars(range(max = TRIPLET_ADDRESS_MAX))]
    pub address: u8,
    /// The mode, 5 bits.
    #[schemars(range(max = TRIPLET_MODE_MAX))]
    pub mode: u8,
    /// The data, 7 bits.
    #[schemars(range(max = TRIPLET_DATA_MAX))]
    pub data: u8,
}

/// A packed line other than X/26, by its packet.
// Read by the reader, which follows `packet` to the line's own type, so a
// fault keeps its full path (`read.rs`).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "packet", rename_all = "snake_case")]
pub enum PackedLine {
    /// X/27 of designation code 1 to 3: thirty-nine values of four bits.
    X27Nibbles(X27NibblesLine),
    /// X/27 of designation code 4 to 15: thirteen triplets.
    X27(X27Line),
    /// X/28: thirteen triplets.
    X28(X28Line),
    /// M/29, the magazine's: thirteen triplets.
    M29(M29Line),
}

/// X/27 of designation code 1 to 3: thirty-nine values of four bits.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct X27NibblesLine {
    /// 1 to 3.
    #[schemars(range(min = 1, max = 3))]
    pub designation: u8,
    /// The values, 0 to 15 each.
    #[schemars(length(min = NIBBLES, max = NIBBLES))]
    pub nibbles: Vec<u8>,
}

/// X/27 of designation code 4 to 15: thirteen triplets.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct X27Line {
    /// 4 to 15.
    #[schemars(range(min = 4, max = DESIGNATION_MAX))]
    pub designation: u8,
    /// The triplets.
    pub triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
}

/// X/28: thirteen triplets.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct X28Line {
    /// 0 to 15.
    #[schemars(range(max = DESIGNATION_MAX))]
    pub designation: u8,
    /// The triplets.
    pub triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
}

/// M/29, the magazine's: thirteen triplets.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct M29Line {
    /// 0 to 15.
    #[schemars(range(max = DESIGNATION_MAX))]
    pub designation: u8,
    /// The triplets.
    pub triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
}

/// The first row a cursor may be on (DOC-1954): the first after
/// the header.
pub(crate) const FIRST_CURSOR_ROW: u8 = neoteletext_core::consts::PACKET_FIRST_ROW;
/// The last row a cursor may be on.
pub(crate) const LAST_CURSOR_ROW: u8 = neoteletext_input::cursor::LAST_ROW;

/// The input cursor (DOC-1954).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    /// Its row, 1 to 23: row 0 is the header, and row 24 the Fastext row.
    #[schemars(range(min = FIRST_CURSOR_ROW, max = LAST_CURSOR_ROW))]
    pub row: u8,
    /// Its column, 0 to 39.
    #[schemars(range(max = LAST_COLUMN))]
    pub column: u8,
    /// A block cursor rather than an underline.
    pub block: bool,
}

/// `sound`, by `kind` (the engine's audio profile, DOC-1785).
// Read by the reader, which follows `kind` to the sound's own type
// (`read.rs`).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sound {
    /// A stored Opus sound (decision 21).
    Opus(OpusSound),
    /// A live Opus sound's object.
    OpusLive(OpusLiveSound),
    /// A live Opus sound's next packets.
    OpusPackets(OpusPacketsSound),
    /// A stored sound as 16-bit samples, which the server encodes as IMA
    /// ADPCM, for receivers that decode no Opus.
    Object(ObjectSound),
    /// A live sound's object.
    Live(LiveSound),
    /// A live sound's next blocks of samples.
    Blocks(BlocksSound),
    /// A stored SID sound: images of the chip's registers (DEC-6410).
    Sid(SidSound),
    /// A live SID sound's object.
    SidLive(SidLiveSound),
    /// A live SID sound's next blocks.
    SidBlocks(SidBlocksSound),
    /// A stored DX7 sound: timed MIDI messages for the instrument (DEC-6410).
    Dx7(Dx7Sound),
    /// A live DX7 sound's object.
    Dx7Live(Dx7LiveSound),
    /// A live DX7 sound's next blocks.
    Dx7Blocks(Dx7BlocksSound),
    /// A bank of DX7 patches, which DX7 sounds name.
    Dx7Bank(Dx7BankSound),
    /// A stored Juno sound: timed MIDI messages (DEC-6429).
    Juno(JunoSound),
    /// A live Juno sound's object.
    JunoLive(JunoLiveSound),
    /// A live Juno sound's next blocks.
    JunoBlocks(SynthBlocksSound),
    /// A bank of Juno patches, which Juno sounds name.
    JunoBank(SynthBankSound),
    /// A stored Minimoog sound: timed MIDI messages (DEC-6429).
    Minimoog(SynthSound),
    /// A live Minimoog sound's object.
    MinimoogLive(SynthLiveSound),
    /// A live Minimoog sound's next blocks.
    MinimoogBlocks(SynthBlocksSound),
    /// A bank of Minimoog patches, which Minimoog sounds name.
    MinimoogBank(SynthBankSound),
    /// A stored Prophet-5 sound: timed MIDI messages (DEC-6429).
    Prophet5(Prophet5Sound),
    /// A live Prophet-5 sound's object.
    Prophet5Live(Prophet5LiveSound),
    /// A live Prophet-5 sound's next blocks.
    Prophet5Blocks(SynthBlocksSound),
    /// A bank of Prophet-5 programs, which Prophet-5 sounds name.
    Prophet5Bank(SynthBankSound),
    /// A stored Jupiter-8 sound: timed MIDI messages (DEC-6429).
    Jupiter8(Jupiter8Sound),
    /// A live Jupiter-8 sound's object.
    Jupiter8Live(Jupiter8LiveSound),
    /// A live Jupiter-8 sound's next blocks.
    Jupiter8Blocks(SynthBlocksSound),
    /// A bank of Jupiter-8 patches, which Jupiter-8 sounds name.
    Jupiter8Bank(SynthBankSound),
    /// A stored CS-80 sound: timed MIDI messages (DEC-6429).
    Cs80(SynthSound),
    /// A live CS-80 sound's object.
    Cs80Live(SynthLiveSound),
    /// A live CS-80 sound's next blocks.
    Cs80Blocks(SynthBlocksSound),
    /// A bank of CS-80 patches, which CS-80 sounds name.
    Cs80Bank(SynthBankSound),
    /// Play a sound on an audio channel.
    Play(PlaySound),
    /// Stop an audio channel.
    Stop(StopSound),
    /// Change an audio channel's gain.
    Gain(GainSound),
}

/// A stored Opus sound (decision 21).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length, 1 to 40 bytes.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
    /// Its length in samples at 48 kHz.
    #[schemars(range(max = MOST_SAMPLES))]
    pub samples: u32,
    /// Its packets back to back, exact base64, at most two minutes.
    pub packets: Base64,
}

/// A live Opus sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusLiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
}

/// A live Opus sound's next packets.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpusPacketsSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Every packet's length.
    #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
    pub packet_bytes: u16,
    /// The first packet's block number.
    pub first: u16,
    /// The packets, exact base64.
    pub packets: Base64,
}

/// A stored sound as 16-bit samples, which the server encodes as IMA
/// ADPCM, for receivers that decode no Opus.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate, one of the profile's.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
    /// 16-bit little-endian mono samples, exact base64, at most two
    /// minutes.
    pub pcm: Base64,
}

/// A live sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
}

/// A live sound's next blocks of samples.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlocksSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its sample rate.
    #[schemars(extend("enum" = SAMPLE_RATES))]
    pub rate: u32,
    /// The first block's number.
    pub first: u16,
    /// The samples, exact base64.
    pub pcm: Base64,
}

/// The SID chip that plays a sound: the two differ in their filter and
/// their combined waveforms.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
pub enum SidChip {
    /// The MOS 6581.
    #[serde(rename = "6581")]
    Mos6581,
    /// The MOS 8580.
    #[serde(rename = "8580")]
    Mos8580,
}

/// The clock a SID chip runs from, which sets the pitch of a frequency
/// register's value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SidClock {
    /// A PAL Commodore 64's: 985,248 Hz.
    Pal,
    /// An NTSC Commodore 64's: 1,022,727 Hz.
    Ntsc,
}

/// A stored SID sound: images of the chip's 25 writable registers, 1, 2 or
/// 4 to a 20 ms block (the audio profile's codec 3, DEC-6410).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SidSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The images of the chip's registers in each 20 ms block: 1, 2 or 4.
    #[schemars(extend("enum" = SID_IMAGES_PER_BLOCK))]
    pub images_per_block: u8,
    /// The chip that plays it.
    pub chip: SidChip,
    /// The clock the chip runs from.
    pub clock: SidClock,
    /// Its blocks back to back, each `images_per_block` images of 25
    /// bytes, exact base64: 1 to 6,000 blocks, two minutes.
    pub images: Base64,
}

/// A live SID sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SidLiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The images of the chip's registers in each 20 ms block: 1, 2 or 4.
    #[schemars(extend("enum" = SID_IMAGES_PER_BLOCK))]
    pub images_per_block: u8,
    /// The chip that plays it.
    pub chip: SidChip,
    /// The clock the chip runs from.
    pub clock: SidClock,
}

/// A live SID sound's next blocks.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SidBlocksSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The images of the chip's registers in each 20 ms block: 1, 2 or 4.
    #[schemars(extend("enum" = SID_IMAGES_PER_BLOCK))]
    pub images_per_block: u8,
    /// The first block's number, counting modulo 65,536.
    pub first: u16,
    /// The blocks back to back, exact base64: 1 to 100 blocks.
    pub images: Base64,
}

/// A stored DX7 sound: MIDI messages for the instrument, each timed within
/// its 20 ms block (the audio profile's codec 4, DEC-6410). A slot is a
/// time in tenths of a millisecond, 0 to 199, a status byte and two data
/// bytes; four zero bytes are an empty slot. The messages taken are note
/// off and note on, control changes 1, 7, 64, 120, 121 and 123, a program
/// change to one of the bank's 32 patches, and pitch bend.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dx7Sound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank its program changes choose patches from.
    pub bank: u8,
    /// Its blocks back to back, each `slots` slots of 4 bytes, exact
    /// base64: 1 to 6,000 blocks, two minutes.
    pub blocks: Base64,
}

/// A live DX7 sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dx7LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank its program changes choose patches from.
    pub bank: u8,
}

/// A live DX7 sound's next blocks.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dx7BlocksSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The first block's number, counting modulo 65,536.
    pub first: u16,
    /// The blocks back to back, exact base64: 1 to 100 blocks.
    pub blocks: Base64,
}

/// A bank of DX7 patches (the audio profile's codec 5, DEC-6410), which a
/// DX7 sound names by this handle.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dx7BankSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its patches back to back, each the instrument's 128 packed bytes,
    /// every byte at most 127, exact base64: 1 to 32 patches.
    pub patches: Base64,
}

/// The analogue synthesizers a program's sound may be for (DEC-6429), each
/// a codec of the audio profile whose block is the DX7's: 1 to 16 slots of
/// a timed MIDI message. Not a message of its own: the kind of a sound
/// names it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Synth {
    /// Roland's Juno-106 or Juno-60.
    Juno,
    /// Moog's Minimoog Model D.
    Minimoog,
    /// Sequential Circuits' Prophet-5.
    Prophet5,
    /// Roland's Jupiter-8.
    Jupiter8,
    /// Yamaha's CS-80.
    Cs80,
}

impl Synth {
    /// Every one, in the order of their codecs.
    pub const ALL: [Self; 5] = [Self::Juno, Self::Minimoog, Self::Prophet5, Self::Jupiter8, Self::Cs80];

    /// The audio profile's codec of its sounds, which is also the code that
    /// names its family in a patch bank (the engine's `TAG_BANK_FAMILY`).
    #[must_use]
    pub const fn codec(self) -> u8 {
        match self {
            Self::Juno => 6,
            Self::Minimoog => 7,
            Self::Prophet5 => 8,
            Self::Jupiter8 => 9,
            Self::Cs80 => 10,
        }
    }

    /// The bytes of one of its patches, in the instrument's own layout.
    #[must_use]
    pub const fn patch_bytes(self) -> u16 {
        match self {
            Self::Juno => JUNO_PATCH_BYTES,
            Self::Minimoog => MINIMOOG_PATCH_BYTES,
            Self::Prophet5 => PROPHET5_PATCH_BYTES,
            Self::Jupiter8 => JUPITER8_PATCH_BYTES,
            Self::Cs80 => CS80_PATCH_BYTES,
        }
    }

    /// Its name, as a sound's kind begins with it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Juno => "juno",
            Self::Minimoog => "minimoog",
            Self::Prophet5 => "prophet5",
            Self::Jupiter8 => "jupiter8",
            Self::Cs80 => "cs80",
        }
    }
}

/// The Juno a sound plays: the two share their sliders and differ in
/// their voices.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
pub enum JunoModel {
    /// The Juno-106: model 0 of the engine's `TAG_MODEL`.
    #[serde(rename = "106")]
    Juno106,
    /// The Juno-60: model 1.
    #[serde(rename = "60")]
    Juno60,
}

/// The Prophet-5 a sound plays: its revisions differ in their filter and
/// oscillator chips.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Prophet5Model {
    /// Revision 3, with Curtis chips: model 0 of the engine's `TAG_MODEL`.
    #[serde(rename = "revision_3")]
    Revision3,
    /// Revisions 1 and 2, with SSM chips: model 1.
    #[serde(rename = "revisions_1_and_2")]
    Revisions1And2,
}

/// How a Jupiter-8's keyboard plays its two patches, the upper chosen by
/// program changes on MIDI channel 1 and the lower on channel 2.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Jupiter8KeyMode {
    /// Every key plays the upper patch, on eight voices.
    Whole,
    /// Every key plays both patches, four voices each.
    Dual,
    /// Keys below `split_key` play the lower patch and the others the
    /// upper, four voices each.
    Split,
}

/// A stored sound of the Minimoog or the CS-80: MIDI messages for the
/// instrument, each timed within its 20 ms block (the audio profile's
/// codecs 7 and 10, DEC-6429). A slot is a time in tenths of a millisecond,
/// 0 to 199, a status byte and two data bytes; four zero bytes are an
/// empty slot. Every instrument takes note off and note on, control
/// changes 1, 7, 64, 120, 121 and 123, a program change to one of the
/// bank's 32 patches, and pitch bend; the CS-80 also polyphonic key
/// pressure and channel pressure, and the Jupiter-8 control changes 5, 8
/// and 65, with program changes on MIDI channels 1 and 2 only.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of the instrument's patches its program
    /// changes choose from.
    pub bank: u8,
    /// Its blocks back to back, each `slots` slots of 4 bytes, exact
    /// base64: 1 to 6,000 blocks, two minutes.
    pub blocks: Base64,
}

/// A live sound's object, of the Minimoog or the CS-80.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthLiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of the instrument's patches its program
    /// changes choose from.
    pub bank: u8,
}

/// A live sound's next blocks, of any of the analogue synthesizers.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthBlocksSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The first block's number, counting modulo 65,536.
    pub first: u16,
    /// The blocks back to back, exact base64: 1 to 100 blocks.
    pub blocks: Base64,
}

/// A bank of patches of one of the analogue synthesizers, the one its kind
/// names (the audio profile's codec 11, DEC-6429), which that
/// instrument's sounds name by this handle. Each patch is the
/// instrument's own layout: 18 bytes for the Juno, 36 for the Minimoog, 24
/// for the Prophet-5, 43 for the Jupiter-8 and 85 for the CS-80, every
/// byte within the range of the control it holds.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthBankSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// Its patches back to back, exact base64: 1 to 32 patches.
    pub patches: Base64,
}

/// A stored Juno sound (the audio profile's codec 6): as [`SynthSound`],
/// with the model that plays it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JunoSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The model that plays it.
    pub model: JunoModel,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Juno patches its program changes choose
    /// from.
    pub bank: u8,
    /// Its blocks back to back, each `slots` slots of 4 bytes, exact
    /// base64: 1 to 6,000 blocks, two minutes.
    pub blocks: Base64,
}

/// A live Juno sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JunoLiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The model that plays it.
    pub model: JunoModel,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Juno patches its program changes choose
    /// from.
    pub bank: u8,
}

/// A stored Prophet-5 sound (the audio profile's codec 8): as
/// [`SynthSound`], with the revision that plays it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Prophet5Sound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The revision that plays it.
    pub model: Prophet5Model,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Prophet-5 programs its program changes
    /// choose from.
    pub bank: u8,
    /// Its blocks back to back, each `slots` slots of 4 bytes, exact
    /// base64: 1 to 6,000 blocks, two minutes.
    pub blocks: Base64,
}

/// A live Prophet-5 sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Prophet5LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The revision that plays it.
    pub model: Prophet5Model,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Prophet-5 programs its program changes
    /// choose from.
    pub bank: u8,
}

/// A stored Jupiter-8 sound (the audio profile's codec 9): as
/// [`SynthSound`], with how its keyboard plays its two patches.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Jupiter8Sound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// How the keyboard plays the upper and lower patches.
    pub key_mode: Jupiter8KeyMode,
    /// The lowest key of the upper patch in the split mode, a MIDI key of
    /// 0 to 127; sent, and unused, in the others.
    #[schemars(range(max = LAST_MIDI_KEY))]
    pub split_key: u8,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Jupiter-8 patches its program changes
    /// choose from.
    pub bank: u8,
    /// Its blocks back to back, each `slots` slots of 4 bytes, exact
    /// base64: 1 to 6,000 blocks, two minutes.
    pub blocks: Base64,
}

/// A live Jupiter-8 sound's object.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Jupiter8LiveSound {
    /// Who it is for.
    pub connections: Connections,
    /// Its handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// How the keyboard plays the upper and lower patches.
    pub key_mode: Jupiter8KeyMode,
    /// The lowest key of the upper patch in the split mode, a MIDI key of
    /// 0 to 127; sent, and unused, in the others.
    #[schemars(range(max = LAST_MIDI_KEY))]
    pub split_key: u8,
    /// The slots of each 20 ms block: 1 to 16, of 4 bytes each.
    #[schemars(range(min = 1, max = DX7_MOST_SLOTS))]
    pub slots: u8,
    /// The handle of the bank of Jupiter-8 patches its program changes
    /// choose from.
    pub bank: u8,
}

/// Play a sound on an audio channel.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaySound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The sound's handle.
    pub handle: u8,
    /// Its generation.
    pub generation: u16,
    /// The gain, 0 to 100.
    #[schemars(range(max = MOST_GAIN))]
    pub gain: u8,
    /// The fade in, in milliseconds.
    pub fade_ms: u16,
    /// It loops.
    pub r#loop: bool,
    /// It plays after the sound playing ends.
    pub after: bool,
    /// How far other channels duck under it, 0 to 6.
    #[schemars(range(max = MOST_DUCK))]
    pub duck: u8,
    /// The page it belongs to, or none for the whole service.
    pub page: Option<PageNumber>,
}

/// Stop an audio channel.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopSound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The fade out, in milliseconds.
    pub fade_ms: u16,
}

/// Change an audio channel's gain.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GainSound {
    /// Who it is for.
    pub connections: Connections,
    /// The audio channel, 0 to 3.
    #[schemars(range(max = LAST_AUDIO_CHANNEL))]
    pub channel: u8,
    /// The gain, 0 to 100.
    #[schemars(range(max = MOST_GAIN))]
    pub gain: u8,
    /// The fade, in milliseconds.
    pub fade_ms: u16,
}

/// `cookie`: sets or clears one of the program's cookies for a connection
/// (DOC-1924), every attribute a field of its own; a raw
/// `Set-Cookie` line is never taken.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cookie {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// Its name; names starting `_` are the server's.
    #[schemars(length(min = 1, max = COOKIE_BYTES))]
    pub name: String,
    /// Its value.
    #[schemars(length(max = COOKIE_BYTES))]
    pub value: String,
    /// `Path`, or none.
    #[schemars(length(min = 1, max = COOKIE_BYTES))]
    pub path: Option<String>,
    /// `Max-Age` in seconds, 0 removing it, or none for a cookie that
    /// lasts the browser's session.
    pub max_age: Option<u32>,
    /// `SameSite`, or none.
    pub same_site: Option<SameSite>,
    /// `Secure`.
    pub secure: bool,
    /// `HttpOnly`.
    pub http_only: bool,
}

/// `SameSite` (RFC 6265bis section 4.1.2.7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[allow(missing_docs, reason = "the attribute's three values")]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

/// `author_log`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthorLog {
    /// Who it is for.
    pub connections: Connections,
    /// Its level.
    pub level: AuthorLevel,
    /// Its text: UTF-8 of 1 to 200 bytes, no control or bidirectional
    /// formatting character (the engine's `log::check`).
    #[schemars(length(min = 1, max = neoteletext_formats::log::MAX_LOG_TEXT_BYTES))]
    pub text: String,
    /// The page it concerns, or none.
    pub page: Option<PageNumber>,
}

/// An author's log line's level, as TTI's `LG` writes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
pub enum AuthorLevel {
    /// An error.
    E,
    /// A warning.
    W,
    /// Information.
    I,
}

/// `log`: a line for the server operator's log.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Log {
    /// Its level.
    pub level: OperatorLevel,
    /// The kind of event, a name that does not change: lower case
    /// letters, digits and `_`.
    #[schemars(length(min = 1, max = EVENT_NAME_CHARACTERS), regex(pattern = "^[a-z][a-z0-9_]*$"))]
    pub event: String,
    /// What happened.
    #[schemars(length(min = 1, max = OPERATOR_LOG_CHARACTERS))]
    pub text: String,
}

/// An operator's log line's level.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "the log's levels")]
pub enum OperatorLevel {
    Error,
    Warn,
    Info,
    Debug,
}

/// `regis`: more bytes of a sub-page's ReGIS, the vector graphics language
/// a receiver's ReGIS renderer draws in place of the sub-page's teletext
/// page (DEC-6335). It goes to the connections named whose selection shows
/// the sub-page. Each of them has one stream of ReGIS for the sub-page it
/// shows: `restart` begins it again, and a message without `restart` adds
/// to it, and is not sent to a connection that has no stream yet. The
/// server reads the bytes with the receiver's own renderer first, and
/// refuses the message if a commit in them would fail at the receiver.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Regis {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// Begin the sub-page's ReGIS again: the receiver starts a new
    /// renderer for it, with nothing drawn and no numbered drawings.
    #[serde(default)]
    pub restart: bool,
    /// The bytes, exact base64: ReGIS commands as a terminal would receive
    /// them inside a device control string, without the string's framing.
    pub data: Base64,
}

/// `naplps`: bytes of a sub-page's NAPLPS (ANSI X3.110-1983), the picture
/// language a receiver's NAPLPS renderer draws in place of the sub-page's
/// teletext page (the NAPLPS renderer's REQ-6408 item 3). It goes to the
/// connections named whose selection shows the sub-page. Each of them has
/// one stream of NAPLPS for the sub-page it shows: `restart` begins it
/// again, and a message without `restart` adds to it, drawing on over what
/// is there, and is not sent to a connection that has no stream yet. The
/// server does not read the bytes: the receiver's renderer bounds what
/// they can cost.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Naplps {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// Begin the sub-page's NAPLPS again: the receiver starts a new
    /// renderer for it, with nothing drawn.
    #[serde(default)]
    pub restart: bool,
    /// The bytes, exact base64: NAPLPS as a terminal would receive it.
    pub data: Base64,
}

/// The most drawing links a sub-page has (the engine's REQ-6370,
/// `MAX_DRAWING_LINKS`; the engine this server is pinned to predates it).
pub const MOST_DRAWING_LINKS: usize = 64;

/// `regis_links`: the numbered drawings of a sub-page's ReGIS picture that
/// are links, and where each leads (REQ-6372). A receiver follows a click
/// on one itself, as it follows a text link of a teletext page. The list
/// replaces the sub-page's drawing links, and an empty one removes them.
/// It goes to the connections named whose selection shows the sub-page.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisLinks {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The links, each drawing named once.
    #[schemars(length(max = MOST_DRAWING_LINKS))]
    pub links: Vec<DrawingLink>,
}

/// One drawing link of `regis_links`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingLink {
    /// The numbered drawing that is the link.
    #[schemars(range(min = 1))]
    pub drawing: u16,
    /// Where it leads, in any form a text link's target takes: a page
    /// (`200`), a sub-page (`200:0001`), a page of another service
    /// (`news:300`), an address, or `web:` and a web address.
    #[schemars(length(min = 1, max = neoteletext_formats::page::LINK_TARGET_BYTES))]
    pub target: String,
}

/// `regis_cursor`: the numbered drawing of a sub-page's ReGIS picture that
/// is its cursor, which a receiver blinks while keys go to the channel, or
/// 0 for none (REQ-6372). It goes to the connections named whose selection
/// shows the sub-page.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisCursor {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The drawing, or 0 for no cursor.
    pub drawing: u16,
}

/// `packets`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Packets {
    /// Who they are for.
    pub connections: Connections,
    /// Whole 42-byte packets back to back, exact base64.
    pub packets: Base64,
}

// ------------------------------------------------------------ the wire

/// One line for a program, with its newline.
#[must_use]
pub fn line(message: &ServerMessage) -> Vec<u8> {
    // The types serialise without fail: no maps with other than string
    // keys, no floats.
    let mut bytes = serde_json::to_vec(message).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// `/stats.json`'s document: the server's figures and each channel's, by
/// folder, from the same types as `stats` (DOC-1968).
#[must_use]
pub fn figures_document(server: &ServerFigures, channels: &BTreeMap<String, ChannelCounts>) -> Vec<u8> {
    #[derive(Serialize)]
    struct Document<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        server: &'a ServerFigures,
        channels: &'a BTreeMap<String, ChannelCounts>,
    }
    // As `line`: these types serialise without fail.
    let mut bytes = serde_json::to_vec(&Document { kind: "stats", server, channels }).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Both directions, for the schema.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code, clippy::large_enum_variant, reason = "only its schema is used")]
enum Protocol {
    Server(ServerMessage),
    Program(ProgramMessage),
}

/// The protocol's JSON Schema, draft 2020-12, generated from these types:
/// `$defs/ServerMessage` is what the server sends, `$defs/ProgramMessage`
/// what a program sends.
#[must_use]
pub fn schema() -> serde_json::Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    let mut root = generator.into_root_schema_for::<Protocol>();
    root.insert("title".to_string(), format!("The NeoTeletext server's protocol with a channel's program, version {PROTOCOL}").into());
    root.to_value()
}

#[cfg(test)]
pub(crate) mod tests;
