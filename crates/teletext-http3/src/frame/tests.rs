#![allow(clippy::arithmetic_side_effects, reason = "a test")]

use alloc::vec;

use super::*;

/// Every event of `input`, given to the reader `piece` bytes at a time,
/// with DATA's parts joined.
fn events(input: &[u8], piece: usize) -> Result<Vec<Event<'static>>, Error> {
    let mut reader = Reader::new();
    let mut found: Vec<Event<'static>> = Vec::new();
    let mut data: Option<Vec<u8>> = None;
    for chunk in input.chunks(piece) {
        let mut at = 0;
        loop {
            let (taken, event) = reader.read(&chunk[at..])?;
            at += taken;
            match event {
                Some(Event::Data(part)) => data.get_or_insert_default().extend_from_slice(part),
                Some(other) => {
                    found.extend(data.take().map(|whole| Event::Data(whole.leak())));
                    found.extend(match other {
                        Event::Headers(payload) => Some(Event::Headers(payload)),
                        Event::Settings(payload) => Some(Event::Settings(payload)),
                        Event::GoAway(identifier) => Some(Event::GoAway(identifier)),
                        Event::MaxPushId(identifier) => Some(Event::MaxPushId(identifier)),
                        Event::CancelPush(identifier) => Some(Event::CancelPush(identifier)),
                        Event::Data(_) => None,
                    });
                }
                None if taken == 0 => break,
                None => {}
            }
        }
        assert_eq!(at, chunk.len(), "every byte is taken");
    }
    found.extend(data.take().map(|whole| Event::Data(whole.leak())));
    Ok(found)
}

fn framed(kind: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    write(kind, payload, &mut out);
    out
}

/// REQ-6967: DATA, HEADERS, SETTINGS and GOAWAY are read however the
/// stream's bytes are cut, and frames of unknown types between them are
/// passed over (RFC 9114 section 9), a reserved type of section 7.2.8
/// among them, whatever their length.
#[test]
fn frames_are_read_however_they_are_cut() {
    let mut stream = framed(HEADERS, b"head");
    stream.extend(framed(0x21, b"of a reserved type"));
    stream.extend(framed(DATA, b"some content"));
    stream.extend(framed(DATA, b""));
    stream.extend(framed(0x40_0000, &[0; 300]));
    stream.extend(framed(SETTINGS, &[0x06, 0x01]));
    stream.extend(framed(GOAWAY, &[0x40, 0x04]));
    stream.extend(framed(MAX_PUSH_ID, &[0x07]));
    stream.extend(framed(CANCEL_PUSH, &[0x03]));
    let expected = vec![
        Event::Headers(b"head".to_vec()),
        Event::Data(b"some content"),
        Event::Settings(vec![0x06, 0x01]),
        Event::GoAway(4),
        Event::MaxPushId(7),
        Event::CancelPush(3),
    ];
    for piece in [1, 2, 3, 7, stream.len()] {
        assert_eq!(events(&stream, piece), Ok(expected.clone()), "in pieces of {piece}");
    }
}

/// RFC 9114 section 7.1: a stream may end only between frames.
#[test]
fn a_stream_may_end_only_between_frames() {
    let stream = framed(HEADERS, b"head");
    let mut reader = Reader::new();
    assert!(reader.between_frames());
    for &byte in &stream[..stream.len() - 1] {
        assert_eq!(reader.read(&[byte]), Ok((1, None)));
        assert!(!reader.between_frames());
    }
    assert_eq!(reader.read(&stream[stream.len() - 1..]), Ok((1, Some(Event::Headers(b"head".to_vec())))));
    assert!(reader.between_frames());
}

/// REQ-6967: a HEADERS frame of the field section bound is held, and one
/// byte over is `H3_EXCESSIVE_LOAD` before any of its payload is held.
#[test]
fn a_headers_frame_is_held_to_its_bound_and_no_further() {
    let most = usize::try_from(MAX_FIELD_SECTION_BYTES).unwrap();
    assert_eq!(events(&framed(HEADERS, &vec![0; most]), 1_000), Ok(vec![Event::Headers(vec![0; most])]));
    let mut over = Vec::new();
    header(HEADERS, most + 1, &mut over);
    assert_eq!(events(&over, over.len()), Err(Error::connection(Code::ExcessiveLoad)));
}

/// REQ-6967: a SETTINGS frame of its bound is held, and one byte over is
/// `H3_EXCESSIVE_LOAD`.
#[test]
fn a_settings_frame_is_held_to_its_bound_and_no_further() {
    let most = usize::try_from(MAX_SETTINGS_BYTES).unwrap();
    assert_eq!(events(&framed(SETTINGS, &vec![0; most]), 100), Ok(vec![Event::Settings(vec![0; most])]));
    let mut over = Vec::new();
    header(SETTINGS, most + 1, &mut over);
    assert_eq!(events(&over, over.len()), Err(Error::connection(Code::ExcessiveLoad)));
}

/// The frames of one integer hold exactly one: an empty one, one longer
/// than an integer can be, and one whose integer is not the whole payload
/// are each `H3_FRAME_ERROR`.
#[test]
fn a_frame_of_one_integer_holds_exactly_one() {
    for kind in [GOAWAY, MAX_PUSH_ID, CANCEL_PUSH] {
        assert_eq!(events(&framed(kind, &[0xC0, 0, 0, 0, 0, 0, 0, 9]), 3), events(&framed(kind, &[9]), 3));
        assert_eq!(events(&framed(kind, &[]), 9), Err(Error::connection(Code::FrameError)));
        assert_eq!(events(&framed(kind, &[0; MAX_VARINT_BYTES + 1]), 99), Err(Error::connection(Code::FrameError)));
        assert_eq!(events(&framed(kind, &[0x01, 0x01]), 9), Err(Error::connection(Code::FrameError)));
        assert_eq!(events(&framed(kind, &[0x40]), 9), Err(Error::connection(Code::FrameError)));
    }
}

/// RFC 9114 section 7.2.8: the frame types HTTP/2 had and HTTP/3 has not
/// are `H3_FRAME_UNEXPECTED`; and `PUSH_PROMISE`, which neither end here
/// may be sent (section 7.2.5).
#[test]
fn frames_of_http2_and_push_promises_are_unexpected() {
    for kind in [0x02, 0x06, 0x08, 0x09, PUSH_PROMISE] {
        assert_eq!(events(&framed(kind, &[0]), 9), Err(Error::connection(Code::FrameUnexpected)), "type {kind}");
    }
}

/// A DATA frame's payload is passed on as it comes, never held: a frame
/// said to be of the largest length there is costs nothing until its
/// bytes arrive.
#[test]
fn data_of_any_length_is_passed_on_unheld() {
    let mut stream = Vec::new();
    varint::encode(DATA, &mut stream);
    varint::encode(varint::MAX, &mut stream);
    let mut reader = Reader::new();
    assert_eq!(reader.read(&stream), Ok((stream.len(), None)));
    assert_eq!(reader.read(b"abc"), Ok((3, Some(Event::Data(b"abc")))));
    assert_eq!(reader.held, [] as [u8; 0]);
    assert!(!reader.between_frames());
}
