//! The server on Cloudflare (decision 2, requirement 1.2; decision 20): a
//! Worker that routes each channel's requests to a Durable Object of its
//! own, and the object, which runs the same `teletext-runtime` as the
//! native host.
//!
//! - **Viewers** connect to `/<channel>/push` over an HTTP/1.1 upgrade,
//!   the only way Cloudflare takes a WebSocket (DEC-6962). The Worker
//!   refuses the upgrade as the native host does, through the runtime's
//!   one rule (`landing::push_upgrade`, DOC-1917): 403 with `Origin`, 426
//!   naming the subprotocol without it (DEC-1759 lessons 14 and 18). The
//!   object accepts the rest with the hibernation interface and names the
//!   subprotocol in its 101.
//! - **The channel's program** connects to `/<channel>/program` with the
//!   channel's secret (requirement 1.5), checked in constant time by the
//!   Worker (`landing::program_admitted`, REQ-1024): 401 without it, 409
//!   while a program is connected. It speaks the protocol of DOC-1964
//!   (DEC-1482), one line a message; the object is the server of that
//!   WebSocket too, and opens no connection itself.
//! - **Hibernation** (DEC-1755 option 1, TODO-4579): the object's memory
//!   goes whenever it hibernates, while its connections stay open. Each
//!   viewer's WebSocket carries, as its attachment, what the runtime keeps
//!   of its connection (`channel::Resume`: its numbers, its selection and
//!   query, the viewer it proved), written after every event that changed
//!   it; at a wake each connection is given back with it and the program,
//!   still connected, is sent `start` and `connected` for each, as after a
//!   restart, and sends each its page again. Nothing of it outlives the
//!   connection: the attachment goes with its WebSocket.
//! - **Nothing of the channel's content is kept** (REQ-1474): the object's
//!   SQLite storage holds only its folder and its viewer-handle secret
//!   (VS-305).
//! - **Time** is the object's single alarm: the runtime's next deadline.
//!   Nothing is sent for the time's sake (requirement 11.2), so a channel
//!   whose pages do not change lets its object hibernate, watched or not.
//! - **Keepalive** (requirement 11.3): the receiver pings a connection it
//!   has heard nothing on, and Cloudflare's runtime answers the ping
//!   itself, without waking the object; a Worker cannot send one.
//!
//! The crate is empty on every other target.

#![cfg(target_arch = "wasm32")]
// No panicking constructs: everything here reads what clients and
// programs send (DEVELOPMENT.md section 6).
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

mod answer;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use teletext_runtime::channel::{DefaultRules, Resume};
use teletext_runtime::descriptions::VisitorId;
use teletext_runtime::frame;
use teletext_runtime::json::{self, Value, Writer};
use teletext_runtime::landing::{self, PUSH_SUBPROTOCOL, PushRefusal, Route, WEBSOCKET_VERSION, push_upgrade, route};
use teletext_runtime::protocol::PageNumber;
use teletext_runtime::viewer::{HANDLE_SECRET_BYTES, HandleSecret, RandomSource, RandomUnavailable};
use teletext_runtime::{Channel, ChannelConfig, ConnectionId, Effect, InitialPage, Level};
use worker::{
    Date, Env, Error, Headers, Method, Request, Response, Result, SqlStorage, SqlStorageValue, State, WebSocket, WebSocketIncomingMessage, WebSocketPair,
    durable_object, event,
};

use answer::Pending;

