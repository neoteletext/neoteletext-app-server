//! The server on Cloudflare (decision 2, requirement 1.2; decision 20): a
//! Worker that routes each channel's requests to a Durable Object of its
//! own, and the object, which runs the same `teletext-runtime` as the
//! native host.
//!
//! - **Viewers** connect to `/<channel>/push`; the object accepts their
//!   connections with the hibernation interface.
//! - **The channel's program** connects to `/<channel>/program` with the
//!   channel's secret (requirement 1.5) and speaks the protocol of
//!   `docs/applications.md` (DEC-1482), one line a message; the object is
//!   the server of that WebSocket too, and opens no connection itself.
//! - **Nothing of the channel's content is kept** (REQ-1474): the object's
//!   SQLite storage holds only its folder and its session key.
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

use std::cell::RefCell;
use std::collections::BTreeMap;

use teletext_runtime::frame;
use teletext_runtime::json::{self, Value, Writer};
use teletext_runtime::protocol::valid_query;
use teletext_runtime::session_cookie::{self, KEY_BYTES, Key, first_session};
use teletext_runtime::{Channel, ChannelConfig, ConnectionId, Effect, Level, SESSION_GRACE_MS, SessionId};
use worker::{
    Date, Env, Error, Headers, Request, Response, Result, SqlStorage, SqlStorageValue, State, WebSocket, WebSocketIncomingMessage, WebSocketPair,
    durable_object, event,
};

/// The Durable Object binding, in `wrangler.toml`.
const CHANNEL_BINDING: &str = "CHANNEL";
/// The channels served, a JSON array of `{"folder","name"}`, in `wrangler.toml`.
const CHANNELS_VARIABLE: &str = "CHANNELS";
/// The address viewers reach the Worker at, such as `https://cf.neoteletext.com`.
const ORIGIN_VARIABLE: &str = "ORIGIN";
/// Each channel's program's secret is the Worker secret of this name and
/// the channel's folder, upper case, `-` as `_`: `PROGRAM_SECRET_HACKADAY`.
const SECRET_PREFIX: &str = "PROGRAM_SECRET_";
/// The push WebSocket's path within a channel's folder (the engine's
/// channel layout).
const PUSH: &str = "push";
/// The channel's one file (DEC-1467 item 1).
const DESCRIPTOR: &str = "channel.json";
/// The program's WebSocket's path within a channel's folder.
const PROGRAM: &str = "program";
/// The tags the object gives the connections it accepts.
const VIEWER_TAG: &str = "viewer";
const PROGRAM_TAG: &str = "program";
/// The page a viewer starts on.
const INITIAL_PAGE: u16 = 0x100;
/// A Close's status: the other side broke the protocol (RFC 6455 section 7.4.1).
const CLOSE_PROTOCOL_ERROR: u16 = 1002;
/// The object's tables, a migration a version (the numbered migrations of
/// the owner's SQLite rule): the object's own names (its folder, its
/// session key, the schema's version); migration 2 drops the pages kept
/// before DEC-1482.
const MIGRATIONS: [&str; 2] = [
    "CREATE TABLE kept (key TEXT PRIMARY KEY, record BLOB NOT NULL) STRICT; \
     CREATE TABLE meta (name TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;",
    "DROP TABLE kept;",
];
/// The meta row naming the schema's version.
const SCHEMA: &str = "schema";
/// The meta row naming the channel's folder, so an object woken by a
/// WebSocket's message knows which channel it is.
const FOLDER: &str = "folder";
/// The meta row holding the session cookie's key.
const SESSION_KEY: &str = "session_key";

/// A runtime failure, as the Worker's error.
fn failure(text: impl Into<String>) -> Error {
    Error::RustError(text.into())
}

/// Milliseconds since the Unix epoch.
fn epoch_ms() -> u64 {
    Date::now().as_millis()
}

