//! One channel, as a state machine: the host gives it what happened, with
//! the time, and carries out the [`Effect`]s it answers with.
//!
//! **The server relays; the program owns every page** (DEC-1482). The
//! channel keeps no content (REQ-1474): only what each open connection
//! is, its session, what its views select and the cursor of what it was
//! sent. Each connection that opens is told to the program as `connected`
//! (REQ-1471), each selection as `selected` (REQ-1472), and what the
//! program sends goes to the connections it names, as it comes
//! (REQ-1473), as teletext packets (REQ-1480).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_audio::encode::packets;
use neoteletext_formats::page::CursorLine;
use neoteletext_input::caps::{CURSOR_CAP, PAGE_CAP};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::cursor::{BLOCK, Cursor, NO_CURSOR, Place};
use neoteletext_input::event::{Event, Item, MAX_STREAM_ITEM, StreamDecoder, units};

use crate::input::Input;
use crate::json::Writer;
use crate::protocol::{
    self, ANY_SUBPAGE, Budgets, Closed, Connected, ConnectionId, Connections, InputMessage, Left, OperatorLevel, PageNumber, ProgramMessage, Refusal, Rule,
    Selected, Selection, ServerMessage, SessionId, Standard, Start, ViewSelection,
};
use crate::sound::{Sounds, audience};
use crate::stats::{ChannelStats, count};

/// The service number audio and input records are sent on (the
/// reference channel's, INPUT.md section 7).
const SERVICE: u8 = 0;

/// The capabilities the channel uses of what a receiver offers in HELLO
/// (INPUT.md section 6.2). There are no modes to agree: every event is
/// sent and the channel ignores what it does not use (section 6.3).
const USED: u32 = PAGE_CAP | CURSOR_CAP;

/// The view every event is of until the receiver's SELECT names views
/// (`INPUT.md` REQ-INT-140): a connection's first.
const FIRST_VIEW: u8 = 0;

/// How long the server waits for a program's answer to `connected` or
/// `selected` before the viewer is told the channel is unavailable
/// (REQ-1481). Provisional, not yet measured: an rdm to-do measures the
/// three channels' programs on the live server and sets it from that.
pub const PROGRAM_REPLY_LIMIT_MS: u64 = 5_000;

/// A channel's description, for `channel.json`.
#[derive(Clone, Debug)]
pub struct ChannelConfig {
    /// Its name.
    pub name: String,
    /// The push WebSocket's address, on the channel's own origin (the
    /// engine's descriptor rule).
    pub push_address: String,
    /// The page a viewer starts on.
    pub initial_page: u16,
    /// The time the host's clock started, in milliseconds since the Unix
    /// epoch, so programs can show the real time (the host's clock itself
    /// starts at 0).
    pub epoch_at_start_ms: u64,
    /// How long a session whose connection closed waits for its viewer to
    /// come back before its program is told it ended, in milliseconds; 0
    /// ends it at once (requirement 5.5).
    pub session_grace_ms: u64,
}

/// How long a session waits for its viewer to come back: 5 minutes, long
/// enough for a phone that closed a background socket or a server that
/// restarted (`SERVER.md` section 8.1: "a grace period of minutes").
pub const SESSION_GRACE_MS: u64 = 5 * 60 * 1_000;

/// How much a report matters to the server's operator, as a log ranks it
/// (requirement 7.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// Something failed that the operator must act on.
    Error,
    /// Something was refused or lost, and the server went on.
    Warn,
    /// What the server did, worth reading.
    Info,
    /// Detail for finding a fault.
    Debug,
}

/// What the host does for the channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Bytes for one session's connection, as one binary WebSocket
    /// message, or several where the host must cut it
    /// ([`crate::frame::messages`]).
    Send {
        /// The session.
        session: SessionId,
        /// The message.
        bytes: Arc<[u8]>,
    },
    /// Close the session's connection: its input broke the protocol.
    Close {
        /// The session.
        session: SessionId,
    },
    /// A line for the channel's program, with its newline.
    ToProgram {
        /// The line.
        line: Vec<u8>,
    },
    /// Something for the server's operator to read (requirement 7.6).
    Log {
        /// How much it matters.
        level: Level,
        /// The kind of event, a name that does not change.
        event: &'static str,
        /// What happened.
        text: String,
    },
}

