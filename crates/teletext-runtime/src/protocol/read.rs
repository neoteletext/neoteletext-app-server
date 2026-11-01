//! The protocol's strict reader (REQ-1475): one line from a program, read
//! whole and refused whole, every refusal naming the path of its first
//! fault and a rule from the closed list [`Rule`].
//!
//! **Why the reader follows the tags itself.** serde reads an internally
//! tagged enum (`type`, a sound's `kind`, a packed line's `packet`) by
//! buffering the object and reading the variant from the buffer, which
//! loses the path of any fault inside it. So the line is read as a JSON
//! value, the tag is taken out by hand, and the variant's own type is read
//! from what is left with `serde_path_to_error`, which keeps the path.
//! Reading into a value would let a repeated key win silently, so the line
//! is first scanned for repeated keys, with their paths.
//!
//! The scan and the value are bounded: the line is at most
//! [`LONGEST_LINE`] bytes, and `serde_json` refuses nesting deeper than 128.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;

use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::check::{self, Refusal};
use super::{LONGEST_LINE, M29Line, MessageType, PackedLine, ProgramMessage, Rule, Sound, X27Line, X27NibblesLine, X28Line};

/// Marks the path of a fault inside a packed line, which serde's error for
/// it can carry only as text: the path, then the message (see
/// [`PackedLine`]'s reading below).
const INNER_PATH: char = '\u{1}';

/// A fault the reader found: where, which rule, and in words.
struct Fault {
    path: String,
    rule: Rule,
    detail: String,
}

impl Fault {
    fn new(path: &str, rule: Rule, detail: impl Into<String>) -> Self {
        Self { path: path.to_owned(), rule, detail: detail.into() }
    }

    fn refusal(self, message: Option<MessageType>) -> Refusal {
        Refusal { message, path: self.path, rule: self.rule, detail: self.detail }
    }
}

/// Reads one line from a program, without its newline: the message, or
/// why it is refused whole.
///
/// # Errors
/// Any rule of REQ-1475 broken, the first found, with its path.
pub fn read(line: &[u8]) -> Result<ProgramMessage, Refusal> {
    if line.len() > LONGEST_LINE {
        return Err(Refusal::whole(None, Rule::LineTooLong, format!("{} bytes, at most {LONGEST_LINE}", line.len())));
    }
    let syntax = |error: &serde_json::Error| Refusal::whole(None, Rule::Syntax, error.to_string());
    let mut scanner = serde_json::Deserializer::from_slice(line);
    Scan { path: String::new() }.deserialize(&mut scanner).map_err(|error| scan_refusal(&error))?;
    scanner.end().map_err(|error| syntax(&error))?;
    let value: Value = serde_json::from_slice(line).map_err(|error| syntax(&error))?;
    let Value::Object(mut object) = value else { return Err(Refusal::whole(None, Rule::WrongType, "a message is a JSON object".to_string())) };
    let kind: MessageType = tag(&mut object, "type", "").map_err(|fault| fault.refusal(None))?;
    let message = body(kind, object).map_err(|fault| fault.refusal(Some(kind)))?;
    check::message(&message)?;
    Ok(message)
}

/// The message of type `kind` from the rest of its object.
fn body(kind: MessageType, object: Map<String, Value>) -> Result<ProgramMessage, Fault> {
    Ok(match kind {
        MessageType::Reply => ProgramMessage::Reply(typed(object, "")?),
        MessageType::Page => ProgramMessage::Page(Box::new(typed(object, "")?)),
        MessageType::Subpages => ProgramMessage::Subpages(typed(object, "")?),
        MessageType::Index => ProgramMessage::Index(typed(object, "")?),
        MessageType::Redirect => ProgramMessage::Redirect(typed(object, "")?),
        MessageType::Denied => ProgramMessage::Denied(typed(object, "")?),
        MessageType::Sound => ProgramMessage::Sound(sound(object)?),
        MessageType::Cookie => ProgramMessage::Cookie(typed(object, "")?),
        MessageType::AuthorLog => ProgramMessage::AuthorLog(typed(object, "")?),
        MessageType::Log => ProgramMessage::Log(typed(object, "")?),
        MessageType::Packets => ProgramMessage::Packets(typed(object, "")?),
    })
}

/// The kinds of sound, as `kind` names them.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SoundKind {
    Opus,
    OpusLive,
    OpusPackets,
    Object,
    Live,
    Blocks,
    Play,
    Stop,
    Gain,
}

fn sound(mut object: Map<String, Value>) -> Result<Sound, Fault> {
    let kind: SoundKind = tag(&mut object, "kind", "")?;
    Ok(match kind {
        SoundKind::Opus => Sound::Opus(typed(object, "")?),
        SoundKind::OpusLive => Sound::OpusLive(typed(object, "")?),
        SoundKind::OpusPackets => Sound::OpusPackets(typed(object, "")?),
        SoundKind::Object => Sound::Object(typed(object, "")?),
        SoundKind::Live => Sound::Live(typed(object, "")?),
        SoundKind::Blocks => Sound::Blocks(typed(object, "")?),
        SoundKind::Play => Sound::Play(typed(object, "")?),
        SoundKind::Stop => Sound::Stop(typed(object, "")?),
        SoundKind::Gain => Sound::Gain(typed(object, "")?),
    })
}

/// The kinds of packed line, as `packet` names them.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LineKind {
    X27Nibbles,
    X27,
    X28,
    M29,
}

