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
use neoteletext_core::packet::{Packet, decode};
use neoteletext_core::policy::is_safe_output_char;
use neoteletext_web::base64;

use super::{
    AuthorLog, COLUMNS, Connections, Cookie, Index, Log, MAX_VIEWS, Magazine, MessageType, OPUS_MOST_PACKET_BYTES, PackedLine, Packets, Page, PageMessage,
    ProgramMessage, Refused, Reply, Rule, SAMPLE_RATES, Sound, SubPages, TITLE_CHARACTERS,
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
        ProgramMessage::Goto(goto) => check_view(&root.field("view"), goto.view),
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

/// A refusal of a line serde could not read as a message.
pub(crate) fn serde_refusal(line: &[u8], error: &serde_json::Error) -> Refusal {
    // The type, if the line is an object that names one the protocol has.
    #[derive(serde::Deserialize)]
    struct Typed {
        #[serde(rename = "type")]
        kind: MessageType,
    }
    let message = serde_json::from_slice::<Typed>(line).ok().map(|typed| typed.kind);
    let text = error.to_string();
    let rule = match error.classify() {
        serde_json::error::Category::Data if text.starts_with("unknown field") => Rule::UnknownField,
        serde_json::error::Category::Data if text.starts_with("missing field") => Rule::MissingField,
        serde_json::error::Category::Data if text.starts_with("duplicate field") => Rule::DuplicateField,
        serde_json::error::Category::Data if text.starts_with("unknown variant") => Rule::NotListed,
        serde_json::error::Category::Data => Rule::WrongType,
        serde_json::error::Category::Io | serde_json::error::Category::Syntax | serde_json::error::Category::Eof => Rule::Syntax,
    };
    Refusal::whole(message, rule, text)
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
    Ok(())
}

/// Exact base64's bytes (RFC 4648 section 4): padded, no whitespace, and
/// canonical, the text the bytes encode to. `most` bounds the decoded
/// length before anything is decoded.
fn check_base64(path: &Path<'_>, text: &str, most: usize) -> Result<Vec<u8>, Fault> {
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

fn check_view(path: &Path<'_>, view: u8) -> Checked {
    if view >= MAX_VIEWS {
        return Err(fault(path, Rule::OutOfRange, format!("{view}, at most {}", MAX_VIEWS.saturating_sub(1))));
    }
    Ok(())
}

fn check_connections(path: &Path<'_>, connections: &Connections) -> Checked {
    let Connections::Listed(listed) = connections else { return Ok(()) };
    if listed.is_empty() {
        return Err(fault(path, Rule::TooShort, "no connection named; \"all\" names every one".to_string()));
    }
    if listed.len() > super::MOST_NAMED_CONNECTIONS {
        return Err(fault(path, Rule::TooLong, format!("{} connections, at most {}", listed.len(), super::MOST_NAMED_CONNECTIONS)));
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
    check_magazines(&root.field("magazines"), &reply.magazines)?;
    let selected = root.field("selected");
    if reply.selected.len() > usize::from(MAX_VIEWS) {
        return Err(fault(&selected, Rule::TooLong, format!("{} pages, at most one a view, {MAX_VIEWS}", reply.selected.len())));
    }
    for (index, view_page) in reply.selected.iter().enumerate() {
        let at = selected.item(index);
        check_view(&at.field("view"), view_page.view)?;
        if reply.selected.iter().take(index).any(|earlier| earlier.view == view_page.view) {
            return Err(fault(&at.field("view"), Rule::Duplicate, format!("view {} twice", view_page.view)));
        }
        let page = &view_page.page;
        check_page(&at.field("page"), page)?;
        let listed = reply.magazines.iter().flat_map(|magazine| magazine.pages.iter()).find(|entry| entry.page == page.page);
        if listed.is_none() {
            return Err(fault(&at.field("page").field("page"), Rule::NotInIndex, format!("{:03X} is not in magazines", page.page.0)));
        }
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
        if subpage.enhancements.iter().take(index).any(|earlier| earlier.designation == packet.designation) {
            return Err(fault(&at.field("designation"), Rule::Duplicate, format!("designation code {} twice", packet.designation)));
        }
        check_triplets(&at.field("triplets"), &packet.triplets)?;
    }
    let packets = path.field("packets");
    for (index, line) in subpage.packets.iter().enumerate() {
        let at = packets.item(index);
        match line {
            PackedLine::X27Nibbles { nibbles, .. } => {
                if nibbles.len() != super::NIBBLES {
                    return Err(fault(&at.field("nibbles"), Rule::WrongLength, format!("{} values, {}", nibbles.len(), super::NIBBLES)));
                }
                if let Some(place) = nibbles.iter().position(|&nibble| nibble > FOUR_BITS) {
                    return Err(fault(&at.field("nibbles").item(place), Rule::OutOfRange, "four bits".to_string()));
                }
            }
            PackedLine::X27 { triplets, .. } | PackedLine::X28 { triplets, .. } | PackedLine::M29 { triplets, .. } => {
                check_triplets(&at.field("triplets"), triplets)?;
            }
        }
    }
    // The bounds serde does not hold: the schema states them all.
    let designations = subpage.enhancements.iter().map(|packet| packet.designation).chain(subpage.packets.iter().map(PackedLine::designation));
    if let Some(designation) = designations.into_iter().find(|&designation| designation > FOUR_BITS) {
        return Err(fault(path, Rule::OutOfRange, format!("designation code {designation}, 0 to 15")));
    }
    if let Some(region) = subpage.character_set.region
        && region > FOUR_BITS
    {
        return Err(fault(&path.field("character_set").field("region"), Rule::OutOfRange, format!("{region}, 0 to 15")));
    }
    if subpage.character_set.national_option > THREE_BITS {
        return Err(fault(&path.field("character_set").field("national_option"), Rule::OutOfRange, "0 to 7".to_string()));
    }
    if let Some(cursor) = subpage.cursor
        && (usize::from(cursor.row) >= super::ROWS || usize::from(cursor.column) >= COLUMNS)
    {
        return Err(fault(&path.field("cursor"), Rule::OutOfRange, format!("row {} column {}: rows 0 to 24, columns 0 to 39", cursor.row, cursor.column)));
    }
    Ok(())
}

/// The schemes an address may have as a link target (the engine's
/// web-native concept section 3).
const ADDRESS_SCHEMES: [&str; 2] = ["https://", "http://"];
/// What marks a web page's address, to open outside teletext (decision
/// E.88).
const WEB_MARK: &str = "web:";

/// Whether `text` is a service name: letters, digits and hyphens
/// (Appendix E.1), of at most the engine's `SERVICE_NAME_BYTES`.
fn is_service(text: &str) -> bool {
    !text.is_empty() && text.len() <= neoteletext_formats::page::SERVICE_NAME_BYTES && text.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Whether `text` is three hexadecimal digits of a page, magazine 1 to 8.
fn is_page(text: &str) -> bool {
    text.len() == 3 && u16::from_str_radix(text, 16).is_ok_and(|number| (limits::FIRST_PAGE_ADDRESS..=limits::LAST_PAGE_ADDRESS).contains(&number))
}

/// Whether `text` is four hexadecimal digits of a sub-code.
fn is_subcode(text: &str) -> bool {
    text.len() == limits::SUBCODE_DIGITS && u16::from_str_radix(text, 16).is_ok_and(limits::is_sub_code)
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

/// A link target's kind, or none for a target in no form of Appendix E.1:
/// printable ASCII without spaces or commas, of at most 256 bytes.
fn target(text: &str) -> Option<Target> {
    if text.is_empty() || text.len() > super::LINK_TARGET_BYTES || !text.bytes().all(|byte| byte.is_ascii_graphic() && byte != b',') {
        return None;
    }
    let address = text.strip_prefix(WEB_MARK).unwrap_or(text);
    if text.starts_with(WEB_MARK) || text.contains("://") {
        let lower = address.to_ascii_lowercase();
        let addressed = ADDRESS_SCHEMES.iter().any(|scheme| lower.starts_with(scheme));
        return addressed.then_some(if text.starts_with(WEB_MARK) { Target::Web } else { Target::Elsewhere });
    }
    let parts: Vec<&str> = text.split(':').collect();
    match parts.as_slice() {
        [page] if is_page(page) => Some(Target::Here),
        [page, sub] if is_page(page) && is_subcode(sub) => Some(Target::Here),
        [service, page] if is_service(service) && is_page(page) => Some(Target::Elsewhere),
        [service, page, sub] if is_service(service) && is_page(page) && is_subcode(sub) => Some(Target::Elsewhere),
        _ => None,
    }
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
            Self::X27Nibbles { designation, .. } | Self::X27 { designation, .. } | Self::X28 { designation, .. } | Self::M29 { designation, .. } => designation,
        }
    }
}

fn check_triplets(path: &Path<'_>, triplets: &[super::Triplet]) -> Checked {
    for (index, triplet) in triplets.iter().enumerate() {
        if triplet.address > ADDRESS_BITS || triplet.mode > MODE_BITS || triplet.data > SEVEN_BITS {
            return Err(fault(&path.item(index), Rule::OutOfRange, "address 0 to 63, mode 0 to 31, data 0 to 127".to_string()));
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
        Sound::Opus { connections, packet_bytes, packets, .. } | Sound::OpusPackets { connections, packet_bytes, packets, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_packet_bytes(&root.field("packet_bytes"), *packet_bytes)?;
            let bytes = check_base64(&root.field("packets"), packets, MOST_OPUS_BYTES)?;
            if bytes.is_empty() || !bytes.len().is_multiple_of(usize::from(*packet_bytes)) {
                return Err(fault(&root.field("packets"), Rule::WrongLength, format!("{} bytes: whole packets of {packet_bytes}", bytes.len())));
            }
            Ok(())
        }
        Sound::OpusLive { connections, packet_bytes, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_packet_bytes(&root.field("packet_bytes"), *packet_bytes)
        }
        Sound::Object { connections, rate, pcm, .. } | Sound::Blocks { connections, rate, pcm, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_rate(&root.field("rate"), *rate)?;
            let bytes = check_base64(&root.field("pcm"), pcm, MOST_PCM_BYTES)?;
            if !bytes.len().is_multiple_of(2) {
                return Err(fault(&root.field("pcm"), Rule::WrongLength, "16-bit samples: an even number of bytes".to_string()));
            }
            Ok(())
        }
        Sound::Live { connections, rate, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_rate(&root.field("rate"), *rate)
        }
        Sound::Play { connections, channel, gain, duck, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_channel(&root.field("channel"), *channel)?;
            check_gain(&root.field("gain"), *gain)?;
            if *duck > super::MOST_DUCK {
                return Err(fault(&root.field("duck"), Rule::OutOfRange, format!("{duck}: 0 to {}", super::MOST_DUCK)));
            }
            Ok(())
        }
        Sound::Stop { connections, channel, .. } => {
            check_connections(&root.field("connections"), connections)?;
            check_channel(&root.field("channel"), *channel)
        }
        Sound::Gain { connections, channel, gain, .. } => {
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
    check_text(&root.field("name"), &cookie.name, 1, super::COOKIE_BYTES)?;
    if cookie.name.starts_with('_') {
        return Err(fault(&root.field("name"), Rule::Reserved, "names starting _ are the server's".to_string()));
    }
    check_text(&root.field("value"), &cookie.value, 0, super::COOKIE_BYTES)?;
    if let Some(path) = &cookie.path {
        check_text(&root.field("path"), path, 1, super::COOKIE_BYTES)?;
    }
    Ok(())
}

fn check_author_log(root: &Path<'_>, log: &AuthorLog) -> Checked {
    check_connections(&root.field("connections"), &log.connections)?;
    neoteletext_formats::log::check(log.text.as_bytes())
        .map(|_| ())
        .map_err(|refused| fault(&root.field("text"), Rule::ForbiddenCharacter, format!("{refused:?}")))
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
    // Until the engine's one packet check (REQ-1462, TODO-1463): an
    // address that decodes under Hamming 8/4 and a packet the decoder reads.
    for (index, chunk) in bytes.as_chunks::<{ neoteletext_core::consts::PACKET_BYTES }>().0.iter().enumerate() {
        if decode(&Packet { bytes: *chunk }).is_err() {
            return Err(fault(&path.item(index), Rule::InvalidPacket, "its address does not decode, or it is not a packet the stream may carry".to_string()));
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
