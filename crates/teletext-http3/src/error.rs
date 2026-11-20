//! Errors, as the codes RFC 9114 section 8.1 and RFC 9204 section 6 give
//! them, and whether each ends the stream or the connection.

/// An HTTP/3 or QPACK error code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Code {
    /// `H3_NO_ERROR` (RFC 9114 section 8.1): nothing is wrong.
    NoError,
    /// `H3_GENERAL_PROTOCOL_ERROR`.
    GeneralProtocolError,
    /// `H3_INTERNAL_ERROR`.
    InternalError,
    /// `H3_STREAM_CREATION_ERROR`: a stream the peer may not open.
    StreamCreationError,
    /// `H3_CLOSED_CRITICAL_STREAM`: a stream the connection needs closed.
    ClosedCriticalStream,
    /// `H3_FRAME_UNEXPECTED`: a frame not permitted where it came.
    FrameUnexpected,
    /// `H3_FRAME_ERROR`: a frame whose layout is wrong.
    FrameError,
    /// `H3_EXCESSIVE_LOAD`: more than this endpoint takes.
    ExcessiveLoad,
    /// `H3_ID_ERROR`: a push identifier used wrongly.
    IdError,
    /// `H3_SETTINGS_ERROR`: a fault in a SETTINGS frame's payload.
    SettingsError,
    /// `H3_MISSING_SETTINGS`: a control stream that did not start with one.
    MissingSettings,
    /// `H3_REQUEST_REJECTED`.
    RequestRejected,
    /// `H3_REQUEST_CANCELLED`.
    RequestCancelled,
    /// `H3_REQUEST_INCOMPLETE`: the stream ended before the request did.
    RequestIncomplete,
    /// `H3_MESSAGE_ERROR`: a malformed message.
    MessageError,
    /// `QPACK_DECOMPRESSION_FAILED` (RFC 9204 section 6).
    QpackDecompressionFailed,
}

impl Code {
    /// The code's value on the wire (RFC 9114 section 8.1's and RFC 9204
    /// section 6's registrations).
    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::NoError => 0x0100,
            Self::GeneralProtocolError => 0x0101,
            Self::InternalError => 0x0102,
            Self::StreamCreationError => 0x0103,
            Self::ClosedCriticalStream => 0x0104,
            Self::FrameUnexpected => 0x0105,
            Self::FrameError => 0x0106,
            Self::ExcessiveLoad => 0x0107,
            Self::IdError => 0x0108,
            Self::SettingsError => 0x0109,
            Self::MissingSettings => 0x010A,
            Self::RequestRejected => 0x010B,
            Self::RequestCancelled => 0x010C,
            Self::RequestIncomplete => 0x010D,
            Self::MessageError => 0x010E,
            Self::QpackDecompressionFailed => 0x0200,
        }
    }
}

/// What an error ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// The stream it was found on (RFC 9114 section 8: a stream error).
    Stream,
    /// The whole connection (a connection error).
    Connection,
}

/// A fault in what the peer sent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Error {
    /// Its code.
    pub code: Code,
    /// What it ends.
    pub scope: Scope,
}

impl Error {
    /// A connection error.
    #[must_use]
    pub const fn connection(code: Code) -> Self {
        Self { code, scope: Scope::Connection }
    }

    /// A stream error.
    #[must_use]
    pub const fn stream(code: Code) -> Self {
        Self { code, scope: Scope::Stream }
    }
}
