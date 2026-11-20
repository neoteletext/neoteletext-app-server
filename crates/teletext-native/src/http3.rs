//! HTTP/3 on QUIC (REQ-6965, REQ-6966): `quinn` carries the streams and
//! `neoteletext_http3` reads and writes what is on them. Each request stream
//! is decided by [`crate::http::decide`], as a request over TCP is, and
//! a stream that an extended CONNECT opened (RFC 9220) is then served as
//! a WebSocket's connection, its bytes the payload of DATA frames.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, ready};
use std::time::{SystemTime, UNIX_EPOCH};

use hyper::ext::Protocol;
use hyper::header::{CONTENT_LENGTH, DATE, HeaderName, HeaderValue};
use hyper::{Method, Request, Response, Uri, Version};
use neoteletext_http3::limits::MAX_FIELD_SECTION_BYTES;
use neoteletext_http3::message::{self, CONNECTION_SPECIFIC, STATUS};
use neoteletext_http3::stream::{Control, Message, MessageEvent, Opened, Role, Unidirectional, open_control};
use neoteletext_http3::{Code, Error, Scope, frame, qpack, varint};
use quinn::{RecvStream, SendStream, VarInt};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Semaphore;

use crate::connection::serve_halves;
use crate::http::{Answer, Routes, Source, Whole, decide};
use crate::limits::{
    H3_DATA_FRAME_BYTES, H3_HEAD_WITHIN, H3_READ_BYTES, HANDSHAKE_WITHIN, MAX_CONCURRENT_STREAMS, MAX_QUIC_CONNECTIONS, MAX_UNIDIRECTIONAL_STREAMS,
    QUIC_CONNECTION_WINDOW_BYTES, QUIC_IDLE_AFTER, QUIC_STREAM_WINDOW_BYTES,
};
use crate::tls::Configured;

/// An error code as QUIC carries it.
fn wire(code: Code) -> VarInt {
    VarInt::from_u32(code.value())
}

/// Binds UDP at `address` and listens for QUIC there, within the bounds
/// of [`crate::limits`].
///
/// # Errors
/// A socket that cannot be bound.
pub(crate) fn endpoint(tls: &Configured, address: SocketAddr) -> Result<quinn::Endpoint, String> {
    let mut transport = quinn::TransportConfig::default();
    transport
        .max_concurrent_bidi_streams(VarInt::from_u32(MAX_CONCURRENT_STREAMS))
        .max_concurrent_uni_streams(VarInt::from_u32(MAX_UNIDIRECTIONAL_STREAMS))
        .stream_receive_window(VarInt::from_u32(QUIC_STREAM_WINDOW_BYTES))
        .receive_window(VarInt::from_u32(QUIC_CONNECTION_WINDOW_BYTES))
        .max_idle_timeout(Some(QUIC_IDLE_AFTER.try_into().map_err(|error| format!("the QUIC idle time cannot be told: {error}"))?));
    let mut config = quinn::ServerConfig::with_crypto(Arc::clone(&tls.quic) as Arc<dyn quinn::crypto::ServerConfig>);
    config.transport_config(Arc::new(transport)).max_incoming(MAX_QUIC_CONNECTIONS);
    quinn::Endpoint::server(config, address).map_err(|error| format!("cannot listen for QUIC on UDP {address}: {error}"))
}

/// Accepts QUIC connections until the endpoint is closed, at most
/// [`MAX_QUIC_CONNECTIONS`] at once: one more is refused.
pub(crate) async fn accept(endpoint: quinn::Endpoint, routes: Arc<Routes>) {
    let open = Arc::new(Semaphore::new(MAX_QUIC_CONNECTIONS));
    while let Some(incoming) = endpoint.accept().await {
        let Ok(place) = Arc::clone(&open).try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        let routes = Arc::clone(&routes);
        tokio::spawn(async move {
            connection(incoming, routes).await;
            drop(place);
        });
    }
}

