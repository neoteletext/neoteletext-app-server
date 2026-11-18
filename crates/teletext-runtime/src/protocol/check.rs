//! The rules serde cannot state, checked over a message once it is read
//! (REQ-1475, REQ-1477, REQ-1478): every bound from the engine's
//! `neoteletext_core::limits`, every text's characters, every base64's
//! exactness and decoded length, orders and duplicates. The first fault
//! found refuses the message whole, with its path.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use neoteletext_core::limits;
use neoteletext_core::packet::{Packet, is_valid_stream_packet};
use neoteletext_core::policy::is_safe_output_char;
use neoteletext_core::types::SubCode;
use neoteletext_formats::log::LogRefused;
use neoteletext_web::base64;
use neoteletext_web::records::redirect::{MAX_ADDRESS, RedirectRecord, encode};
use unicode_normalization::{IsNormalized, is_nfc, is_nfc_quick};

use super::{
    AuthorLog, COLUMNS, Connections, DENIED_STATUSES, DESCRIPTION_TEXT_CHARACTERS, Denied, Description, Index, Log, Magazine, MessageType,
    OPUS_MOST_PACKET_BYTES, PackedLine, Packets, Page, PageMessage, ProgramMessage, Redirect, Refused, Reply, RequestNumber, Rule, SAFE_INTEGER_LIMIT,
    SAMPLE_RATES, Sound, SubPages, TITLE_CHARACTERS,
};
use super::{
    BlocksSound, GainSound, LiveSound, M29Line, ObjectSound, OpusLiveSound, OpusPacketsSound, OpusSound, PlaySound, StopSound, X27Line, X27NibblesLine, X28Line,
};

mod instrument;
mod rules;

/// Why a message was refused: what `refused` carries back.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Refusal {
    /// The type of the message, when it had one the protocol knows.
    pub message: Option<MessageType>,
    /// The path to the first fault; empty for the message as a whole.
    pub path: String,
    /// The rule broken.
    pub rule: Rule,
    /// The fault in words.
    pub detail: String,
}

impl Refusal {
    /// A refusal of the message as a whole.
    #[must_use]
    pub const fn whole(message: Option<MessageType>, rule: Rule, detail: String) -> Self {
        Self { message, path: String::new(), rule, detail }
    }

    /// The `refused` message that carries it.
    #[must_use]
    pub fn refused(self) -> Refused {
        Refused { message: self.message, path: self.path, rule: self.rule, detail: self.detail, request: None, sound: None }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?} ({})", self.path, self.rule, self.detail)
    }
}

/// Where a value is in its message, built only when a fault is found.
#[derive(Clone, Copy)]
pub(crate) enum Path<'a> {
    /// The message.
    Root,
    /// A field of what `0` is.
    Field(&'a Path<'a>, &'static str),
    /// An item of the list `0` is.
    Item(&'a Path<'a>, usize),
}

impl Path<'_> {
    const fn field<'b>(&'b self, name: &'static str) -> Path<'b> {
        Path::Field(self, name)
    }

    const fn item(&self, index: usize) -> Path<'_> {
        Path::Item(self, index)
    }
}

impl fmt::Display for Path<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Path::Root => Ok(()),
            Path::Field(Path::Root, name) => formatter.write_str(name),
            Path::Field(parent, name) => write!(formatter, "{parent}.{name}"),
            Path::Item(parent, index) => write!(formatter, "{parent}[{index}]"),
        }
    }
}

/// A fault at `path`, before the message's type is known to it.
pub(crate) struct Fault {
    path: String,
    rule: Rule,
    detail: String,
}

type Checked = Result<(), Fault>;

fn fault(path: &Path<'_>, rule: Rule, detail: String) -> Fault {
    Fault { path: path.to_string(), rule, detail }
}

/// Checks a message read, and refuses it with its type at the first fault.
///
/// # Errors
/// The first fault.
pub fn message(message: &ProgramMessage) -> Result<(), Refusal> {
    let root = Path::Root;
    let checked = match message {
        ProgramMessage::Reply(reply) => check_reply(&root, reply),
        ProgramMessage::Page(message) => {
            let PageMessage { connections, request, page } = message.as_ref();
            check_connections(&root.field("connections"), connections)
                .and_then(|()| check_answer(&root, connections, *request))
                .and_then(|()| check_page(&root.field("page"), page))
        }
        ProgramMessage::Subpages(list) => check_subpages(&root, list),
        ProgramMessage::Index(Index { connections, magazines }) => {
            check_connections(&root.field("connections"), connections).and_then(|()| check_magazines(&root.field("magazines"), magazines))
        }
        ProgramMessage::Redirect(redirect) => check_redirect(&root, redirect),
        ProgramMessage::Denied(denied) => check_denied(&root, denied),
        ProgramMessage::Description(description) => check_description(&root, description),
        ProgramMessage::Measure(measure) => check_page(&root.field("page"), &measure.page),
        ProgramMessage::Sound(sound) => check_sound(&root, sound),
        ProgramMessage::AuthorLog(log) => check_author_log(&root, log),
        ProgramMessage::Log(log) => check_log(&root, log),
        ProgramMessage::Packets(Packets { connections, packets }) => {
            check_connections(&root.field("connections"), connections).and_then(|()| check_packets(&root.field("packets"), packets))
        }
        ProgramMessage::Regis(regis) => check_regis(&root, regis),
        ProgramMessage::Naplps(naplps) => check_naplps(&root, naplps),
        ProgramMessage::RegisLinks(links) => check_regis_links(&root, links),
        ProgramMessage::RegisCursor(cursor) => {
            check_connections(&root.field("connections"), &cursor.connections).and_then(|()| check_shown_subpage(&root, cursor.sub))
        }
        // VS-352: a connection's number; a handle was read whole by its
        // type, and whether the channel holds it is the channel's to say.
        ProgramMessage::RequestState(super::RequestState { connection }) | ProgramMessage::OfferRemember(super::OfferRemember { connection }) => {
            check_connection(&root.field("connection"), *connection)
        }
        ProgramMessage::EndState(_) => Ok(()),
        ProgramMessage::Rules(rules) => check_rules(&root, rules).map(|_| ()),
        ProgramMessage::QueryState(query) => check_connection(&root.field("connection"), query.connection),
        ProgramMessage::Frame(frame) => check_connections(&root.field("connections"), &frame.connections).and_then(|()| check_shown_subpage(&root, frame.sub)),
        ProgramMessage::Animate(animate) => {
            check_connections(&root.field("connections"), &animate.connections).and_then(|()| rules::operations(&root.field("ops"), &animate.ops).map(|_| ()))
        }
        ProgramMessage::Sprites(sprites) => check_sprites(&root, sprites),
        ProgramMessage::NaplpsObjects(objects) => check_naplps_objects(&root, objects),
        ProgramMessage::SpriteSheet(sheet) => check_connections(&root.field("connections"), &sheet.connections).and_then(|()| {
            check_base64(&root.field("data"), &sheet.data, neoteletext_web::consts::MAX_SHEET_BYTES).map(|_| ()).map_err(|refused| match refused.rule {
                // More than a whole sheet in one message.
                Rule::TooLong => Fault { rule: Rule::SheetTooLarge, ..refused },
                _ => refused,
            })
        }),
        ProgramMessage::Timeline(timeline) => {
            check_connections(&root.field("connections"), &timeline.connections).and_then(|()| check_timeline(&root, timeline).map(|_| ()))
        }
        ProgramMessage::TimelineStop(stop) => check_connections(&root.field("connections"), &stop.connections),
        ProgramMessage::LocalMidi(message) => check_command(&root, &super::CommandMessage::LocalMidi(message.clone())),
        ProgramMessage::Variable(message) => check_command(&root, &super::CommandMessage::Variable(message.clone())),
        ProgramMessage::Timer(message) => check_command(&root, &super::CommandMessage::Timer(message.clone())),
        ProgramMessage::Report(message) => check_command(&root, &super::CommandMessage::Report(message.clone())),
        ProgramMessage::Run(message) => check_command(&root, &super::CommandMessage::Run(message.clone())),
        ProgramMessage::Sequence(message) => check_command(&root, &super::CommandMessage::Sequence(message.clone())),
        ProgramMessage::ObjectPage(message) => {
            check_connections(&root.field("connections"), &message.connections).and_then(|()| check_shown_subpage(&root, message.sub)).and_then(|()| {
                crate::object_page::packets(&message.object).map(|_| ()).map_err(|detail| fault(&root.field("object"), Rule::InvalidObjectPage, detail))
            })
        }
    };
    checked.map_err(|Fault { path, rule, detail }| Refusal { message: Some(message.kind()), path, rule, detail })
}