/// The Durable Object binding, in `wrangler.toml`.
const CHANNEL_BINDING: &str = "CHANNEL";
/// The channels served, a JSON array of `{"folder","name","wheel","default_rules"}`, in `wrangler.toml`.
const CHANNELS_VARIABLE: &str = "CHANNELS";
/// The address viewers reach the Worker at, such as `https://cf.neoteletext.com`.
const ORIGIN_VARIABLE: &str = "ORIGIN";
/// Each channel's program's secret is the Worker secret of this name and
/// the channel's folder, upper case, `-` as `_`: `PROGRAM_SECRET_HACKADAY`.
const SECRET_PREFIX: &str = "PROGRAM_SECRET_";
/// The tags the object gives the connections it accepts.
const VIEWER_TAG: &str = "viewer";
const PROGRAM_TAG: &str = "program";
/// The page a viewer starts on.
const INITIAL_PAGE: u16 = 0x100;
/// The most a WebSocket's attachment may hold once serialized: "Maximum
/// serialized size is 16,384 bytes" (Cloudflare's Durable Objects
/// documentation, "Use `WebSockets`", `serializeAttachment`, read 21
/// November 2026). The attachment is written as JSON text, whose length
/// is checked against this before it is written.
const MOST_ATTACHMENT_BYTES: usize = 16_384;
/// The object's tables, a migration a version (the numbered migrations of
/// the owner's SQLite rule): the object's own names (its folder, its
/// viewer-handle secret, the schema's version); migration 2 drops the pages
/// kept before DEC-1482; migration 3 the session cookie's key, which viewer
/// state replaced (VS-603).
const MIGRATIONS: [&str; 3] = [
    "CREATE TABLE kept (key TEXT PRIMARY KEY, record BLOB NOT NULL) STRICT; \
     CREATE TABLE meta (name TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;",
    "DROP TABLE kept;",
    "DELETE FROM meta WHERE name = 'session_key';",
];
/// The meta row naming the schema's version.
const SCHEMA: &str = "schema";
/// The meta row naming the channel's folder, so an object woken by a
/// WebSocket's message knows which channel it is.
const FOLDER: &str = "folder";
/// The meta row holding the viewer-handle secret (VS-305).
const HANDLE_SECRET: &str = "viewer_handle_key";
/// The header in which Cloudflare names the viewer's address, which it
/// sets itself, for the bound on new viewers (VS-304).
const CONNECTING_ADDRESS: &str = "cf-connecting-ip";
/// The value of `Upgrade` that asks for a WebSocket (RFC 6455 section 4.1
/// item 5), matched without regard to case.
const WEBSOCKET: &str = "websocket";
/// HTTP statuses (RFC 9110 section 15).
const PERMANENT_REDIRECT: u16 = 308;
const BAD_REQUEST: u16 = 400;
const UNAUTHORIZED: u16 = 401;
const NOT_FOUND: u16 = 404;
const CONFLICT: u16 = 409;
const UPGRADE_REQUIRED: u16 = 426;

/// The Web Crypto interface's random bytes, through `getrandom` (VS-205).
struct WebCrypto;

impl RandomSource for WebCrypto {
    fn fill(&mut self, bytes: &mut [u8]) -> std::result::Result<(), RandomUnavailable> {
        getrandom::fill(bytes).map_err(|_| RandomUnavailable)
    }
}

/// A runtime failure, as the Worker's error.
fn failure(text: impl Into<String>) -> Error {
    Error::RustError(text.into())
}

/// Milliseconds since the Unix epoch.
fn epoch_ms() -> u64 {
    Date::now().as_millis()
}

/// An empty answer of `code`.
fn status(code: u16) -> Result<Response> {
    Ok(Response::empty()?.with_status(code))
}

/// Sends binary bytes as one message, or as several when they are more
/// than a receiver takes in one (the runtime's `frame::messages`).
fn send_binary(socket: &WebSocket, bytes: &[u8]) {
    for part in frame::messages(bytes, frame::MOST_MESSAGE_BYTES) {
        let _ = socket.send_with_bytes(part);
    }
}

/// One channel the Worker serves, as `CHANNELS` lists it: its folder, its
/// name, whether it uses the mouse wheel over its pages, and the table its
/// pages have when its program gives them none (page-rules.md RULE-53),
/// read as the native host reads them from `channels.json`.
struct Served {
    folder: String,
    name: String,
    wheel: bool,
    default_rules: DefaultRules,
}