/// Closes every connection (RFC 9114 section 8.1: `H3_NO_ERROR`) and
/// waits until each peer has been told, or its idle time has passed.
pub(crate) async fn close(endpoint: &quinn::Endpoint) {
    endpoint.close(wire(Code::NoError), b"");
    endpoint.wait_idle().await;
}

/// Ends the connection with a connection error (RFC 9114 section 8).
fn fail(connection: &quinn::Connection, code: Code) {
    tracing::debug!(target: "http3", peer = %connection.remote_address(), ?code, "connection error");
    connection.close(wire(code), b"");
}

/// Serves one QUIC connection: its control stream out, the peer's
/// unidirectional streams in, and each bidirectional stream a request.
async fn connection(incoming: quinn::Incoming, routes: Arc<Routes>) {
    let Ok(Ok(connection)) = tokio::time::timeout(HANDSHAKE_WITHIN, incoming).await else { return };
    let peer = connection.remote_address().ip();
    // RFC 9114 section 6.2.1: each side opens one control stream, SETTINGS
    // first, and never closes it; it is held until the connection ends.
    let Ok(mut control) = connection.open_uni().await else { return };
    let mut opening = Vec::new();
    open_control(&mut opening);
    if control.write_all(&opening).await.is_err() {
        return;
    }
    let opened = Arc::new(Mutex::new(Opened::new(Role::Server)));
    loop {
        tokio::select! {
            stream = connection.accept_bi() => {
                let Ok((send, recv)) = stream else { break };
                tokio::spawn(request(Arc::clone(&routes), connection.clone(), send, recv, peer));
            }
            stream = connection.accept_uni() => {
                let Ok(recv) = stream else { break };
                tokio::spawn(unidirectional(connection.clone(), recv, Arc::clone(&opened)));
            }
        }
    }
    drop(control);
}

/// Reads a unidirectional stream the peer opened (RFC 9114 section 6.2):
/// its control stream is read to the connection's end, QPACK's streams
/// are read and dropped, since there is no dynamic table for them to
/// speak of, and a stream of any other type is stopped.
async fn unidirectional(connection: quinn::Connection, mut recv: RecvStream, opened: Arc<Mutex<Opened>>) {
    let mut buffer = vec![0_u8; H3_READ_BYTES];
    let mut kind = varint::Reader::default();
    let (kind, mut at, mut filled) = loop {
        let Ok(Some(count)) = recv.read(&mut buffer).await else { return };
        let (taken, kind) = kind.read(buffer.get(..count).unwrap_or(&[]));
        if let Some(kind) = kind {
            break (kind, taken, count);
        }
    };
    let what = opened.lock().unwrap_or_else(PoisonError::into_inner).open(kind);
    let mut control = match what {
        Err(error) => return fail(&connection, error.code),
        Ok(Unidirectional::Refuse) => {
            let _ = recv.stop(wire(Code::StreamCreationError));
            return;
        }
        Ok(Unidirectional::Drain) => None,
        Ok(Unidirectional::Control) => Some(Control::new(Role::Server)),
    };
    loop {
        if let Some(control) = &mut control {
            while at < filled {
                match control.read(buffer.get(at..filled).unwrap_or(&[])) {
                    Ok((taken, _)) => at = at.saturating_add(taken),
                    Err(error) => return fail(&connection, error.code),
                }
            }
        }
        match recv.read(&mut buffer).await {
            Ok(Some(count)) => (at, filled) = (0, count),
            // RFC 9114 section 6.2.1, RFC 9204 section 4.2: these streams
            // last as long as the connection.
            Ok(None) | Err(quinn::ReadError::Reset(_)) => return fail(&connection, Code::ClosedCriticalStream),
            Err(_) => return,
        }
    }
}

/// Why a request's head did not come.
enum NoHead {
    /// The stream ended, or the connection did.
    Gone,
    /// What came broke the protocol.
    Fault(Error),
}

