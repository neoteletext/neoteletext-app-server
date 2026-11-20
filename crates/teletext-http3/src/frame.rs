//! RFC 9114 section 7's frames: a type and a length, each a
//! variable-length integer, then the payload. [`Reader`] reads them as a
//! stream's bytes arrive. A DATA frame's payload is passed on as it comes
//! and an unknown frame's is passed over, neither held; every other
//! payload is held whole, within a bound of [`crate::limits`].

use alloc::vec::Vec;

use crate::error::{Code, Error};
use crate::limits::{MAX_FIELD_SECTION_BYTES, MAX_SETTINGS_BYTES, MAX_VARINT_BYTES};
use crate::varint;

/// DATA (RFC 9114 section 7.2.1).
pub const DATA: u64 = 0x00;
/// HEADERS (section 7.2.2).
pub const HEADERS: u64 = 0x01;
/// `CANCEL_PUSH` (section 7.2.3).
pub const CANCEL_PUSH: u64 = 0x03;
/// SETTINGS (section 7.2.4).
pub const SETTINGS: u64 = 0x04;
/// `PUSH_PROMISE` (section 7.2.5).
pub const PUSH_PROMISE: u64 = 0x05;
/// GOAWAY (section 7.2.6).
pub const GOAWAY: u64 = 0x07;
/// `MAX_PUSH_ID` (section 7.2.7).
pub const MAX_PUSH_ID: u64 = 0x0D;
/// The frame types of HTTP/2 that HTTP/3 has no frame for: PRIORITY, PING,
/// `WINDOW_UPDATE` and CONTINUATION. RFC 9114 section 7.2.8: their receipt
/// is a connection error of type `H3_FRAME_UNEXPECTED`.
const OF_HTTP2: [u64; 4] = [0x02, 0x06, 0x08, 0x09];

/// What [`Reader::read`] found.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event<'input> {
    /// Part of a DATA frame's payload, never empty.
    Data(&'input [u8]),
    /// A whole HEADERS frame's payload: an encoded field section.
    Headers(Vec<u8>),
    /// A whole SETTINGS frame's payload.
    Settings(Vec<u8>),
    /// GOAWAY, with its stream or push identifier.
    GoAway(u64),
    /// `MAX_PUSH_ID`, with the identifier.
    MaxPushId(u64),
    /// `CANCEL_PUSH`, with the identifier.
    CancelPush(u64),
}

#[derive(Clone, Debug)]
enum State {
    /// Reading the type.
    Type(varint::Reader),
    /// Reading the length of a frame of this type.
    Length(u64, varint::Reader),
    /// In a DATA frame's payload, with this much to come.
    Data(u64),
    /// In an unknown frame's payload, with this much to come.
    Skip(u64),
    /// Holding the payload of a frame of this type until this many more
    /// bytes have come.
    Hold(u64, usize),
}

/// A stream's frames, read as its bytes arrive.
#[derive(Clone, Debug)]
pub struct Reader {
    state: State,
    held: Vec<u8>,
}

impl Default for Reader {
    fn default() -> Self {
        Self { state: State::Type(varint::Reader::default()), held: Vec::new() }
    }
}

impl Reader {
    /// A reader at the start of a frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the stream may end here: between frames (RFC 9114 section
    /// 7.1: a stream that ends inside a frame is a connection error of
    /// type `H3_FRAME_ERROR`).
    #[must_use]
    pub const fn between_frames(&self) -> bool {
        matches!(&self.state, State::Type(reader) if !reader.started())
    }

