//! One channel, as a state machine: the host gives it what happened, with
//! the time, and carries out the [`Effect`]s it answers with.
//!
//! **The server relays; the program owns every page** (DEC-1482). A
//! connection is one browser tab with one selection (DEC-1527). The
//! channel keeps no content (REQ-1474): only what each open connection
//! is, its session, its selection, and the one sub-page that selection
//! shows as it was last sent, against which its changes are taken
//! (DEC-1499). Each connection that opens is told to the program as
//! `connected` (REQ-1471), each selection as `selected` (REQ-1472), and
//! what the program sends goes to the connections it names, as it comes
//! (REQ-1473), as checked teletext packets (REQ-1480, REQ-1462).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_audio::encode::packets;
use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Packet, is_valid_stream_packet};
use neoteletext_formats::page::CursorLine;
use neoteletext_input::caps::{CURSOR_CAP, PAGE_CAP};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::cursor::{BLOCK, Cursor, NO_CURSOR, Place};
use neoteletext_input::event::{Event, Item, MAX_STREAM_ITEM, StreamDecoder, units};

use crate::input::Input;
use crate::json::Writer;
use crate::page::Change;
use crate::protocol::{
    self, ANY_SUBPAGE, Budgets, Closed, Connected, ConnectionId, Connections, InputMessage, Left, OperatorLevel, PageNumber, ProgramMessage, Refusal, Rule,
    Selected, Selection, ServerMessage, SessionId, Standard, Start,
};
use crate::records;
use crate::sound::{Sounds, audience};
use crate::stats::{ChannelStats, count};

/// The service number audio and input records are sent on (the
/// reference channel's, INPUT.md section 7).
const SERVICE: u8 = 0;

/// The capabilities the channel uses of what a receiver offers in HELLO
/// (INPUT.md section 6.2). There are no modes to agree: every event is
/// sent and the channel ignores what it does not use (section 6.3).
const USED: u32 = PAGE_CAP | CURSOR_CAP;

/// How long the server waits for a program's answer to `connected` or
/// `selected` before the viewer is told the channel is unavailable
/// (REQ-1481). Provisional, not yet measured: TODO-1521 measures the
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
    /// How long a session whose last connection closed waits for its
    /// viewer to come back before its program is told it ended, in
    /// milliseconds; 0 ends it at once (requirement 5.5).
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
    /// Bytes for one connection, whole packets every one checked
    /// (REQ-1462), as one binary WebSocket message, or several where the
    /// host must cut it ([`crate::frame::messages`]).
    Send {
        /// The connection.
        connection: ConnectionId,
        /// The message.
        bytes: Arc<[u8]>,
    },
    /// Close the connection: its input broke the protocol.
    Close {
        /// The connection.
        connection: ConnectionId,
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

/// The sub-page a connection's selection shows, as it was last sent: the
/// baseline of its changes (DEC-1499).
struct Shown {
    /// The page.
    page: u16,
    /// The sub-page's sub-code.
    sub: u16,
    /// Its packets as last sent whole.
    packets: Vec<[u8; PACKET_BYTES]>,
    /// Its cursor.
    cursor: Option<CursorLine>,
}

/// One open connection: all the channel holds of a viewer (REQ-1474).
struct Connection {
    /// Its session; a session may have several connections, one a tab
    /// (DEC-1527).
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
    /// What it selected: a page and a sub-code, the sub-code 3F7F until
    /// the program answers with the page's first sub-page (DEC-1526).
    selection: Option<Selection>,
    /// The sub-page its selection shows, as last sent.
    shown: Option<Shown>,
    /// The program running now was told of it with `connected`.
    announced: bool,
    /// The program answered that `connected`.
    replied: bool,
    /// When the program's `reply` is due, while it is awaited.
    reply_due: Option<u64>,
    /// When the sub-page its selection asked for is due, while awaited.
    selection_due: Option<u64>,
    /// The index's sending, numbered for the receiver (`INPUT.md`
    /// section 7.5).
    index_generation: u16,
    /// The sub-page list it holds of its selected page, and its version.
    list_held: Option<(u16, u16)>,
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
            selection: None,
            shown: None,
            announced: false,
            replied: false,
            reply_due: None,
            selection_due: None,
            index_generation: 0,
            list_held: None,
        }
    }

    /// The next record number, advanced.
    const fn next_record(&mut self) -> u8 {
        let id = self.record_id;
        self.record_id = id.wrapping_add(1);
        id
    }

    /// The cursor record for `page`, numbered next: the cursor of the
    /// sub-page shown when it is of that page, else none.
    fn cursor_packet(&mut self, page: u16) -> Option<Arc<[u8]>> {
        let line = self.shown.as_ref().filter(|shown| shown.page == page).and_then(|shown| shown.cursor);
        let cursor = cursor_of(page, self.acked, line.as_ref());
        let record = self.next_record();
        cursor.packet(SERVICE, record).map(|packet| Arc::from(&packet.bytes[..]))
    }

    /// Whether its selection shows the sub-page `sub` of `page`: it
    /// selected that sub-page, or the page with 3F7F and awaits its first.
    fn shows(&self, page: u16, sub: u16) -> bool {
        self.selection.is_some_and(|selection| selection.page.0 == page && (selection.sub.0 == sub || selection.sub == ANY_SUBPAGE))
    }
}

