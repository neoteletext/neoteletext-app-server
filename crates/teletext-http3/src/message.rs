//! A request's control data and fields (RFC 9114 sections 4.2 and 4.3.1),
//! with the extended CONNECT of RFC 9220, which is RFC 8441's for HTTP/3.

use alloc::vec::Vec;

use crate::error::{Code, Error};
use crate::qpack::Field;

/// `:method`.
pub const METHOD: &[u8] = b":method";
/// `:scheme`.
pub const SCHEME: &[u8] = b":scheme";
/// `:authority`.
pub const AUTHORITY: &[u8] = b":authority";
/// `:path`.
pub const PATH: &[u8] = b":path";
/// `:protocol` (RFC 8441 section 4).
pub const PROTOCOL: &[u8] = b":protocol";
/// `:status`, a response's (RFC 9114 section 4.3.2).
pub const STATUS: &[u8] = b":status";
/// The method of a tunnel.
pub const CONNECT: &[u8] = b"CONNECT";
/// The fields of a connection and not of a message, which HTTP/3 does not
/// carry (RFC 9114 section 4.2): a message with one is malformed.
pub const CONNECTION_SPECIFIC: [&[u8]; 5] = [b"connection", b"keep-alive", b"proxy-connection", b"transfer-encoding", b"upgrade"];
/// `te`, which may say `trailers` and nothing else (RFC 9114 section 4.2).
const TE: &[u8] = b"te";
const TRAILERS: &[u8] = b"trailers";
const HOST: &[u8] = b"host";
/// The schemes whose requests must name an authority (RFC 9114 section
/// 4.3.1).
const WITH_AUTHORITY: [&[u8]; 2] = [b"http", b"https"];

/// A malformed message (RFC 9114 section 4.1.2): a stream error.
const MALFORMED: Error = Error::stream(Code::MessageError);

/// A request's control data, and its fields without the pseudo-header
/// fields.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    /// `:method`.
    pub method: Vec<u8>,
    /// `:scheme`, which a CONNECT without `:protocol` has none of.
    pub scheme: Option<Vec<u8>>,
    /// `:authority`.
    pub authority: Option<Vec<u8>>,
    /// `:path`, which a CONNECT without `:protocol` has none of.
    pub path: Option<Vec<u8>>,
    /// `:protocol`: what an extended CONNECT asks to speak (RFC 9220).
    pub protocol: Option<Vec<u8>>,
    /// The other fields, in their order.
    pub fields: Vec<Field>,
}

/// Whether `name` is a field name HTTP/3 may carry: a token (RFC 9110
/// section 5.6.2) with no upper-case letter (RFC 9114 section 4.2).
fn is_name(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// Whether `value` is free of the bytes RFC 9114 section 4.2 names as
/// never permitted in a field value: NUL, line feed and carriage return;
/// and of white space at either end (RFC 9110 section 5.5).
fn is_value(value: &[u8]) -> bool {
    let blank = |byte: Option<&u8>| matches!(byte, Some(b' ' | b'\t'));
    !value.iter().any(|&byte| matches!(byte, 0 | b'\n' | b'\r')) && !blank(value.first()) && !blank(value.last())
}

/// Puts a pseudo-header field's value in its place, once.
fn once(place: &mut Option<Vec<u8>>, value: Vec<u8>) -> Result<(), Error> {
    if place.is_some() {
        return Err(MALFORMED);
    }
    *place = Some(value);
    Ok(())
}

/// Reads a request's decoded field section.
///
/// # Errors
/// `H3_MESSAGE_ERROR`, a stream error, for a malformed request (RFC 9114
/// sections 4.1.2, 4.2, 4.3 and 4.3.1; RFC 8441 section 4): a name that is
/// not a lower-case token; a value with a forbidden byte; a pseudo-header
/// field after another field, given twice, unknown, or a response's; a
/// connection-specific field; `te` other than `trailers`; a missing
/// `:method`; a CONNECT with `:scheme` or `:path` but no `:protocol`, or
/// without `:authority`; `:protocol` on another method, or without
/// `:scheme`, `:path` and `:authority`; any other request without
/// `:scheme` or a `:path` that is not empty, or of `http` or `https`
/// without an authority, or whose `:authority` and `host` differ.
pub fn request(fields: Vec<Field>) -> Result<Request, Error> {
    let (mut method, mut scheme, mut authority, mut path, mut protocol) = (None, None, None, None, None);
    let mut rest = Vec::new();
    for Field { name, value } in fields {
        if !is_value(&value) {
            return Err(MALFORMED);
        }
        if name.first() == Some(&b':') {
            // RFC 9114 section 4.3: all pseudo-header fields come first.
            if !rest.is_empty() {
                return Err(MALFORMED);
            }
            let place = match name.as_slice() {
                METHOD => &mut method,
                SCHEME => &mut scheme,
                AUTHORITY => &mut authority,
                PATH => &mut path,
                PROTOCOL => &mut protocol,
                _ => return Err(MALFORMED),
            };
            once(place, value)?;
            continue;
        }
        if !is_name(&name) || CONNECTION_SPECIFIC.contains(&name.as_slice()) || (name == TE && value != TRAILERS) {
            return Err(MALFORMED);
        }
        rest.push(Field { name, value });
    }
    let method = method.filter(|method| !method.is_empty()).ok_or(MALFORMED)?;
    let host = rest.iter().find(|field| field.name == HOST).map(|field| field.value.as_slice());
    let named = |authority: &Option<Vec<u8>>| authority.as_ref().is_some_and(|authority| !authority.is_empty());
    if method == CONNECT && protocol.is_none() {
        // RFC 9114 section 4.4: :scheme and :path are omitted, and
        // :authority names the tunnel's far end.
        if scheme.is_some() || path.is_some() || !named(&authority) {
            return Err(MALFORMED);
        }
    } else {
        // RFC 8441 section 4: :protocol is CONNECT's alone, and a request
        // with it carries :scheme, :path and :authority as any other does.
        if protocol.is_some() && (method != CONNECT || !named(&authority) || protocol.as_ref().is_some_and(Vec::is_empty)) {
            return Err(MALFORMED);
        }
        let scheme = scheme.as_ref().filter(|scheme| !scheme.is_empty()).ok_or(MALFORMED)?;
        if path.as_ref().is_none_or(Vec::is_empty) {
            return Err(MALFORMED);
        }
        if WITH_AUTHORITY.contains(&scheme.as_slice()) && !named(&authority) && host.is_none_or(<[u8]>::is_empty) {
            return Err(MALFORMED);
        }
    }
    if let (Some(authority), Some(host)) = (&authority, host)
        && authority != host
    {
        return Err(MALFORMED);
    }
    Ok(Request { method, scheme, authority, path, protocol, fields: rest })
}

#[cfg(test)]
mod tests;