/// The channels the Worker serves. An entry is
/// `{"folder":…,"name":…,"wheel":true|false,"default_rules":"none"|"send_all"}`;
/// `name` is the folder when left out, `wheel` false and `default_rules`
/// `"none"`. A value of `default_rules` or `wheel` of another form is a
/// fault of the configuration, refused as the native host refuses it.
fn channels(env: &Env) -> Result<Vec<Served>> {
    let text = env.var(CHANNELS_VARIABLE)?.to_string();
    let value = json::parse(text.as_bytes()).map_err(|fault| failure(format!("{CHANNELS_VARIABLE} is not JSON: {fault:?}")))?;
    let items = value.items().ok_or_else(|| failure(format!("{CHANNELS_VARIABLE} must be an array")))?;
    let mut served = Vec::new();
    for item in items {
        let Some(folder) = item.get("folder").and_then(Value::text) else { continue };
        let name = item.get("name").and_then(Value::text).unwrap_or(folder);
        let wheel = match item.get("wheel") {
            None => false,
            Some(value) => value.boolean().ok_or_else(|| failure(format!("{CHANNELS_VARIABLE}: {folder}'s wheel is true or false")))?,
        };
        let default_rules = match item.get("default_rules").map(Value::text) {
            None | Some(Some("none")) => DefaultRules::None,
            Some(Some("send_all")) => DefaultRules::SendAll,
            Some(_) => return Err(failure(format!("{CHANNELS_VARIABLE}: {folder}'s default_rules is \"none\" or \"send_all\""))),
        };
        served.push(Served { folder: folder.to_owned(), name: name.to_owned(), wheel, default_rules });
    }
    Ok(served)
}

/// Whether a request is a WebSocket's opening handshake the object can
/// accept, or the answer to one that is not, as the native host answers
/// it: a GET with no query (neither WebSocket's address takes one), else
/// 400; version 13, else 426 naming it (RFC 6455 section 4.2.2 item 4);
/// `Upgrade: websocket` and a key, else 426. Cloudflare takes a WebSocket
/// over HTTP/1.1 alone, so there is no extended CONNECT to read
/// (DEC-6962).
fn handshake(request: &Request, query: Option<&str>) -> Result<Option<Response>> {
    if request.method() != Method::Get || query.is_some() {
        return status(BAD_REQUEST).map(Some);
    }
    let version = request.headers().get("sec-websocket-version")?;
    if version.as_deref().map(str::as_bytes) != Some(WEBSOCKET_VERSION) {
        let mut response = status(UPGRADE_REQUIRED)?;
        response.headers_mut().set("sec-websocket-version", &String::from_utf8_lossy(WEBSOCKET_VERSION))?;
        return Ok(Some(response));
    }
    let wants = request.headers().get("upgrade")?.is_some_and(|value| value.trim().eq_ignore_ascii_case(WEBSOCKET));
    if !wants || request.headers().get("sec-websocket-key")?.is_none() {
        return status(UPGRADE_REQUIRED).map(Some);
    }
    Ok(None)
}

/// The answer to a push upgrade the runtime's rule refused (DOC-1917):
/// its status, and for the subprotocol the one spoken.
fn refused_push(refusal: PushRefusal) -> Result<Response> {
    let mut response = status(refusal.status())?;
    if refusal == PushRefusal::Subprotocol {
        response.headers_mut().set("upgrade", WEBSOCKET)?;
        response.headers_mut().set("sec-websocket-protocol", PUSH_SUBPROTOCOL)?;
    }
    Ok(response)
}