/// A session: it outlives its connections (requirement 5.5).
struct Session {
    /// Its connections open now.
    open: usize,
    /// When its last connection closed, while it waits for its viewer to
    /// come back.
    waiting_since: Option<u64>,
}

/// One channel's state.
pub struct Channel {
    config: ChannelConfig,
    /// Whether the program is running, `start` sent to it.
    program: bool,
    connections: BTreeMap<ConnectionId, Connection>,
    sessions: BTreeMap<SessionId, Session>,
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
            sessions: BTreeMap::new(),
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

    /// Sends `bytes` to `connection`, every packet checked first with the
    /// engine's one check of a stream's packets (REQ-1462, TODO-1465): one
    /// that fails is not sent, and the operator is told, since it is a
    /// fault of the server or the engine.
    fn send(&mut self, connection: ConnectionId, bytes: Arc<[u8]>) {
        let (packets, rest) = bytes.as_chunks::<PACKET_BYTES>();
        let valid = |packet: &[u8; PACKET_BYTES]| is_valid_stream_packet(&Packet { bytes: *packet });
        if rest.is_empty() && packets.iter().all(valid) {
            self.effects.push(Effect::Send { connection, bytes });
            return;
        }
        let kept: Vec<u8> = packets.iter().filter(|packet| valid(packet)).flatten().copied().collect();
        let dropped = packets.len().saturating_sub(kept.len() / PACKET_BYTES).saturating_add(usize::from(!rest.is_empty()));
        self.stats.invalid_packets = self.stats.invalid_packets.saturating_add(u64::try_from(dropped).unwrap_or(u64::MAX));
        self.log(Level::Error, "invalid_packets", format!("{dropped} packets for connection {connection} were not valid stream packets and were not sent"));
        if !kept.is_empty() {
            self.effects.push(Effect::Send { connection, bytes: Arc::from(kept) });
        }
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
            state.selection_due = None;
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
        let message = Connected { connection, session: state.session, cookies: Vec::new(), selection: state.selection };
        self.tell(&ServerMessage::Connected(message));
    }

    /// A connection the host numbered `connection` opened for `session`,
    /// which it continues or starts (requirement 5.5). The host never
    /// reuses a connection's number while the channel runs.
    pub fn opened(&mut self, connection: ConnectionId, session: SessionId, now: u64) {
        let state = self.sessions.entry(session).or_insert(Session { open: 0, waiting_since: None });
        if state.waiting_since.take().is_some() {
            self.stats.resumed();
        } else if state.open == 0 {
            self.stats.joined();
        }
        state.open = state.open.saturating_add(1);
        self.connections.insert(connection, Connection::new(session));
        self.announce(connection, now);
    }

