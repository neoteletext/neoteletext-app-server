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

pub use check::Refusal;

/// The protocol's version, in `start` and in the schema's name. Version 1
/// was `docs/applications.md`'s before DEC-1482: declared pages, TTI and
/// publishing for everyone.
pub const PROTOCOL: u32 = 2;
/// The longest line either side may send: 16 MiB. A stored sound's two
/// minutes of 16-bit samples in base64 are about 15 MB, the largest
/// message there is; a page of the standard's 8,190 sub-pages of whole
/// rows is about 12 MB.
pub const LONGEST_LINE: usize = 16 * 1024 * 1024;
/// The longest page title, magazine name or section heading: the
/// engine's contents rule (its REQ-WEB-025, decision E.120).
pub const TITLE_CHARACTERS: usize = neoteletext_web::contents::MAX_TITLE;
/// The views a connection may have: each a browser tab showing the
/// channel, numbered 0 to 15 (`INPUT.md` section 5.4, SELECT's `view`).
/// The engine names no constant for it yet; when SELECT is built there it
/// is to be its `MAX_VIEWS`.
pub const MAX_VIEWS: u8 = 16;
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
/// The last view, 15.
const LAST_VIEW: u8 = MAX_VIEWS - 1;
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
/// (`neoteletext_input::keys`, INPUT.md section 5.6), or the one
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
            "description": "A W3C KeyboardEvent.key name, from the engine's table (INPUT.md section 5.6), or the one character of a character key.",
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
    /// A view selected a page or a sub-page (REQ-1472); answer with `page`.
    Selected(Selected),
    /// Input on a page (INPUT.md).
    Input(InputMessage),
    /// A view left a page, or its connection closed.
    Left(Left),
    /// A session ended, five minutes after its last connection closed.
    Closed(Closed),
    /// The server's and the channel's figures, every ten seconds.
    Stats(Stats),
    /// A message the program sent was refused (REQ-1475).
    Refused(Refused),
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
    /// Views a connection may have.
    pub views: u64,
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
    /// The cookies the program set on earlier connections.
    #[schemars(length(max = MOST_COOKIES))]
    pub cookies: Vec<CookiePair>,
    /// Each view's selection at the start; a view with none asks for the
    /// channel's initial page.
    #[schemars(length(min = 1, max = MAX_VIEWS))]
    pub selections: Vec<ViewSelection>,
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

/// One view's selection.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewSelection {
    /// The view, 0 to 15.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
    /// What it selected, or none: the channel's initial page.
    pub selection: Option<Selection>,
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

/// `selected` (REQ-1472).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selected {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The view, 0 to 15.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
    /// The page.
    pub page: PageNumber,
    /// The sub-code, or 3F7F for the page and none of its sub-pages.
    pub sub: SubCode,
}

/// `input`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputMessage {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The view, 0 to 15.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
    /// The page with the keys.
    pub page: PageNumber,
    /// What happened.
    pub event: InputEvent,
}

/// An input event (INPUT.md), by `kind`.
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

/// `left`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Left {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The view.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
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
    Index,
    Goto,
    Sound,
    Cookie,
    AuthorLog,
    Log,
    Packets,
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
    /// The same number, view or code twice.
    Duplicate,
    /// A page number the standard reserves.
    Reserved,
    /// A page listed under another magazine.
    WrongMagazine,
    /// A character text may not have: a control or a bidirectional
    /// formatting character.
    ForbiddenCharacter,
    /// Not exact base64 (RFC 4648 section 4, padded, no whitespace).
    NotBase64,
    /// Bytes of the wrong length once decoded.
    WrongLength,
    /// A selected page not in the index, or whose sub-pages disagree with
    /// its count.
    NotInIndex,
    /// A connection that is not open.
    UnknownConnection,
    /// An answer to nothing the server asked: a second `reply`.
    NotAwaited,
    /// A sound outside the profile the receivers decode.
    OutsideProfile,
    /// A raw packet the stream may not carry (REQ-1462).
    InvalidPacket,
}

// ------------------------------------------------------- program to server

