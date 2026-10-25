use super::*;

/// A client frame: masked, with the given key.
fn masked(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
    let key = [0x11, 0x22, 0x33, 0x44];
    let mut bytes = vec![if fin { FIN } else { 0 } | opcode];
    if payload.len() < 126 {
        bytes.push(MASKED | u8::try_from(payload.len()).unwrap());
    } else {
        bytes.push(MASKED | LENGTH_16);
        bytes.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
    }
    bytes.extend_from_slice(&key);
    bytes.extend(payload.iter().zip(key.iter().cycle()).map(|(byte, key)| byte ^ key));
    bytes
}

#[test]
fn a_message_arriving_in_pieces_is_read_whole() {
    let mut reader = Reader::new();
    let frame = masked(0x2, true, b"\xffhello");
    let (first, rest) = frame.split_at(3);
    reader.feed(first);
    assert_eq!(reader.next_message(), Ok(None), "not yet");
    reader.feed(rest);
    assert_eq!(reader.next_message(), Ok(Some(Message::Binary(b"\xffhello".to_vec()))));
    assert_eq!(reader.next_message(), Ok(None));
}

#[test]
fn fragments_join_and_controls_pass_between() {
    let mut reader = Reader::new();
    reader.feed(&masked(0x1, false, b"subscribe "));
    reader.feed(&masked(0x9, true, b"p"));
    reader.feed(&masked(0x0, true, b"all"));
    assert_eq!(reader.next_message(), Ok(Some(Message::Ping(b"p".to_vec()))));
    assert_eq!(reader.next_message(), Ok(Some(Message::Text(b"subscribe all".to_vec()))));
}

#[test]
fn what_breaks_the_protocol_is_refused() {
    let mut unmasked = Reader::new();
    unmasked.feed(&[FIN | 0x2, 0x01, 0x00]);
    assert_eq!(unmasked.next_message(), Err(FrameError::Malformed));
    let mut huge = Reader::new();
    huge.feed(&[FIN | 0x2, MASKED | LENGTH_64, 0, 0, 0, 0, 0, 1, 0, 0]);
    assert_eq!(huge.next_message(), Err(FrameError::TooLong));
    let mut stray = Reader::new();
    stray.feed(&masked(0x0, true, b"x"));
    assert_eq!(stray.next_message(), Err(FrameError::Fragmentation));
}

#[test]
fn headers_use_the_shortest_length() {
    assert_eq!(header(Opcode::Binary, 125).1, 2);
    assert_eq!(&header(Opcode::Binary, 126).0[..4], &[FIN | 0x2, LENGTH_16, 0, 126]);
    assert_eq!(header(Opcode::Binary, 70_000).1, 10);
}
