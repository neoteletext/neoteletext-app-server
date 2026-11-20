//! The HTTP/3 layer's readers with any bytes, in pieces the input chooses
//! (REQ-6967): the frame reader, the control stream at either end, a
//! request stream with its field sections decoded and read as a request,
//! and the SETTINGS and QPACK readers on the bytes as they are. None may
//! panic, each must end, and a field section that decodes must, encoded
//! again, decode to the same fields.
#![no_main]

use libfuzzer_sys::fuzz_target;
use teletext_http3::limits::MAX_FIELD_SECTION_BYTES;
use teletext_http3::stream::{Control, Message, MessageEvent, Opened, Role};
use teletext_http3::{frame, message, qpack, settings, varint};

/// Decodes a field section and holds the decoder to its encoder.
fn section(encoded: &[u8]) {
    let most = u64::from(MAX_FIELD_SECTION_BYTES);
    let Ok(fields) = qpack::decode(encoded, most) else { return };
    let mut again = Vec::new();
    qpack::encode(fields.iter().map(|field| (field.name.as_slice(), field.value.as_slice())), &mut again);
    assert_eq!(qpack::decode(&again, most).as_ref(), Ok(&fields), "what decoded is encoded and decodes the same");
    let _ = message::request(fields);
}

fuzz_target!(|data: &[u8]| {
    let Some((&split, rest)) = data.split_first() else { return };
    let piece = usize::from(split).max(1);

    let mut frames = frame::Reader::new();
    let mut controls = [Control::new(Role::Server), Control::new(Role::Client)];
    let mut live = [true, true];
    let mut request = Message::new();
    let (mut framing, mut requesting) = (true, true);
    for chunk in rest.chunks(piece) {
        let mut at = 0;
        while framing && at < chunk.len() {
            match frames.read(&chunk[at..]) {
                Ok((taken, _)) => at += taken,
                Err(_) => framing = false,
            }
        }
        for (control, live) in controls.iter_mut().zip(&mut live) {
            let mut at = 0;
            while *live && at < chunk.len() {
                match control.read(&chunk[at..]) {
                    Ok((taken, _)) => at += taken,
                    Err(_) => *live = false,
                }
            }
        }
        let mut at = 0;
        while requesting && at < chunk.len() {
            match request.read(&chunk[at..]) {
                Ok((taken, event)) => {
                    at += taken;
                    if let Some(MessageEvent::Head(encoded) | MessageEvent::Trailers(encoded)) = event {
                        section(&encoded);
                    }
                }
                Err(_) => requesting = false,
            }
        }
        let _ = request.may_end();
    }

    section(rest);
    let _ = settings::read(rest);
    let _ = varint::decode(rest);
    let mut opened = Opened::new(if split & 1 == 0 { Role::Server } else { Role::Client });
    let mut left = rest;
    while let Some((kind, taken)) = varint::decode(left) {
        let _ = opened.open(kind);
        left = &left[taken..];
    }
});
