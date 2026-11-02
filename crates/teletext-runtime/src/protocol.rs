//! The protocol between the server and a channel's program (DEC-1482,
//! REQ-1475 to REQ-1479; `docs/applications.md` sections 2 and 3): every
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
pub use read::read;

/// The protocol's version, in `start` and in the schema's name. Version 1
/// was `docs/applications.md`'s before DEC-1482: declared pages, TTI and
/// publishing for everyone. Version 2 had `goto`, which `redirect` replaced
/// (DEC-1683), and which could name no other channel. Version 6 numbers
/// each selection (DEC-1759 lesson 13), passes a program its cookies, adds
/// `denied`, lets a redirect keep a query, and adds `describe` and
/// `description` (REQ-1728).
pub const PROTOCOL: u32 = 6;
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
    Stats(Stats),
    /// A message the program sent was refused (REQ-1475).
    Refused(Refused),
    /// A web visitor or a search engine is at a page's address: answer with
    /// `description` (REQ-1728).
    Describe(Describe),
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
        /// The cell's column, 0 to 39.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The cell's row, 0 to 24.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
    },
    /// The wheel turned over a cell, in 1/120ths of a line, positive right
    /// and down.
    Wheel {
        /// Across.
        dx: i16,
        /// Down.
        dy: i16,
        /// The cell's column.
        #[schemars(range(max = LAST_COLUMN))]
        column: u8,
        /// The cell's row.
        #[schemars(range(max = LAST_ROW))]
        row: u8,
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

/// `stats`: the figures of `docs/applications.md` section 7.
#[derive(Clone, PartialEq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    /// The server's figures.
    pub server: serde_json::Value,
    /// The channel's figures.
    pub channel: serde_json::Value,
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
    /// A selection's number given with other than one connection: a number
    /// belongs to one connection.
    OneConnection,
    /// A page other than the one the message is about.
    WrongPage,
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
        }
    }

    /// Whether it answers something the server asked, which only a program
    /// reading its lines can do (requirement 8.4).
    #[must_use]
    pub const fn is_answer(&self) -> bool {
        matches!(self, Self::Reply(_) | Self::Denied(_) | Self::Description(_))
    }
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
    /// The default character set.
    pub character_set: CharacterSet,
    /// Packets X/26, at most one of each designation code.
    #[schemars(length(max = neoteletext_core::consts::DESIGNATION_CODES))]
    pub enhancements: Vec<Enhancement>,
    /// Packets X/27 of designation codes 1 to 15, X/28 and M/29.
    #[schemars(length(max = neoteletext_formats::page::RAW_PACKETS))]
    pub packets: Vec<PackedLine>,
    /// The input cursor (DOC-1954), or none.
    pub cursor: Option<Cursor>,
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