    /// A connection closed: the program is told its selection left its
    /// page, and when it was its session's last, the session waits for its
    /// viewer for the channel's grace (requirement 5.5); the program is
    /// told it ended with `closed` only when the grace passes.
    pub fn closed(&mut self, connection: ConnectionId, now: u64) {
        let Some(state) = self.connections.remove(&connection) else { return };
        if state.announced
            && let Some(selection) = state.selection
        {
            self.tell(&ServerMessage::Left(Left { connection, page: selection.page }));
        }
        let session = state.session;
        let Some(open) = self.sessions.get_mut(&session) else { return };
        open.open = open.open.saturating_sub(1);
        if open.open > 0 {
            return;
        }
        if self.config.session_grace_ms == 0 {
            self.sessions.remove(&session);
            self.stats.left();
            self.tell(&ServerMessage::Closed(Closed { session }));
            return;
        }
        open.waiting_since = Some(now);
        self.stats.waiting();
    }

    /// A binary message from a connection: its input stream (INPUT.md).
    pub fn received(&mut self, connection: ConnectionId, bytes: &[u8], now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let mut decoder = state.decoder.take().unwrap_or_else(|| Box::new(StreamDecoder::<MAX_STREAM_ITEM>::default()));
        let result = decoder.push(bytes, &mut |item| self.item(connection, &item, now));
        if result.is_err() {
            // A protocol error ends the session (INPUT.md section 8).
            self.effects.push(Effect::Close { connection });
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
        let expired: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, state)| state.waiting_since.is_some_and(|since| since.saturating_add(grace) <= now))
            .map(|(&session, _)| session)
            .collect();
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
            if state.selection_due.is_some_and(|due| due <= now) {
                state.selection_due = None;
                late.push(format!("connection {connection}: no page for selected"));
            }
        }
        // DENIED "unavailable" (`SERVER.md` section 5.5) goes to the viewer
        // here once its record exists (TODO-1522); until then the operator
        // is told.
        for text in late {
            self.log(Level::Warn, "program_late", format!("{text} within {PROGRAM_REPLY_LIMIT_MS} ms"));
        }
    }

    /// When the channel next needs [`Channel::deadline`].
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        let grace = self.config.session_grace_ms;
        let expiry = self.sessions.values().filter_map(|state| state.waiting_since).min().map(|since| since.saturating_add(grace));
        let dues = self.connections.values().flat_map(|state| state.reply_due.into_iter().chain(state.selection_due));
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
                self.tell(&ServerMessage::Input(InputMessage { connection, page: PageNumber(keys), event }));
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
            self.send(connection, Arc::from(&packet.bytes[..]));
        }
    }

    /// PAGE: the page with the keys changed. Until the receiver's SELECT is
    /// wired here (the engine's TODO-1489), a PAGE of a page other than the
    /// one selected stands for a selection of that page with 3F7F.
    fn page_open(&mut self, connection: ConnectionId, page: u16, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.went = None;
        state.keys = page;
        if page == 0 || state.selection.is_some_and(|selection| selection.page.0 == page) {
            return;
        }
        self.select(connection, Selection { page: PageNumber(page), sub: ANY_SUBPAGE }, now);
    }

    /// The connection selected a page (sub-code 3F7F) or one sub-page: the
    /// program is told it left the page it had, and asked for the new one
    /// (REQ-1472). What was kept of the sub-page shown is dropped, so the
    /// first sub-page after a new selection goes whole (DEC-1499).
    fn select(&mut self, connection: ConnectionId, selection: Selection, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let before = state.selection.replace(selection);
        if before == Some(selection) {
            return;
        }
        state.shown = None;
        let announced = state.announced;
        if announced {
            state.selection_due = Some(now.saturating_add(PROGRAM_REPLY_LIMIT_MS));
        }
        self.stats.opened(selection.page.0);
        if !announced {
            // The program, when it starts, is told of it in `connected`.
            return;
        }
        if let Some(before) = before
            && before.page != selection.page
        {
            self.tell(&ServerMessage::Left(Left { connection, page: before.page }));
        }
        self.tell(&ServerMessage::Selected(Selected { connection, page: selection.page, sub: selection.sub }));
    }

    /// After a message's input: the cursor record says what was acted on.
    fn acknowledge(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if !state.welcomed {
            return;
        }
        let page = state.went.unwrap_or(state.keys);
        if let Some(bytes) = state.cursor_packet(page) {
            self.send(connection, bytes);
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

    /// Sends a record made for each of `targets` by `make`, from its own
    /// record id; a record the engine's encoder refuses refuses the
    /// message (checked before any is sent, by the first target's).
    fn send_records(
        &mut self,
        targets: &[ConnectionId],
        kind: protocol::MessageType,
        mut make: impl FnMut(&mut Connection) -> Result<Vec<u8>, records::Refused>,
    ) -> Result<(), Refusal> {
        let mut sends = Vec::new();
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let bytes = make(state).map_err(|detail| Refusal { message: Some(kind), path: String::new(), rule: Rule::WrongType, detail })?;
            sends.push((connection, bytes));
        }
        for (connection, bytes) in sends {
            if !bytes.is_empty() {
                self.send(connection, Arc::from(bytes));
            }
        }
        Ok(())
    }

    fn act(&mut self, message: ProgramMessage, now: u64) -> Result<(), Refusal> {
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
                // A connection with no selection asked for the initial page:
                // the sub-page given is its selection's.
                if let Some(page) = &reply.selected
                    && state.selection.is_none()
                {
                    state.selection = Some(Selection { page: page.page, sub: ANY_SUBPAGE });
                }
                let magazines = reply.magazines;
                self.send_records(&[reply.connection], kind, |state| {
                    state.index_generation = state.index_generation.wrapping_add(1);
                    records::index(state.index_generation, &magazines, &mut state.record_id)
                })?;
                if let Some(page) = &reply.selected {
                    self.send_page(&[reply.connection], page);
                }
            }
            ProgramMessage::Page(message) => {
                let targets = self.targets(&message.connections);
                self.send_page(&targets, &message.page);
            }
            ProgramMessage::Subpages(list) => {
                let page = list.page.0;
                let targets: Vec<ConnectionId> = self
                    .targets(&list.connections)
                    .into_iter()
                    .filter(|connection| {
                        self.connections.get(connection).is_some_and(|state| {
                            state.selection.is_some_and(|selection| selection.page.0 == page) && state.list_held != Some((page, list.version))
                        })
                    })
                    .collect();
                self.send_records(&targets, kind, |state| {
                    state.list_held = Some((page, list.version));
                    records::subpage_list(&list, &mut state.record_id)
                })?;
            }
            ProgramMessage::Index(index) => {
                let targets = self.targets(&index.connections);
                self.send_records(&targets, kind, |state| {
                    state.index_generation = state.index_generation.wrapping_add(1);
                    records::index(state.index_generation, &index.magazines, &mut state.record_id)
                })?;
            }
            // A goto is itself a selection (DEC-1526 item 4): the receiver
            // goes to the page of the cursor record, and the program is
            // asked for it.
            ProgramMessage::Goto(goto) => {
                let Some(state) = self.connections.get_mut(&goto.connection) else { return Ok(()) };
                count(&mut self.stats.gotos);
                state.went = Some(goto.page.0);
                if let Some(bytes) = state.cursor_packet(goto.page.0) {
                    self.send(goto.connection, bytes);
                }
                self.select(goto.connection, Selection { page: goto.page, sub: goto.sub }, now);
            }
            ProgramMessage::Sound(sound) => {
                let records = self.sounds.records(&sound).map_err(|(rule, detail)| refuse("", rule, detail))?;
                let targets = self.targets(audience(&sound));
                self.records(&targets, &records);
            }
            ProgramMessage::Cookie(cookie) => {
                if !self.connections.contains_key(&cookie.connection) {
                    return Ok(());
                }
                self.send_records(&[cookie.connection], kind, |state| records::cookie(&cookie, &mut state.record_id))?;
            }
            ProgramMessage::AuthorLog(line) => {
                let targets = self.targets(&line.connections);
                self.send_records(&targets, kind, |state| records::author_log(&line, &mut state.record_id))?;
            }
            ProgramMessage::Log(line) => {
                let level = match line.level {
                    OperatorLevel::Error => Level::Error,
                    OperatorLevel::Warn => Level::Warn,
                    OperatorLevel::Info => Level::Info,
                    OperatorLevel::Debug => Level::Debug,
                };
                self.log(level, "program_log", format!("{}: {}", line.event, line.text));
            }
            ProgramMessage::Packets(packets) => {
                let bytes: Arc<[u8]> = Arc::from(protocol::check::bytes_of(&packets.packets));
                for connection in self.targets(&packets.connections) {
                    self.send(connection, Arc::clone(&bytes));
                }
            }
        }
        Ok(())
    }

    /// Sends the sub-page `page` to those of `targets` whose selection
    /// shows it: those that selected it, and those that selected its page
    /// with 3F7F, whose selection it then becomes (DEC-1526 item 3). Each
    /// is sent it whole, with the erase bit, unless its baseline is the
    /// same sub-code, when only what changed is sent (DEC-1499); every
    /// frame of a sub-page with Link Extension links is followed by its
    /// LINKS record (DEC-1517). It is encoded once, whatever the number of
    /// viewers.
    fn send_page(&mut self, targets: &[ConnectionId], page: &protocol::Page) {
        let encoded = crate::page::encode(page);
        let (number, sub) = (page.page.0, page.sub.0);
        let mut sends: Vec<(ConnectionId, Vec<u8>)> = Vec::new();
        let mut faults: Vec<String> = Vec::new();
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if !state.shows(number, sub) {
                continue;
            }
            state.selection = Some(Selection { page: page.page, sub: page.sub });
            state.selection_due = None;
            let change = match &state.shown {
                Some(kept) if kept.page == number && kept.sub == sub => crate::page::changes(&kept.packets, &encoded),
                _ => Change::Whole,
            };
            state.shown = Some(Shown { page: number, sub, packets: encoded.packets.clone(), cursor: encoded.cursor });
            let mut bytes = match change {
                Change::Nothing => continue,
                Change::Whole => encoded.packets.concat(),
                Change::Rows(bytes) => bytes,
            };
            count(&mut self.stats.pages_sent);
            match records::sub_page_links(page, &mut state.record_id) {
                Ok(links) => bytes.extend_from_slice(&links),
                Err(fault) => faults.push(fault),
            }
            if state.welcomed
                && state.keys == number
                && let Some(cursor) = state.cursor_packet(number)
            {
                bytes.extend_from_slice(&cursor);
            }
            sends.push((connection, bytes));
        }
        for fault in faults {
            self.log(Level::Error, "links_refused", format!("{number:03X}/{sub:04X}: the engine refused its links: {fault}"));
        }
        for (connection, bytes) in sends {
            self.send(connection, Arc::from(bytes));
        }
    }

    /// Sends audio records to `targets`, each numbered on its connection.
    fn records(&mut self, targets: &[ConnectionId], records: &[Vec<u8>]) {
        self.stats.sound_records = self.stats.sound_records.saturating_add(u64::try_from(records.len()).unwrap_or(u64::MAX));
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let (packets, next) = packets(SERVICE, state.record_id, records);
            state.record_id = next;
            let mut bytes = Vec::with_capacity(packets.len().saturating_mul(PACKET_BYTES));
            for packet in &packets {
                bytes.extend_from_slice(&packet.bytes);
            }
            self.send(connection, Arc::from(bytes));
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