// ----------------------------------------------------------- the pieces

/// A language of the server's closed list (REQ-2215).
fn check_language(path: &Path<'_>, tag: &super::LanguageTag) -> Checked {
    if super::language_tags().contains(&tag.0) { Ok(()) } else { Err(fault(path, Rule::UnknownLanguage, format!("{:?} is not one of start.languages", tag.0))) }
}

/// A page's language and text runs (REQ-2214 items 2 and 4): the language
/// instead of a character set, at most 64 runs, each on rows 1 to 24 and
/// columns 0 to 39, of 1 to 40 characters of text a page may hold, in a
/// language of the list. Where the runs fall and what they cost is the
/// encoder's to say ([`crate::text`]).
fn check_text_runs(path: &Path<'_>, page: &Page) -> Checked {
    if let Some(language) = &page.language {
        if page.character_set.is_some() {
            return Err(fault(
                &path.field("character_set"),
                Rule::LanguageAndCharacterSet,
                "a page names its language or its character set, never both".to_string(),
            ));
        }
        check_language(&path.field("language"), language)?;
    }
    let Some(runs) = &page.text else { return Ok(()) };
    let at = path.field("text");
    if runs.len() > super::MOST_TEXT_RUNS {
        return Err(fault(&at, Rule::TooLong, format!("{} runs, at most {}", runs.len(), super::MOST_TEXT_RUNS)));
    }
    for (index, run) in runs.iter().enumerate() {
        let item = at.item(index);
        if !(super::FIRST_TEXT_ROW..=LAST_TEXT_ROW).contains(&run.row) {
            return Err(fault(&item.field("row"), Rule::OutOfRange, format!("{}: rows {} to {LAST_TEXT_ROW}", run.row, super::FIRST_TEXT_ROW)));
        }
        if usize::from(run.column) >= COLUMNS {
            return Err(fault(&item.field("column"), Rule::OutOfRange, format!("{}: columns 0 to {}", run.column, COLUMNS - 1)));
        }
        check_text(&item.field("text"), &run.text, 1, super::TEXT_RUN_CHARACTERS)?;
        if let Some(language) = &run.language {
            check_language(&item.field("language"), language)?;
        }
    }
    Ok(())
}

/// Text of at most `most` characters, none a control or a bidirectional
/// formatting character (REQ-1475 item 6).
fn check_text(path: &Path<'_>, text: &str, least: usize, most: usize) -> Checked {
    let count = text.chars().count();
    if count < least {
        return Err(fault(path, Rule::TooShort, format!("{count} characters, at least {least}")));
    }
    if count > most {
        return Err(fault(path, Rule::TooLong, format!("{count} characters, at most {most}")));
    }
    if let Some((place, character)) = text.chars().enumerate().find(|&(_, character)| !is_safe_output_char(character)) {
        return Err(fault(path, Rule::ForbiddenCharacter, format!("U+{:04X} at character {place}", u32::from(character))));
    }
    check_nfc(path, text)
}

/// Text in Unicode normalisation form C (REQ-1475 item 6; Unicode Standard
/// Annex 15): the quick check settles almost every text, the full check the
/// rest.
fn check_nfc(path: &Path<'_>, text: &str) -> Checked {
    let normal = match is_nfc_quick(text.chars()) {
        IsNormalized::Yes => true,
        IsNormalized::No => false,
        IsNormalized::Maybe => is_nfc(text),
    };
    if normal { Ok(()) } else { Err(fault(path, Rule::NotNfc, "not in Unicode normalisation form C".to_string())) }
}

/// Exact base64's bytes (RFC 4648 section 4): padded, no whitespace, and
/// canonical, the text the bytes encode to. `most` bounds the decoded
/// length before anything is decoded.
fn check_base64(path: &Path<'_>, text: &str, most: usize) -> Result<Vec<u8>, Fault> {
    if !text.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')) {
        return Err(fault(path, Rule::NotBase64, "a character outside base64's alphabet (RFC 4648 section 4)".to_string()));
    }
    if text.len() > base64::encoded_len(most) {
        return Err(fault(path, Rule::TooLong, format!("over {most} bytes once decoded")));
    }
    let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(text.len())];
    let length = base64::decode(text.as_bytes(), &mut bytes).map_err(|error| fault(path, Rule::NotBase64, format!("{error:?}")))?;
    bytes.truncate(length);
    let mut again = alloc::vec![0_u8; base64::encoded_len(bytes.len())];
    let written = base64::encode(&bytes, &mut again).map_err(|error| fault(path, Rule::NotBase64, format!("{error:?}")))?;
    if again.get(..written) != Some(text.as_bytes()) {
        return Err(fault(path, Rule::NotBase64, "not the canonical base64 of its bytes".to_string()));
    }
    Ok(bytes)
}

/// A connection's number: 1 to 2^53 less one, which every language's
/// JSON reader holds exactly.
fn check_connection(path: &Path<'_>, connection: super::ConnectionId) -> Checked {
    if (1..=SAFE_INTEGER_LIMIT).contains(&connection) { Ok(()) } else { Err(fault(path, Rule::OutOfRange, format!("{connection}, 1 to {SAFE_INTEGER_LIMIT}"))) }
}

fn check_connections(path: &Path<'_>, connections: &Connections) -> Checked {
    let Connections::Listed(listed) = connections else { return Ok(()) };
    if listed.is_empty() {
        return Err(fault(path, Rule::TooShort, "no connection named; \"all\" names every one".to_string()));
    }
    if listed.len() > super::MOST_NAMED_CONNECTIONS {
        return Err(fault(path, Rule::TooLong, format!("{} connections, at most {}", listed.len(), super::MOST_NAMED_CONNECTIONS)));
    }
    for (index, &connection) in listed.iter().enumerate() {
        check_connection(&path.item(index), connection)?;
    }
    Ok(())
}