/// The Worker: routes each channel's requests to its object, refusing a
/// program without the channel's secret (requirement 1.5) and a push
/// upgrade the runtime's rule refuses before the object is woken.
#[event(fetch)]
async fn fetch(request: Request, env: Env, _context: worker::Context) -> Result<Response> {
    let url = request.url()?;
    let (folder, asked) = route(url.path());
    let channels = channels(&env)?;
    // The answers of REQ-1693 item 7 and the two WebSockets: no listing
    // at `/`, no channel's files.
    let reads = request.method() == Method::Get || request.method() == Method::Head;
    let answered = matches!(asked, Route::Program | Route::Descriptor | Route::Push) || (reads && asked != Route::Missing);
    if !channels.iter().any(|served| served.folder == folder) || !answered {
        return status(NOT_FOUND);
    }
    match asked {
        Route::ToFolder => {
            let mut response = status(PERMANENT_REDIRECT)?;
            response.headers_mut().set("location", &landing::folder_location(folder, url.query()))?;
            return Ok(response);
        }
        Route::ToPlace => {
            let mut response = status(PERMANENT_REDIRECT)?;
            response.headers_mut().set("location", &landing::place_location(url.path(), url.query()))?;
            return Ok(response);
        }
        // 401 before anything else of the request is looked at, so that
        // nothing is told to whoever lacks the secret (REQ-1024).
        Route::Program => {
            let name = format!("{SECRET_PREFIX}{}", folder.to_uppercase().replace('-', "_"));
            let expected = env.secret(&name).map(|secret| secret.to_string()).unwrap_or_default();
            let presented = request.headers().get("authorization")?;
            if !landing::program_admitted(presented.as_deref().map(str::as_bytes), expected.as_bytes()) {
                return status(UNAUTHORIZED);
            }
            if let Some(response) = handshake(&request, url.query())? {
                return Ok(response);
            }
        }
        Route::Push => {
            if let Some(response) = handshake(&request, url.query())? {
                return Ok(response);
            }
            let origin = request.headers().has("origin")?;
            let offered = request.headers().get("sec-websocket-protocol")?;
            if let Err(refusal) = push_upgrade(origin, offered.iter().map(String::as_bytes)) {
                return refused_push(refusal);
            }
        }
        Route::Page | Route::Described(_) | Route::Descriptor | Route::Missing => {}
    }
    env.durable_object(CHANNEL_BINDING)?.id_from_name(folder)?.get_stub()?.fetch_with_request(request).await
}

/// What a viewer's WebSocket carries as its attachment, kept by Cloudflare
/// through the object's hibernation: the connection's number, the address
/// it came from (VS-304) and what the runtime keeps of it (DEC-1755
/// option 1), none while it is not yet known to the channel.
#[derive(Serialize, Deserialize)]
struct Attachment {
    connection: ConnectionId,
    address: Option<IpAddr>,
    resume: Option<Resume>,
}

/// A viewer's attachment, read; none for one that cannot be read.
fn attachment_of(socket: &WebSocket) -> Option<Attachment> {
    let text = socket.deserialize_attachment::<String>().ok().flatten()?;
    serde_json::from_str(&text).ok()
}

/// A channel's running state, made when the object wakes.
struct Live {
    folder: String,
    channel: Channel,
    /// When the host's clock started, in milliseconds since the Unix epoch.
    started_ms: u64,
    /// The viewers' connections, one a tab (DEC-1527), with the address
    /// each came from.
    viewers: BTreeMap<ConnectionId, (WebSocket, Option<IpAddr>)>,
    /// The next connection's number: never reused while the object runs.
    next_connection: ConnectionId,
    program: Option<WebSocket>,
    /// The alarm last set, in milliseconds since the Unix epoch.
    alarm: Option<u64>,
    /// Web visitors waiting for what a page's address shows (REQ-1728).
    visitors: BTreeMap<VisitorId, Pending>,
    /// The next visitor's number.
    next_visitor: VisitorId,
}

impl Live {
    /// The host's clock.
    fn now(&self) -> u64 {
        epoch_ms().saturating_sub(self.started_ms)
    }

