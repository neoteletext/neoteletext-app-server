//! The channel's files over HTTP, in the engine's layout (decision E.70):
//! `channel.json`, `contents.txt`, `index.t42` and each `NNN.t42`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

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
const TITLE_CHARACTERS: usize = 32;

impl Channel {
    /// The channel's file `name`, or none.
    #[must_use]
    pub fn resource(&self, name: &str) -> Option<Resource> {
        match name {
            "channel.json" => Some(Resource { content_type: "application/json", body: self.descriptor().into_bytes() }),
            "contents.txt" => Some(Resource { content_type: "text/plain", body: self.contents().into_bytes() }),
            "index.t42" => {
                let mut body = Vec::new();
                for (_, frame) in self.shared_frames() {
                    body.extend_from_slice(frame);
                }
                Some(Resource { content_type: T42_TYPE, body })
            }
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
        for entry in self.entries() {
            // contents.txt names decimal pages only.
            let number = format!("{:03X}", entry.number);
            if number.bytes().all(|digit| digit.is_ascii_digit()) {
                let _ = writeln!(text, "p\t{number}\t{}", plain(&entry.title));
            }
        }
        text
    }
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
