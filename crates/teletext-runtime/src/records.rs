//! What a program sends that is not page content, as the engine's records
//! on data channel 12 (REQ-1480; `INPUT.md` section 7): the index, a
//! page's SUBPAGES list, the author's LOG lines, typed `SET_COOKIE`, a
//! sub-page's LINKS and a REDIRECT. Each is written by the engine's encoder, which checks
//! what it writes with its reader, never by hand here; this module only
//! turns the protocol's types into the encoder's.
//!
//! Every record numbers its packets from the connection's record id on the
//! service the audio and input records share, and moves it on.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::packet::Packet;
use neoteletext_core::types::SubCode as EngineSubCode;
use neoteletext_formats::log::LogLevel;
use neoteletext_formats::page::{LinkKey as EngineLinkKey, LinkLine, NumberLink as EngineNumberLink, TextLink as EngineTextLink};
use neoteletext_formats::text::Text;
use neoteletext_formats::tti::link_destination;
use neoteletext_web::cookie::SameSite as EngineSameSite;
use neoteletext_web::records::cookie::SetCookie;
use neoteletext_web::records::index::{IndexMagazine, IndexPage};
use neoteletext_web::records::links::SubPageLinks;
use neoteletext_web::records::redirect::RedirectRecord;
use neoteletext_web::records::subpages::SubPageEntry;
use neoteletext_web::records::{index, links, log, redirect as redirect_record, subpages};

use crate::page::address;
use crate::protocol::{AuthorLevel, AuthorLog, Cookie, LinkKey, Magazine, Page, Redirect, SameSite, SubPages};

/// The audio and input service the records go on (the reference channel's,
/// INPUT.md section 7).
const SERVICE: u8 = 0;

/// Why the engine's encoder refused what the protocol's checks passed.
pub(crate) type Refused = String;

/// The packets `write` gives, back to back.
fn collect(write: impl FnOnce(&mut dyn FnMut(Packet))) -> Vec<u8> {
    let mut bytes = Vec::new();
    write(&mut |packet| bytes.extend_from_slice(&packet.bytes));
    bytes
}

/// The channel's index for one connection, as one sending of
/// `generation` (`INPUT.md` section 7.5).
pub(crate) fn index(generation: u16, magazines: &[Magazine], record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let pages: Vec<Vec<IndexPage<'_>>> = magazines
        .iter()
        .map(|magazine| {
            magazine
                .pages
                .iter()
                .map(|entry| IndexPage { page: entry.page.0, subpages: entry.subpages, title: &entry.title, section: entry.section.as_deref().unwrap_or("") })
                .collect()
        })
        .collect();
    let magazines: Vec<IndexMagazine<'_>> =
        magazines.iter().zip(&pages).map(|(magazine, pages)| IndexMagazine { magazine: magazine.magazine, name: &magazine.name, pages }).collect();
    let mut sent = None;
    let bytes = collect(|out| sent = Some(index::encode(generation, &magazines, SERVICE, *record_id, out)));
    let sent = sent.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = sent.next_record_id;
    Ok(bytes)
}

/// A page's list of sub-pages (`INPUT.md` section 7.9).
pub(crate) fn subpage_list(list: &SubPages, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let entries: Vec<SubPageEntry<'_>> =
        list.subpages.iter().map(|entry| SubPageEntry { subcode: EngineSubCode::new(entry.sub.0), title: entry.title.as_deref().unwrap_or("") }).collect();
    let mut sent = None;
    let bytes = collect(|out| sent = Some(subpages::encode(address(list.page.0), list.version, &entries, SERVICE, *record_id, out)));
    let sent = sent.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = sent.next_record_id;
    Ok(bytes)
}

/// A line for the channel's author (`INPUT.md` section 7.6).
pub(crate) fn author_log(line: &AuthorLog, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let level = match line.level {
        AuthorLevel::E => LogLevel::Error,
        AuthorLevel::W => LogLevel::Warning,
        AuthorLevel::I => LogLevel::Information,
    };
    let mut written = None;
    let bytes = collect(|out| written = Some(log::encode(level, line.page.map(|page| address(page.0)), &line.text, SERVICE, *record_id, out)));
    written.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = record_id.wrapping_add(1);
    Ok(bytes)
}