/// Sends binary bytes as one message, or as several when they are more
/// than a receiver takes in one (the runtime's `frame::messages`).
fn send_binary(socket: &WebSocket, bytes: &[u8]) {
    for part in frame::messages(bytes, frame::MOST_MESSAGE_BYTES) {
        let _ = socket.send_with_bytes(part);
    }
}

/// The channels the Worker serves: folder and name.
fn channels(env: &Env) -> Result<Vec<(String, String)>> {
    let text = env.var(CHANNELS_VARIABLE)?.to_string();
    let value = json::parse(text.as_bytes()).map_err(|fault| failure(format!("{CHANNELS_VARIABLE} is not JSON: {fault:?}")))?;
    let items = value.items().ok_or_else(|| failure(format!("{CHANNELS_VARIABLE} must be an array")))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let folder = item.get("folder").and_then(Value::text)?;
            let name = item.get("name").and_then(Value::text).unwrap_or(folder);
            Some((folder.to_owned(), name.to_owned()))
        })
        .collect())
}

/// The folder and the file a path names: `/hackaday/100.t42`.
fn folder_and_file(path: &str) -> (&str, &str) {
    let path = path.trim_start_matches('/');
    path.split_once('/').unwrap_or((path, ""))
}

/// Whether two secrets are the same, in time that does not depend on
/// where they differ.
fn same_secret(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0_u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

/// The Worker: routes each channel's requests to its object, and refuses a
/// program without the channel's secret (requirement 1.5).
#[event(fetch)]
async fn fetch(request: Request, env: Env, _context: worker::Context) -> Result<Response> {
    let url = request.url()?;
    let (folder, file) = folder_and_file(url.path());
    let channels = channels(&env)?;
    // Only a channel's `channel.json`, its push WebSocket and its program's
    // WebSocket (DEC-1467): no listing at `/`, no channel's files.
    if !channels.iter().any(|(served, _)| served == folder) || ![DESCRIPTOR, PUSH, PROGRAM].contains(&file) {
        return Response::error("Not Found", 404);
    }
    if file == PROGRAM {
        let name = format!("{SECRET_PREFIX}{}", folder.to_uppercase().replace('-', "_"));
        let expected = env.secret(&name).map(|secret| secret.to_string()).unwrap_or_default();
        let presented = request.headers().get("authorization")?.unwrap_or_default();
        let presented = presented.strip_prefix("Bearer ").unwrap_or_default();
        if expected.is_empty() || !same_secret(presented.as_bytes(), expected.as_bytes()) {
            return Response::error("Unauthorized", 401);
        }
    }
    env.durable_object(CHANNEL_BINDING)?.id_from_name(folder)?.get_stub()?.fetch_with_request(request).await
}

/// A channel's running state, made when the object wakes.
struct Live {
    folder: String,
    channel: Channel,
    /// When the host's clock started, in milliseconds since the Unix epoch.
    started_ms: u64,
    /// The viewers' connections, one a tab (DEC-1527).
    viewers: BTreeMap<ConnectionId, WebSocket>,
    /// The next connection's number: never reused while the object runs.
    next_connection: ConnectionId,
    program: Option<WebSocket>,
    next_session: SessionId,
    key: Key,
    /// Whether viewers reach the Worker over `https`.
    secure: bool,
    /// The alarm last set, in milliseconds since the Unix epoch.
    alarm: Option<u64>,
}

impl Live {
    /// The host's clock.
    fn now(&self) -> u64 {
        epoch_ms().saturating_sub(self.started_ms)
    }

    /// Carries out the channel's effects.
    fn apply(&mut self) {
        for effect in self.channel.take_effects() {
            match effect {
                Effect::Send { connection, bytes } => {
                    if let Some(socket) = self.viewers.get(&connection) {
                        send_binary(socket, &bytes);
                    }
                }
                Effect::Close { connection } => {
                    if let Some(socket) = self.viewers.get(&connection) {
                        let _ = socket.close(Some(CLOSE_PROTOCOL_ERROR), Some("protocol error"));
                    }
                }
                Effect::ToProgram { line } => {
                    if let Some(program) = &self.program {
                        let text = String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(&line)).into_owned();
                        let _ = program.send_with_str(text);
                    }
                }
                Effect::Log { level, event, text } => log(level, &self.folder, event, &text),
            }
        }
    }

    /// When the object next needs its alarm, in milliseconds since the
    /// Unix epoch: the runtime's next deadline.
    fn next_alarm(&self) -> Option<u64> {
        self.channel.next_wake().map(|at| self.started_ms.saturating_add(at))
    }

    /// A session number never given before.
    fn new_session(&mut self) -> SessionId {
        let session = self.next_session;
        self.next_session = self.next_session.saturating_add(1);
        session
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

    /// Makes the channel's state if the object has just woken: what was
    /// kept, the connections it still holds, its key. `folder` is the
    /// channel's, when a request names it.
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
        let key = if let Some(key) = Self::meta(&sql, SESSION_KEY)?.and_then(|bytes| Key::try_from(bytes.as_slice()).ok()) {
            key
        } else {
            let mut key = [0_u8; KEY_BYTES];
            getrandom::fill(&mut key).map_err(|error| failure(format!("no random bytes: {error}")))?;
            Self::set_meta(&sql, SESSION_KEY, key.to_vec())?;
            key
        };
        let name = channels(&self.env)?.into_iter().find(|(served, _)| *served == folder).map_or_else(|| folder.clone(), |(_, name)| name);
        let origin = self.env.var(ORIGIN_VARIABLE)?.to_string();
        let push = origin.strip_prefix("https://").map_or_else(|| origin.replacen("http://", "ws://", 1), |rest| format!("wss://{rest}"));
        let started_ms = epoch_ms();
        let config = ChannelConfig {
            name,
            push_address: format!("{push}/{folder}/{PUSH}"),
            initial_page: INITIAL_PAGE,
            epoch_at_start_ms: started_ms,
            session_grace_ms: SESSION_GRACE_MS,
        };
        let channel = Channel::new(config);
        let mut live = Live {
            folder,
            channel,
            started_ms,
            viewers: BTreeMap::new(),
            next_connection: 1,
            program: self.state.get_websockets_with_tag(PROGRAM_TAG).into_iter().next(),
            next_session: first_session(started_ms),
            key,
            secure: origin.starts_with("https://"),
            alarm: None,
        };
        // Viewers still connected after an eviction each start afresh, and a
        // program still connected is started again, told of each.
        for socket in self.state.get_websockets_with_tag(VIEWER_TAG) {
            let Ok(Some((connection, session))) = socket.deserialize_attachment::<(ConnectionId, SessionId)>() else { continue };
            live.next_connection = live.next_connection.max(connection.saturating_add(1));
            // A socket's attachment holds 2 KiB, less than a query may be,
            // so a viewer starting afresh here has none (REQ-1674).
            Self::welcome(&mut live, connection, session, None, socket);
        }
        if live.program.is_some() {
            let now = live.now();
            live.channel.program_started(now);
        }
        live.apply();
        *self.live.borrow_mut() = Some(live);
        Ok(())
    }

    /// A viewer's connection for `session` opened: its program is told.
    fn welcome(live: &mut Live, connection: ConnectionId, session: SessionId, query: Option<String>, socket: WebSocket) {
        live.viewers.insert(connection, socket);
        let now = live.now();
        live.channel.opened(connection, session, query, now);
    }

    /// Sets the object's alarm to when it is next needed, if that changed.
    async fn schedule(&self) -> Result<()> {
        let (wanted, set) = {
            let live = self.live.borrow();
            let Some(live) = live.as_ref() else { return Ok(()) };
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

    /// Runs `work` on the live channel, then carries out its effects.
    fn with_live<T>(&self, work: impl FnOnce(&mut Live) -> T) -> Result<T> {
        let mut live = self.live.borrow_mut();
        let live = live.as_mut().ok_or_else(|| failure("the channel is not awake"))?;
        let answer = work(live);
        live.apply();
        Ok(answer)
    }

    /// A viewer's push WebSocket (requirement 5.5 for its session).
    fn accept_viewer(&self, request: &Request) -> Result<Response> {
        // REQ-1674: the query is the program's, and it is not read here
        // beyond its rules, or logged.
        let query = request.url()?.query().map(str::to_owned);
        if query.as_deref().is_some_and(|query| !valid_query(query)) {
            return Response::error("Bad Request", 400);
        }
        let pair = WebSocketPair::new()?;
        let cookie = request.headers().get("cookie")?;
        // Several tabs may share a session (DEC-1527).
        let (connection, session, set_cookie) = self.with_live(|live| {
            let now_s = epoch_ms() / 1_000;
            let named = cookie.as_deref().and_then(|header| session_cookie::session_of(header.as_bytes(), &live.folder, now_s, &[&live.key]));
            let session = named.unwrap_or_else(|| live.new_session());
            let connection = live.next_connection;
            live.next_connection = live.next_connection.saturating_add(1);
            (connection, session, session_cookie::set_cookie(&live.key, &live.folder, session, now_s, live.secure))
        })?;
        self.state.accept_websocket_with_tags(&pair.server, &[VIEWER_TAG]);
        pair.server.serialize_attachment((connection, session))?;
        self.with_live(|live| Self::welcome(live, connection, session, query, pair.server))?;
        let headers = Headers::new();
        headers.set("set-cookie", &set_cookie)?;
        Ok(Response::from_websocket(pair.client)?.with_headers(headers))
    }

    /// The program's WebSocket: one at a time.
    fn accept_program(&self) -> Result<Response> {
        if self.live.borrow().as_ref().is_some_and(|live| live.program.is_some()) {
            return Response::error("a program is connected already: one at a time", 409);
        }
        let pair = WebSocketPair::new()?;
        self.state.accept_websocket_with_tags(&pair.server, &[PROGRAM_TAG]);
        self.with_live(|live| {
            live.program = Some(pair.server);
            let now = live.now();
            live.channel.program_started(now);
            log(Level::Info, &live.folder, "program_connected", "the program connected");
        })?;
        Response::from_websocket(pair.client)
    }

    /// The channel's `channel.json`, generated (DEC-1467 item 1).
    fn descriptor(&self) -> Result<Response> {
        let body = self.live.borrow().as_ref().map(|live| live.channel.descriptor()).unwrap_or_default();
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-cache")?;
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
        let (folder, file) = folder_and_file(url.path());
        self.wake(Some(folder))?;
        let response = match file {
            PUSH => self.accept_viewer(&request),
            PROGRAM => self.accept_program(),
            DESCRIPTOR => self.descriptor(),
            _ => Response::error("Not Found", 404),
        };
        self.schedule().await?;
        response
    }

    async fn alarm(&self) -> Result<Response> {
        self.wake(None)?;
        self.with_live(|live| {
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
        self.with_live(|live| {
            let now = live.now();
            match message {
                WebSocketIncomingMessage::String(text) if program => {
                    for line in text.split('\n').map(str::trim_end).filter(|line| !line.is_empty()) {
                        live.channel.from_program(line.as_bytes(), now);
                    }
                }
                WebSocketIncomingMessage::Binary(bytes) if !program => {
                    if let Ok(Some((connection, _))) = socket.deserialize_attachment::<(ConnectionId, SessionId)>() {
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
    /// A WebSocket closed: a viewer's session waits for its viewer; the
    /// program's lines are dropped until it connects again, and its
    /// channel is unavailable meanwhile.
    async fn gone(&self, socket: &WebSocket) -> Result<()> {
        self.wake(None)?;
        let program = self.is_program(socket);
        let connection = socket.deserialize_attachment::<(ConnectionId, SessionId)>().ok().flatten().map(|(connection, _)| connection);
        self.with_live(|live| {
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
