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
use neoteletext_formats::log::LogRefused;
use neoteletext_web::base64;
use neoteletext_web::records::redirect::{MAX_ADDRESS, RedirectRecord, encode};
use unicode_normalization::{IsNormalized, is_nfc, is_nfc_quick};

use super::{
    AuthorLog, COLUMNS, Connections, Cookie, Index, Log, Magazine, MessageType, OPUS_MOST_PACKET_BYTES, PackedLine, Packets, Page, PageMessage, ProgramMessage,
    Redirect, Refused, Reply, Rule, SAMPLE_RATES, Sound, SubPages, TITLE_CHARACTERS,
};
use super::{
    BlocksSound, GainSound, LiveSound, M29Line, ObjectSound, OpusLiveSound, OpusPacketsSound, OpusSound, PlaySound, StopSound, X27Line, X27NibblesLine, X28Line,
};

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
        Refused { message: self.message, path: self.path, rule: self.rule, detail: self.detail }
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
            let PageMessage { connections, page } = message.as_ref();
            check_connections(&root.field("connections"), connections).and_then(|()| check_page(&root.field("page"), page))
        }
        ProgramMessage::Subpages(list) => check_subpages(&root, list),
        ProgramMessage::Index(Index { connections, magazines }) => {
            check_connections(&root.field("connections"), connections).and_then(|()| check_magazines(&root.field("magazines"), magazines))
        }
        ProgramMessage::Redirect(redirect) => check_redirect(&root, redirect),
        ProgramMessage::Sound(sound) => check_sound(&root, sound),
        ProgramMessage::Cookie(cookie) => check_cookie(&root, cookie),
        ProgramMessage::AuthorLog(log) => check_author_log(&root, log),
        ProgramMessage::Log(log) => check_log(&root, log),
        ProgramMessage::Packets(Packets { connections, packets }) => {
            check_connections(&root.field("connections"), connections).and_then(|()| check_packets(&root.field("packets"), packets))
        }
    };
    checked.map_err(|Fault { path, rule, detail }| Refusal { message: Some(message.kind()), path, rule, detail })
}

// ----------------------------------------------------------- the pieces

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
    if (1..=super::SAFE_INTEGER_LIMIT).contains(&connection) {
        Ok(())
    } else {
        Err(fault(path, Rule::OutOfRange, format!("{connection}, 1 to {}", super::SAFE_INTEGER_LIMIT)))
    }
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
    if list.page.is_reserved() {
        return Err(fault(&root.field("page"), Rule::Reserved, format!("{:03X} is reserved by the standard (EN 300 706 Annex A.10)", list.page.0)));
    }
    let entries = root.field("subpages");
    if list.subpages.is_empty() || list.subpages.len() > limits::MAX_SUBPAGES {
        return Err(fault(&entries, Rule::OutOfRange, format!("{} sub-pages, 1 to {}", list.subpages.len(), limits::MAX_SUBPAGES)));
    }
    let mut seen = alloc::collections::BTreeSet::new();
    for (index, entry) in list.subpages.iter().enumerate() {
        let at = entries.item(index);
        if entry.sub.0 > limits::LAST_SUBPAGE_CODE {
            return Err(fault(&at.field("sub"), Rule::OutOfRange, format!("{:04X}: 3F7F means any sub-page, never one", entry.sub.0)));
        }
        if !seen.insert(entry.sub.0) {
            return Err(fault(&at.field("sub"), Rule::Duplicate, format!("{:04X} twice", entry.sub.0)));
        }
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
    if let Some(region) = subpage.character_set.region
        && region > FOUR_BITS
    {
        return Err(fault(&path.field("character_set").field("region"), Rule::OutOfRange, format!("{region}, 0 to 15")));
    }
    if subpage.character_set.national_option > THREE_BITS {
        return Err(fault(&path.field("character_set").field("national_option"), Rule::OutOfRange, "0 to 7".to_string()));
    }
    if let Some(cursor) = subpage.cursor {
        if usize::from(cursor.row) >= super::ROWS {
            return Err(fault(&path.field("cursor").field("row"), Rule::OutOfRange, format!("{}: rows 0 to 24", cursor.row)));
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
/// Appendix E.1: printable ASCII without spaces or commas, of at most 256
/// bytes.
fn target(text: &str) -> Option<Target> {
    if text.len() > super::LINK_TARGET_BYTES || !text.bytes().all(|byte| byte.is_ascii_graphic() && byte != b',') {
        return None;
    }
    let destination = neoteletext_formats::tti::link_destination(text.as_bytes())?;
    Some(if destination.is_web() {
        Target::Web
    } else if !destination.service.is_empty() || !destination.url.is_empty() {
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

fn check_cookie(root: &Path<'_>, cookie: &Cookie) -> Checked {
    check_connection(&root.field("connection"), cookie.connection)?;
    // A cookie's bound is in bytes (RFC 6265 section 6.1), its characters
    // checked as every text's.
    let field = |name: &'static str, text: &str, least: usize| -> Checked {
        let at = root.field(name);
        check_text(&at, text, least, super::COOKIE_BYTES)?;
        if text.len() > super::COOKIE_BYTES {
            return Err(fault(&at, Rule::TooLong, format!("{} bytes, at most {}", text.len(), super::COOKIE_BYTES)));
        }
        Ok(())
    };
    field("name", &cookie.name, 1)?;
    if cookie.name.starts_with('_') {
        return Err(fault(&root.field("name"), Rule::Reserved, "names starting _ are the server's".to_string()));
    }
    field("value", &cookie.value, 0)?;
    if let Some(path) = &cookie.path {
        field("path", path, 1)?;
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
    let both = |field: &'static str| fault(&root.field(field), Rule::UnknownField, "a redirect names a page or an address, never both".to_string());
    let missing = |field: &'static str| fault(&root.field(field), Rule::MissingField, "a redirect names a page and its sub-code, or an address".to_string());
    match (redirect.page, redirect.sub, redirect.address.as_deref()) {
        (Some(_), Some(_), None) => Ok(()),
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
        if !is_valid_stream_packet(&Packet { bytes: *chunk }) {
            return Err(fault(&path.item(index), Rule::InvalidPacket, "its address does not decode, or it is a kind the stream may not carry".to_string()));
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