fn packed_line(mut object: Map<String, Value>) -> Result<PackedLine, Fault> {
    let kind: LineKind = tag(&mut object, "packet", "")?;
    Ok(match kind {
        LineKind::X27Nibbles => PackedLine::X27Nibbles(typed::<X27NibblesLine>(object, "")?),
        LineKind::X27 => PackedLine::X27(typed::<X27Line>(object, "")?),
        LineKind::X28 => PackedLine::X28(typed::<X28Line>(object, "")?),
        LineKind::M29 => PackedLine::M29(typed::<M29Line>(object, "")?),
    })
}

/// A packed line inside a page: read by following `packet`, as the message
/// is, and a fault inside it carries its path out within serde's error
/// text, marked, which [`fault_of`] joins to the path outside.
impl<'de> Deserialize<'de> for PackedLine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Value::Object(object) = Value::deserialize(deserializer)? else {
            return Err(de::Error::invalid_type(de::Unexpected::Other("not an object"), &"a packed line, an object with `packet`"));
        };
        packed_line(object).map_err(|fault| de::Error::custom(format!("{INNER_PATH}{}{INNER_PATH}{:?}{INNER_PATH}{}", fault.path, fault.rule, fault.detail)))
    }
}

/// Takes the tag `name` out of `object` and reads it as `T`, a closed
/// list: missing, not a string, or not in the list are refused at
/// `at.name`.
fn tag<T: DeserializeOwned>(object: &mut Map<String, Value>, name: &str, at: &str) -> Result<T, Fault> {
    let path = join(at, name);
    let value = object.remove(name).ok_or_else(|| Fault::new(at, Rule::MissingField, format!("missing field `{name}`")))?;
    if !value.is_string() {
        return Err(Fault::new(&path, Rule::WrongType, format!("`{name}` is a string")));
    }
    T::deserialize(value).map_err(|error| Fault::new(&path, Rule::NotListed, error.to_string()))
}

/// Reads `object` as `T`, keeping the path of a fault, under `at`.
fn typed<T: DeserializeOwned>(object: Map<String, Value>, at: &str) -> Result<T, Fault> {
    serde_path_to_error::deserialize(Value::Object(object)).map_err(|error| {
        let path = error.path().to_string();
        let inner = error.into_inner();
        fault_of(&join(at, if path == "." { "" } else { &path }), &inner)
    })
}

/// The fault serde's `error` at `path` is: the rule from its words, and a
/// packed line's own path and rule when it carries them.
fn fault_of(path: &str, error: &serde_json::Error) -> Fault {
    let text = error.to_string();
    if let Some(inner) = text.strip_prefix(INNER_PATH) {
        let mut parts = inner.splitn(3, INNER_PATH);
        let (inner_path, rule, detail) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        return Fault::new(&join(path, inner_path), rule_named(rule), detail);
    }
    // serde names a field missing or repeated; the path is of the object
    // that holds it, so the field is joined to it.
    let field = |prefix: &str| text.strip_prefix(prefix).and_then(|rest| rest.split('`').nth(1)).map(|name| join(path, name));
    // The path of an unknown field already ends with its name.
    if text.starts_with("unknown field ") {
        return Fault::new(path, Rule::UnknownField, text);
    }
    if let Some(at) = field("missing field ") {
        return Fault::new(&at, Rule::MissingField, text);
    }
    if let Some(at) = field("duplicate field ") {
        return Fault::new(&at, Rule::DuplicateField, text);
    }
    let rule = if text.starts_with("unknown variant") { Rule::NotListed } else { Rule::WrongType };
    Fault::new(path, rule, text)
}

/// A rule by its name, as [`PackedLine`]'s reading writes it.
fn rule_named(name: &str) -> Rule {
    match name {
        "UnknownField" => Rule::UnknownField,
        "MissingField" => Rule::MissingField,
        "DuplicateField" => Rule::DuplicateField,
        "NotListed" => Rule::NotListed,
        _ => Rule::WrongType,
    }
}

/// `at` and `name` joined as a path: `a.b`, `a[3]`, or `name` alone.
fn join(at: &str, name: &str) -> String {
    match (at.is_empty(), name.is_empty(), name.starts_with('[')) {
        (_, true, _) => at.to_owned(),
        (true, false, _) | (false, false, true) => format!("{at}{name}"),
        (false, false, false) => format!("{at}.{name}"),
    }
}

/// A repeated key, found by the scan, carried out of serde as its error's
/// text: the path, marked.
fn scan_refusal(error: &serde_json::Error) -> Refusal {
    let text = error.to_string();
    match text.strip_prefix(INNER_PATH).and_then(|rest| rest.split_once(INNER_PATH)) {
        Some((path, key)) => Refusal { message: None, path: path.to_owned(), rule: Rule::DuplicateField, detail: format!("`{key}` is given twice") },
        None => Refusal::whole(None, Rule::Syntax, text),
    }
}

/// Walks a line's JSON, refusing a key given twice in one object, with the
/// path of the second.
struct Scan {
    path: String,
}

impl<'de> DeserializeSeed<'de> for Scan {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Scan {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys: BTreeSet<String> = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let path = join(&self.path, &key);
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom(format!("{INNER_PATH}{path}{INNER_PATH}{key}")));
            }
            map.next_value_seed(Self { path })?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut index: usize = 0;
        while seq.next_element_seed(Self { path: format!("{}[{index}]", self.path) })?.is_some() {
            index = index.saturating_add(1);
        }
        Ok(())
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
}
