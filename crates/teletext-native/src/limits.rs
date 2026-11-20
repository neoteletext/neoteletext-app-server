//! Every bound the server keeps on TLS, HTTP/2, HTTP/3 and QUIC input when
//! it serves them itself (DEC-6964 item 6, REQ-6967), each defined here
//! and nowhere else. The HTTP/3 layer's own bounds, on what it reads from
//! a stream, are `teletext_http3::limits`.

use std::time::Duration;

use crate::connection::{DEAD_AFTER_PING, PING_AFTER};

/// How long a TLS handshake over TCP, or a QUIC handshake, may take before
/// the connection is dropped. A handshake is two round trips and one
/// signature; ten seconds is far more than a slow link needs, and keeps a
/// peer that connects and says nothing from holding a socket or a place
/// among [`MAX_QUIC_CONNECTIONS`] for longer.
pub const HANDSHAKE_WITHIN: Duration = Duration::from_secs(10);

/// The streams a peer may have open at once on one HTTP/2 connection
/// (`SETTINGS_MAX_CONCURRENT_STREAMS`, RFC 9113 section 6.5.2) or one
/// QUIC connection (its bidirectional streams, RFC 9114 section 6.1).
/// Each WebSocket is one stream for as long as it lasts, so this is how
/// many of one viewer's tabs share a connection. Both RFCs recommend no
/// fewer than 100, and a stream past the bound waits, or is refused, at
/// the peer; a hundred viewers' worth of one peer is as much as one
/// connection is given.
pub const MAX_CONCURRENT_STREAMS: u32 = 100;

/// The unidirectional streams a peer may have open at once on one QUIC
/// connection. RFC 9114 section 6.2 asks for at least three, the control
/// stream and QPACK's two, which stay open as long as the connection;
/// the five more are for streams of types sent to exercise the rule that
/// unknown types are ignored, each of which is stopped as soon as its
/// type is read.
pub const MAX_UNIDIRECTIONAL_STREAMS: u32 = 8;

/// The largest header list taken over HTTP/2
/// (`SETTINGS_MAX_HEADER_LIST_SIZE`, RFC 9113 section 6.5.2): the bound
/// HTTP/3's field sections have, by the same measure, so that a request
/// is bounded alike whichever carried it.
pub const MAX_HEADER_LIST_BYTES: u32 = teletext_http3::limits::MAX_FIELD_SECTION_BYTES;

/// The largest HTTP/2 frame taken (`SETTINGS_MAX_FRAME_SIZE`): 2^14
/// bytes, the smallest RFC 9113 section 4.2 allows and its initial value.
/// A viewer's message is at most `websocket::MAX_MESSAGE`, half of it.
pub const H2_MAX_FRAME_BYTES: u32 = 1 << 14;

/// An HTTP/2 connection is pinged after this long, and dropped when no
/// answer comes within [`H2_DEAD_AFTER_PING`]: the times a WebSocket's own
/// keepalive keeps (`connection::PING_AFTER`), so that a connection whose
/// peer has gone does not keep its streams' places.
pub const H2_PING_AFTER: Duration = PING_AFTER;
/// See [`H2_PING_AFTER`].
pub const H2_DEAD_AFTER_PING: Duration = DEAD_AFTER_PING;

/// The QUIC connections open at once, handshakes in progress among them;
/// one more is refused. With [`QUIC_CONNECTION_WINDOW_BYTES`] it bounds
/// what all peers together can make the server hold unread, at 1 GiB, and
/// a thousand connections of a hundred streams each is far more viewers
/// than one machine's channels have had.
pub const MAX_QUIC_CONNECTIONS: usize = 1_024;

/// What a peer may send on one QUIC stream before the server has read it
/// (the stream's flow-control window, RFC 9000 section 4.1). A viewer's
/// message is at most `websocket::MAX_MESSAGE`, 8 KiB; a program's line
/// may be megabytes, and is read as it comes, so the window bounds what
/// waits, not what may be sent.
pub const QUIC_STREAM_WINDOW_BYTES: u32 = 256 * 1024;

/// What a peer may send on all of one QUIC connection's streams before
/// the server has read it (the connection's window, RFC 9000 section 4.1).
pub const QUIC_CONNECTION_WINDOW_BYTES: u32 = 1024 * 1024;

/// A QUIC connection on which nothing has arrived for this long is closed
/// (the idle timeout, RFC 9000 section 10.1). It is longer than
/// `connection::PING_AFTER`, after which a quiet WebSocket is pinged, so
/// a connection carrying a live WebSocket is never idle this long.
pub const QUIC_IDLE_AFTER: Duration = Duration::from_secs(60);
const _: () = assert!(QUIC_IDLE_AFTER.as_micros() > PING_AFTER.as_micros(), "a WebSocket's keepalive must keep its QUIC connection");

/// How long an HTTP/3 request stream has to deliver its header section
/// once it is opened. A request's head is one packet; a stream opened and
/// left would otherwise keep its place among [`MAX_CONCURRENT_STREAMS`]
/// until the connection went idle.
pub const H3_HEAD_WITHIN: Duration = Duration::from_secs(10);

/// The bytes read from a QUIC stream at a time, and so the largest piece
/// of a DATA frame handed on at once.
pub const H3_READ_BYTES: usize = 4 * 1024;

/// The largest DATA frame the server writes on an HTTP/3 stream: one
/// write's bytes, up to this, are one frame.
pub const H3_DATA_FRAME_BYTES: usize = 16 * 1024;