/// The most pages a magazine may have: every page number but those each
/// magazine reserves, and in magazine 1 those it alone reserves too
/// (EN 300 706 Annex A.10).
const fn most_pages(magazine: u8) -> usize {
    let every = limits::PAGE_NUMBERS_PER_MAGAZINE.saturating_sub(limits::RESERVED_IN_EVERY_MAGAZINE.len());
    if magazine == limits::FIRST_MAGAZINE { every.saturating_sub(limits::RESERVED_PAGES.len()) } else { every }
}

fn check_magazines(path: &Path<'_>, magazines: &[Magazine]) -> Checked {
    if magazines.len() > limits::MAGAZINES {
        return Err(fault(path, Rule::TooLong, format!("{} magazines, at most {}", magazines.len(), limits::MAGAZINES)));
    }
    let mut last_magazine = 0_u8;
    for (index, magazine) in magazines.iter().enumerate() {
        let at = path.item(index);
        let number = magazine.magazine;
        let first = limits::FIRST_MAGAZINE;
        let last = u8::try_from(limits::MAGAZINES).unwrap_or(u8::MAX);
        if !(first..=last).contains(&number) {
            return Err(fault(&at.field("magazine"), Rule::OutOfRange, format!("{number}, 1 to {last}")));
        }
        if number <= last_magazine {
            return Err(fault(
                &at.field("magazine"),
                if number == last_magazine { Rule::Duplicate } else { Rule::NotAscending },
                format!("{number} after {last_magazine}"),
            ));
        }
        last_magazine = number;
        check_text(&at.field("name"), &magazine.name, 0, TITLE_CHARACTERS)?;
        let pages = at.field("pages");
        if magazine.pages.is_empty() {
            return Err(fault(&pages, Rule::TooShort, "a magazine with no pages is left out".to_string()));
        }
        if magazine.pages.len() > most_pages(number) {
            return Err(fault(&pages, Rule::TooLong, format!("{} pages, at most {}", magazine.pages.len(), most_pages(number))));
        }
        let mut last_page = 0_u16;
        for (place, entry) in magazine.pages.iter().enumerate() {
            let at = pages.item(place);
            let page = entry.page;
            if page.magazine() != number {
                return Err(fault(&at.field("page"), Rule::WrongMagazine, format!("{:03X} is not in magazine {number}", page.0)));
            }
            if page.is_reserved() {
                return Err(fault(&at.field("page"), Rule::Reserved, format!("{:03X} is reserved by the standard (EN 300 706 Annex A.10)", page.0)));
            }
            if page.0 <= last_page {
                return Err(fault(
                    &at.field("page"),
                    if page.0 == last_page { Rule::Duplicate } else { Rule::NotAscending },
                    format!("{:03X} after {last_page:03X}", page.0),
                ));
            }
            last_page = page.0;
            check_text(&at.field("title"), &entry.title, 0, TITLE_CHARACTERS)?;
            if let Some(section) = &entry.section {
                check_text(&at.field("section"), section, 0, TITLE_CHARACTERS)?;
            }
            if entry.subpages == 0 || usize::from(entry.subpages) > limits::MAX_SUBPAGES {
                return Err(fault(&at.field("subpages"), Rule::OutOfRange, format!("{}, 1 to {}", entry.subpages, limits::MAX_SUBPAGES)));
            }
        }
    }
    Ok(())
}

fn check_reply(root: &Path<'_>, reply: &Reply) -> Checked {
    check_connection(&root.field("connection"), reply.connection)?;
    check_request(&root.field("request"), reply.request)?;
    check_magazines(&root.field("magazines"), &reply.magazines)?;
    let Some(page) = &reply.selected else { return Ok(()) };
    let at = root.field("selected");
    check_page(&at, page)?;
    if !reply.magazines.iter().flat_map(|magazine| magazine.pages.iter()).any(|entry| entry.page == page.page) {
        return Err(fault(&at.field("page"), Rule::NotInIndex, format!("{:03X} is not in magazines", page.page.0)));
    }
    Ok(())
}

pub(crate) fn check_page(path: &Path<'_>, page: &Page) -> Checked {
    if page.page.is_reserved() {
        return Err(fault(&path.field("page"), Rule::Reserved, format!("{:03X} is reserved by the standard (EN 300 706 Annex A.10)", page.page.0)));
    }
    let sub = page.sub.0;
    if sub > limits::LAST_SUBPAGE_CODE {
        return Err(fault(&path.field("sub"), Rule::OutOfRange, format!("{sub:04X}: 3F7F means any sub-page, never one")));
    }
    check_subpage(path, page)
}

fn check_subpages(root: &Path<'_>, list: &SubPages) -> Checked {
    check_connections(&root.field("connections"), &list.connections)?;
    check_answer(root, &list.connections, list.request)?;
    if list.page.is_reserved() {
        return Err(fault(&root.field("page"), Rule::Reserved, format!("{:03X} is reserved by the standard (EN 300 706 Annex A.10)", list.page.0)));
    }
    let entries = root.field("subpages");
    if list.subpages.is_empty() || list.subpages.len() > limits::MAX_SUBPAGES {
        return Err(fault(&entries, Rule::OutOfRange, format!("{} sub-pages, 1 to {}", list.subpages.len(), limits::MAX_SUBPAGES)));
    }
    let mut before = None;
    for (index, entry) in list.subpages.iter().enumerate() {
        let at = entries.item(index);
        if entry.sub.0 > limits::LAST_SUBPAGE_CODE {
            return Err(fault(&at.field("sub"), Rule::OutOfRange, format!("{:04X}: 3F7F means any sub-page, never one", entry.sub.0)));
        }
        if let Some(before) = before
            && entry.sub.0 <= before
        {
            let (rule, detail) = if entry.sub.0 == before {
                (Rule::Duplicate, format!("{:04X} twice", entry.sub.0))
            } else {
                (Rule::NotAscending, format!("{:04X} after {before:04X}", entry.sub.0))
            };
            return Err(fault(&at.field("sub"), rule, detail));
        }
        before = Some(entry.sub.0);
        if let Some(title) = &entry.title {
            check_text(&at.field("title"), title, 0, super::SUBPAGE_TITLE_CHARACTERS)?;
        }
    }
    Ok(())
}