/// A message from a program to the server.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgramMessage {
    /// The answer to `connected` (REQ-1471).
    Reply(Reply),
    /// A page, to the connections named (REQ-1472, REQ-1473).
    Page(PageMessage),
    /// A change to the magazines and their pages.
    Index(Index),
    /// Sends one view of a connection to a page.
    Goto(Goto),
    /// A sound, its data, or a command.
    Sound(Sound),
    /// Sets or clears one of the program's cookies for a connection.
    Cookie(Cookie),
    /// A line for the developer console of the viewer's browser (the
    /// engine's REQ-INP-026).
    AuthorLog(AuthorLog),
    /// A line for the server operator's log.
    Log(Log),
    /// Raw teletext packets (`SERVER.md` section 7.4).
    Packets(Packets),
}

impl ProgramMessage {
    /// Its type.
    #[must_use]
    pub const fn kind(&self) -> MessageType {
        match self {
            Self::Reply(_) => MessageType::Reply,
            Self::Page(_) => MessageType::Page,
            Self::Index(_) => MessageType::Index,
            Self::Goto(_) => MessageType::Goto,
            Self::Sound(_) => MessageType::Sound,
            Self::Cookie(_) => MessageType::Cookie,
            Self::AuthorLog(_) => MessageType::AuthorLog,
            Self::Log(_) => MessageType::Log,
            Self::Packets(_) => MessageType::Packets,
        }
    }
}

/// `reply` (REQ-1471).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    /// The connection whose `connected` this answers.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The channel's magazines for this viewer (REQ-1477).
    #[schemars(length(max = limits::MAGAZINES))]
    pub magazines: Vec<Magazine>,
    /// The page each view selected, or the initial page for a view that
    /// selected none.
    #[schemars(length(max = MAX_VIEWS))]
    pub selected: Vec<ViewPage>,
}

/// A view's page, in a reply.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewPage {
    /// The view, 0 to 15.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
    /// Its page.
    pub page: Page,
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

/// `page`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PageMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: Page,
}

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

/// `goto`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Goto {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The view.
    #[schemars(range(max = LAST_VIEW))]
    pub view: u8,
    /// The page.
    pub page: PageNumber,
    /// The sub-code, or 3F7F for the page.
    pub sub: SubCode,
}

/// A page, as the engine's page model (`PageFile`), field by field
/// (REQ-1478).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// Its number, never a reserved one.
    pub page: PageNumber,
    /// Its sub-pages, in ascending order of sub-code.
    #[schemars(length(min = 1, max = limits::MAX_SUBPAGES))]
    pub subpages: Vec<SubPage>,
}

/// One sub-page.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubPage {
    /// Its sub-code, 0000 to 3F7E.
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
    /// The input cursor (INPUT.md section 7.1), or none.
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

/// The longest link target: the engine's `LINK_ADDRESS_BYTES`, 256 bytes.
pub const LINK_TARGET_BYTES: usize = neoteletext_formats::page::LINK_ADDRESS_BYTES;
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
/// that each replaces the last and stands alone (`SERVER.md` section 5.2).
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
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(tag = "packet", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackedLine {
    /// X/27 of designation code 1 to 3: thirty-nine values of four bits.
    X27Nibbles {
        /// 1 to 3.
        #[schemars(range(min = 1, max = 3))]
        designation: u8,
        /// The values, 0 to 15 each.
        #[schemars(length(min = NIBBLES, max = NIBBLES))]
        nibbles: Vec<u8>,
    },
    /// X/27 of designation code 4 to 15: thirteen triplets.
    X27 {
        /// 4 to 15.
        #[schemars(range(min = 4, max = DESIGNATION_MAX))]
        designation: u8,
        /// The triplets.
        triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
    },
    /// X/28: thirteen triplets.
    X28 {
        /// 0 to 15.
        #[schemars(range(max = DESIGNATION_MAX))]
        designation: u8,
        /// The triplets.
        triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
    },
    /// M/29, the magazine's: thirteen triplets.
    M29 {
        /// 0 to 15.
        #[schemars(range(max = DESIGNATION_MAX))]
        designation: u8,
        /// The triplets.
        triplets: [Triplet; neoteletext_core::consts::TRIPLETS_PER_PACKET],
    },
}

/// The input cursor (INPUT.md section 7.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    /// Its row, 0 to 24.
    #[schemars(range(max = LAST_ROW))]
    pub row: u8,
    /// Its column, 0 to 39.
    #[schemars(range(max = LAST_COLUMN))]
    pub column: u8,
    /// A block cursor rather than an underline.
    pub block: bool,
}

