//! The channel's files over HTTP, in the engine's layout (decision E.70):
//! `channel.json`, `contents.txt`, `index.t42` and each `NNN.t42`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use neoteletext_web::consts::DEFAULT_PAGE_LIMIT;

use crate::channel::Channel;

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
/// The longest title `contents.txt` keeps (the engine's contents rule).
const TITLE_CHARACTERS: usize = neoteletext_web::contents::MAX_TITLE;

impl Channel {
    /// The channel's file `name`, or none.
    #[must_use]
    pub fn resource(&self, name: &str) -> Option<Resource> {
        match name {
            "channel.json" => Some(Resource { content_type: "application/json", body: self.descriptor().into_bytes() }),
            "contents.txt" => Some(Resource { content_type: "text/plain", body: self.contents().into_bytes() }),
            "index.t42" => Some(Resource { content_type: T42_TYPE, body: bundle(self.shared_frames().map(|(_, frame)| &frame[..])) }),
            _ => {
                let page = page_file(name)?;
                let (_, frame) = self.shared_frames().find(|&(number, _)| number == page)?;
                Some(Resource { content_type: T42_TYPE, body: frame.to_vec() })
            }
        }
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

/// `NNN.t42` as a page number: three capital hexadecimal digits, magazine
/// 1 to 8.
fn page_file(name: &str) -> Option<u16> {
    let digits = name.strip_suffix(".t42")?;
    if digits.len() != 3 || !digits.bytes().all(|digit| digit.is_ascii_digit() || (b'A'..=b'F').contains(&digit)) {
        return None;
    }
    let number = u16::from_str_radix(digits, 16).ok()?;
    (0x100..=0x8FF).contains(&number).then_some(number)
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
