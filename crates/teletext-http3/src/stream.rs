//! Streams: the types of unidirectional streams and the control stream
//! (RFC 9114 section 6.2), and the frames of a request stream in their
//! order (section 4.1).

use alloc::vec::Vec;

use crate::error::{Code, Error};
use crate::frame::{self, Event};
use crate::settings::{self, Settings};
use crate::varint;

/// A control stream (RFC 9114 section 6.2.1).
pub const CONTROL: u64 = 0x00;
/// A push stream (section 6.2.2).
pub const PUSH: u64 = 0x01;
/// A QPACK encoder stream (RFC 9204 section 4.2).
pub const QPACK_ENCODER: u64 = 0x02;
/// A QPACK decoder stream (RFC 9204 section 4.2).
pub const QPACK_DECODER: u64 = 0x03;

/// Which end of the connection this is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The server.
    Server,
    /// The client.
    Client,
}

/// What to do with a unidirectional stream the peer opened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unidirectional {
    /// It is the peer's control stream: read it with [`Control`]. Its end
    /// is the connection error `H3_CLOSED_CRITICAL_STREAM` (RFC 9114
    /// section 6.2.1).
    Control,
    /// It is the peer's QPACK encoder or decoder stream: read it and drop
    /// what it carries, which with no dynamic table means nothing. Its
    /// end is `H3_CLOSED_CRITICAL_STREAM` (RFC 9204 section 4.2).
    Drain,
    /// Its type is unknown: stop reading it, with
    /// `H3_STREAM_CREATION_ERROR` (RFC 9114 section 6.2.3). The
    /// connection goes on.
    Refuse,
}

/// The unidirectional streams the peer has opened, of the kinds it may
/// open one of.
#[derive(Clone, Copy, Debug)]
pub struct Opened {
    role: Role,
    control: bool,
    encoder: bool,
    decoder: bool,
}

impl Opened {
    /// None yet, at this end of the connection.
    #[must_use]
    pub const fn new(role: Role) -> Self {
        Self { role, control: false, encoder: false, decoder: false }
    }

    /// The peer opened a unidirectional stream whose first integer, its
    /// type, is `kind`.
    ///
    /// # Errors
    /// A connection error. `H3_STREAM_CREATION_ERROR`: a second control,
    /// encoder or decoder stream (RFC 9114 section 6.2.1, RFC 9204
    /// section 4.2), or a push stream from a client (RFC 9114 section
    /// 6.2.2). `H3_ID_ERROR`: a push stream from a server, since no
    /// client here sends the `MAX_PUSH_ID` that would allow one (section
    /// 4.6).
    pub fn open(&mut self, kind: u64) -> Result<Unidirectional, Error> {
        let (seen, what) = match kind {
            CONTROL => (&mut self.control, Unidirectional::Control),
            QPACK_ENCODER => (&mut self.encoder, Unidirectional::Drain),
            QPACK_DECODER => (&mut self.decoder, Unidirectional::Drain),
            PUSH => {
                return Err(Error::connection(match self.role {
                    Role::Server => Code::StreamCreationError,
                    Role::Client => Code::IdError,
                }));
            }
            _ => return Ok(Unidirectional::Refuse),
        };
        if core::mem::replace(seen, true) {
            return Err(Error::connection(Code::StreamCreationError));
        }
        Ok(what)
    }
}

/// What the peer's control stream said.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlEvent {
    /// Its settings, the stream's first frame.
    Settings(Settings),
    /// GOAWAY (RFC 9114 section 5.2): the peer is closing the connection.
    GoAway(u64),
}

/// The peer's control stream, after its type (RFC 9114 section 6.2.1).
#[derive(Clone, Debug)]
pub struct Control {
    role: Role,
    frames: frame::Reader,
    settled: bool,
    /// The largest push identifier the client has allowed.
    max_push: Option<u64>,
}

impl Control {
    /// The control stream the peer of the end `role` opened.
    #[must_use]
    pub fn new(role: Role) -> Self {
        Self { role, frames: frame::Reader::new(), settled: false, max_push: None }
    }