fn check_subpage(path: &Path<'_>, subpage: &Page) -> Checked {
    let rows = path.field("rows");
    for (index, row) in subpage.rows.iter().enumerate() {
        let Some(row) = row else { continue };
        let at = rows.item(index);
        let bytes = check_base64(&at, row, COLUMNS)?;
        if bytes.len() != COLUMNS {
            return Err(fault(&at, Rule::WrongLength, format!("{} bytes, a row is {COLUMNS}", bytes.len())));
        }
        if let Some(column) = bytes.iter().position(|&code| code > SEVEN_BITS) {
            return Err(fault(&at, Rule::OutOfRange, format!("column {column}: a character code is seven bits")));
        }
    }
    check_link_extension(&path.field("link_extension"), &subpage.link_extension)?;
    let enhancements = path.field("enhancements");
    for (index, packet) in subpage.enhancements.iter().enumerate() {
        let at = enhancements.item(index);
        check_designation(&at.field("designation"), packet.designation, 0..=FOUR_BITS)?;
        if subpage.enhancements.iter().take(index).any(|earlier| earlier.designation == packet.designation) {
            return Err(fault(&at.field("designation"), Rule::Duplicate, format!("designation code {} twice", packet.designation)));
        }
        check_triplets(&at.field("triplets"), &packet.triplets)?;
    }
    let packets = path.field("packets");
    for (index, line) in subpage.packets.iter().enumerate() {
        let at = packets.item(index);
        match line {
            PackedLine::X27Nibbles(X27NibblesLine { designation, nibbles }) => {
                check_designation(&at.field("designation"), *designation, X27_NIBBLES)?;
                if nibbles.len() != super::NIBBLES {
                    return Err(fault(&at.field("nibbles"), Rule::WrongLength, format!("{} values, {}", nibbles.len(), super::NIBBLES)));
                }
                if let Some(place) = nibbles.iter().position(|&nibble| nibble > FOUR_BITS) {
                    return Err(fault(&at.field("nibbles").item(place), Rule::OutOfRange, "four bits".to_string()));
                }
            }
            PackedLine::X27(X27Line { designation, triplets }) => {
                check_designation(&at.field("designation"), *designation, X27_TRIPLETS)?;
                check_triplets(&at.field("triplets"), triplets)?;
            }
            PackedLine::X28(X28Line { designation, triplets }) | PackedLine::M29(M29Line { designation, triplets }) => {
                check_designation(&at.field("designation"), *designation, 0..=FOUR_BITS)?;
                check_triplets(&at.field("triplets"), triplets)?;
            }
        }
    }
    if let Some(character_set) = subpage.character_set {
        if let Some(region) = character_set.region
            && region > FOUR_BITS
        {
            return Err(fault(&path.field("character_set").field("region"), Rule::OutOfRange, format!("{region}, 0 to 15")));
        }
        if character_set.national_option > THREE_BITS {
            return Err(fault(&path.field("character_set").field("national_option"), Rule::OutOfRange, "0 to 7".to_string()));
        }
    }
    check_text_runs(path, subpage)?;
    if let Some(cursor) = subpage.cursor {
        if !(super::FIRST_CURSOR_ROW..=super::LAST_CURSOR_ROW).contains(&cursor.row) {
            let rows = format!("{}: rows {} to {}", cursor.row, super::FIRST_CURSOR_ROW, super::LAST_CURSOR_ROW);
            return Err(fault(&path.field("cursor").field("row"), Rule::OutOfRange, rows));
        }
        if usize::from(cursor.column) >= COLUMNS {
            return Err(fault(&path.field("cursor").field("column"), Rule::OutOfRange, format!("{}: columns 0 to 39", cursor.column)));
        }
    }
    Ok(())
}

/// Packet X/27's designation codes 1 to 3, which carry nibbles, and 4 to
/// 15, which carry triplets (the engine's `RawBody`; EN 300 706 clause
/// 9.6).
const X27_NIBBLES: core::ops::RangeInclusive<u8> = 1..=3;
const X27_TRIPLETS: core::ops::RangeInclusive<u8> = 4..=FOUR_BITS;

fn check_designation(path: &Path<'_>, designation: u8, allowed: core::ops::RangeInclusive<u8>) -> Checked {
    if allowed.contains(&designation) {
        Ok(())
    } else {
        Err(fault(path, Rule::OutOfRange, format!("designation code {designation}, {} to {}", allowed.start(), allowed.end())))
    }
}

/// What a link target is (Appendix E.1, the web-native concept section 3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// A page of this service.
    Here,
    /// A page of another service, or an address.
    Elsewhere,
    /// A web page, to open outside teletext.
    Web,
}

/// A link target's kind, read by the engine's own parser of TTI's link
/// targets (`tti::link_destination`), or none for a target in no form of
/// Appendix E.1: printable ASCII without spaces or commas, of at most
/// [`LINK_TARGET_BYTES`](super::LINK_TARGET_BYTES) bytes.
fn target(text: &str) -> Option<Target> {
    if text.len() > super::LINK_TARGET_BYTES || !text.bytes().all(|byte| byte.is_ascii_graphic() && byte != b',') {
        return None;
    }
    let (destination, address) = neoteletext_formats::tti::parse_target(text.as_bytes())?;
    Some(if address.is_some_and(|address| address.starts_with(neoteletext_formats::page::WEB_MARK)) {
        Target::Web
    } else if !destination.service.is_empty() || address.is_some() {
        Target::Elsewhere
    } else {
        Target::Here
    })
}

fn check_target(path: &Path<'_>, text: &str) -> Result<Target, Fault> {
    target(text).ok_or_else(|| {
        fault(path, Rule::WrongType, "a target is mpp, mpp:ssss, name:mpp, name:mpp:ssss, an http or https address, or web: and one; printable ASCII without spaces or commas, at most 256 bytes".to_string())
    })
}

fn check_link_extension(path: &Path<'_>, links: &super::LinkExtension) -> Checked {
    let keys = path.field("keys");
    if links.keys.len() > neoteletext_formats::page::LINK_LINES {
        return Err(fault(&keys, Rule::TooLong, format!("{} links, at most one a key", links.keys.len())));
    }
    for (index, link) in links.keys.iter().enumerate() {
        let at = keys.item(index);
        if links.keys.iter().take(index).any(|earlier| earlier.key == link.key) {
            return Err(fault(&at.field("key"), Rule::Duplicate, format!("{:?} twice", link.key)));
        }
        let kind = check_target(&at.field("target"), &link.target)?;
        match (&link.label, link.key) {
            (Some(_), super::LinkKey::Index) => return Err(fault(&at.field("label"), Rule::NotListed, "the Index key has no label".to_string())),
            (Some(label), _) => {
                let most = if kind == Target::Here { super::LINK_LABEL_CHARACTERS } else { super::OTHER_SERVICE_LABEL_CHARACTERS };
                check_text(&at.field("label"), label, 1, most)?;
            }
            (None, _) => {}
        }
    }
    let numbers = path.field("numbers");
    if links.numbers.len() > neoteletext_formats::page::NUMBER_LINKS {
        return Err(fault(&numbers, Rule::TooLong, format!("{} links, at most {}", links.numbers.len(), neoteletext_formats::page::NUMBER_LINKS)));
    }
    for (index, link) in links.numbers.iter().enumerate() {
        let at = numbers.item(index);
        if check_target(&at.field("target"), &link.target)? == Target::Web {
            return Err(fault(&at.field("target"), Rule::NotListed, "a page number never leads to web:".to_string()));
        }
    }
    let texts = path.field("texts");
    if links.texts.len() > neoteletext_formats::page::TEXT_LINKS {
        return Err(fault(&texts, Rule::TooLong, format!("{} links, at most {}", links.texts.len(), neoteletext_formats::page::TEXT_LINKS)));
    }
    for (index, link) in links.texts.iter().enumerate() {
        let at = texts.item(index);
        if !(FIRST_TEXT_LINK_ROW..=LAST_TEXT_LINK_ROW).contains(&link.row) {
            return Err(fault(&at.field("row"), Rule::OutOfRange, format!("{}: rows 1 to 23", link.row)));
        }
        if link.length == 0 || usize::from(link.column).saturating_add(usize::from(link.length)) > COLUMNS {
            return Err(fault(
                &at.field("length"),
                Rule::OutOfRange,
                format!("column {} and {} characters: the run stays within {COLUMNS} columns", link.column, link.length),
            ));
        }
        check_target(&at.field("target"), &link.target)?;
    }
    Ok(())
}

