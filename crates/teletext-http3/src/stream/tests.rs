use alloc::vec;

use super::*;

fn framed(kind: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    frame::write(kind, payload, &mut out);
    out
}

/// Every event of `input` on a control stream.
fn control(role: Role, input: &[u8]) -> Result<Vec<ControlEvent>, Error> {
    let mut reader = Control::new(role);
    let mut found = Vec::new();
    let mut at = 0;
    loop {
        let (taken, event) = reader.read(&input[at..])?;
        at = at.saturating_add(taken);
        match event {
            Some(event) => found.push(event),
            None if taken == 0 => return Ok(found),
            None => {}
        }
    }
}

/// REQ-6967, RFC 9114 section 6.2: the peer has one control stream and
/// one QPACK stream of each kind; another of any is
/// `H3_STREAM_CREATION_ERROR`. A client's push stream is the same error,
/// and a stream of an unknown type is refused alone.
#[test]
fn each_critical_stream_is_opened_once() {
    let mut opened = Opened::new(Role::Server);
    assert_eq!(opened.open(CONTROL), Ok(Unidirectional::Control));
    assert_eq!(opened.open(QPACK_ENCODER), Ok(Unidirectional::Drain));
    assert_eq!(opened.open(QPACK_DECODER), Ok(Unidirectional::Drain));
    // 0x21 is of the reserved form 0x1f * N + 0x21 (section 6.2.3).
    assert_eq!(opened.open(0x21), Ok(Unidirectional::Refuse));
    assert_eq!(opened.open(0x21), Ok(Unidirectional::Refuse));
    for kind in [CONTROL, QPACK_ENCODER, QPACK_DECODER, PUSH] {
        assert_eq!(opened.open(kind), Err(Error::connection(Code::StreamCreationError)), "type {kind}");
    }
    assert_eq!(Opened::new(Role::Client).open(PUSH), Err(Error::connection(Code::IdError)));
}

/// REQ-6966, REQ-6967: what opens this endpoint's control stream is its
/// type and its SETTINGS, which say extended CONNECT and no dynamic
/// table; a peer reads it so.
#[test]
fn our_control_stream_opens_with_our_settings() {
    let mut opening = Vec::new();
    open_control(&mut opening);
    assert_eq!(opening[0], 0x00, "the control stream's type");
    let events = control(Role::Client, &opening[1..]).unwrap();
    let [ControlEvent::Settings(settings)] = events[..] else { panic!("one SETTINGS, not {events:?}") };
    assert!(settings.connect_protocol);
    assert_eq!((settings.qpack_max_table_capacity, settings.qpack_blocked_streams), (0, 0));
    assert_eq!(settings.max_field_section_size, Some(u64::from(crate::limits::MAX_FIELD_SECTION_BYTES)));
}

/// RFC 9114 section 6.2.1 and 7.2.4: SETTINGS comes first and once; DATA
/// and HEADERS never come on the control stream.
#[test]
fn the_control_stream_starts_with_one_settings_frame() {
    let settings = framed(frame::SETTINGS, &[]);
    assert_eq!(control(Role::Server, &framed(frame::GOAWAY, &[0])), Err(Error::connection(Code::MissingSettings)));
    assert_eq!(control(Role::Server, &framed(frame::HEADERS, &[0, 0])), Err(Error::connection(Code::MissingSettings)));
    for after in [framed(frame::SETTINGS, &[]), framed(frame::HEADERS, &[0, 0]), framed(frame::DATA, b"x")] {
        assert_eq!(control(Role::Server, &[&settings[..], &after].concat()), Err(Error::connection(Code::FrameUnexpected)));
    }
    // A frame of an unknown type before SETTINGS is passed over (section 9).
    let greased = [&framed(0x21, b"x")[..], &settings, &framed(frame::GOAWAY, &[8])].concat();
    assert_eq!(control(Role::Server, &greased), Ok(vec![ControlEvent::Settings(Settings::default()), ControlEvent::GoAway(8)]));
}