    /// Reads from the start of `input`: how many bytes it took and what
    /// they said, if anything. Call it again with the rest while it takes
    /// bytes or gives events.
    ///
    /// # Errors
    /// A connection error. `H3_MISSING_SETTINGS`: a first frame that is
    /// not SETTINGS. `H3_FRAME_UNEXPECTED`: a second SETTINGS, a DATA or
    /// HEADERS frame, or a `MAX_PUSH_ID` from a server (RFC 9114 section
    /// 7.2). `H3_ID_ERROR`: a `MAX_PUSH_ID` under the last, or a
    /// `CANCEL_PUSH` of a push never allowed. And what [`frame::Reader`]
    /// and [`settings::read`] find.
    pub fn read(&mut self, input: &[u8]) -> Result<(usize, Option<ControlEvent>), Error> {
        let mut taken = 0_usize;
        loop {
            let (used, event) = self.frames.read(input.get(taken..).unwrap_or(&[]))?;
            taken = taken.saturating_add(used);
            let Some(event) = event else { return Ok((taken, None)) };
            if !self.settled {
                let Event::Settings(payload) = event else { return Err(Error::connection(Code::MissingSettings)) };
                self.settled = true;
                return settings::read(&payload).map(|settings| (taken, Some(ControlEvent::Settings(settings))));
            }
            match event {
                Event::GoAway(identifier) => return Ok((taken, Some(ControlEvent::GoAway(identifier)))),
                Event::MaxPushId(identifier) if self.role == Role::Server => {
                    // RFC 9114 section 7.2.7: it cannot go down.
                    if self.max_push.is_some_and(|last| identifier < last) {
                        return Err(Error::connection(Code::IdError));
                    }
                    self.max_push = Some(identifier);
                }
                // Section 7.2.3: no push was ever made here, so one may be
                // cancelled only within what was allowed.
                Event::CancelPush(identifier) if self.max_push.is_some_and(|most| identifier <= most) => {}
                Event::CancelPush(_) => return Err(Error::connection(Code::IdError)),
                Event::Data(_) | Event::Headers(_) | Event::Settings(_) | Event::MaxPushId(_) => return Err(Error::connection(Code::FrameUnexpected)),
            }
        }
    }
}

/// Appends what opens this endpoint's control stream: its type and its
/// SETTINGS frame (RFC 9114 sections 6.2.1 and 7.2.4).
pub fn open_control(out: &mut Vec<u8>) {
    varint::encode(CONTROL, out);
    let mut payload = Vec::new();
    settings::ours(&mut payload);
    frame::write(frame::SETTINGS, &payload, out);
}

/// What a request stream carried.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MessageEvent<'input> {
    /// The message's header section, still encoded: a request's, a
    /// response's, or an interim response's.
    Head(Vec<u8>),
    /// Part of its content, never empty.
    Data(&'input [u8]),
    /// Its trailer section, still encoded, after which only the stream's
    /// end may come.
    Trailers(Vec<u8>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Head,
    Content,
    Ended,
}

/// One direction of a request stream (RFC 9114 section 4.1): a HEADERS
/// frame, DATA frames, and perhaps a last HEADERS frame of trailers.
#[derive(Clone, Debug)]
pub struct Message {
    frames: frame::Reader,
    stage: Stage,
}

impl Default for Message {
    fn default() -> Self {
        Self { frames: frame::Reader::new(), stage: Stage::Head }
    }
}

impl Message {
    /// A stream at its start.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// After a head that was an interim response's (RFC 9114 section
    /// 4.1): another head follows.
    pub const fn interim(&mut self) {
        self.stage = Stage::Head;
    }

    /// Whether the stream may end here: after a head, between frames. A
    /// stream that ends sooner is `H3_REQUEST_INCOMPLETE` (RFC 9114
    /// section 4.1.2), or inside a frame `H3_FRAME_ERROR` (section 7.1).
    #[must_use]
    pub fn may_end(&self) -> bool {
        self.stage != Stage::Head && self.frames.between_frames()
    }

    /// Reads from the start of `input`: how many bytes it took and what
    /// they carried, if anything. Call it again with the rest while it
    /// takes bytes or gives events.
    ///
    /// # Errors
    /// `H3_FRAME_UNEXPECTED`, a connection error (RFC 9114 sections 4.1
    /// and 7.2): a DATA frame before the head or after the trailers, a
    /// HEADERS frame after the trailers, or a frame of the control
    /// stream. And what [`frame::Reader`] finds.
    pub fn read<'input>(&mut self, input: &'input [u8]) -> Result<(usize, Option<MessageEvent<'input>>), Error> {
        let (taken, event) = self.frames.read(input)?;
        let event = match (event, self.stage) {
            (None, _) => None,
            (Some(Event::Headers(section)), Stage::Head) => {
                self.stage = Stage::Content;
                Some(MessageEvent::Head(section))
            }
            (Some(Event::Headers(section)), Stage::Content) => {
                self.stage = Stage::Ended;
                Some(MessageEvent::Trailers(section))
            }
            (Some(Event::Data(part)), Stage::Content) => Some(MessageEvent::Data(part)),
            (Some(_), _) => return Err(Error::connection(Code::FrameUnexpected)),
        };
        Ok((taken, event))
    }
}

#[cfg(test)]
mod tests;
