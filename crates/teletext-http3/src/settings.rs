//! The SETTINGS frame's payload (RFC 9114 section 7.2.4): pairs of an
//! identifier and a value, each a variable-length integer.

use alloc::vec::Vec;

use crate::error::{Code, Error};
use crate::limits::MAX_FIELD_SECTION_BYTES;
use crate::varint;

/// `SETTINGS_QPACK_MAX_TABLE_CAPACITY` (RFC 9204 section 5).
pub const QPACK_MAX_TABLE_CAPACITY: u64 = 0x01;
/// `SETTINGS_MAX_FIELD_SECTION_SIZE` (RFC 9114 section 7.2.4.1).
pub const MAX_FIELD_SECTION_SIZE: u64 = 0x06;
/// `SETTINGS_QPACK_BLOCKED_STREAMS` (RFC 9204 section 5).
pub const QPACK_BLOCKED_STREAMS: u64 = 0x07;
/// `SETTINGS_ENABLE_CONNECT_PROTOCOL` (RFC 9220 section 3, the identifier
/// RFC 8441 section 3 registered).
pub const ENABLE_CONNECT_PROTOCOL: u64 = 0x08;
/// The identifiers RFC 9114 section 11.2.2 reserves, those of HTTP/2's
/// settings that HTTP/3 has none for among them. Section 7.2.4.1: their
/// receipt is a connection error of type `H3_SETTINGS_ERROR`.
const RESERVED: [u64; 5] = [0x00, 0x02, 0x03, 0x04, 0x05];

/// What a peer's SETTINGS frame said, of the settings this crate knows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Settings {
    /// The largest field section the peer accepts, if it gave a bound.
    pub max_field_section_size: Option<u64>,
    /// The capacity the peer allows the QPACK dynamic table. Nothing here
    /// uses one whatever it allows.
    pub qpack_max_table_capacity: u64,
    /// The streams the peer allows to wait on the dynamic table.
    pub qpack_blocked_streams: u64,
    /// Whether the peer takes extended CONNECT (RFC 9220 section 3).
    pub connect_protocol: bool,
}

/// Appends this endpoint's settings (REQ-6966, REQ-6967): no QPACK dynamic
/// table and no stream waiting on one (RFC 9204 section 5), the largest
/// field section taken, and extended CONNECT (RFC 9220 section 3).
pub fn ours(out: &mut Vec<u8>) {
    for (identifier, value) in
        [(QPACK_MAX_TABLE_CAPACITY, 0), (MAX_FIELD_SECTION_SIZE, u64::from(MAX_FIELD_SECTION_BYTES)), (QPACK_BLOCKED_STREAMS, 0), (ENABLE_CONNECT_PROTOCOL, 1)]
    {
        varint::encode(identifier, out);
        varint::encode(value, out);
    }
}

/// Reads a SETTINGS frame's payload. Unknown settings are ignored (RFC
/// 9114 section 7.2.4).
///
/// # Errors
/// `H3_SETTINGS_ERROR`, a connection error: a setting given twice, a
/// reserved identifier, or an extended CONNECT setting that is neither 0
/// nor 1 (RFC 8441 section 3). `H3_FRAME_ERROR`: a payload that ends
/// inside a setting.
pub fn read(payload: &[u8]) -> Result<Settings, Error> {
    let mut settings = Settings::default();
    // The identifiers seen. The payload's bound keeps them few.
    let mut seen: Vec<u64> = Vec::new();
    let mut rest = payload;
    while !rest.is_empty() {
        let (identifier, used) = varint::decode(rest).ok_or(Error::connection(Code::FrameError))?;
        rest = rest.get(used..).unwrap_or(&[]);
        let (value, used) = varint::decode(rest).ok_or(Error::connection(Code::FrameError))?;
        rest = rest.get(used..).unwrap_or(&[]);
        if seen.contains(&identifier) || RESERVED.contains(&identifier) {
            return Err(Error::connection(Code::SettingsError));
        }
        seen.push(identifier);
        match identifier {
            QPACK_MAX_TABLE_CAPACITY => settings.qpack_max_table_capacity = value,
            MAX_FIELD_SECTION_SIZE => settings.max_field_section_size = Some(value),
            QPACK_BLOCKED_STREAMS => settings.qpack_blocked_streams = value,
            ENABLE_CONNECT_PROTOCOL => {
                settings.connect_protocol = match value {
                    0 => false,
                    1 => true,
                    _ => return Err(Error::connection(Code::SettingsError)),
                }
            }
            _ => {}
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-6966, REQ-6967: what this endpoint sends is no dynamic table,
    /// no blocked stream, the field section bound and extended CONNECT.
    #[test]
    fn ours_say_no_dynamic_table_and_extended_connect() {
        let mut payload = Vec::new();
        ours(&mut payload);
        assert_eq!(payload, [0x01, 0x00, 0x06, 0x80, 0x00, 0x40, 0x00, 0x07, 0x00, 0x08, 0x01]);
        let read = read(&payload).unwrap();
        assert_eq!(
            read,
            Settings {
                max_field_section_size: Some(u64::from(MAX_FIELD_SECTION_BYTES)),
                qpack_max_table_capacity: 0,
                qpack_blocked_streams: 0,
                connect_protocol: true
            }
        );
    }

    /// RFC 9114 section 7.2.4: an unknown setting is ignored; a setting
    /// twice, and section 7.2.4.1's reserved identifiers, are
    /// `H3_SETTINGS_ERROR`.
    #[test]
    fn unknown_settings_pass_and_faults_are_settings_errors() {
        // 0x21 is of the reserved form 0x1f * N + 0x21, sent for exercise.
        assert_eq!(read(&[0x21, 0x05, 0x40, 0x4A, 0x00]), Ok(Settings::default()));
        assert_eq!(read(&[0x06, 0x01, 0x06, 0x01]), Err(Error::connection(Code::SettingsError)));
        assert_eq!(read(&[0x21, 0x00, 0x21, 0x00]), Err(Error::connection(Code::SettingsError)));
        for reserved in RESERVED {
            assert_eq!(read(&[u8::try_from(reserved).unwrap(), 0x00]), Err(Error::connection(Code::SettingsError)));
        }
        assert_eq!(read(&[0x08, 0x02]), Err(Error::connection(Code::SettingsError)), "RFC 8441 section 3: 0 or 1 alone");
        assert_eq!(read(&[0x08, 0x00]).map(|settings| settings.connect_protocol), Ok(false));
    }

    /// A payload that ends inside a setting is malformed.
    #[test]
    fn a_cut_setting_is_a_frame_error() {
        assert_eq!(read(&[0x06]), Err(Error::connection(Code::FrameError)));
        assert_eq!(read(&[0x06, 0x40]), Err(Error::connection(Code::FrameError)));
    }
}