/// One open connection: all the channel holds of a viewer (REQ-1474).
struct Connection {
    /// Its session.
    session: SessionId,
    /// The input stream's decoder, made at the first input: most viewers
    /// of a channel never type, and the decoder holds a paste's worth of
    /// bytes (decision 7 item 4).
    decoder: Option<Box<StreamDecoder<MAX_STREAM_ITEM>>>,
    /// WELCOME was sent: the receiver acts on cursor records.
    welcomed: bool,
    /// The input count acted on, modulo 65 536 (INPUT.md section 7.1).
    acked: u16,
    /// The page with the keys, as PAGE last said; 0 for none.
    keys: u16,
    /// A page the program sent the viewer to, until PAGE says it is open:
    /// acknowledgements name it meanwhile, since the receiver goes to the
    /// page of the last cursor record it reads (INPUT.md section 7.1).
    went: Option<u16>,
    /// The next record number on the audio and input service.
    record_id: u8,
    /// What each view selects.
    selections: BTreeMap<u8, Selection>,
    /// The cursor of each page it was sent, for its cursor records.
    cursors: BTreeMap<u16, Option<CursorLine>>,
    /// The program running now was told of it with `connected`.
    announced: bool,
    /// The program answered that `connected`.
    replied: bool,
    /// When the program's `reply` is due, while it is awaited.
    reply_due: Option<u64>,
    /// Each view's page awaited from the program for its selection, and
    /// when it is due.
    selection_due: BTreeMap<u8, (u16, u64)>,
    /// For each view, the last version sent it of the sub-page it shows
    /// (DEC-1499): REQ-1474's one exception, at most one sub-page a view.
    shown: BTreeMap<u8, Shown>,
}

/// The sub-page a view shows, as it was last sent.
struct Shown {
    /// The page.
    page: u16,
    /// The sub-page's sub-code.
    sub: u16,
    /// Its packets as last sent whole.
    packets: Vec<[u8; neoteletext_core::consts::PACKET_BYTES]>,
}

impl Connection {
    const fn new(session: SessionId) -> Self {
        Self {
            session,
            decoder: None,
            welcomed: false,
            acked: 0,
            keys: 0,
            went: None,
            record_id: 0,
            selections: BTreeMap::new(),
            cursors: BTreeMap::new(),
            announced: false,
            replied: false,
            reply_due: None,
            selection_due: BTreeMap::new(),
            shown: BTreeMap::new(),
        }
    }

    /// The next record number, advanced.
    const fn next_record(&mut self) -> u8 {
        let id = self.record_id;
        self.record_id = id.wrapping_add(1);
        id
    }

    /// The cursor record for `page`, numbered next.
    fn cursor_packet(&mut self, page: u16) -> Option<Arc<[u8]>> {
        let cursor = cursor_of(page, self.acked, self.cursors.get(&page).copied().flatten().as_ref());
        let record = self.next_record();
        cursor.packet(SERVICE, record).map(|packet| Arc::from(&packet.bytes[..]))
    }
}

/// One channel's state.
pub struct Channel {
    config: ChannelConfig,
    /// Whether the program is running, `start` sent to it.
    program: bool,
    connections: BTreeMap<ConnectionId, Connection>,
    /// The connection each session has open.
    by_session: BTreeMap<SessionId, ConnectionId>,
    /// Every session there is, connected or waiting for its viewer since
    /// the time given (requirement 5.5).
    sessions: BTreeMap<SessionId, Option<u64>>,
    /// The next connection's number: from 1, never reused while the
    /// channel runs.
    next_connection: ConnectionId,
    sounds: Sounds,
    effects: Vec<Effect>,
    stats: ChannelStats,
}

impl Channel {
    /// A channel; its program is not running until
    /// [`Channel::program_started`].
    #[must_use]
    pub fn new(config: ChannelConfig) -> Self {
        Self {
            config,
            program: false,
            connections: BTreeMap::new(),
            by_session: BTreeMap::new(),
            sessions: BTreeMap::new(),
            next_connection: 1,
            sounds: Sounds::default(),
            effects: Vec::new(),
            stats: ChannelStats::default(),
        }
    }

