//! JSON for the application protocol (`docs/applications.md`): a value
//! tree read with the engine's bounded pull reader (`neoteletext_web::json`,
//! decision 5), and a writer for the lines the server sends. Nesting is
//! the reader's `MAX_JSON_DEPTH`; the host bounds a line's length.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_web::json::{JsonReader, JsonStr, Token, integer};

/// A JSON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as an integer; any other number is refused.
    Integer(i64),
    /// A string.
    Text(String),
    /// An array.
    Array(Vec<Self>),
    /// An object, its members in order.
    Object(Vec<(String, Self)>),
}

impl Value {
    /// The member `name` of an object.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members.iter().find(|(key, _)| key == name).map(|(_, value)| value),
            _ => None,
        }
    }

    /// The string, if this is one.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The integer, if this is one.
    #[must_use]
    pub const fn integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The boolean, if this is one.
    #[must_use]
    pub const fn boolean(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The items, if this is an array.
    #[must_use]
    pub fn items(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }
}

/// Why a line is not a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonFault {
    /// The reader refused it: bad grammar, encoding or nesting.
    Grammar,
    /// A number that is not an integer of 64 bits.
    Number,
    /// More than one value, or none.
    Shape,
}

/// Reads one JSON value from `line`.
///
/// # Errors
/// A line that is not exactly one value this module reads.
pub fn parse(line: &[u8]) -> Result<Value, JsonFault> {
    let mut reader = JsonReader::new(line).map_err(|_| JsonFault::Grammar)?;
    let first = reader.next_token().map_err(|_| JsonFault::Grammar)?.ok_or(JsonFault::Shape)?;
    let value = value_from(&mut reader, first)?;
    match reader.next_token() {
        Ok(None) => Ok(value),
        Ok(Some(_)) => Err(JsonFault::Shape),
        Err(_) => Err(JsonFault::Grammar),
    }
}

fn decoded(text: JsonStr<'_>) -> Result<String, JsonFault> {
    let mut buffer = alloc::vec![0_u8; text.raw_len()];
    let length = text.decode_into(&mut buffer).map_err(|_| JsonFault::Grammar)?;
    buffer.truncate(length);
    String::from_utf8(buffer).map_err(|_| JsonFault::Grammar)
}

/// The value that begins with `first`. The reader limits the nesting, so
/// the recursion is bounded by `MAX_JSON_DEPTH`.
fn value_from<'a>(reader: &mut JsonReader<'a>, first: Token<'a>) -> Result<Value, JsonFault> {
    fn next<'a>(reader: &mut JsonReader<'a>) -> Result<Token<'a>, JsonFault> {
        reader.next_token().map_err(|_| JsonFault::Grammar)?.ok_or(JsonFault::Shape)
    }
    match first {
        Token::Null => Ok(Value::Null),
        Token::Bool(value) => Ok(Value::Bool(value)),
        Token::Number(text) => integer(text).map(Value::Integer).ok_or(JsonFault::Number),
        Token::Str(text) => decoded(text).map(Value::Text),
        Token::BeginArray => {
            let mut items = Vec::new();
            loop {
                match next(reader)? {
                    Token::EndArray => return Ok(Value::Array(items)),
                    token => items.push(value_from(reader, token)?),
                }
            }
        }
        Token::BeginObject => {
            let mut members = Vec::new();
            loop {
                match next(reader)? {
                    Token::EndObject => return Ok(Value::Object(members)),
                    Token::Key(key) => {
                        let name = decoded(key)?;
                        let token = next(reader)?;
                        members.push((name, value_from(reader, token)?));
                    }
                    _ => return Err(JsonFault::Shape),
                }
            }
        }
        Token::EndArray | Token::EndObject | Token::Key(_) => Err(JsonFault::Shape),
    }
}

/// A JSON object written member by member, for one line.
#[derive(Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
    members: usize,
}

impl Writer {
    /// An object of type `kind`: `{"type":"kind"`.
    #[must_use]
    pub fn new(kind: &str) -> Self {
        let mut writer = Self { bytes: Vec::new(), members: 0 };
        writer.bytes.push(b'{');
        writer.text("type", kind);
        writer
    }

    fn name(&mut self, name: &str) {
        if self.members > 0 {
            self.bytes.push(b',');
        }
        self.members = self.members.saturating_add(1);
        string(&mut self.bytes, name);
        self.bytes.push(b':');
    }

    /// A string member.
    pub fn text(&mut self, name: &str, value: &str) -> &mut Self {
        self.name(name);
        string(&mut self.bytes, value);
        self
    }

    /// An integer member.
    pub fn integer(&mut self, name: &str, value: i64) -> &mut Self {
        self.name(name);
        self.bytes.extend_from_slice(alloc::format!("{value}").as_bytes());
        self
    }

    /// A boolean member.
    pub fn boolean(&mut self, name: &str, value: bool) -> &mut Self {
        self.name(name);
        self.bytes.extend_from_slice(if value { b"true" } else { b"false" });
        self
    }

    /// A member whose value is another object, already written.
    pub fn object(&mut self, name: &str, value: Self) -> &mut Self {
        self.name(name);
        self.bytes.extend_from_slice(&value.finish_object());
        self
    }

    fn finish_object(mut self) -> Vec<u8> {
        self.bytes.push(b'}');
        self.bytes
    }

    /// The finished line, with its newline.
    #[must_use]
    pub fn line(self) -> Vec<u8> {
        let mut bytes = self.finish_object();
        bytes.push(b'\n');
        bytes
    }

    /// A nested object, without the `type` member.
    #[must_use]
    pub fn nested() -> Self {
        Self { bytes: alloc::vec![b'{'], members: 0 }
    }
}

/// `text` as a JSON string (RFC 8259 section 7): quotes, backslashes and
/// control characters escaped; everything else as its UTF-8.
fn string(bytes: &mut Vec<u8>, text: &str) {
    bytes.push(b'"');
    for character in text.chars() {
        match character {
            '"' => bytes.extend_from_slice(b"\\\""),
            '\\' => bytes.extend_from_slice(b"\\\\"),
            '\n' => bytes.extend_from_slice(b"\\n"),
            '\r' => bytes.extend_from_slice(b"\\r"),
            '\t' => bytes.extend_from_slice(b"\\t"),
            control if u32::from(control) < 0x20 || control == '\u{7F}' => {
                bytes.extend_from_slice(alloc::format!("\\u{:04x}", u32::from(control)).as_bytes());
            }
            other => {
                let mut utf8 = [0_u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut utf8).as_bytes());
            }
        }
    }
    bytes.push(b'"');
}

#[cfg(test)]
mod tests;