/// A request stream's receiving side: the request's head first, and then,
/// as [`AsyncRead`], the payload of its DATA frames, which for a stream an
/// extended CONNECT opened are the WebSocket's bytes (RFC 9220 section 3,
/// RFC 8441 section 5).
struct Reading {
    recv: RecvStream,
    connection: quinn::Connection,
    message: Message,
    raw: Vec<u8>,
    at: usize,
    filled: usize,
    /// Trailers came: nothing follows them.
    ended: bool,
}

impl Reading {
    fn new(recv: RecvStream, connection: quinn::Connection) -> Self {
        Self { recv, connection, message: Message::new(), raw: vec![0; H3_READ_BYTES], at: 0, filled: 0, ended: false }
    }

    /// The request's header section, still encoded.
    async fn head(&mut self) -> Result<Vec<u8>, NoHead> {
        loop {
            while self.at < self.filled {
                let (taken, event) = self.message.read(self.raw.get(self.at..self.filled).unwrap_or(&[])).map_err(NoHead::Fault)?;
                self.at = self.at.saturating_add(taken);
                match event {
                    Some(MessageEvent::Head(section)) => return Ok(section),
                    Some(_) => return Err(NoHead::Fault(Error::connection(Code::FrameUnexpected))),
                    None => {}
                }
            }
            match self.recv.read(&mut self.raw).await {
                Ok(Some(count)) => (self.at, self.filled) = (0, count),
                _ => return Err(NoHead::Gone),
            }
        }
    }
}

impl AsyncRead for Reading {
    fn poll_read(self: Pin<&mut Self>, context: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            while this.at < this.filled && !this.ended {
                // No more of the stream's bytes than there is room for, so
                // that a piece of a DATA frame's payload always fits.
                let end = this.filled.min(this.at.saturating_add(buffer.remaining()));
                if end == this.at {
                    return Poll::Ready(Ok(()));
                }
                match this.message.read(this.raw.get(this.at..end).unwrap_or(&[])) {
                    Ok((taken, event)) => {
                        this.at = this.at.saturating_add(taken);
                        match event {
                            Some(MessageEvent::Data(part)) => {
                                buffer.put_slice(part);
                                return Poll::Ready(Ok(()));
                            }
                            // Trailers end the content: the WebSocket's
                            // bytes end there.
                            Some(MessageEvent::Trailers(_) | MessageEvent::Head(_)) => this.ended = true,
                            None => {}
                        }
                    }
                    Err(error) => {
                        fail(&this.connection, error.code);
                        return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData, "the stream's frames broke HTTP/3")));
                    }
                }
            }
            if this.ended {
                return Poll::Ready(Ok(()));
            }
            let mut raw = ReadBuf::new(&mut this.raw);
            ready!(Pin::new(&mut this.recv).poll_read(context, &mut raw))?;
            let count = raw.filled().len();
            if count == 0 {
                return Poll::Ready(Ok(()));
            }
            (this.at, this.filled) = (0, count);
        }
    }
}

/// A request stream's sending side as [`AsyncWrite`]: each write's bytes,
/// up to [`H3_DATA_FRAME_BYTES`], go out as one DATA frame.
struct Writing {
    send: SendStream,
    /// The frame being written, its header first; empty between writes.
    frame: Vec<u8>,
    /// Where its payload starts.
    payload: usize,
    /// How much of it QUIC has taken.
    sent: usize,
}

impl Writing {
    const fn new(send: SendStream) -> Self {
        Self { send, frame: Vec::new(), payload: 0, sent: 0 }
    }