    /// The channel's description.
    #[must_use]
    pub const fn config(&self) -> &ChannelConfig {
        &self.config
    }

    /// The channel's `channel.json`, generated (DEC-1467 item 1): its name,
    /// its initial page and its push WebSocket, which takes input. A
    /// receiver reads it first to know the channel is live (the engine's
    /// REQ-WEB-018), and then asks for nothing else over HTTP.
    #[must_use]
    pub fn descriptor(&self) -> Vec<u8> {
        let mut push = Writer::nested();
        push.text("address", &self.config.push_address).text("method", "websocket").boolean("input", true);
        let mut descriptor = Writer::nested();
        descriptor.text("name", &self.config.name).text("initial_page", &format!("{:03X}", self.config.initial_page)).object("push", push);
        let mut bytes = descriptor.line();
        bytes.pop();
        bytes
    }

    /// Whether the channel's program is running, so it serves its viewers
    /// (requirement 8.4).
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.program
    }

    /// What the channel has counted since it started.
    #[must_use]
    pub const fn stats(&self) -> &ChannelStats {
        &self.stats
    }

    /// The number of sessions, connected or waiting for their viewer.
    #[must_use]
    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }

    /// What the host must do now.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        core::mem::take(&mut self.effects)
    }

    fn log(&mut self, level: Level, event: &'static str, text: String) {
        self.effects.push(Effect::Log { level, event, text });
    }

    fn send(&mut self, session: SessionId, bytes: Arc<[u8]>) {
        self.effects.push(Effect::Send { session, bytes });
    }

    /// A line for the program, while it runs; lines for a program that is
    /// not running are dropped, since it is told of every connection when
    /// it starts.
    fn tell(&mut self, message: &ServerMessage) {
        if self.program {
            count(&mut self.stats.lines_to_application);
            self.effects.push(Effect::ToProgram { line: protocol::line(message) });
        }
    }

    /// The program started, the first time or again after it ended: it is
    /// sent `start`, then `connected` for each connection open (REQ-1481,
    /// REQ-1038 as revised).
    pub fn program_started(&mut self, now: u64) {
        self.program = true;
        let start = Start {
            protocol: protocol::PROTOCOL,
            epoch_ms: self.config.epoch_at_start_ms.saturating_add(now),
            standard: standard(),
            budgets: Budgets {
                line_bytes: wide(protocol::LONGEST_LINE),
                title_characters: wide(protocol::TITLE_CHARACTERS),
                author_log_bytes: wide(neoteletext_formats::log::MAX_LOG_TEXT_BYTES),
                log_characters: wide(protocol::OPERATOR_LOG_CHARACTERS),
                named_connections: wide(protocol::MOST_NAMED_CONNECTIONS),
                reply_limit_ms: PROGRAM_REPLY_LIMIT_MS,
            },
        };
        self.tell(&ServerMessage::Start(start));
        let open: Vec<ConnectionId> = self.connections.keys().copied().collect();
        for connection in open {
            self.announce(connection, now);
        }
    }

    /// The program ended or went: nothing is awaited from it, and each
    /// connection is told to the next as it starts.
    pub fn program_ended(&mut self) {
        self.program = false;
        for state in self.connections.values_mut() {
            state.announced = false;
            state.replied = false;
            state.reply_due = None;
            state.selection_due.clear();
        }
    }

    /// Tells the program of `connection` with `connected`, awaiting its
    /// reply, if it runs (REQ-1471).
    fn announce(&mut self, connection: ConnectionId, now: u64) {
        if !self.program {
            return;
        }
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.announced = true;
        state.replied = false;
        state.reply_due = Some(now.saturating_add(PROGRAM_REPLY_LIMIT_MS));
        // Every view's selection; until SELECT names views, the first view
        // with none, which asks for the initial page.
        let mut selections: Vec<ViewSelection> =
            state.selections.iter().map(|(&view, &selection)| ViewSelection { view, selection: Some(selection) }).collect();
        if selections.is_empty() {
            selections.push(ViewSelection { view: FIRST_VIEW, selection: None });
        }
        let message = Connected { connection, session: state.session, cookies: Vec::new(), selections };
        self.tell(&ServerMessage::Connected(message));
    }

    /// A connection opened for `session`; the host keys what it sends by
    /// the session. A session waiting for its viewer continues
    /// (requirement 5.5) on a new input session (INPUT.md section 4).
    pub fn opened(&mut self, session: SessionId, now: u64) {
        match self.sessions.insert(session, None) {
            Some(Some(_)) => self.stats.resumed(),
            Some(None) => {}
            None => self.stats.joined(),
        }
        let connection = self.next_connection;
        self.next_connection = self.next_connection.saturating_add(1);
        self.connections.insert(connection, Connection::new(session));
        self.by_session.insert(session, connection);
        self.announce(connection, now);
    }

    /// A connection closed: the program is told its views left their
    /// pages, and its session waits for its viewer for the channel's grace
    /// (requirement 5.5); the program is told it ended with `closed` only
    /// when the grace passes.
    pub fn closed(&mut self, session: SessionId, now: u64) {
        let Some(connection) = self.by_session.remove(&session) else { return };
        let Some(state) = self.connections.remove(&connection) else { return };
        if state.announced {
            for (&view, selection) in &state.selections {
                self.tell(&ServerMessage::Left(Left { connection, view, page: selection.page }));
            }
        }
        if self.config.session_grace_ms == 0 {
            self.sessions.remove(&session);
            self.stats.left();
            self.tell(&ServerMessage::Closed(Closed { session }));
            return;
        }
        self.sessions.insert(session, Some(now));
        self.stats.waiting();
    }

    /// A binary message from a session's connection: its input stream
    /// (INPUT.md).
    pub fn received(&mut self, session: SessionId, bytes: &[u8], now: u64) {
        let Some(&connection) = self.by_session.get(&session) else { return };
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let mut decoder = state.decoder.take().unwrap_or_else(|| Box::new(StreamDecoder::<MAX_STREAM_ITEM>::default()));
        let result = decoder.push(bytes, &mut |item| self.item(connection, &item, now));
        if result.is_err() {
            // A protocol error ends the session (INPUT.md section 8).
            self.effects.push(Effect::Close { session });
            return;
        }
        if let Some(state) = self.connections.get_mut(&connection) {
            state.decoder = Some(decoder);
        }
        self.acknowledge(connection);
    }

    /// The time asked for has come: sessions whose grace passed end, and
    /// answers the program did not give in time are given up (REQ-1481).
    pub fn deadline(&mut self, now: u64) {
        let grace = self.config.session_grace_ms;
        let expired: Vec<SessionId> =
            self.sessions.iter().filter(|(_, since)| since.is_some_and(|since| since.saturating_add(grace) <= now)).map(|(&session, _)| session).collect();
        for session in expired {
            self.sessions.remove(&session);
            self.stats.expired();
            self.tell(&ServerMessage::Closed(Closed { session }));
        }
        let mut late: Vec<String> = Vec::new();
        for (connection, state) in &mut self.connections {
            if state.reply_due.is_some_and(|due| due <= now) {
                state.reply_due = None;
                late.push(format!("connection {connection}: no reply to connected"));
            }
            state.selection_due.retain(|view, &mut (page, due)| {
                let waiting = due > now;
                if !waiting {
                    late.push(format!("connection {connection} view {view}: no page {page:03X} for selected"));
                }
                waiting
            });
        }
        // DENIED "unavailable" (`SERVER.md` section 5.5) goes to the viewer
        // here once the engine encodes it; until then the operator is told.
        for text in late {
            self.log(Level::Warn, "program_late", format!("{text} within {PROGRAM_REPLY_LIMIT_MS} ms"));
        }
    }

    /// When the channel next needs [`Channel::deadline`].
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        let grace = self.config.session_grace_ms;
        let expiry = self.sessions.values().filter_map(|&since| since).min().map(|since| since.saturating_add(grace));
        let dues = self.connections.values().flat_map(|state| state.reply_due.into_iter().chain(state.selection_due.values().map(|&(_, due)| due)));
        dues.chain(expiry).min()
    }

    fn item(&mut self, connection: ConnectionId, item: &Item<'_>, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.acked = state.acked.wrapping_add(u16::try_from(units(item) & 0xFFFF).unwrap_or(0));
        let keys = state.keys;
        match *item {
            Item::Event(Event::Hello { version, caps, .. }) => self.welcome(connection, version, caps),
            Item::Event(Event::Page(page)) => self.page_open(connection, page, now),
            _ => {
                let Some(input) = Input::of(item) else { return };
                let Some(event) = input.event() else { return };
                if keys == 0 {
                    return;
                }
                count(&mut self.stats.inputs);
                if matches!(input, Input::Key { action: neoteletext_input::event::Action::Press, .. } | Input::Text(_)) {
                    count(&mut self.stats.keys);
                }
                self.tell(&ServerMessage::Input(InputMessage { connection, view: FIRST_VIEW, page: PageNumber(keys), event }));
            }
        }
    }

    fn welcome(&mut self, connection: ConnectionId, version: u32, caps: u32) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if version != neoteletext_input::VERSION {
            return;
        }
        let record = state.next_record();
        if let Ok(packet) = packet_of(&Control::Welcome { version, caps: caps & USED, resume: None }, SERVICE, record) {
            state.welcomed = true;
            let session = state.session;
            self.send(session, Arc::from(&packet.bytes[..]));
        }
    }

    /// PAGE: the page with the keys changed. Until the receiver's SELECT is
    /// built (`INPUT.md` REQ-INT-140, the engine's TODO-1489), it is also
    /// the first view's selection: the page it opened, and none of its
    /// sub-pages in particular.
    fn page_open(&mut self, connection: ConnectionId, page: u16, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.went = None;
        state.keys = page;
        if page == 0 {
            return;
        }
        self.select(connection, FIRST_VIEW, Selection { page: PageNumber(page), sub: ANY_SUBPAGE }, now);
    }

    /// A view selected a page or a sub-page: the program is told it left
    /// the page it had, and asked for the new one (REQ-1472). SELECT calls
    /// this for each view once the engine decodes it.
    fn select(&mut self, connection: ConnectionId, view: u8, selection: Selection, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let before = state.selections.insert(view, selection);
        if before == Some(selection) {
            return;
        }
        // What the view was last sent is dropped: the first sub-page after
        // a new selection goes whole (DEC-1499 items 3 and 4).
        state.shown.remove(&view);
        let announced = state.announced;
        if announced {
            state.selection_due.insert(view, (selection.page.0, now.saturating_add(PROGRAM_REPLY_LIMIT_MS)));
        }
        self.stats.opened(selection.page.0);
        if !announced {
            // The program, when it starts, is told of it in `connected`.
            return;
        }
        if let Some(before) = before
            && before.page != selection.page
        {
            self.tell(&ServerMessage::Left(Left { connection, view, page: before.page }));
        }
        self.tell(&ServerMessage::Selected(Selected { connection, view, page: selection.page, sub: selection.sub }));
    }

    /// After a message's input: the cursor record says what was acted on.
    fn acknowledge(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if !state.welcomed {
            return;
        }
        let page = state.went.unwrap_or(state.keys);
        let session = state.session;
        if let Some(bytes) = state.cursor_packet(page) {
            self.send(session, bytes);
        }
    }

    /// A line from the channel's program, without its newline: read,
    /// checked and carried out whole, or refused whole with `refused`
    /// (REQ-1475).
    pub fn from_program(&mut self, line: &[u8], now: u64) {
        count(&mut self.stats.lines_from_application);
        let acted = protocol::read(line).and_then(|message| self.act(message, now));
        if let Err(refusal) = acted {
            count(&mut self.stats.refused);
            self.log(Level::Warn, "line_refused", format!("refused a line from the program: {refusal}"));
            self.tell(&ServerMessage::Refused(refusal.refused()));
        }
    }

    /// The open connections `connections` names.
    fn targets(&self, connections: &Connections) -> Vec<ConnectionId> {
        match connections {
            Connections::All(_) => self.connections.keys().copied().collect(),
            Connections::Listed(listed) => listed.iter().copied().filter(|connection| self.connections.contains_key(connection)).collect(),
        }
    }

    fn act(&mut self, message: ProgramMessage, _now: u64) -> Result<(), Refusal> {
        let kind = message.kind();
        let refuse = |path: &str, rule: Rule, detail: String| Refusal { message: Some(kind), path: path.to_string(), rule, detail };
        match message {
            ProgramMessage::Reply(reply) => {
                let Some(state) = self.connections.get_mut(&reply.connection) else {
                    return Err(refuse("connection", Rule::UnknownConnection, format!("{} is not open", reply.connection)));
                };
                if !state.announced || state.replied {
                    return Err(refuse("connection", Rule::NotAwaited, format!("{}'s connected was answered already", reply.connection)));
                }
                state.replied = true;
                state.reply_due = None;
                // A view given its page holds it as selected, so the PAGE
                // that follows when the receiver shows it asks for nothing.
                for view_page in &reply.selected {
                    state.selections.entry(view_page.view).or_insert(Selection { page: view_page.page.page, sub: ANY_SUBPAGE });
                }
                // The index goes as index records on data channel 12 once
                // the engine encodes them (REQ-1480, TODO-1491).
                for view_page in &reply.selected {
                    self.send_page(&[reply.connection], &view_page.page);
                }
            }
            ProgramMessage::Page(message) => {
                let targets = self.targets(&message.connections);
                self.send_page(&targets, &message.page);
            }
            // The index goes as index records (REQ-1480, TODO-1491), and a
            // sub-page list as a SUBPAGES record to the connections named
            // whose views show the page and do not hold its version
            // (REQ-1503), once the engine encodes them.
            ProgramMessage::Index(_) | ProgramMessage::Subpages(_) => {}
            ProgramMessage::Goto(goto) => {
                let Some(state) = self.connections.get_mut(&goto.connection) else { return Ok(()) };
                count(&mut self.stats.gotos);
                state.went = Some(goto.page.0);
                let session = state.session;
                if let Some(bytes) = state.cursor_packet(goto.page.0) {
                    self.send(session, bytes);
                }
            }
            ProgramMessage::Sound(sound) => {
                let records = self.sounds.records(&sound).map_err(|(rule, detail)| refuse("", rule, detail))?;
                let targets = self.targets(audience(&sound));
                self.records(&targets, &records);
            }
            // Sent as a SET_COOKIE record once the engine encodes the typed
            // one (REQ-1480, TODO-1491).
            ProgramMessage::Cookie(cookie) => {
                self.log(Level::Debug, "cookie_held_back", format!("cookie {} for connection {}: not sent yet", cookie.name, cookie.connection));
            }
            // Sent as LOG records once the engine encodes them (REQ-1480,
            // TODO-1460); until then the operator sees them.
            ProgramMessage::AuthorLog(log) => self.log(Level::Debug, "author_log", format!("{:?} {}", log.level, log.text)),
            ProgramMessage::Log(log) => {
                let level = match log.level {
                    OperatorLevel::Error => Level::Error,
                    OperatorLevel::Warn => Level::Warn,
                    OperatorLevel::Info => Level::Info,
                    OperatorLevel::Debug => Level::Debug,
                };
                self.log(level, "program_log", format!("{}: {}", log.event, log.text));
            }
            ProgramMessage::Packets(packets) => {
                let bytes: Arc<[u8]> = Arc::from(protocol::check::bytes_of(&packets.packets));
                for connection in self.targets(&packets.connections) {
                    if let Some(session) = self.connections.get(&connection).map(|state| state.session) {
                        self.send(session, Arc::clone(&bytes));
                    }
                }
            }
        }
        Ok(())
    }

    /// Sends the sub-page `page` to the views of `targets` that show it:
    /// those that selected it, and those that selected its page with 3F7F,
    /// for which it is now the page's current sub-page (DEC-1500 items 2
    /// and 4). Each is sent the whole sub-page the first time, and then
    /// only what changed since the version it was last sent (DEC-1499,
    /// REQ-1504). It is encoded once, whatever the number of viewers.
    fn send_page(&mut self, targets: &[ConnectionId], page: &protocol::Page) {
        let encoded = crate::page::encode(page);
        let whole: Arc<[u8]> = Arc::from(encoded.packets.concat());
        let (number, sub) = (page.page.0, page.sub.0);
        let mut sends = Vec::new();
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let showing: Vec<u8> = state
                .selections
                .iter()
                .filter(|(_, selection)| selection.page.0 == number && (selection.sub.0 == sub || selection.sub == ANY_SUBPAGE))
                .map(|(&view, _)| view)
                .collect();
            if showing.is_empty() {
                continue;
            }
            let mut queued: Vec<Arc<[u8]>> = Vec::new();
            for view in showing {
                let change = match state.shown.get(&view) {
                    Some(kept) if kept.page == number && kept.sub == sub => crate::page::changes(&kept.packets, &encoded),
                    _ => crate::page::Change::Whole,
                };
                state.shown.insert(view, Shown { page: number, sub, packets: encoded.packets.clone() });
                let bytes = match change {
                    crate::page::Change::Nothing => continue,
                    crate::page::Change::Whole => Arc::clone(&whole),
                    crate::page::Change::Rows(bytes) => Arc::from(bytes),
                };
                // Views of one connection share its stream: the same bytes
                // go once.
                if !queued.contains(&bytes) {
                    queued.push(bytes);
                }
            }
            count(&mut self.stats.pages_sent);
            state.cursors.insert(number, encoded.cursor);
            state.selection_due.retain(|_, &mut (awaited, _)| awaited != number);
            let session = state.session;
            sends.extend(queued.into_iter().map(|bytes| (session, bytes)));
            if state.welcomed
                && state.keys == number
                && let Some(cursor) = state.cursor_packet(number)
            {
                sends.push((session, cursor));
            }
        }
        for (session, bytes) in sends {
            self.send(session, bytes);
        }
    }

    /// Sends audio records to `targets`, each numbered on its connection.
    fn records(&mut self, targets: &[ConnectionId], records: &[Vec<u8>]) {
        self.stats.sound_records = self.stats.sound_records.saturating_add(u64::try_from(records.len()).unwrap_or(u64::MAX));
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let (packets, next) = packets(SERVICE, state.record_id, records);
            state.record_id = next;
            let mut bytes = Vec::with_capacity(packets.len().saturating_mul(neoteletext_core::consts::PACKET_BYTES));
            for packet in &packets {
                bytes.extend_from_slice(&packet.bytes);
            }
            let session = state.session;
            self.send(session, Arc::from(bytes));
        }
    }
}