/// A redirect, in its form, as its REDIRECT record (`INPUT.md` section
/// 7.11). The protocol's check left one of its two forms.
pub(crate) fn redirect(redirect: &Redirect, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let record = match (redirect.page, redirect.sub, redirect.address.as_deref()) {
        (Some(page), Some(sub), None) => RedirectRecord::Page { page: address(page.0), subcode: EngineSubCode::new(sub.0) },
        (None, None, Some(address)) => RedirectRecord::Address(address),
        _ => return Err(String::from("a redirect names a page or an address")),
    };
    let mut written = None;
    let bytes = collect(|out| written = Some(redirect_record::encode(record, SERVICE, *record_id, out)));
    written.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = record_id.wrapping_add(1);
    Ok(bytes)
}

/// A cookie, typed, as its `SET_COOKIE` record (`INPUT.md` section 7.7).
pub(crate) fn cookie(cookie: &Cookie, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    let same_site = match cookie.same_site {
        Some(SameSite::Strict) => EngineSameSite::Strict,
        Some(SameSite::Lax) => EngineSameSite::Lax,
        Some(SameSite::None) => EngineSameSite::None,
        None => EngineSameSite::Default,
    };
    let typed = SetCookie {
        name: &cookie.name,
        value: &cookie.value,
        path: cookie.path.as_deref(),
        max_age: cookie.max_age,
        same_site,
        secure: cookie.secure,
        http_only: cookie.http_only,
    };
    let mut written = None;
    let bytes = collect(|out| written = Some(typed.encode(SERVICE, *record_id, out)));
    written.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = record_id.wrapping_add(1);
    Ok(bytes)
}

/// Whether the sub-page has Link Extension links, so every frame of it is
/// followed by its LINKS record (REQ-1494, DEC-1517).
pub(crate) fn has_links(page: &Page) -> bool {
    let links = &page.link_extension;
    !(links.keys.is_empty() && links.numbers.is_empty() && links.texts.is_empty())
}

/// A sub-page's LINKS record (`INPUT.md` section 7.8), empty for a
/// sub-page with no Link Extension links. The targets are read by the
/// engine's own parser of TTI's targets; the protocol's checks passed
/// each already.
pub(crate) fn sub_page_links(page: &Page, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
    if !has_links(page) {
        return Ok(Vec::new());
    }
    let destination = |target: &str| link_destination(target.as_bytes()).ok_or_else(|| alloc::format!("target {target}"));
    let mut keys = Vec::new();
    for link in &page.link_extension.keys {
        let to = destination(&link.target)?;
        let (label, label_truncated) = Text::from_bytes(link.label.as_deref().unwrap_or("").as_bytes());
        let key = match link.key {
            LinkKey::Red => EngineLinkKey::Red,
            LinkKey::Green => EngineLinkKey::Green,
            LinkKey::Yellow => EngineLinkKey::Yellow,
            LinkKey::Cyan => EngineLinkKey::Cyan,
            LinkKey::Index => EngineLinkKey::Index,
        };
        keys.push(LinkLine { key, service: to.service, address: to.address, subcode: to.subcode, url: to.url, label, label_truncated, line: 0 });
    }
    let mut numbers = Vec::new();
    for link in &page.link_extension.numbers {
        numbers.push(EngineNumberLink { number: address(link.number.0), destination: destination(&link.target)?, line: 0 });
    }
    let mut texts = Vec::new();
    for link in &page.link_extension.texts {
        texts.push(EngineTextLink { row: link.row, column: link.column, length: link.length, destination: destination(&link.target)?, line: 0 });
    }
    let links = SubPageLinks { keys: &keys, numbers: &numbers, texts: &texts };
    let mut sent = None;
    let bytes = collect(|out| sent = Some(links::encode(address(page.page.0), EngineSubCode::new(page.sub.0), links, SERVICE, *record_id, out)));
    let sent = sent.ok_or_else(String::new)?.map_err(|error| alloc::format!("{error:?}"))?;
    *record_id = sent.next_record_id;
    Ok(bytes)
}