/// Text links' rows: 1 to 23 (the web-native concept section 5 item 2).
const FIRST_TEXT_LINK_ROW: u8 = 1;
const LAST_TEXT_LINK_ROW: u8 = 23;

/// Seven bits: a character code (EN 300 706 clause 8.2).
const SEVEN_BITS: u8 = 0x7F;
/// The last row a text run may be on, 24 (REQ-2214 item 4).
const LAST_TEXT_ROW: u8 = neoteletext_formats::write_text::LAST_TEXT_ROW;
/// Four bits: a designation code, a region, a nibble.
const FOUR_BITS: u8 = 0x0F;
/// Three bits: the national option.
const THREE_BITS: u8 = 0x07;
/// A triplet's address, mode and data: 6, 5 and 7 bits (clause 12.3.1).
const ADDRESS_BITS: u8 = 0x3F;
const MODE_BITS: u8 = 0x1F;

impl PackedLine {
    /// Its designation code.
    #[must_use]
    pub const fn designation(&self) -> u8 {
        match *self {
            Self::X27Nibbles(X27NibblesLine { designation, .. })
            | Self::X27(X27Line { designation, .. })
            | Self::X28(X28Line { designation, .. })
            | Self::M29(M29Line { designation, .. }) => designation,
        }
    }
}

fn check_triplets(path: &Path<'_>, triplets: &[super::Triplet]) -> Checked {
    for (index, triplet) in triplets.iter().enumerate() {
        let at = path.item(index);
        for (name, value, most) in [("address", triplet.address, ADDRESS_BITS), ("mode", triplet.mode, MODE_BITS), ("data", triplet.data, SEVEN_BITS)] {
            if value > most {
                return Err(fault(&at.field(name), Rule::OutOfRange, format!("{value}, 0 to {most}")));
            }
        }
    }
    Ok(())
}

fn check_rate(path: &Path<'_>, rate: u32) -> Checked {
    if SAMPLE_RATES.contains(&rate) { Ok(()) } else { Err(fault(path, Rule::NotListed, format!("{rate}: one of {SAMPLE_RATES:?}"))) }
}

fn check_packet_bytes(path: &Path<'_>, packet_bytes: u16) -> Checked {
    if packet_bytes == 0 || packet_bytes > OPUS_MOST_PACKET_BYTES {
        return Err(fault(path, Rule::OutOfRange, format!("{packet_bytes}: 1 to {OPUS_MOST_PACKET_BYTES}, the profile's 16 kbit/s at constant rate")));
    }
    Ok(())
}

/// The bytes of two minutes of 16-bit samples.
const MOST_PCM_BYTES: usize = super::MOST_SAMPLES * 2;
/// The bytes of two minutes of the profile's packets: 6,000 of 40 bytes.
const MOST_OPUS_BYTES: usize = super::MOST_SAMPLES / neoteletext_opus::FRAME_SAMPLES * OPUS_MOST_PACKET_BYTES as usize;