/// A count as the protocol writes it.
fn wide(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX)
}

/// The standard's maxima, from the engine's `limits`, the one place they
/// are written (requirement 3.3; `STANDARD-LIMITS.md` section 3 item 3).
fn standard() -> Standard {
    use neoteletext_core::limits;
    let per_magazine = limits::PAGE_NUMBERS_PER_MAGAZINE.saturating_sub(limits::RESERVED_IN_EVERY_MAGAZINE.len());
    Standard {
        magazines: wide(limits::MAGAZINES),
        pages: wide(limits::USABLE_PAGE_NUMBERS),
        pages_per_magazine: wide(per_magazine),
        pages_in_magazine_1: wide(per_magazine.saturating_sub(limits::RESERVED_PAGES.len())),
        keyable_pages: wide(limits::KEYABLE_PAGES),
        subpages: wide(limits::MAX_SUBPAGES),
        decimal_subpages: wide(limits::DECIMAL_SUBCODES),
        views: u64::from(protocol::MAX_VIEWS),
    }
}

/// The cursor record for `page`: the page's cursor if it has one, else
/// none, with the input count.
fn cursor_of(page: u16, acked: u16, line: Option<&CursorLine>) -> Cursor {
    let place = line.filter(|line| line.row != NO_CURSOR).map(|line| Place { row: line.row, column: line.column });
    // Only BLOCK is a flag now: echo and masked echo are withdrawn (the
    // engine's E.134).
    Cursor { page, acked, place, flags: line.map_or(0, |line| line.flags & BLOCK) }
}
