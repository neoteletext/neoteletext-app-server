//! Every bound the server keeps on TLS, HTTP/2, HTTP/3 and QUIC input when
//! it serves them itself (DEC-6964 item 6, REQ-6967), each defined here
//! and nowhere else. The HTTP/3 layer's own bounds, on what it reads from
//! a stream, are `neoteletext_http3::limits`. With them, the two bounds on
//! the PROXY protocol header read before TLS (REQ-6974), whose
//! specification is `HAProxy` Technologies' "The PROXY protocol, Versions 1
//! & 2".

use std::time::Duration;

use teletext_runtime::kept::CONNECTIONS_WITH_OWN_PICTURES;

use crate::connection::{DEAD_AFTER_PING, PING_AFTER};
use crate::websocket::MAX_MESSAGE;

/// How long a TLS handshake over TCP, or a QUIC handshake, may take before
/// the connection is dropped. A handshake is two round trips and one
/// signature; ten seconds is far more than a slow link needs, and keeps a
/// peer that connects and says nothing from holding a socket or a place
/// among [`MAX_QUIC_CONNECTIONS`] for longer.
pub const HANDSHAKE_WITHIN: Duration = Duration::from_secs(10);

/// The smallest datagram every IP host must accept, and the IP and TCP
/// headers in it, as the PROXY protocol's section 2 counts them.
const SMALLEST_DATAGRAM_BYTES: usize = 576;
const IP_AND_TCP_HEADER_BYTES: usize = 40;

/// The longest PROXY protocol header accepted, its 16 fixed bytes
/// included (REQ-6974 item 8). The specification's section 2: "Both
/// formats are designed to fit in the smallest TCP segment that any
/// TCP/IP host is required to support (576 - 40 = 536 bytes)", and a
/// sender "must always ensure that the header is sent at once". The
/// longest header the server makes use of is 52 bytes, TCP over IPv6;
/// the rest is room for the TLV vectors a proxy may add, which are
/// skipped.
pub const MAX_PROXY_HEADER_BYTES: usize = SMALLEST_DATAGRAM_BYTES - IP_AND_TCP_HEADER_BYTES;

/// How long after a TCP connection is accepted its whole PROXY protocol
/// header may take to come, before the connection is dropped (REQ-6974
/// item 8). The specification's section 2: "The receiver may apply a
/// short timeout and decide to abort the connection if the protocol
/// header is not seen within a few seconds (at least 3 seconds to cover a
/// TCP retransmit)". The proxy writes the header itself, at once, over
/// the machine's own network, so no viewer's slow link is in it; five
/// seconds covers a retransmit with room, and [`HANDSHAKE_WITHIN`] starts
/// after it.
pub const PROXY_HEADER_WITHIN: Duration = Duration::from_secs(5);

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
pub const MAX_HEADER_LIST_BYTES: u32 = neoteletext_http3::limits::MAX_FIELD_SECTION_BYTES;

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

/// The push WebSocket connections open at once (REQ-7006), over HTTP/1.1, HTTP/2 and
/// HTTP/3 together; one more is answered 503. Over HTTP/2 and HTTP/3 a
/// connection carries up to [`MAX_CONCURRENT_STREAMS`] of them, so the
/// bounds on connections alone would let the server be made to hold a
/// hundred times as many; this is the one bound on all of them. It is
/// above [`CONNECTIONS_WITH_OWN_PICTURES`], the channel's connections the
/// server is built to carry (DEC-6898, REQ-6897), by a fifth for other
/// channels' and for a restart's overlap.
pub const MAX_PUSH_WEBSOCKETS: usize = 60_000;
const _: () = assert!(MAX_PUSH_WEBSOCKETS > CONNECTIONS_WITH_OWN_PICTURES, "the server holds every connection it is built to carry");

/// The QUIC connections open at once, handshakes in progress among them;
/// one more is refused. As many as [`MAX_PUSH_WEBSOCKETS`]: a browser that
/// shares no connection among its tabs makes one connection a tab, and
/// one that shares makes fewer. Measured 21 November 2026 (teletext-load
/// from foundintelquiet4060 to chunko, 1,000 tabs at rest), a QUIC
/// connection with one push WebSocket holds 55,347 bytes of the server's
/// memory, the WebSocket's channel state among them, and 26,195 a tab
/// with four tabs a connection; so at this bound the connections hold
/// about 3.3 GB with one tab each. With [`QUIC_CONNECTION_WINDOW_BYTES`]
/// it also bounds what all peers together can make the server hold
/// unread: 60,000 times 64 KiB, 3.7 GiB, about what as many TCP
/// connections hold in the kernel at its default receive buffer.
pub const MAX_QUIC_CONNECTIONS: usize = MAX_PUSH_WEBSOCKETS;

/// What a peer may send on one QUIC stream before the server has read it
/// (the stream's flow-control window, RFC 9000 section 4.1): four of a
/// viewer's largest messages (`websocket::MAX_MESSAGE`, 8 KiB). A
/// program's line may be megabytes and is read as it comes, a window a
/// round trip; a program over HTTP/3 is held to 32 KiB a round trip, and
/// one beside the server, as the deployment's are, is not slowed.
pub const QUIC_STREAM_WINDOW_BYTES: u32 = 32 * 1024;
const _: () = assert!(QUIC_STREAM_WINDOW_BYTES as usize >= 4 * MAX_MESSAGE, "a stream's window holds four of a viewer's messages");

