use alloc::vec;

use super::*;

fn fields(pairs: &[(&[u8], &[u8])]) -> Vec<Field> {
    pairs.iter().map(|(name, value)| Field::new(name, value)).collect()
}

const MALFORMED: Result<Request, Error> = Err(Error::stream(Code::MessageError));
const GET: [(&[u8], &[u8]); 4] = [(b":method", b"GET"), (b":scheme", b"https"), (b":authority", b"example.org"), (b":path", b"/ch/")];
const WEBSOCKET: [(&[u8], &[u8]); 5] =
    [(b":method", b"CONNECT"), (b":protocol", b"websocket"), (b":scheme", b"https"), (b":authority", b"example.org"), (b":path", b"/ch/push")];

/// `base` and then `more`.
fn with(base: &[(&[u8], &[u8])], more: &[(&[u8], &[u8])]) -> Vec<Field> {
    fields(&[base, more].concat())
}

/// `base` without the field `name`.
fn without(base: &[(&[u8], &[u8])], name: &[u8]) -> Vec<Field> {
    fields(base).into_iter().filter(|field| field.name != name).collect()
}

/// REQ-6966: a GET and an extended CONNECT (RFC 9220 section 3) are read
/// into their control data and their other fields.
#[test]
fn a_get_and_an_extended_connect_are_read() {
    let read = request(with(&GET, &[(b"accept", b"*/*")])).unwrap();
    assert_eq!((read.method.as_slice(), read.path.as_deref(), read.protocol), (&b"GET"[..], Some(&b"/ch/"[..]), None));
    assert_eq!(read.fields, fields(&[(b"accept", b"*/*")]));
    let read = request(with(&WEBSOCKET, &[(b"sec-websocket-protocol", b"neoteletext"), (b"te", b"trailers")])).unwrap();
    assert_eq!(read.method, b"CONNECT");
    assert_eq!(read.protocol.as_deref(), Some(&b"websocket"[..]));
    assert_eq!(read.scheme.as_deref(), Some(&b"https"[..]));
    assert_eq!(read.authority.as_deref(), Some(&b"example.org"[..]));
    assert_eq!(read.fields.len(), 2);
}

/// RFC 9114 section 4.3.1: every request has one `:method`, `:scheme` and
/// `:path`, and one of `http` or `https` an authority, as `:authority` or
/// `host`, which agree when both are there.
#[test]
fn a_request_without_its_control_data_is_malformed() {
    for name in [&b":method"[..], b":scheme", b":path", b":authority"] {
        assert_eq!(request(without(&GET, name)), MALFORMED, "without {}", core::str::from_utf8(name).unwrap());
    }
    let hosted = [&without(&GET, b":authority")[..], &fields(&[(b"host", b"example.org")])].concat();
    assert_eq!(request(hosted).map(|read| read.authority), Ok(None));
    assert!(request(with(&GET, &[(b"host", b"example.org")])).is_ok());
    assert_eq!(request(with(&GET, &[(b"host", b"other.example")])), MALFORMED);
    assert_eq!(request(fields(&[(b":method", b"GET"), (b":scheme", b"https"), (b":authority", b"example.org"), (b":path", b"")])), MALFORMED);
    assert_eq!(request(fields(&[(b":method", b""), (b":scheme", b"https"), (b":authority", b"example.org"), (b":path", b"/")])), MALFORMED);
}

/// RFC 9114 section 4.3: pseudo-header fields come first, once each, and
/// only a request's.
#[test]
fn misplaced_repeated_and_unknown_pseudo_header_fields_are_malformed() {
    let late = [&fields(&[(b"accept", b"*/*")])[..], &fields(&GET)].concat();
    assert_eq!(request(late), MALFORMED);
    assert_eq!(request(with(&GET, &[(b":path", b"/other")])), MALFORMED);
    assert_eq!(request(with(&GET[..1], &[(b":method", b"GET"), GET[1], GET[2], GET[3]])), MALFORMED);
    assert_eq!(request(with(&GET[..3], &[(b":status", b"200"), GET[3]])), MALFORMED);
    assert_eq!(request(with(&GET[..3], &[(b":unknown", b"1"), GET[3]])), MALFORMED);
}

/// RFC 9114 section 4.2: names are lower-case tokens, no value holds NUL,
/// a line feed or a carriage return, the fields of an HTTP/1.1
/// connection are not carried, and `te` says `trailers` or nothing.
#[test]
fn forbidden_names_values_and_fields_are_malformed() {
    for name in [&b"Accept"[..], b"", b"a b", b"a:b", b"caf\xC3\xA9"] {
        assert_eq!(request(with(&GET, &[(name, b"x")])), MALFORMED, "{name:?}");
    }
    for value in [&b"a\0b"[..], b"a\nb", b"a\rb", b" a", b"a\t"] {
        assert_eq!(request(with(&GET, &[(b"accept", value)])), MALFORMED, "{value:?}");
    }
    assert_eq!(request(fields(&[(b":method", b"GET"), (b":scheme", b"https"), (b":authority", b"example.org"), (b":path", b"/a\nb")])), MALFORMED);
    for name in CONNECTION_SPECIFIC {
        assert_eq!(request(with(&GET, &[(name, b"x")])), MALFORMED, "{name:?}");
    }
    assert_eq!(request(with(&GET, &[(b"te", b"gzip")])), MALFORMED);
    assert!(request(with(&GET, &[(b"te", b"trailers")])).is_ok());
}

/// RFC 9114 section 4.4 and RFC 8441 section 4: a CONNECT without
/// `:protocol` names an authority alone; with `:protocol` it carries
/// `:scheme`, `:path` and `:authority`; and `:protocol` is CONNECT's
/// alone.
#[test]
fn connect_takes_its_two_forms_and_no_other() {
    let tunnel = vec![Field::new(b":method", b"CONNECT"), Field::new(b":authority", b"example.org:443")];
    assert_eq!(request(tunnel.clone()).map(|read| (read.scheme, read.path, read.protocol)), Ok((None, None, None)));
    assert_eq!(request(tunnel[..1].to_vec()), MALFORMED);
    assert_eq!(request([&tunnel[..], &fields(&[(b":path", b"/")])].concat()), MALFORMED);
    assert_eq!(request([&tunnel[..], &fields(&[(b":scheme", b"https")])].concat()), MALFORMED);
    for name in [&b":scheme"[..], b":path", b":authority"] {
        assert_eq!(request(without(&WEBSOCKET, name)), MALFORMED, "without {}", core::str::from_utf8(name).unwrap());
    }
    assert_eq!(request(with(&[(b":method", b"GET"), WEBSOCKET[1], WEBSOCKET[2], WEBSOCKET[3], WEBSOCKET[4]], &[])), MALFORMED);
    assert_eq!(request(with(&[WEBSOCKET[0], (b":protocol", b""), WEBSOCKET[2], WEBSOCKET[3], WEBSOCKET[4]], &[])), MALFORMED);
}
