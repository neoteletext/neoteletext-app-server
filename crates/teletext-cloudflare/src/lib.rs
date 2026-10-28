//! The server on Cloudflare (decision 2, requirement 1.2; decision 20): a
//! Worker that routes each channel's requests to a Durable Object of its
//! own, and the object, which runs the same `teletext-runtime` as the
//! native host.
//!
//! - **Viewers** connect to `/<channel>/push`; the object accepts their
//!   connections with the hibernation interface.
//! - **The channel's program** connects to `/<channel>/program` with the
//!   channel's secret (requirement 1.5) and speaks `docs/applications.md`,
//!   one line a message; the object is the server of that WebSocket too,
//!   and opens no connection itself.
//! - **What survives** eviction and deployment is kept in the object's
//!   SQLite storage through the runtime's `Store` and `Forget` effects
//!   (requirement 5.2).
//! - **Time** is the object's single alarm: the runtime's next deadline,
//!   and while anyone watches, the time-filling header each second, which
//!   keeps a watched channel's object awake (decision 20).
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
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use teletext_runtime::json::{self, Value, Writer};
use teletext_runtime::remote::declared_pages;
use teletext_runtime::session_cookie::{self, KEY_BYTES, Key, first_session};
use teletext_runtime::{Channel, ChannelConfig, Effect, Level, Remote, SESSION_GRACE_MS, SessionId, StoreKey};
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
/// The program's WebSocket's path within a channel's folder.
const PROGRAM: &str = "program";
/// The tags the object gives the connections it accepts.
const VIEWER_TAG: &str = "viewer";
const PROGRAM_TAG: &str = "program";
/// The page a viewer starts on.
const INITIAL_PAGE: u16 = 0x100;
/// How often viewers are sent a time-filling header, in milliseconds (EN
/// 300 706 clause 9.3.1; the native host's `CLOCK_EVERY_S`).
const CLOCK_EVERY_MS: u64 = 1_000;
/// A Close's status: the other side broke the protocol (RFC 6455 section 7.4.1).
const CLOSE_PROTOCOL_ERROR: u16 = 1002;
/// The object's tables, a migration a version (the numbered migrations of
/// the owner's SQLite rule): what the channel keeps, and the object's own
/// names (its folder, its session key, the schema's version).
const MIGRATIONS: [&str; 1] = ["CREATE TABLE kept (key TEXT PRIMARY KEY, record BLOB NOT NULL) STRICT; \
     CREATE TABLE meta (name TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;"];
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
    if folder.is_empty() {
        let mut text = String::from("NeoTeletext server\n\n");
        for (folder, _) in &channels {
            let _ = writeln!(text, "/{folder}/");
        }
        return Response::ok(text);
    }
    if !channels.iter().any(|(served, _)| served == folder) {
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

/// A viewer's connection, as the object holds it.
struct Viewer {
    socket: WebSocket,
    /// The pages its session owns: the shared page of each is not sent to it.
    own: BTreeSet<u16>,
}

/// A channel's running state, made when the object wakes.
struct Live {
    folder: String,
    channel: Channel,
    /// When the host's clock started, in milliseconds since the Unix epoch.
    started_ms: u64,
    viewers: BTreeMap<SessionId, Viewer>,
    program: Option<WebSocket>,
    /// The program has declared its pages on its connection.
    declared: bool,
    next_session: SessionId,
    key: Key,
    /// Whether viewers reach the Worker over `https`.
    secure: bool,
    /// When the time-filling header is next due.
    clock_due: u64,
    /// The alarm last set, in milliseconds since the Unix epoch.
    alarm: Option<u64>,
}

impl Live {
    /// The host's clock.
    fn now(&self) -> u64 {
        epoch_ms().saturating_sub(self.started_ms)
    }

    /// Carries out the channel's effects.
    fn apply(&mut self, sql: &SqlStorage) {
        for effect in self.channel.take_effects() {
            match effect {
                Effect::Shared { page, frame } => {
                    for viewer in self.viewers.values().filter(|viewer| !viewer.own.contains(&page)) {
                        let _ = viewer.socket.send_with_bytes(&frame);
                    }
                }
                Effect::Own { session, page } => {
                    if let Some(viewer) = self.viewers.get_mut(&session) {
                        viewer.own.insert(page);
                    }
                }
                Effect::Disown { session, page } => {
                    if let Some(viewer) = self.viewers.get_mut(&session) {
                        viewer.own.remove(&page);
                    }
                }
                Effect::Send { session, bytes } => {
                    if let Some(viewer) = self.viewers.get(&session) {
                        let _ = viewer.socket.send_with_bytes(&bytes);
                    }
                }
                Effect::Close { session } => {
                    if let Some(viewer) = self.viewers.get(&session) {
                        let _ = viewer.socket.close(Some(CLOSE_PROTOCOL_ERROR), Some("protocol error"));
                    }
                }
                Effect::ToApplication { line, .. } => {
                    if self.declared
                        && let Some(program) = &self.program
                    {
                        let text = String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(&line)).into_owned();
                        let _ = program.send_with_str(text);
                    }
                }
                Effect::Store { key, bytes } => {
                    let stored = sql.exec("INSERT OR REPLACE INTO kept (key, record) VALUES (?, ?)", vec![key.name().into(), bytes.into()]);
                    if let Err(error) = stored {
                        log(Level::Error, &self.folder, "store_failed", &format!("{}: {error}", key.name()));
                    }
                }
                Effect::Forget { key } => {
                    let _ = sql.exec("DELETE FROM kept WHERE key = ?", vec![key.name().into()]);
                }
                Effect::Log { level, event, text } => log(level, &self.folder, event, &text),
                Effect::Withdrawn { .. } | Effect::Contents => {}
            }
        }
    }

    /// When the object next needs its alarm, in milliseconds since the
    /// Unix epoch: the runtime's next deadline, and the clock while anyone
    /// watches.
    fn next_alarm(&self) -> Option<u64> {
        let deadline = self.channel.next_wake().map(|at| self.started_ms.saturating_add(at));
        let clock = (!self.viewers.is_empty()).then(|| self.started_ms.saturating_add(self.clock_due));
        deadline.into_iter().chain(clock).min()
    }

    /// A session number never given before, and not connected now.
    fn new_session(&mut self) -> SessionId {
        loop {
            let session = self.next_session;
            self.next_session = self.next_session.saturating_add(1);
            if !self.viewers.contains_key(&session) {
                return session;
            }
        }
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
        let mut channel = Channel::new(config, vec![Box::new(Remote::new(Vec::new()))], started_ms);
        for row in sql.exec("SELECT key, record FROM kept", None)?.raw() {
            let mut values = row?.into_iter();
            let (Some(SqlStorageValue::String(name)), Some(SqlStorageValue::Blob(record))) = (values.next(), values.next()) else { continue };
            let loaded = StoreKey::parse(&name).ok_or(teletext_runtime::StoreError::Invalid).and_then(|key| channel.load(key, &record));
            if let Err(error) = loaded {
                log(Level::Warn, &folder, "record_ignored", &format!("{name}: {error:?}"));
            }
        }
        let mut live = Live {
            folder,
            channel,
            started_ms,
            viewers: BTreeMap::new(),
            program: self.state.get_websockets_with_tag(PROGRAM_TAG).into_iter().next(),
            declared: false,
            next_session: first_session(started_ms),
            key,
            secure: origin.starts_with("https://"),
            clock_due: CLOCK_EVERY_MS,
            alarm: None,
        };
        // A program still connected declared its pages before; they were kept.
        live.declared = live.program.is_some() && live.channel.is_ready();
        // Viewers still connected after an eviction: each starts afresh,
        // sent every shared page again.
        for socket in self.state.get_websockets_with_tag(VIEWER_TAG) {
            let Ok(Some(session)) = socket.deserialize_attachment::<SessionId>() else { continue };
            Self::welcome(&mut live, session, socket);
        }
        live.apply(&sql);
        *self.live.borrow_mut() = Some(live);
        Ok(())
    }

    /// A viewer's connection for `session` opened: every shared page goes
    /// to it, then whatever its session owns.
    fn welcome(live: &mut Live, session: SessionId, socket: WebSocket) {
        for (_, frame) in live.channel.shared_frames() {
            let _ = socket.send_with_bytes(frame.as_ref());
        }
        live.viewers.insert(session, Viewer { socket, own: BTreeSet::new() });
        let now = live.now();
        live.channel.opened(session, now);
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
        let sql = self.state.storage().sql();
        let mut live = self.live.borrow_mut();
        let live = live.as_mut().ok_or_else(|| failure("the channel is not awake"))?;
        let answer = work(live);
        live.apply(&sql);
        Ok(answer)
    }

    /// A viewer's push WebSocket (requirement 5.5 for its session).
    fn accept_viewer(&self, request: &Request) -> Result<Response> {
        let pair = WebSocketPair::new()?;
        let cookie = request.headers().get("cookie")?;
        let (session, set_cookie) = self.with_live(|live| {
            let now_s = epoch_ms() / 1_000;
            let named = cookie.as_deref().and_then(|header| session_cookie::session_of(header.as_bytes(), &live.folder, now_s, &[&live.key]));
            let (session, give_cookie) = match named {
                Some(session) if !live.viewers.contains_key(&session) => (session, true),
                Some(_) => (live.new_session(), false),
                None => (live.new_session(), true),
            };
            let set_cookie = give_cookie.then(|| session_cookie::set_cookie(&live.key, &live.folder, session, now_s, live.secure));
            (session, set_cookie)
        })?;
        self.state.accept_websocket_with_tags(&pair.server, &[VIEWER_TAG]);
        pair.server.serialize_attachment(session)?;
        self.with_live(|live| Self::welcome(live, session, pair.server))?;
        let headers = Headers::new();
        if let Some(cookie) = set_cookie {
            headers.set("set-cookie", &cookie)?;
        }
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
            live.declared = false;
        })?;
        Response::from_websocket(pair.client)
    }

    /// A file of the channel's layout.
    fn resource(&self, file: &str) -> Result<Response> {
        let resource = self.live.borrow().as_ref().and_then(|live| live.channel.resource(file));
        let Some(resource) = resource else { return Response::error("Not Found", 404) };
        let headers = Headers::new();
        headers.set("content-type", resource.content_type)?;
        headers.set("cache-control", "no-cache")?;
        Ok(Response::from_bytes(resource.body)?.with_headers(headers))
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
            other => self.resource(other),
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
            if !live.viewers.is_empty() && now >= live.clock_due {
                let header = live.channel.time_filling(now);
                for viewer in live.viewers.values() {
                    let _ = viewer.socket.send_with_bytes(header.as_ref());
                }
                live.clock_due = now.saturating_add(CLOCK_EVERY_MS);
            }
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
                        if live.declared {
                            live.channel.from_application(0, line.as_bytes(), now);
                            continue;
                        }
                        match declared_pages(line.as_bytes()) {
                            Ok(pages) => {
                                live.channel.declared(0, pages);
                                live.declared = true;
                                live.channel.application_started(0, now);
                                log(Level::Info, &live.folder, "program_declared", "the program declared its pages");
                            }
                            Err(refusal) => {
                                log(Level::Warn, &live.folder, "program_refused", &refusal);
                                let _ = socket.close(Some(CLOSE_PROTOCOL_ERROR), Some("the first line must declare the pages"));
                            }
                        }
                    }
                }
                WebSocketIncomingMessage::Binary(bytes) if !program => {
                    if let Ok(Some(session)) = socket.deserialize_attachment::<SessionId>() {
                        live.channel.received(session, &bytes, now);
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
    /// program's lines are dropped until it connects again.
    async fn gone(&self, socket: &WebSocket) -> Result<()> {
        self.wake(None)?;
        let program = self.is_program(socket);
        let session = socket.deserialize_attachment::<SessionId>().ok().flatten();
        self.with_live(|live| {
            if program {
                live.program = None;
                live.declared = false;
                log(Level::Info, &live.folder, "program_gone", "the program went");
            } else if let Some(session) = session
                && live.viewers.remove(&session).is_some()
            {
                let now = live.now();
                live.channel.closed(session, now);
            }
        })?;
        self.schedule().await
    }
}