    /// Writes `slices` as one DATA frame, as far as [`H3_DATA_FRAME_BYTES`]
    /// allows, and answers how many of their bytes it carried once QUIC
    /// has taken the whole frame.
    ///
    /// A frame's length is said before its payload, so a write that could
    /// not finish must be made again with the same bytes, as `write_all`
    /// and every writer of this server does; one made with others is an
    /// error, since the frame already begun cannot be taken back.
    fn poll_frame(&mut self, context: &mut Context<'_>, slices: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        let mut gathered = Vec::new();
        for slice in slices {
            let room = H3_DATA_FRAME_BYTES.saturating_sub(gathered.len());
            gathered.extend_from_slice(slice.get(..room.min(slice.len())).unwrap_or(&[]));
        }
        if gathered.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.frame.is_empty() {
            frame::header(frame::DATA, gathered.len(), &mut self.frame);
            self.payload = self.frame.len();
            self.frame.extend_from_slice(&gathered);
            self.sent = 0;
        } else if !gathered.starts_with(self.frame.get(self.payload..).unwrap_or(&[])) {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidInput, "a write left inside a DATA frame was made again with other bytes")));
        }
        ready!(self.poll_rest(context))?;
        let carried = self.frame.len().saturating_sub(self.payload);
        self.frame.clear();
        Poll::Ready(Ok(carried))
    }

    /// Gives QUIC the rest of the frame begun.
    fn poll_rest(&mut self, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.sent < self.frame.len() {
            let taken = ready!(Pin::new(&mut self.send).poll_write(context, self.frame.get(self.sent..).unwrap_or(&[])))?;
            if taken == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            self.sent = self.sent.saturating_add(taken);
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Writing {
    fn poll_write(self: Pin<&mut Self>, context: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        self.get_mut().poll_frame(context, &[io::IoSlice::new(bytes)])
    }

    fn poll_write_vectored(self: Pin<&mut Self>, context: &mut Context<'_>, slices: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        self.get_mut().poll_frame(context, slices)
    }

    /// A WebSocket frame's header and payload go out as one DATA frame.
    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().send).poll_flush(context)
    }

    /// Ends the stream, after any frame begun.
    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        ready!(this.poll_rest(context))?;
        this.frame.clear();
        Pin::new(&mut this.send).poll_shutdown(context)
    }
}

/// Serves one request stream.
async fn request(routes: Arc<Routes>, connection: quinn::Connection, mut send: SendStream, recv: RecvStream, peer: IpAddr) {
    let mut reading = Reading::new(recv, connection.clone());
    let section = match tokio::time::timeout(H3_HEAD_WITHIN, reading.head()).await {
        Ok(Ok(section)) => section,
        Ok(Err(NoHead::Fault(error))) => return fail(&connection, error.code),
        // RFC 9114 section 4.1.2: a request that ends before its head, or
        // does not come, is incomplete.
        Ok(Err(NoHead::Gone)) | Err(_) => return abandon(&mut send, &mut reading.recv, Code::RequestIncomplete),
    };
    let asked = qpack::decode(&section, u64::from(MAX_FIELD_SECTION_BYTES)).and_then(message::request).and_then(|read| as_request(&read));
    let asked = match asked {
        Ok(asked) => asked,
        Err(Error { code, scope: Scope::Connection }) => return fail(&connection, code),
        Err(Error { code, scope: Scope::Stream }) => return abandon(&mut send, &mut reading.recv, code),
    };
    match decide(&routes, &asked, Source::Peer(peer)).await {
        Answer::Response(response) => {
            let (head, body) = whole(response, *asked.method() != Method::HEAD);
            if send.write_all(&head).await.is_ok() && send.write_all(&body).await.is_ok() {
                let _ = send.finish();
            }
            // RFC 9114 section 4.1: the answer is whole, so whatever the
            // request still sends is not wanted.
            let _ = reading.recv.stop(wire(Code::NoError));
        }
        Answer::Push { response, hub, connection: number, address } => {
            if send.write_all(&head(&response, None)).await.is_ok() {
                serve_halves(reading, Writing::new(send), &[], hub, number, address).await;
            }
        }
        Answer::Program { response, door, claim } => {
            if send.write_all(&head(&response, None)).await.is_ok() {
                door.arrive(Box::new(reading), Box::new(Writing::new(send)), Vec::new(), claim).await;
            }
        }
    }
}

/// Ends a request stream with a stream error, both ways (RFC 9114 section
/// 8).
fn abandon(send: &mut SendStream, recv: &mut RecvStream, code: Code) {
    let _ = send.reset(wire(code));
    let _ = recv.stop(wire(code));
}