fn check_sound(root: &Path<'_>, sound: &Sound) -> Checked {
    if let Some(checked) = instrument::check(root, sound) {
        return checked;
    }
    match sound {
        Sound::Opus(OpusSound { connections, packet_bytes, packets, .. }) | Sound::OpusPackets(OpusPacketsSound { connections, packet_bytes, packets, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_packet_bytes(&root.field("packet_bytes"), *packet_bytes)?;
            let bytes = check_base64(&root.field("packets"), packets, MOST_OPUS_BYTES)?;
            if bytes.is_empty() || !bytes.len().is_multiple_of(usize::from(*packet_bytes)) {
                return Err(fault(&root.field("packets"), Rule::WrongLength, format!("{} bytes: whole packets of {packet_bytes}", bytes.len())));
            }
            Ok(())
        }
        Sound::OpusLive(OpusLiveSound { connections, packet_bytes, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_packet_bytes(&root.field("packet_bytes"), *packet_bytes)
        }
        Sound::Object(ObjectSound { connections, rate, pcm, .. }) | Sound::Blocks(BlocksSound { connections, rate, pcm, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_rate(&root.field("rate"), *rate)?;
            let bytes = check_base64(&root.field("pcm"), pcm, MOST_PCM_BYTES)?;
            if !bytes.len().is_multiple_of(2) {
                return Err(fault(&root.field("pcm"), Rule::WrongLength, "16-bit samples: an even number of bytes".to_string()));
            }
            Ok(())
        }
        Sound::Live(LiveSound { connections, rate, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_rate(&root.field("rate"), *rate)
        }
        Sound::Play(PlaySound { connections, channel, gain, duck, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_channel(&root.field("channel"), *channel)?;
            check_gain(&root.field("gain"), *gain)?;
            if *duck > super::MOST_DUCK {
                return Err(fault(&root.field("duck"), Rule::OutOfRange, format!("{duck}: 0 to {}", super::MOST_DUCK)));
            }
            Ok(())
        }
        Sound::Stop(StopSound { connections, channel, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_channel(&root.field("channel"), *channel)
        }
        Sound::Gain(GainSound { connections, channel, gain, .. }) => {
            check_connections(&root.field("connections"), connections)?;
            check_channel(&root.field("channel"), *channel)?;
            check_gain(&root.field("gain"), *gain)
        }
        // The synthesizer kinds are `instrument::check`'s, above.
        _ => Ok(()),
    }
}

fn check_channel(path: &Path<'_>, channel: u8) -> Checked {
    if channel >= neoteletext_audio::CHANNELS {
        return Err(fault(path, Rule::OutOfRange, format!("{channel}: 0 to {}", neoteletext_audio::CHANNELS.saturating_sub(1))));
    }
    Ok(())
}

fn check_gain(path: &Path<'_>, gain: u8) -> Checked {
    if gain > neoteletext_audio::MAX_GAIN {
        return Err(fault(path, Rule::OutOfRange, format!("{gain}: 0 to {}", neoteletext_audio::MAX_GAIN)));
    }
    Ok(())
}

fn check_author_log(root: &Path<'_>, log: &AuthorLog) -> Checked {
    check_connections(&root.field("connections"), &log.connections)?;
    let rule = |refused: LogRefused| match refused {
        LogRefused::Empty => Rule::TooShort,
        LogRefused::TooLong => Rule::TooLong,
        LogRefused::Character | LogRefused::NotUtf8 | LogRefused::Level => Rule::ForbiddenCharacter,
    };
    neoteletext_formats::log::check(log.text.as_bytes()).map_err(|refused| fault(&root.field("text"), rule(refused), format!("{refused:?}")))?;
    check_nfc(&root.field("text"), &log.text)
}

/// A redirect names a page and its sub-code, or an address, and the
/// engine's encoder alone says what an address may be (REQ-1688).
fn check_redirect(root: &Path<'_>, redirect: &Redirect) -> Checked {
    check_connection(&root.field("connection"), redirect.connection)?;
    check_request(&root.field("request"), redirect.request)?;
    if redirect.address.is_some() && redirect.query.is_some() {
        return Err(fault(&root.field("query"), Rule::UnknownField, "a query goes with a page: an address holds its own".to_string()));
    }
    let both = |field: &'static str| fault(&root.field(field), Rule::UnknownField, "a redirect names a page or an address, never both".to_string());
    let missing = |field: &'static str| fault(&root.field(field), Rule::MissingField, "a redirect names a page and its sub-code, or an address".to_string());
    match (redirect.page, redirect.sub, redirect.address.as_deref()) {
        // A reserved page is the program's fault in `page`, not the
        // encoder's in the query, which would name the wrong field (TODO-2135
        // item 8).
        (Some(page), Some(_), None) if page.is_reserved() => {
            Err(fault(&root.field("page"), Rule::Reserved, format!("{:03X} is reserved by the standard (EN 300 706 Annex A.10)", page.0)))
        }
        (Some(page), Some(sub), None) => {
            let parameters = redirect.query.as_deref();
            let record = RedirectRecord::Page { page: crate::page::address(page.0), subcode: SubCode::new(sub.0), parameters };
            encode(record, 0, 0, &mut |_| {}).map(|_| ()).map_err(|refused| fault(&root.field("query"), Rule::ForbiddenCharacter, format!("{refused:?}")))
        }
        (None, None, Some(address)) if address.len() > MAX_ADDRESS => {
            Err(fault(&root.field("address"), Rule::TooLong, format!("{} bytes, at most {MAX_ADDRESS}", address.len())))
        }
        (None, None, Some(address)) => encode(RedirectRecord::Address(address), 0, 0, &mut |_| {})
            .map(|_| ())
            .map_err(|refused| fault(&root.field("address"), Rule::ForbiddenCharacter, format!("{refused:?}"))),
        (Some(_), _, Some(_)) => Err(both("page")),
        (None, Some(_), Some(_)) => Err(both("sub")),
        (Some(_), None, None) => Err(missing("sub")),
        (None, _, None) => Err(missing("page")),
    }
}

/// A selection's number, as the server gives them.
fn check_request(path: &Path<'_>, request: RequestNumber) -> Checked {
    if (1..=SAFE_INTEGER_LIMIT).contains(&request) { Ok(()) } else { Err(fault(path, Rule::OutOfRange, format!("{request} is not a selection's number"))) }
}

/// A number belongs to one connection, so a message that carries one names
/// exactly one (DEC-1759 lesson 13).
fn check_answer(root: &Path<'_>, connections: &Connections, request: Option<RequestNumber>) -> Checked {
    let Some(request) = request else { return Ok(()) };
    check_request(&root.field("request"), request)?;
    if matches!(connections, Connections::Listed(listed) if listed.len() == 1) {
        Ok(())
    } else {
        Err(fault(&root.field("connections"), Rule::OneConnection, "a selection's number names one connection".to_string()))
    }
}

fn check_denied(root: &Path<'_>, denied: &Denied) -> Checked {
    check_connection(&root.field("connection"), denied.connection)?;
    check_request(&root.field("request"), denied.request)?;
    if DENIED_STATUSES.contains(&denied.status) {
        Ok(())
    } else {
        Err(fault(&root.field("status"), Rule::NotListed, format!("{} is not one of {DENIED_STATUSES:?}", denied.status)))
    }
}

/// A public page gives its title, text and sub-page, the sub-page of the page
/// named; a page that is not public gives nothing, so that nothing of it can
/// be kept (REQ-1728 items 3 and 6).
fn check_description(root: &Path<'_>, description: &Description) -> Checked {
    let Description { page, public, title, text, content } = description;
    let present = [("title", title.is_some()), ("text", text.is_some()), ("content", content.is_some())];
    if !*public {
        return present
            .iter()
            .find(|(_, given)| *given)
            .map_or(Ok(()), |(name, _)| Err(fault(&root.field(name), Rule::UnknownField, "a page that is not public gives nothing".to_string())));
    }
    if let Some((name, _)) = present.iter().find(|(_, given)| !*given) {
        return Err(fault(&root.field(name), Rule::MissingField, "a public page gives its title, text and content".to_string()));
    }
    if let Some(title) = title {
        check_text(&root.field("title"), title, 1, TITLE_CHARACTERS)?;
    }
    if let Some(text) = text {
        check_text(&root.field("text"), text, 1, DESCRIPTION_TEXT_CHARACTERS)?;
    }
    let Some(content) = content else { return Ok(()) };
    if content.page != *page {
        return Err(fault(&root.field("content").field("page"), Rule::WrongPage, format!("{:03X}, for the description of {:03X}", content.page.0, page.0)));
    }
    check_page(&root.field("content"), content)
}

fn check_log(root: &Path<'_>, log: &Log) -> Checked {
    let event = &log.event;
    let good = event.len() <= super::EVENT_NAME_CHARACTERS
        && event.bytes().next().is_some_and(|first| first.is_ascii_lowercase())
        && event.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if !good {
        return Err(fault(
            &root.field("event"),
            Rule::WrongType,
            format!("lower case letters, digits and _, a letter first, at most {}", super::EVENT_NAME_CHARACTERS),
        ));
    }
    check_text(&root.field("text"), &log.text, 1, super::OPERATOR_LOG_CHARACTERS)
}

/// `rules`: who it is for, a page that can be shown, and its table as the
/// engine holds it, encoded and checked with the engine's crate
/// (page-rules.md RULE-39, RULE-59).
fn check_rules(root: &Path<'_>, message: &super::RulesMessage) -> Result<Option<crate::rules::Compiled>, Fault> {
    check_connections(&root.field("connections"), &message.connections)?;
    check_shown_subpage(root, message.sub)?;
    message.table.0.as_deref().map(|table| rules::compile(&root.field("table"), table, (message.page.0, message.sub.0), CHECKED_NAPLPS_GENERATION)).transpose()
}

/// The NAPLPS stream generation a table is checked with when its message
/// arrives: the first a stream has. Each connection is sent the table
/// written for its own stream's generation (ANIM-21), which changes no
/// fault, since the record's reader accepts every generation.
const CHECKED_NAPLPS_GENERATION: u16 = 1;

/// `table` for `page` and `sub` written again for a connection whose
/// NAPLPS stream of the page has `generation` (ANIM-21), or none should it
/// now be refused.
pub(crate) fn recompiled(table: &super::RuleTable, (page, sub): (u16, u16), generation: u16) -> Option<crate::rules::Compiled> {
    rules::compile(&Path::Root, table, (page, sub), generation).ok()
}

/// A command sent directly (RULE-114): who it is for, a page that can be
/// shown, and its record, which a command sent directly encodes with
/// constants alone (`invalid_command`). The names it gives are checked
/// against each connection's table when it is sent.
fn check_command(root: &Path<'_>, message: &super::CommandMessage) -> Checked {
    check_connections(&root.field("connections"), message.connections())?;
    let (_, sub) = message.page();
    check_shown_subpage(root, super::SubCode(sub))?;
    rules::check_direct(root, message)
}

/// The table of a `rules` message, ready to send, or none for null: what
/// [`message`] checked.
///
/// # Errors
/// The first fault of RULE-39.
pub(crate) fn compiled(message: &super::RulesMessage) -> Result<Option<crate::rules::Compiled>, Refusal> {
    check_rules(&Path::Root, message).map_err(|fault| Refusal { message: Some(MessageType::Rules), path: fault.path, rule: fault.rule, detail: fault.detail })
}

/// A command sent directly, as its record's type and body for a connection
/// whose table in force has `names` (RULE-114).
///
/// # Errors
/// The first fault, `unknown_name` among them.
pub(crate) fn direct_record(message: &super::CommandMessage, names: &crate::rules::Names) -> Result<rules::Built, Refusal> {
    rules::direct_built(message, Some(names)).map_err(|fault| Refusal {
        message: Some(message.kind()),
        path: fault.path,
        rule: fault.rule,
        detail: fault.detail,
    })
}

/// The operations of an `animate` message, as the engine holds them: what
/// [`message`] checked.
///
/// # Errors
/// `invalid_animation`, with the operation's path.
pub(crate) fn operations(kind: MessageType, at: &'static str, ops: &[super::AnimationOp]) -> Result<Vec<neoteletext_rules::animate::Operation>, Refusal> {
    rules::operations(&Path::Root.field(at), ops).map_err(|fault| Refusal { message: Some(kind), path: fault.path, rule: fault.rule, detail: fault.detail })
}

/// A timeline's steps, each its tick and its operations as the engine
/// holds them (animation.md ANIM-21): 1 to 1,024, their ticks ascending,
/// and every one before the tick at which the timeline begins again.
type Steps = Vec<(u64, Vec<neoteletext_rules::animate::Operation>)>;

fn check_timeline(root: &Path<'_>, timeline: &super::TimelineMessage) -> Result<Steps, Fault> {
    let path = root.field("steps");
    if timeline.steps.is_empty() {
        return Err(fault(&path, Rule::TooShort, "no step".to_string()));
    }
    if timeline.steps.len() > super::MAX_TIMELINE_STEPS {
        return Err(fault(&path, Rule::TooLong, format!("{} steps, at most {}", timeline.steps.len(), super::MAX_TIMELINE_STEPS)));
    }
    let mut steps = Steps::with_capacity(timeline.steps.len());
    for (index, step) in timeline.steps.iter().enumerate() {
        let at = path.item(index);
        let tick = step.tick.get();
        if steps.last().is_some_and(|(before, _)| *before >= tick) {
            return Err(fault(&at.field("tick"), Rule::NotAscending, format!("tick {tick} does not come after the step before it")));
        }
        if let Some(loop_ticks) = timeline.loop_ticks.filter(|loop_ticks| loop_ticks.get() <= tick) {
            let detail = format!("tick {tick} is not before {}, where the timeline begins again", loop_ticks.get());
            return Err(fault(&at.field("tick"), Rule::InvalidAnimation, detail));
        }
        steps.push((tick, rules::operations(&at.field("ops"), &step.ops)?));
    }
    Ok(steps)
}

/// A `timeline` message's steps: what [`message`] checked.
///
/// # Errors
/// The first fault of its steps, with its path.
pub(crate) fn timeline_steps(timeline: &super::TimelineMessage) -> Result<Steps, Refusal> {
    check_timeline(&Path::Root, timeline).map_err(|fault| Refusal {
        message: Some(MessageType::Timeline),
        path: fault.path,
        rule: fault.rule,
        detail: fault.detail,
    })
}

/// `sprites`: who they are for, a sub-page that can be shown, each
/// sprite's number once, and a record the engine's reader takes
/// (animation.md ANIM-7, ANIM-21).
fn check_sprites(root: &Path<'_>, sprites: &super::Sprites) -> Checked {
    check_connections(&root.field("connections"), &sprites.connections)?;
    check_shown_subpage(root, sprites.sub)?;
    let path = root.field("sprites");
    let refuse = |path: &Path<'_>, detail: String| Err(fault(path, Rule::InvalidSprites, detail));
    if sprites.sprites.len() > super::MOST_SPRITES {
        return refuse(&path, format!("{} sprites, at most {}", sprites.sprites.len(), super::MOST_SPRITES));
    }
    for (index, sprite) in sprites.sprites.iter().enumerate() {
        let at = path.item(index);
        if sprites.sprites.iter().take(index).any(|earlier| earlier.number == sprite.number) {
            return refuse(&at.field("number"), format!("sprite {} is given twice", sprite.number.get()));
        }
        if sprite.frames.is_empty() || sprite.frames.len() > super::MOST_OBJECT_FRAMES {
            return refuse(&at.field("frames"), format!("{} frames, 1 to {}", sprite.frames.len(), super::MOST_OBJECT_FRAMES));
        }
    }
    crate::animation::sprites_record(sprites.page.0, sprites.sub.0, &sprites.sprites, &mut 0).map(|_| ()).or_else(|detail| refuse(&path, detail))
}

/// `naplps_objects`: who they are for, a sub-page that can be shown, each
/// object's number once, a macro object's frames and a region's none, and
/// a record the engine's reader takes (animation.md ANIM-11, ANIM-21).
fn check_naplps_objects(root: &Path<'_>, objects: &super::NaplpsObjects) -> Checked {
    check_connections(&root.field("connections"), &objects.connections)?;
    check_shown_subpage(root, objects.sub)?;
    let path = root.field("objects");
    let refuse = |path: &Path<'_>, detail: String| Err(fault(path, Rule::InvalidNaplpsObjects, detail));
    if objects.objects.len() > super::MOST_NAPLPS_OBJECTS {
        return refuse(&path, format!("{} objects, at most {}", objects.objects.len(), super::MOST_NAPLPS_OBJECTS));
    }
    for (index, object) in objects.objects.iter().enumerate() {
        let at = path.item(index);
        if objects.objects.iter().take(index).any(|earlier| earlier.number == object.number) {
            return refuse(&at.field("number"), format!("object {} is given twice", object.number.get()));
        }
        let frames = object.frames.len();
        match object.kind {
            super::NaplpsObjectKind::Macro if frames == 0 || frames > super::MOST_OBJECT_FRAMES => {
                return refuse(&at.field("frames"), format!("a macro object has 1 to {} frames", super::MOST_OBJECT_FRAMES));
            }
            super::NaplpsObjectKind::Macro if object.cover.is_some() => return refuse(&at.field("cover"), "a macro object has no cover: null".to_string()),
            super::NaplpsObjectKind::Region if frames != 0 => return refuse(&at.field("frames"), "a region has no frames".to_string()),
            _ => {}
        }
    }
    if let Some((field, detail)) = crate::animation::naplps_set_fault(objects.into()) {
        return refuse(&root.field(field), detail);
    }
    crate::animation::naplps_objects_record(objects.page.0, objects.sub.0, 1, objects.into(), &mut 0).map(|_| ()).or_else(|detail| refuse(&path, detail))
}

/// The most ReGIS one message may carry: a megabyte, a quarter of all the
/// contents a receiver's numbered drawings may hold.
const MOST_REGIS_BYTES: usize = 1024 * 1024;

/// A message about one sub-page's picture names that sub-page.
fn check_shown_subpage(root: &Path<'_>, sub: super::SubCode) -> Checked {
    if sub == super::ANY_SUBPAGE {
        return Err(fault(&root.field("sub"), Rule::OutOfRange, "3F7F names no sub-page".to_string()));
    }
    Ok(())
}

fn check_regis(root: &Path<'_>, regis: &super::Regis) -> Checked {
    check_connections(&root.field("connections"), &regis.connections)?;
    check_shown_subpage(root, regis.sub)?;
    check_base64(&root.field("data"), &regis.data, MOST_REGIS_BYTES).map(|_| ())
}

/// `naplps`: who it is for, a sub-page that can be shown, and at most as
/// many bytes as a `regis` message may carry.
fn check_naplps(root: &Path<'_>, naplps: &super::Naplps) -> Checked {
    check_connections(&root.field("connections"), &naplps.connections)?;
    check_shown_subpage(root, naplps.sub)?;
    check_base64(&root.field("data"), &naplps.data, MOST_REGIS_BYTES).map(|_| ())
}

/// Whether a drawing link's target is one a receiver reads (the engine's
/// REQ-6370): printable ASCII with no comma, of a length a link target may
/// be, in a form the engine's own parser of link targets takes.
fn is_drawing_link_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    let printable = bytes.iter().all(|&byte| byte.is_ascii_graphic() && byte != b',');
    !bytes.is_empty() && bytes.len() <= neoteletext_formats::page::LINK_TARGET_BYTES && printable && neoteletext_formats::tti::parse_target(bytes).is_some()
}

fn check_regis_links(root: &Path<'_>, links: &super::RegisLinks) -> Checked {
    check_connections(&root.field("connections"), &links.connections)?;
    check_shown_subpage(root, links.sub)?;
    let path = root.field("links");
    let refuse = |path: &Path<'_>, detail: String| Err(fault(path, Rule::InvalidDrawingLinks, detail));
    if links.links.len() > super::MOST_DRAWING_LINKS {
        return refuse(&path, format!("{} links, at most {}", links.links.len(), super::MOST_DRAWING_LINKS));
    }
    for (index, link) in links.links.iter().enumerate() {
        let item = path.item(index);
        if link.drawing == 0 {
            return refuse(&item.field("drawing"), "drawing numbers are 1 to 65,535".to_string());
        }
        if links.links.iter().take(index).any(|earlier| earlier.drawing == link.drawing) {
            return refuse(&item.field("drawing"), format!("drawing {} is named twice", link.drawing));
        }
        if !is_drawing_link_target(&link.target) {
            return refuse(&item.field("target"), "not a link target: a page, a page of another service, an address or web: and a web address".to_string());
        }
    }
    Ok(())
}

/// The record type of a sub-page's ReGIS (the engine's REQ-6336), which
/// only the `regis` message sends.
pub(crate) const TYPE_REGIS: u8 = 14;
/// The record type of a sub-page's drawing links (the engine's REQ-6370),
/// which only the `regis_links` message sends.
pub(crate) const TYPE_DRAWING_LINKS: u8 = 15;
/// The record type of a sub-page's cursor drawing (the engine's REQ-6371),
/// which only the `regis_cursor` message sends.
pub(crate) const TYPE_REGIS_CURSOR: u8 = 16;
/// The record type of a sub-page's NAPLPS (the engine's NAPLPS record, the
/// NAPLPS renderer's REQ-6408 item 2), which only the `naplps` message sends.
pub(crate) const TYPE_NAPLPS: u8 = 17;

/// The audio profile's record types a `packets` message may not carry: a
/// sound's OBJECT, DATA and CONTROL records (the engine's `record.rs`).
const AUDIO_RECORD_TYPES: [u8; 3] = [neoteletext_audio::record::TYPE_OBJECT, neoteletext_audio::record::TYPE_DATA, neoteletext_audio::record::TYPE_CONTROL];

/// The most raw packets one message may carry: a megabyte of them.
const MOST_RAW_BYTES: usize = 1024 * 1024;

fn check_packets(path: &Path<'_>, packets: &str) -> Checked {
    let bytes = check_base64(path, packets, MOST_RAW_BYTES)?;
    let size = neoteletext_core::consts::PACKET_BYTES;
    if bytes.is_empty() || !bytes.len().is_multiple_of(size) {
        return Err(fault(path, Rule::WrongLength, format!("{} bytes: whole packets of {size}", bytes.len())));
    }
    // The engine's one check of a stream's packets (REQ-1462, TODO-1465):
    // an address that decodes under Hamming 8/4, and a kind the stream may
    // carry.
    for (index, chunk) in bytes.as_chunks::<{ neoteletext_core::consts::PACKET_BYTES }>().0.iter().enumerate() {
        let packet = Packet { bytes: *chunk };
        if !is_valid_stream_packet(&packet) {
            return Err(fault(&path.item(index), Rule::InvalidPacket, "its address does not decode, or it is a kind the stream may not carry".to_string()));
        }
        // A record's type is in its first fragment: no record of a
        // sub-page's ReGIS, its drawing links or its cursor can begin
        // here, so none passes unchecked (DEC-6335, REQ-6372).
        if let Some(Ok(fragment)) = neoteletext_audio::idl::fragment(&packet)
            && fragment.index == 0
            && let Some(kind) = fragment.data.get(1)
            && [TYPE_REGIS, TYPE_DRAWING_LINKS, TYPE_REGIS_CURSOR, TYPE_NAPLPS].contains(kind)
        {
            let detail = "a record of a sub-page's ReGIS or NAPLPS goes in a regis, regis_links, regis_cursor or naplps message, which checks it";
            return Err(fault(&path.item(index), Rule::InvalidPacket, detail.to_string()));
        }
        // Nor a record of a page's rule table or of animation
        // (page-rules.md RULE-39, animation.md ANIM-21): each goes in its
        // own message, which names, checks and keeps what it carries.
        if let Some(Ok(fragment)) = neoteletext_audio::idl::fragment(&packet)
            && fragment.index == 0
            && let Some(kind) = fragment.data.get(1)
            && (neoteletext_web::records::TYPE_RULES..=neoteletext_web::records::TYPE_FRAME).contains(kind)
        {
            let detail =
                "a record of a rule table or of animation goes in a rules, query_state, animate, sprites, naplps_objects, sprite_sheet or frame message";
            return Err(fault(&path.item(index), Rule::InvalidPacket, detail.to_string()));
        }
        // Nor an audio record (TODO-6475): a sound's OBJECT, DATA and CONTROL
        // records go in a sound message, whose blocks, banks and patches the
        // server checks with the receiver's own code (MIDI-21); in raw
        // packets a program could send a synthesizer what no check saw.
        if let Some(Ok(fragment)) = neoteletext_audio::idl::fragment(&packet)
            && fragment.index == 0
            && let Some(kind) = fragment.data.get(1)
            && AUDIO_RECORD_TYPES.contains(kind)
        {
            let detail = "TODO-6475: an audio record (OBJECT, DATA or CONTROL) goes in a sound message, which checks what it carries";
            return Err(fault(&path.item(index), Rule::InvalidPacket, detail.to_string()));
        }
    }
    Ok(())
}

/// The decoded bytes of a base64 field already checked.
pub(crate) fn bytes_of(text: &str) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(text.len())];
    let length = base64::decode(text.as_bytes(), &mut bytes).unwrap_or(0);
    bytes.truncate(length);
    bytes
}