    /// Reads from the start of `input`: how many bytes it took, and what
    /// they completed, if anything. Call it again with the rest while it
    /// takes bytes or gives events.
    ///
    /// # Errors
    /// A connection error: a frame of HTTP/2 alone or a `PUSH_PROMISE`,
    /// which no client may send and no server here does
    /// (`H3_FRAME_UNEXPECTED`); a HEADERS or SETTINGS frame over its
    /// bound (`H3_EXCESSIVE_LOAD`); a frame of one integer whose payload
    /// is not exactly one (`H3_FRAME_ERROR`).
    pub fn read<'input>(&mut self, input: &'input [u8]) -> Result<(usize, Option<Event<'input>>), Error> {
        let mut taken = 0_usize;
        loop {
            let rest = input.get(taken..).unwrap_or(&[]);
            match &mut self.state {
                State::Type(reader) => {
                    let (used, kind) = reader.read(rest);
                    taken = taken.saturating_add(used);
                    let Some(kind) = kind else { return Ok((taken, None)) };
                    self.state = State::Length(kind, varint::Reader::default());
                }
                State::Length(kind, reader) => {
                    let (used, length) = reader.read(rest);
                    taken = taken.saturating_add(used);
                    let Some(length) = length else { return Ok((taken, None)) };
                    let kind = *kind;
                    self.state = Self::payload(kind, length)?;
                    if let State::Hold(_, 0) = self.state {
                        return self.whole(kind).map(|event| (taken, Some(event)));
                    }
                }
                State::Data(remaining) => {
                    let part = rest.get(..usize::try_from(*remaining).unwrap_or(usize::MAX).min(rest.len())).unwrap_or(&[]);
                    *remaining = remaining.saturating_sub(u64::try_from(part.len()).unwrap_or(u64::MAX));
                    if *remaining == 0 {
                        self.state = State::Type(varint::Reader::default());
                    }
                    if part.is_empty() {
                        return Ok((taken, None));
                    }
                    return Ok((taken.saturating_add(part.len()), Some(Event::Data(part))));
                }
                State::Skip(remaining) => {
                    let skipped = usize::try_from(*remaining).unwrap_or(usize::MAX).min(rest.len());
                    *remaining = remaining.saturating_sub(u64::try_from(skipped).unwrap_or(u64::MAX));
                    taken = taken.saturating_add(skipped);
                    if *remaining > 0 {
                        return Ok((taken, None));
                    }
                    self.state = State::Type(varint::Reader::default());
                }
                State::Hold(kind, remaining) => {
                    let part = rest.get(..(*remaining).min(rest.len())).unwrap_or(&[]);
                    self.held.extend_from_slice(part);
                    *remaining = remaining.saturating_sub(part.len());
                    taken = taken.saturating_add(part.len());
                    if *remaining > 0 {
                        return Ok((taken, None));
                    }
                    let kind = *kind;
                    return self.whole(kind).map(|event| (taken, Some(event)));
                }
            }
        }
    }

    /// The state for the payload of a frame of `kind` and `length`.
    fn payload(kind: u64, length: u64) -> Result<State, Error> {
        let bound = match kind {
            DATA => return Ok(if length == 0 { State::Type(varint::Reader::default()) } else { State::Data(length) }),
            // RFC 9114 section 7.2.5: a client cannot send one, and a
            // client that set no MAX_PUSH_ID, as the one here, cannot be
            // sent one.
            PUSH_PROMISE => return Err(Error::connection(Code::FrameUnexpected)),
            kind if OF_HTTP2.contains(&kind) => return Err(Error::connection(Code::FrameUnexpected)),
            HEADERS => u64::from(MAX_FIELD_SECTION_BYTES),
            SETTINGS => MAX_SETTINGS_BYTES,
            GOAWAY | MAX_PUSH_ID | CANCEL_PUSH => {
                // Each is one integer and nothing else.
                if length == 0 || length > u64::try_from(MAX_VARINT_BYTES).unwrap_or(u64::MAX) {
                    return Err(Error::connection(Code::FrameError));
                }
                length
            }
            // RFC 9114 section 9: a frame of an unknown type is ignored,
            // the reserved types of section 7.2.8 among them.
            _ => return Ok(if length == 0 { State::Type(varint::Reader::default()) } else { State::Skip(length) }),
        };
        if length > bound {
            return Err(Error::connection(Code::ExcessiveLoad));
        }
        usize::try_from(length).map(|length| State::Hold(kind, length)).map_err(|_| Error::connection(Code::ExcessiveLoad))
    }

    /// The event of the held frame of `kind`, now whole.
    fn whole<'input>(&mut self, kind: u64) -> Result<Event<'input>, Error> {
        self.state = State::Type(varint::Reader::default());
        let payload = core::mem::take(&mut self.held);
        let integer = || match varint::decode(&payload) {
            Some((value, bytes)) if bytes == payload.len() => Ok(value),
            _ => Err(Error::connection(Code::FrameError)),
        };
        match kind {
            HEADERS => Ok(Event::Headers(payload)),
            SETTINGS => Ok(Event::Settings(payload)),
            GOAWAY => integer().map(Event::GoAway),
            MAX_PUSH_ID => integer().map(Event::MaxPushId),
            CANCEL_PUSH => integer().map(Event::CancelPush),
            _ => Err(Error::connection(Code::InternalError)),
        }
    }
}

/// Appends the header of a frame of `kind` whose payload is `length`
/// bytes.
pub fn header(kind: u64, length: usize, out: &mut Vec<u8>) {
    varint::encode(kind, out);
    varint::encode(u64::try_from(length).unwrap_or(u64::MAX), out);
}

/// Appends a whole frame of `kind` with `payload`.
pub fn write(kind: u64, payload: &[u8], out: &mut Vec<u8>) {
    header(kind, payload.len(), out);
    out.extend_from_slice(payload);
}

#[cfg(test)]
mod tests;