/// A request read from a field section, as the request [`decide`] takes:
/// its method, its path and query, its fields, and `:protocol` as hyper
/// carries HTTP/2's (`hyper::ext::Protocol`).
///
/// # Errors
/// `H3_MESSAGE_ERROR`, a stream error, for a method, a path, a field or a
/// `:protocol` that is not one (RFC 9114 section 4.1.2).
fn as_request(read: &message::Request) -> Result<Request<()>, Error> {
    let malformed = Error::stream(Code::MessageError);
    let mut request = Request::new(());
    *request.version_mut() = Version::HTTP_3;
    *request.method_mut() = Method::from_bytes(&read.method).map_err(|_| malformed)?;
    // A CONNECT without `:protocol` has no path (RFC 9114 section 4.4);
    // it names no channel, and is answered as a path that names none.
    *request.uri_mut() = read.path.as_deref().map_or_else(|| Ok(Uri::from_static("/")), Uri::try_from).map_err(|_| malformed)?;
    for field in &read.fields {
        let name = HeaderName::from_bytes(&field.name).map_err(|_| malformed)?;
        request.headers_mut().append(name, HeaderValue::from_bytes(&field.value).map_err(|_| malformed)?);
    }
    if let Some(protocol) = &read.protocol {
        request.extensions_mut().insert(Protocol::from(str::from_utf8(protocol).map_err(|_| malformed)?));
    }
    Ok(request)
}

/// A response's HEADERS frame: `:status`, its fields without those of an
/// HTTP/1.1 connection, which HTTP/3 does not carry (RFC 9114 section
/// 4.2), `date`, and `content-length` when `length` is its content's.
fn head(response: &Response<Whole>, length: Option<usize>) -> Vec<u8> {
    let length = length.map(|length| length.to_string());
    let date = SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|since| http_date(since.as_secs()));
    let fields =
        response.headers().iter().filter(|(name, _)| !CONNECTION_SPECIFIC.contains(&name.as_str().as_bytes()) && *name != CONTENT_LENGTH && *name != DATE);
    let status = response.status();
    let all = std::iter::once((STATUS, status.as_str().as_bytes()))
        .chain(fields.map(|(name, value)| (name.as_str().as_bytes(), value.as_bytes())))
        .chain(date.as_ref().map(|date| (DATE.as_str().as_bytes(), date.as_bytes())))
        .chain(length.as_ref().map(|length| (CONTENT_LENGTH.as_str().as_bytes(), length.as_bytes())));
    let mut section = Vec::new();
    qpack::encode(all, &mut section);
    let mut out = Vec::new();
    frame::write(frame::HEADERS, &section, &mut out);
    out
}