/// What a peer may send on all of one QUIC connection's streams before
/// the server has read it (the connection's window, RFC 9000 section
/// 4.1): two streams' windows, for a viewer typing in two tabs at once;
/// a viewer's upstream is a few bytes a key.
pub const QUIC_CONNECTION_WINDOW_BYTES: u32 = 2 * QUIC_STREAM_WINDOW_BYTES;

/// The bytes of received datagrams the system is asked to hold for the
/// QUIC socket until the server reads them (`SO_RCVBUF`); the system gives
/// at most its own bound, `net.core.rmem_max` on Linux, and the serving
/// line says what was had. Every QUIC connection's packets arrive on the
/// one socket, and a reconnect storm's handshakes come at once, each an
/// Initial packet padded to 1,200 bytes (RFC 9000 section 14.1): at
/// Linux's default 208 KiB, 10,000 viewers reconnecting over HTTP/3 after
/// a restart overflowed it (58,889 datagrams dropped, counted by
/// `UdpRcvbufErrors`), and each client waited out its handshake's
/// doubling retransmission timer, 28 seconds in all (21 November 2026).
/// 8 MiB holds about 7,000 such packets.
pub const QUIC_RECEIVE_BUFFER_BYTES: usize = 8 * 1024 * 1024;

/// What the server sends one QUIC connection's peer and holds until it is
/// acknowledged, at most (the connection's send window). quinn's own
/// default is ten megabytes, which across [`MAX_QUIC_CONNECTIONS`] peers
/// that read slowly would be more memory than the machine has; a quarter
/// of a megabyte is a page's records many times over and a picture's
/// largest stream (1 MiB) in four windows, as a TCP connection's send
/// buffer holds about as much before its writes wait.
pub const QUIC_SEND_WINDOW_BYTES: u64 = 256 * 1024;

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

/// The TCP connections served at once, whatever they carry; one more is
/// closed as soon as it is accepted, with nothing read from it. A TCP
/// connection holds a file descriptor and its socket's buffers in the
/// kernel, and its task here, until it ends, however little its peer
/// sends; this bounds what all peers together can make the server hold
/// that way. It is above [`CONNECTIONS_WITH_OWN_PICTURES`], the channel's
/// connections the server is built to carry (DEC-6898, REQ-6897), each of
/// which may be a TCP connection of its own over HTTP/1.1, and below the
/// 65,536 descriptors production's unit allows (`LimitNOFILE` in
/// `deploy/ionos/deploy.sh`, checked by the lifecycle tests), leaving the
/// rest for the QUIC socket, the programs' pipes and the server's files.
pub const MAX_TCP_CONNECTIONS: usize = 60_000;
const _: () = assert!(MAX_TCP_CONNECTIONS > CONNECTIONS_WITH_OWN_PICTURES, "every connection the server is built to carry may be a TCP connection");

/// The TLS handshakes over TCP in progress at once; a connection that
/// would be one more is closed before its `ClientHello` is read. A
/// handshake costs the server a signature, and one in progress holds its
/// place for up to [`HANDSHAKE_WITHIN`], so a peer that starts handshakes
/// faster than they finish could otherwise hold every TCP place and keep
/// every core signing. A signature over `ring` takes well under a
/// millisecond on the server's cores, so a thousand handshakes in flight
/// is a burst of new viewers far larger than the channels have seen,
/// while what a stalled thousand hold is a thousand small buffers.
pub const MAX_TLS_HANDSHAKES: usize = 1_024;

/// How long the end of an HTTP/3 request stream waits for the peer to
/// acknowledge what was sent on it (REQ-7002), so that a WebSocket's
/// Close is not discarded by the connection's close that a stop sends
/// next. A live peer acknowledges within a round trip and its
/// acknowledgement delay (RFC 9000 section 13.2.1, 25 milliseconds unless
/// it says otherwise); one second covers a slow path's few round trips
/// and a lost packet's retransmission, and a peer that has gone, which
/// never acknowledges, holds the stop, and the stream's task, no longer:
/// without the bound it held them until the connection's idle timeout,
/// and the stop until `STOP_WITHIN` (found 21 November 2026 with a load
/// client killed before the stop).
pub const STREAM_ACKNOWLEDGED_WITHIN: Duration = Duration::from_secs(1);

/// How long a stop waits, after closing every QUIC connection with
/// `H3_NO_ERROR`, for the endpoint to be idle before the server exits. A
/// closed connection stays in its closing state for three times its probe
/// timeout (RFC 9000 section 10.2), answering what still arrives with its
/// `CONNECTION_CLOSE` again, so that a peer whose first copy was lost still
/// learns of the close at once rather than at its idle timeout; on the
/// paths the server's viewers have, three probe timeouts are well under a
/// second, and three seconds keeps the whole stop within the unit's
/// `TimeoutStopSec` with the channels' own time.
pub const QUIC_CLOSED_WITHIN: Duration = Duration::from_secs(3);