    /// Carries out the channel's effects, then writes again the attachment
    /// of each connection whose state they or `also` may have changed.
    fn apply(&mut self, also: Option<ConnectionId>) {
        let mut changed: Vec<ConnectionId> = also.into_iter().collect();
        for effect in self.channel.take_effects() {
            match effect {
                Effect::Send { connection, bytes } => {
                    if let Some((socket, _)) = self.viewers.get(&connection) {
                        send_binary(socket, &bytes);
                        changed.push(connection);
                    }
                }
                Effect::Close { connection, code } => {
                    if let Some((socket, _)) = self.viewers.get(&connection) {
                        let _ = socket.close(Some(code), Some("protocol error"));
                    }
                }
                Effect::ToProgram { line, .. } | Effect::MoveToProgram { line, .. } => {
                    if let Some(program) = &self.program {
                        let text = String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(&line)).into_owned();
                        let _ = program.send_with_str(text);
                    }
                }
                Effect::Described { visitor, answer } => {
                    if let Some(pending) = self.visitors.remove(&visitor) {
                        pending.answer(answer);
                    }
                }
                Effect::Log { level, event, text } => log(level, &self.folder, event, &text),
            }
        }
        changed.sort_unstable();
        changed.dedup();
        for connection in changed {
            self.keep(connection);
        }
    }

    /// Writes `connection`'s attachment: what the runtime keeps of it now,
    /// or none of that when it would not fit (the connection then starts
    /// afresh at a wake, and the operator is told).
    fn keep(&self, connection: ConnectionId) {
        let Some((socket, address)) = self.viewers.get(&connection) else { return };
        let resume = self.channel.resume_of(connection);
        let mut attachment = Attachment { connection, address: *address, resume };
        let mut text = serde_json::to_string(&attachment).unwrap_or_default();
        if text.len() > MOST_ATTACHMENT_BYTES {
            log(Level::Warn, &self.folder, "attachment_too_long", &format!("connection {connection}'s state is past {MOST_ATTACHMENT_BYTES} bytes: not kept"));
            attachment.resume = None;
            text = serde_json::to_string(&attachment).unwrap_or_default();
        }
        if let Err(error) = socket.serialize_attachment(text) {
            log(Level::Error, &self.folder, "attachment", &format!("connection {connection}'s state was not kept: {error}"));
        }
    }

    /// When the object next needs its alarm, in milliseconds since the
    /// Unix epoch: the runtime's next deadline, which takes the channel
    /// mutably because it first recounts the connections changed since.
    fn next_alarm(&mut self) -> Option<u64> {
        self.channel.next_wake().map(|at| self.started_ms.saturating_add(at))
    }
}

/// Writes one event for Workers Logs, one JSON object a line, as the native
/// host's log writes it (requirement 7.1).
fn log(level: Level, channel: &str, event: &str, text: &str) {
    let level = match level {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
    };
    let mut writer = Writer::nested();
    writer.text("level", level).text("channel", channel).text("event", event).text("message", text);
    let mut line = writer.line();
    line.pop();
    worker::console_log!("{}", String::from_utf8_lossy(&line));
}

pub use object::ChannelObject;

#[allow(missing_docs, reason = "the durable_object macro generates the methods the Workers runtime calls, which carry no documentation")]
mod object {
    use super::{Env, Live, RefCell, State, durable_object};

    /// One channel's Durable Object.
    #[durable_object]
    pub struct ChannelObject {
        pub(crate) state: State,
        pub(crate) env: Env,
        pub(crate) live: RefCell<Option<Live>>,
    }
}

impl ChannelObject {
    /// Runs the object's tables' migrations not yet run.
    fn migrate(sql: &SqlStorage) -> Result<()> {
        let version = Self::meta(sql, SCHEMA).ok().flatten().and_then(|bytes| bytes.first().copied()).map_or(0, usize::from);
        for (number, migration) in MIGRATIONS.iter().enumerate().skip(version) {
            sql.exec(migration, None)?;
            let reached = u8::try_from(number.saturating_add(1)).map_err(|_| failure("too many migrations"))?;
            Self::set_meta(sql, SCHEMA, vec![reached])?;
        }
        Ok(())
    }