/// A whole response: its HEADERS frame, and its content as one DATA
/// frame, unless there is none or `with_content` is false, as for the
/// answer to a HEAD, whose head still says the content's length (RFC 9110
/// section 9.3.2).
fn whole(response: Response<Whole>, with_content: bool) -> (Vec<u8>, Vec<u8>) {
    let (parts, body) = response.into_parts();
    let content = body.into_bytes();
    let head = head(&Response::from_parts(parts, Whole::default()), Some(content.len()));
    let mut data = Vec::new();
    if with_content && !content.is_empty() {
        frame::write(frame::DATA, &content, &mut data);
    }
    (head, data)
}

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_DAY: u64 = 86_400;
const DAYS_PER_WEEK: u64 = 7;
/// 1 January 1970 was a Thursday, the fifth day of a week from Sunday.
const WEEKDAY_OF_EPOCH: u64 = 4;
/// The Gregorian calendar repeats every 400 years: 146,097 days. Within
/// one, a century is 36,524 days, four years 1,461 and a year 365.
const DAYS_PER_400_YEARS: u64 = 146_097;
const DAYS_PER_100_YEARS: u64 = 36_524;
const DAYS_PER_4_YEARS: u64 = 1_461;
const DAYS_PER_YEAR: u64 = 365;
/// The days from 1 March of year 0 to 1 January 1970: years are counted
/// from March, so that a leap day is a year's last day.
const DAYS_BEFORE_EPOCH: u64 = 719_468;
/// The days of the five months from March to July, 153: the pattern of
/// 31- and 30-day months repeats from August.
const DAYS_PER_5_MONTHS: u64 = 153;
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The time `seconds` after the start of 1970, as HTTP writes a date (RFC
/// 9110 section 5.6.7's IMF-fixdate): `Sun, 06 Nov 1994 08:49:37 GMT`.
fn http_date(seconds: u64) -> String {
    let (days, of_day) = (seconds / SECONDS_PER_DAY, seconds % SECONDS_PER_DAY);
    let weekday = days.saturating_add(WEEKDAY_OF_EPOCH) % DAYS_PER_WEEK;
    // The day within its 400 years, then the year within them, each from
    // 1 March: a year's days before it are 365 a year and the leap days
    // of every fourth year but each hundredth.
    let from_march_0 = days.saturating_add(DAYS_BEFORE_EPOCH);
    let (cycle, day_of_cycle) = (from_march_0 / DAYS_PER_400_YEARS, from_march_0 % DAYS_PER_400_YEARS);
    let leap_days_passed =
        (day_of_cycle / (DAYS_PER_4_YEARS - 1)).saturating_sub(day_of_cycle / DAYS_PER_100_YEARS).saturating_add(day_of_cycle / (DAYS_PER_400_YEARS - 1));
    let year_of_cycle = day_of_cycle.saturating_sub(leap_days_passed) / DAYS_PER_YEAR;
    let day_of_year =
        day_of_cycle.saturating_sub((DAYS_PER_YEAR.saturating_mul(year_of_cycle)).saturating_add(year_of_cycle / 4).saturating_sub(year_of_cycle / 100));
    // The month from March as 0, and the day in it.
    let month_from_march = (day_of_year.saturating_mul(5).saturating_add(2)) / DAYS_PER_5_MONTHS;
    let day = day_of_year.saturating_sub((month_from_march.saturating_mul(DAYS_PER_5_MONTHS).saturating_add(2)) / 5).saturating_add(1);
    let month = if month_from_march < 10 { month_from_march.saturating_add(2) } else { month_from_march.saturating_sub(10) };
    // January and February belong to the next year of the calendar.
    let year = year_of_cycle.saturating_add(cycle.saturating_mul(400)).saturating_add(u64::from(month < 2));
    let name = |names: &[&'static str], index: u64| usize::try_from(index).ok().and_then(|index| names.get(index).copied()).unwrap_or_default();
    format!(
        "{}, {day:02} {} {year:04} {:02}:{:02}:{:02} GMT",
        name(&WEEKDAYS, weekday),
        name(&MONTHS, month),
        of_day / SECONDS_PER_HOUR,
        of_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        of_day % SECONDS_PER_MINUTE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 9110 section 5.6.7's example, the epoch, and days either side
    /// of a leap day, in a year divisible by 400 and one that is not.
    #[test]
    fn dates_are_written_as_http_writes_them() {
        assert_eq!(http_date(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(http_date(951_782_399), "Mon, 28 Feb 2000 23:59:59 GMT");
        assert_eq!(http_date(951_782_400), "Tue, 29 Feb 2000 00:00:00 GMT");
        assert_eq!(http_date(951_868_800), "Wed, 01 Mar 2000 00:00:00 GMT");
        assert_eq!(http_date(1_709_164_800), "Thu, 29 Feb 2024 00:00:00 GMT");
        assert_eq!(http_date(1_735_689_599), "Tue, 31 Dec 2024 23:59:59 GMT");
        assert_eq!(http_date(4_107_542_400), "Mon, 01 Mar 2100 00:00:00 GMT");
        assert_eq!(http_date(4_107_542_399), "Sun, 28 Feb 2100 23:59:59 GMT");
    }
}
