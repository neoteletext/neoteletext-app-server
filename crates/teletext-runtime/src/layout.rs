//! The channel's files over HTTP, in the engine's layout (decisions E.70
//! and E.132; its REQ-WEB-019): `channel.json`, `contents.txt`,
//! `index.t42`, each `NNN.t42`, and for a page of several sub-pages each
//! `NNN-SSSS.t42` and the catalogue `NNN.sub`. `NNN.t42` and `index.t42`
//! hold first sub-pages only, so a page costs one sub-page until a
//! receiver asks for more (`STANDARD-LIMITS.md` section 4 items 7 and 8).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use neoteletext_core::types::SubCode;
use neoteletext_web::bytes::Writer;
use neoteletext_web::channel::ChannelFile;
use neoteletext_web::consts::DEFAULT_PAGE_LIMIT;
use neoteletext_web::subpages::{MAX_LINE, Validator, write_entry};

use crate::channel::Channel;
use crate::held::Held;

/// A file of the channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// Its media type, one the engine's agent accepts.
    pub content_type: &'static str,
    /// Its bytes.
    pub body: Vec<u8>,
}

/// T42's media type (the engine's `requirements/web.md`).
pub const T42_TYPE: &str = "application/x-teletext-t42";
/// A catalogue's media type: a line format of text (the engine's
/// REQ-WEB-034).
pub const SUBPAGES_TYPE: &str = "text/plain";
/// The longest title `contents.txt` keeps (the engine's contents rule).
const TITLE_CHARACTERS: usize = neoteletext_web::contents::MAX_TITLE;

impl Channel {
    /// The channel's file `name`, or none.
    #[must_use]
    pub fn resource(&self, name: &str) -> Option<Resource> {
        match name {
            "channel.json" => Some(Resource { content_type: "application/json", body: self.descriptor().into_bytes() }),
            "contents.txt" => Some(Resource { content_type: "text/plain", body: self.contents().into_bytes() }),
            "index.t42" => Some(Resource { content_type: T42_TYPE, body: bundle(self.shared_pages().map(|(_, held)| held.first())) }),
            _ => match ChannelFile::from_name(name)? {
                ChannelFile::Page { page, t42: true } => Some(Resource { content_type: T42_TYPE, body: self.held(page.mpp())?.first().to_vec() }),
                ChannelFile::SubPage { page, subcode } => {
                    let held = self.held(page.mpp()).filter(|held| held.parts.len() > 1)?;
                    Some(Resource { content_type: T42_TYPE, body: held.subpage(subcode.value())?.to_vec() })
                }
                ChannelFile::SubPages { page } => {
                    let held = self.held(page.mpp()).filter(|held| held.parts.len() > 1)?;
                    Some(Resource { content_type: SUBPAGES_TYPE, body: catalogue(held) })
                }
                _ => None,
            },
        }
    }

    /// The shared page `page`, if the channel has it.
    fn held(&self, page: u16) -> Option<&Held> {
        self.shared_pages().find(|&(number, _)| number == page).map(|(_, held)| held)
    }

    fn descriptor(&self) -> String {
        let config = self.config();
        format!(
            "{{\"name\":\"{}\",\"initial_page\":\"{:03X}\",\"push\":{{\"address\":\"{}\",\"method\":\"websocket\",\"input\":true}}}}",
            json_text(&config.name),
            config.initial_page,
            json_text(&config.push_address)
        )
    }

    fn contents(&self) -> String {
        let mut text = format!("s\t{}\n", plain(&self.config().name));
        // Only what there is to read: a page declared but not yet
        // published, such as a story slot no post fills, is left out, and a
        // section's heading goes before the first of its pages listed.
        let mut heading: Option<&str> = None;
        for entry in self.entries() {
            if let Some(section) = &entry.section {
                heading = Some(section);
            }
            // contents.txt names decimal pages only.
            let number = format!("{:03X}", entry.number);
            if self.is_shared(entry.number) && number.bytes().all(|digit| digit.is_ascii_digit()) {
                if let Some(section) = heading.take() {
                    let _ = writeln!(text, "s\t{}", plain(section));
                }
                let _ = writeln!(text, "p\t{number}\t{}", plain(&entry.title));
            }
        }
        text
    }
}

/// The bundle, `index.t42`: whole pages in page order, as many as a
/// receiver takes in one response (the engine's `DEFAULT_PAGE_LIMIT`,
/// REQ-WEB-008). A receiver that refuses a bundle opens the channel
/// without it (the engine's decision E.70 item 5), so one too large would
/// cost every receiver its pages at opening; the pages left out come by
/// push or by their own `NNN.t42`.
pub fn bundle<'a>(frames: impl Iterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut body = Vec::new();
    for frame in frames {
        if body.len().saturating_add(frame.len()) > DEFAULT_PAGE_LIMIT {
            break;
        }
        body.extend_from_slice(frame);
    }
    body
}

/// A page's catalogue, `NNN.sub` (the engine's decision E.132): each
/// sub-page's sub-code and validator, in the page's order. The titles are
/// left out, which the format allows.
fn catalogue(held: &Held) -> Vec<u8> {
    let mut body = alloc::vec![0_u8; held.parts.len().saturating_mul(MAX_LINE)];
    let mut writer = Writer::new(&mut body);
    for (subcode, packets) in held.subpages() {
        // Room for every entry was made above.
        let _ = write_entry(&mut writer, SubCode::new(subcode), &Validator::of(packets), "");
    }
    let length = writer.len();
    body.truncate(length);
    body
}

/// Text for a JSON string: printable ASCII, with quote and backslash
/// escaped.
fn json_text(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars().filter(|character| (' '..='~').contains(character)) {
        if matches!(character, '"' | '\\') {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// A `contents.txt` field: printable ASCII without tabs, cut to 32.
fn plain(text: &str) -> String {
    text.chars().filter(|character| (' '..='~').contains(character)).take(TITLE_CHARACTERS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A channel of more pages than one response may carry bundles the
    /// first of them whole, within the limit, in page order.
    #[test]
    fn the_bundle_is_whole_pages_within_a_receivers_limit() {
        let frames: Vec<Vec<u8>> = (0_u8..=255).map(|page| alloc::vec![page; 2_000]).collect();
        let body = bundle(frames.iter().map(Vec::as_slice));
        assert!(body.len() <= DEFAULT_PAGE_LIMIT);
        assert_eq!(body.len(), DEFAULT_PAGE_LIMIT / 2_000 * 2_000, "as many whole pages as fit");
        assert_eq!(body.first(), Some(&0), "from the first");
        assert_eq!(bundle(frames.iter().take(3).map(Vec::as_slice)).len(), 6_000, "a small channel's whole");
    }
}