    fn meta(sql: &SqlStorage, name: &str) -> Result<Option<Vec<u8>>> {
        let cursor = sql.exec("SELECT value FROM meta WHERE name = ?", vec![name.into()])?;
        for row in cursor.raw() {
            if let Some(SqlStorageValue::Blob(bytes)) = row?.into_iter().next() {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn set_meta(sql: &SqlStorage, name: &str, value: Vec<u8>) -> Result<()> {
        sql.exec("INSERT OR REPLACE INTO meta (name, value) VALUES (?, ?)", vec![name.into(), value.into()]).map(|_| ())
    }

    /// Makes the channel's state if the object has just woken: its folder,
    /// its key, and the connections it still holds, each given back as its
    /// attachment kept it. `folder` is the channel's, when a request names
    /// it.
    fn wake(&self, folder: Option<&str>) -> Result<()> {
        if self.live.borrow().is_some() {
            return Ok(());
        }
        let sql = self.state.storage().sql();
        if Self::meta(&sql, SCHEMA).is_err() {
            sql.exec(MIGRATIONS.first().copied().unwrap_or_default(), None)?;
            Self::set_meta(&sql, SCHEMA, vec![1])?;
        }
        Self::migrate(&sql)?;
        let folder = match folder {
            Some(folder) => {
                Self::set_meta(&sql, FOLDER, folder.as_bytes().to_vec())?;
                folder.to_owned()
            }
            None => Self::meta(&sql, FOLDER)?
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .ok_or_else(|| failure("the object does not know its channel"))?,
        };
        let secret = if let Some(bytes) = Self::meta(&sql, HANDLE_SECRET)?.and_then(|bytes| <[u8; HANDLE_SECRET_BYTES]>::try_from(bytes.as_slice()).ok()) {
            HandleSecret::new(bytes)
        } else {
            let mut bytes = [0_u8; HANDLE_SECRET_BYTES];
            getrandom::fill(&mut bytes).map_err(|error| failure(format!("no random bytes: {error}")))?;
            Self::set_meta(&sql, HANDLE_SECRET, bytes.to_vec())?;
            HandleSecret::new(bytes)
        };
        let served = channels(&self.env)?.into_iter().find(|served| served.folder == folder);
        let (name, wheel, default_rules) =
            served.map_or_else(|| (folder.clone(), false, DefaultRules::None), |served| (served.name, served.wheel, served.default_rules));
        let origin = self.env.var(ORIGIN_VARIABLE)?.to_string();
        let started_ms = epoch_ms();
        let config = ChannelConfig {
            name,
            address: format!("{origin}/{folder}/"),
            initial_page: InitialPage::new(INITIAL_PAGE).map_err(failure)?,
            epoch_at_start_ms: started_ms,
            other_addresses: Vec::new(),
            wheel,
            default_rules,
        };
        let channel = Channel::new(config, &secret, Box::new(WebCrypto));
        let mut live = Live {
            folder,
            channel,
            started_ms,
            viewers: BTreeMap::new(),
            next_connection: 1,
            program: self.state.get_websockets_with_tag(PROGRAM_TAG).into_iter().next(),
            alarm: None,
            visitors: BTreeMap::new(),
            next_visitor: 0,
        };
        // Each viewer still connected is given back as its attachment kept
        // it (DEC-1755 option 1), or starts afresh where nothing was kept.
        let mut resumed = 0_usize;
        for socket in self.state.get_websockets_with_tag(VIEWER_TAG) {
            let Some(Attachment { connection, address, resume }) = attachment_of(&socket) else { continue };
            live.next_connection = live.next_connection.max(connection.saturating_add(1));
            live.viewers.insert(connection, (socket, address));
            let now = live.now();
            if let Some(resume) = resume {
                live.channel.resumed(connection, address, resume, now);
                resumed = resumed.saturating_add(1);
            } else {
                live.channel.opened(connection, address, now);
            }
        }
        // A program still connected is started again and told of each
        // connection, which it answers with each one's page.
        if live.program.is_some() {
            let now = live.now();
            live.channel.program_started(now);
        }
        if !live.viewers.is_empty() || live.program.is_some() {
            let text = format!("{} connections given back, {resumed} with their state", live.viewers.len());
            log(Level::Info, &live.folder, "woke", &text);
        }
        live.apply(None);
        *self.live.borrow_mut() = Some(live);
        Ok(())
    }

    /// Sets the object's alarm to when it is next needed, if that changed.
    async fn schedule(&self) -> Result<()> {
        let (wanted, set) = {
            let mut live = self.live.borrow_mut();
            let Some(live) = live.as_mut() else { return Ok(()) };
            (live.next_alarm(), live.alarm)
        };
        if wanted == set {
            return Ok(());
        }
        let storage = self.state.storage();
        match wanted {
            Some(at) => storage.set_alarm(std::time::Duration::from_millis(at.saturating_sub(epoch_ms()))).await?,
            None => storage.delete_alarm().await?,
        }
        if let Some(live) = self.live.borrow_mut().as_mut() {
            live.alarm = wanted;
        }
        Ok(())
    }

    /// Runs `work` on the live channel, then carries out its effects,
    /// writing again the attachment of `connection` and of each connection
    /// they sent to.
    fn with_live<T>(&self, connection: Option<ConnectionId>, work: impl FnOnce(&mut Live) -> T) -> Result<T> {
        let mut live = self.live.borrow_mut();
        let live = live.as_mut().ok_or_else(|| failure("the channel is not awake"))?;
        let answer = work(live);
        live.apply(connection);
        Ok(answer)
    }

    /// A viewer's push WebSocket, which the Worker found a receiver's:
    /// 101 naming the subprotocol (DEC-1759 lesson 14). No `Cookie` is
    /// read and no `Set-Cookie` sent (VS-201, VS-354).
    fn accept_viewer(&self, request: &Request) -> Result<Response> {
        let pair = WebSocketPair::new()?;
        let address = request.headers().get(CONNECTING_ADDRESS)?.and_then(|text| text.trim().parse::<IpAddr>().ok());
        self.state.accept_websocket_with_tags(&pair.server, &[VIEWER_TAG]);
        let connection = self.with_live(None, |live| {
            let connection = live.next_connection;
            live.next_connection = live.next_connection.saturating_add(1);
            live.viewers.insert(connection, (pair.server, address));
            let now = live.now();
            live.channel.opened(connection, address, now);
            connection
        })?;
        if let Some(live) = self.live.borrow().as_ref() {
            live.keep(connection);
        }
        let mut response = Response::from_websocket(pair.client)?;
        response.headers_mut().set("sec-websocket-protocol", PUSH_SUBPROTOCOL)?;
        Ok(response)
    }

    /// The program's WebSocket: one at a time.
    fn accept_program(&self) -> Result<Response> {
        if self.live.borrow().as_ref().is_some_and(|live| live.program.is_some()) {
            return status(CONFLICT);
        }
        let pair = WebSocketPair::new()?;
        self.state.accept_websocket_with_tags(&pair.server, &[PROGRAM_TAG]);
        self.with_live(None, |live| {
            live.program = Some(pair.server);
            let now = live.now();
            live.channel.program_started(now);
            log(Level::Info, &live.folder, "program_connected", "the program connected");
        })?;
        Response::from_websocket(pair.client)
    }

    /// The channel's `channel.ttx`, generated (DEC-1467 item 1).
    fn descriptor(&self) -> Result<Response> {
        let body = self.live.borrow().as_ref().map(|live| live.channel.descriptor()).unwrap_or_default();
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-cache")?;
        Ok(Response::from_bytes(body)?.with_headers(headers))
    }

    /// The small page a web browser shows at an address of the channel
    /// (DEC-1669), naming what was asked for.
    fn page(&self, tail: &str) -> Result<Response> {
        let body = self.live.borrow().as_ref().map(|live| live.channel.page(tail)).unwrap_or_default();
        Self::html(body, landing::CONTENT_SECURITY_POLICY)
    }

    /// What a page's address answers (REQ-1728, REQ-1725 item 2): the
    /// program is asked, through the runtime, and its answer comes at once
    /// or within the runtime's bound, which the object's alarm keeps; the
    /// same for whoever asks, since no request header reaches here.
    async fn described(&self, page: PageNumber, tail: &str) -> Result<Response> {
        let (pending, waiting) = answer::pair();
        self.with_live(None, |live| {
            let visitor = live.next_visitor;
            live.next_visitor = live.next_visitor.wrapping_add(1);
            live.visitors.insert(visitor, pending);
            let now = live.now();
            live.channel.describe(visitor, page, now);
        })?;
        self.schedule().await?;
        let answer = waiting.await;
        let (body, policy) = {
            let live = self.live.borrow();
            let live = live.as_ref().ok_or_else(|| failure("the channel is not awake"))?;
            landing::described(live.channel.name(), live.channel.address(), tail, page, &answer)
        };
        Self::html(body.into_bytes(), policy)
    }

    /// A page of HTML, under `policy`.
    fn html(body: Vec<u8>, policy: &str) -> Result<Response> {
        let headers = Headers::new();
        headers.set("content-type", "text/html; charset=utf-8")?;
        headers.set("cache-control", "no-cache")?;
        headers.set("content-security-policy", policy)?;
        headers.set("x-content-type-options", "nosniff")?;
        headers.set("referrer-policy", "no-referrer")?;
        Ok(Response::from_bytes(body)?.with_headers(headers))
    }

    /// Whether `socket` is the program's.
    fn is_program(&self, socket: &WebSocket) -> bool {
        self.state.get_tags(socket).iter().any(|tag| tag == PROGRAM_TAG)
    }
}

impl worker::DurableObject for ChannelObject {
    fn new(state: State, env: Env) -> Self {
        Self { state, env, live: RefCell::new(None) }
    }

    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        let (folder, asked) = route(url.path());
        self.wake(Some(folder))?;
        let response = match asked {
            Route::Program => self.accept_program(),
            Route::Push => self.accept_viewer(&request),
            Route::Descriptor => self.descriptor(),
            Route::Page => self.page(&landing::tail(url.path(), url.query())),
            Route::Described(page) => self.described(page, &landing::tail(url.path(), url.query())).await,
            Route::ToFolder | Route::ToPlace | Route::Missing => status(NOT_FOUND),
        };
        self.schedule().await?;
        response
    }

    async fn alarm(&self) -> Result<Response> {
        self.wake(None)?;
        self.with_live(None, |live| {
            live.alarm = None;
            let now = live.now();
            live.channel.deadline(now);
        })?;
        self.schedule().await?;
        Response::ok("")
    }

    async fn websocket_message(&self, socket: WebSocket, message: WebSocketIncomingMessage) -> Result<()> {
        self.wake(None)?;
        let program = self.is_program(&socket);
        let connection = if program { None } else { attachment_of(&socket).map(|attachment| attachment.connection) };
        self.with_live(connection, |live| {
            let now = live.now();
            match message {
                WebSocketIncomingMessage::String(text) if program => {
                    for line in landing::program_lines(text.as_bytes()) {
                        live.channel.from_program(line, now);
                    }
                }
                WebSocketIncomingMessage::Binary(bytes) if !program => {
                    if let Some(connection) = connection {
                        live.channel.received(connection, &bytes, now);
                    }
                }
                _ => {}
            }
        })?;
        self.schedule().await
    }

    async fn websocket_close(&self, socket: WebSocket, _code: usize, _reason: String, _was_clean: bool) -> Result<()> {
        self.gone(&socket).await
    }

    async fn websocket_error(&self, socket: WebSocket, _error: Error) -> Result<()> {
        self.gone(&socket).await
    }
}

impl ChannelObject {
    /// A WebSocket closed: a viewer's connection is `gone`, and what its
    /// attachment kept goes with it; the program's lines are dropped until
    /// it connects again, and its channel is unavailable meanwhile.
    async fn gone(&self, socket: &WebSocket) -> Result<()> {
        self.wake(None)?;
        let program = self.is_program(socket);
        let connection = attachment_of(socket).map(|attachment| attachment.connection);
        self.with_live(None, |live| {
            if program {
                live.program = None;
                live.channel.program_ended();
                log(Level::Info, &live.folder, "program_gone", "the program went");
            } else if let Some(connection) = connection
                && live.viewers.remove(&connection).is_some()
            {
                let now = live.now();
                live.channel.closed(connection, now);
            }
        })?;
        self.schedule().await
    }
}