/// RFC 9114 sections 7.2.3 and 7.2.7: a client may raise `MAX_PUSH_ID`
/// and never lower it, a server may not send it, and a push that was
/// never allowed cannot be cancelled.
#[test]
fn push_identifiers_keep_their_rules() {
    let settings = framed(frame::SETTINGS, &[]);
    let then = |frames: &[Vec<u8>]| [&settings[..], &frames.concat()].concat();
    assert_eq!(
        control(Role::Server, &then(&[framed(frame::MAX_PUSH_ID, &[3]), framed(frame::MAX_PUSH_ID, &[3]), framed(frame::CANCEL_PUSH, &[3])])).map(|e| e.len()),
        Ok(1)
    );
    assert_eq!(control(Role::Server, &then(&[framed(frame::MAX_PUSH_ID, &[3]), framed(frame::MAX_PUSH_ID, &[2])])), Err(Error::connection(Code::IdError)));
    assert_eq!(control(Role::Server, &then(&[framed(frame::MAX_PUSH_ID, &[3]), framed(frame::CANCEL_PUSH, &[4])])), Err(Error::connection(Code::IdError)));
    assert_eq!(control(Role::Server, &then(&[framed(frame::CANCEL_PUSH, &[0])])), Err(Error::connection(Code::IdError)));
    assert_eq!(control(Role::Client, &then(&[framed(frame::MAX_PUSH_ID, &[3])])), Err(Error::connection(Code::FrameUnexpected)));
}

/// Every event of `input` on a request stream, DATA's parts as they came.
fn message(input: &[u8]) -> Result<(Vec<MessageEvent<'_>>, bool), Error> {
    let mut reader = Message::new();
    let mut found = Vec::new();
    let mut at = 0;
    loop {
        let (taken, event) = reader.read(&input[at..])?;
        at = at.saturating_add(taken);
        match event {
            Some(event) => found.push(event),
            None if taken == 0 => return Ok((found, reader.may_end())),
            None => {}
        }
    }
}

/// RFC 9114 section 4.1: a head, content, and perhaps trailers, with
/// unknown frames anywhere among them; the stream may end once the head
/// has come, between frames.
#[test]
fn a_message_is_a_head_content_and_trailers() {
    let stream =
        [framed(0x21, b"x"), framed(frame::HEADERS, b"head"), framed(frame::DATA, b"abc"), framed(0x21, b""), framed(frame::HEADERS, b"tail")].concat();
    let expected = vec![MessageEvent::Head(b"head".to_vec()), MessageEvent::Data(b"abc"), MessageEvent::Trailers(b"tail".to_vec())];
    assert_eq!(message(&stream), Ok((expected, true)));
    assert_eq!(message(&framed(frame::HEADERS, b"head")), Ok((vec![MessageEvent::Head(b"head".to_vec())], true)));
    assert_eq!(message(&framed(0x21, b"x")), Ok((vec![], false)), "no head yet");
    let cut = [&framed(frame::HEADERS, b"head")[..], &[0x00, 0x05, b'a']].concat();
    assert_eq!(message(&cut).map(|(_, may_end)| may_end), Ok(false), "inside a frame");
}

/// RFC 9114 sections 4.1 and 7.2: DATA before the head, anything after
/// the trailers, and the control stream's frames are
/// `H3_FRAME_UNEXPECTED`.
#[test]
fn frames_out_of_order_are_unexpected() {
    let unexpected = Err(Error::connection(Code::FrameUnexpected));
    let head = framed(frame::HEADERS, b"head");
    assert_eq!(message(&framed(frame::DATA, b"abc")), unexpected);
    let ended = [&head[..], &framed(frame::HEADERS, b"tail")].concat();
    assert_eq!(message(&[&ended[..], &framed(frame::DATA, b"abc")].concat()), unexpected);
    assert_eq!(message(&[&ended[..], &head].concat()), unexpected);
    for kind in [frame::SETTINGS, frame::GOAWAY, frame::MAX_PUSH_ID, frame::CANCEL_PUSH] {
        assert_eq!(message(&[&head[..], &framed(kind, &[0])].concat()), unexpected, "type {kind}");
        assert_eq!(message(&framed(kind, &[0])), unexpected, "type {kind} first");
    }
}

/// An interim response's head is followed by another (RFC 9114 section
/// 4.1).
#[test]
fn an_interim_head_is_followed_by_another() {
    let mut reader = Message::new();
    let head = framed(frame::HEADERS, b"100");
    assert_eq!(reader.read(&head), Ok((head.len(), Some(MessageEvent::Head(b"100".to_vec())))));
    reader.interim();
    assert!(!reader.may_end());
    assert_eq!(reader.read(&head), Ok((head.len(), Some(MessageEvent::Head(b"100".to_vec())))));
    assert!(reader.may_end());
}