/// `sound`, by `kind` (the engine's audio profile, `SERVER.md` section 9).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Sound {
    /// A stored Opus sound (decision 21).
    Opus {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Every packet's length, 1 to 40 bytes.
        #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
        packet_bytes: u16,
        /// Its length in samples at 48 kHz.
        #[schemars(range(max = MOST_SAMPLES))]
        samples: u32,
        /// Its packets back to back, exact base64, at most two minutes.
        packets: Base64,
    },
    /// A live Opus sound's object.
    OpusLive {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Every packet's length.
        #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
        packet_bytes: u16,
    },
    /// A live Opus sound's next packets.
    OpusPackets {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Every packet's length.
        #[schemars(range(min = 1, max = OPUS_MOST_PACKET_BYTES))]
        packet_bytes: u16,
        /// The first packet's block number.
        first: u16,
        /// The packets, exact base64.
        packets: Base64,
    },
    /// A stored sound as 16-bit samples, which the server encodes as IMA
    /// ADPCM, for receivers that decode no Opus.
    Object {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Its sample rate, one of the profile's.
        #[schemars(extend("enum" = SAMPLE_RATES))]
        rate: u32,
        /// 16-bit little-endian mono samples, exact base64, at most two
        /// minutes.
        pcm: Base64,
    },
    /// A live sound's object.
    Live {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Its sample rate.
        #[schemars(extend("enum" = SAMPLE_RATES))]
        rate: u32,
    },
    /// A live sound's next blocks of samples.
    Blocks {
        /// Who it is for.
        connections: Connections,
        /// Its handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// Its sample rate.
        #[schemars(extend("enum" = SAMPLE_RATES))]
        rate: u32,
        /// The first block's number.
        first: u16,
        /// The samples, exact base64.
        pcm: Base64,
    },
    /// Play a sound on an audio channel.
    Play {
        /// Who it is for.
        connections: Connections,
        /// The audio channel, 0 to 3.
        #[schemars(range(max = LAST_AUDIO_CHANNEL))]
        channel: u8,
        /// The sound's handle.
        handle: u8,
        /// Its generation.
        generation: u16,
        /// The gain, 0 to 100.
        #[schemars(range(max = MOST_GAIN))]
        gain: u8,
        /// The fade in, in milliseconds.
        fade_ms: u16,
        /// It loops.
        r#loop: bool,
        /// It plays after the sound playing ends.
        after: bool,
        /// How far other channels duck under it, 0 to 6.
        #[schemars(range(max = MOST_DUCK))]
        duck: u8,
        /// The page it belongs to, or none for the whole service.
        page: Option<PageNumber>,
    },
    /// Stop an audio channel.
    Stop {
        /// Who it is for.
        connections: Connections,
        /// The audio channel, 0 to 3.
        #[schemars(range(max = LAST_AUDIO_CHANNEL))]
        channel: u8,
        /// The fade out, in milliseconds.
        fade_ms: u16,
    },
    /// Change an audio channel's gain.
    Gain {
        /// Who it is for.
        connections: Connections,
        /// The audio channel, 0 to 3.
        #[schemars(range(max = LAST_AUDIO_CHANNEL))]
        channel: u8,
        /// The gain, 0 to 100.
        #[schemars(range(max = MOST_GAIN))]
        gain: u8,
        /// The fade, in milliseconds.
        fade_ms: u16,
    },
}

/// `cookie`: sets or clears one of the program's cookies for a connection
/// (`SERVER.md` section 8.2), every attribute a field of its own; a raw
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
    /// `Max-Age` in seconds, or none.
    pub max_age: Option<u32>,
    /// `Expires`, in seconds since the Unix epoch, or none.
    pub expires: Option<u64>,
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

/// Reads one line from a program, without its newline: the message, or
/// why it is refused whole.
///
/// # Errors
/// Any rule of REQ-1475 broken, the first found.
pub fn read(line: &[u8]) -> Result<ProgramMessage, Refusal> {
    if line.len() > LONGEST_LINE {
        return Err(Refusal::whole(None, Rule::LineTooLong, format!("{} bytes, at most {LONGEST_LINE}", line.len())));
    }
    let message: ProgramMessage = serde_json::from_slice(line).map_err(|error| check::serde_refusal(line, &error))?;
    check::message(&message)?;
    Ok(message)
}

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
#[allow(dead_code, reason = "only its schema is used")]
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
mod tests;
