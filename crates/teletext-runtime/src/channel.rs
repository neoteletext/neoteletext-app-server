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

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_audio::encode::packets;
use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::{Packet, is_valid_stream_packet};
use neoteletext_formats::page::CursorLine;
use neoteletext_input::MAX_PER_SECOND;
use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::cursor::{BLOCK, Cursor, Place};
use neoteletext_input::event::{Action, Event, Item, MouseAction, Stamp, read_message, units};
use neoteletext_input::held::{Held, Released};
use neoteletext_input::pace::{Offer, Pacer};
use neoteletext_input::session::REMEMBERED;
use neoteletext_web::consts::{CLOSE_BROKE_PROTOCOL, CLOSE_WRONG_VERSION};
use neoteletext_web::records::denied::{DeniedRecord, STATUS_UNAVAILABLE, encode as encode_denied};

use crate::descriptions::{Answer, Asked, DESCRIPTION_WAIT_MS, Descriptions, PublicPage, VisitorId};
use crate::input::{self, Input};
use crate::json::Writer;
use crate::page::Change;
use crate::protocol::{
    self, ANY_SUBPAGE, Budgets, Closed, Connected, ConnectionId, Connections, CookiePair, Describe, Description, InputMessage, Left, OperatorLevel, PageNumber,
    ProgramMessage, Refusal, RequestNumber, Rule, Selected, Selection, ServerMessage, SessionId, Standard, Start,
};
use crate::records;
use crate::sound::{Sounds, audience};
use crate::stats::{ChannelStats, count};

/// The service number audio and input records are sent on (the
/// reference channel's, DOC-1951).
const SERVICE: u8 = 0;

/// The capabilities the channel uses of what a receiver offers in HELLO
/// (DOC-1922): the cursor record, and SELECT, the tab's
/// selection (REQ-INT-140). There are no modes to agree: every event is
/// sent and the channel ignores what it does not use (DOC-1922).
const USED: u32 = CURSOR_CAP | SELECT_CAP;

/// How often the operator is told of input dropped for its stamp, at most
/// (REQ-1557): the interval of invalid packets (REQ-1462).
pub const STALE_INPUT_REPORT_SECONDS: u64 = neoteletext_web::consts::INVALID_PACKET_REPORT_SECONDS;
/// [`STALE_INPUT_REPORT_SECONDS`] on the host's clock.
const STALE_INPUT_REPORT_MS: u64 = STALE_INPUT_REPORT_SECONDS * 1_000;

/// How long an input count waits for a page frame to carry it before its
/// cursor record goes alone (REQ-1334). A record carried saves a packet a key;
/// one late costs a viewer nothing, since no receiver waits for an
/// acknowledgement to show anything, and only keeps its input to resume with a
/// little longer. Provisional, not measured: a page answering a key is sent
/// well within it, on the programs seen so far.
pub const ACKNOWLEDGEMENT_WAIT_MS: u64 = 250;
// A receiver keeps what is not yet acknowledged, and must not run out of room
// in the wait, at the input rate.
const _: () =
    assert!(ACKNOWLEDGEMENT_WAIT_MS * MAX_PER_SECOND as u64 / 1_000 < REMEMBERED as u64, "the wait is longer than a receiver can keep input at the input rate");

/// How long the server waits for a program's answer to `connected` or
/// `selected` before the viewer is told the channel is unavailable
/// (REQ-1481): the deadline every selection ends by (DOC-1921).
/// Provisional, not yet measured: TODO-1521 measures the
/// three channels' programs on the live server and sets it from that.
pub const PROGRAM_REPLY_LIMIT_MS: u64 = 5_000;
/// How long `connected` is held for a new connection's first SELECT
/// (TODO-1731, DOC-1921), so that a shared link goes straight
/// to its page. A receiver sends SELECT one round trip after the connection
/// opens, HELLO to WELCOME; this allows for a slow satellite or mobile link
/// and is short enough not to be noticed on a channel's first page when a
/// receiver has nothing to select.
pub const FIRST_SELECTION_WAIT_MS: u64 = 1_500;

/// The engine's pace of moves counts in microseconds, the host's clock in
/// milliseconds.
const MICROSECONDS_PER_MILLISECOND: u64 = 1_000;

/// The host's time as the pace counts it.
const fn microseconds(now_ms: u64) -> u64 {
    now_ms.saturating_mul(MICROSECONDS_PER_MILLISECOND)
}

/// A channel's description, for `channel.ttx`.
#[derive(Clone, Debug)]
pub struct ChannelConfig {
    /// Its name.
    pub name: String,
    /// The channel's address, ending in `/`: its origin and folder, which
    /// its small page names.
    pub address: String,
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
/// restarted (DOC-1924: "a grace period of minutes").
pub const SESSION_GRACE_MS: u64 = 5 * 60 * 1_000;

/// The sessions a channel keeps waiting for their viewer at once (REQ-1040,
/// TODO-1301): 65,536, the open files the native host's unit allows it
/// (`LimitNOFILE` in `deploy/ionos/deploy.sh`), so that every viewer the
/// host can hold connected can lose their socket together, as a restart
/// makes them, and still find their session. A waiting session holds no
/// socket, so nothing else bounds them: without this, connections opened
/// and closed at a high rate would grow the channel's memory, and its
/// program's, by the rate times [`SESSION_GRACE_MS`]. At the bound the
/// session waiting longest ends as though its grace had passed, the
/// pattern of the descriptions' cache (DEC-2145).
pub const MOST_WAITING_SESSIONS: usize = 65_536;

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
    /// Close the connection with this close code (DOC-1919).
    Close {
        /// The connection.
        connection: ConnectionId,
        /// Why: a code a receiver stops on.
        code: u16,
    },
    /// A line for the channel's program, with its newline.
    ToProgram {
        /// The line.
        line: Vec<u8>,
        /// The connection it is about, if it is about one: that connection's
        /// move before it goes before it, and no later move replaces that.
        about: Option<ConnectionId>,
    },
    /// A connection's pointer move for its program, with its newline: at
    /// most 60 a second, and the latest, so it takes the place of any of that
    /// connection's not yet given to the program, and costs no other line
    /// its room (REQ-1360).
    MoveToProgram {
        /// The connection.
        connection: ConnectionId,
        /// The line.
        line: Vec<u8>,
    },
    /// What a web visitor [`Channel::describe`] named is at the page's
    /// address now.
    Described {
        /// The visitor.
        visitor: VisitorId,
        /// The answer, the same for everyone.
        answer: Answer,
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

/// A connection's first selection's number.
const FIRST_REQUEST: RequestNumber = 1;

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
    /// Its Link Extension links as last sent, which its packets do not
    /// carry: they travel as its LINKS record (REQ-1494).
    links: protocol::LinkExtension,
}

/// How a connection's selection stands (DOC-1921, REQ-1481).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// A page answered it, or there is nothing to answer.
    Answered,
    /// It must end by this time: passing it sends DENIED 503.
    Due(u64),
    /// DENIED ended it, and nothing was shown since. The selection stands,
    /// since a later page for it is still shown, but a SELECT repeating it
    /// has nothing to wait on, and so asks again.
    Denied,
}

impl Ending {
    /// When it must end, while it has not.
    const fn due(self) -> Option<u64> {
        if let Self::Due(due) = self { Some(due) } else { None }
    }
}

/// One open connection: all the channel holds of a viewer (REQ-1474).
struct Connection {
    /// Its session; a session may have several connections, one a tab
    /// (DEC-1527).
    session: SessionId,
    /// The cookies it opened with, for its program (REQ-1471).
    cookies: Vec<CookiePair>,
    /// WELCOME was sent: the receiver acts on cursor records.
    welcomed: bool,
    /// The input count acted on, modulo 65 536 (DOC-1943).
    acked: u16,
    /// What it holds: keys and buttons down, whose releases the program is
    /// owed if the page loses the input (REQ-207).
    held: Held,
    /// Where the pointer last was, column and row: a lost button's place.
    pointer: (u8, u8),
    /// Its moves, each paced and the latest kept (REQ-1360). A slot a
    /// connection and not a session: a session has a connection for each tab
    /// (DEC-1527), each with a pointer of its own.
    moves: Pacer,
    /// The next record number on the audio and input service.
    record_id: u8,
    /// What it selected: a page and a sub-code, the sub-code 3F7F until
    /// the program answers with the page's first sub-page (DEC-1526).
    selection: Option<Selection>,
    /// The page parameters of its selection (REQ-1693 item 6).
    parameters: Option<String>,
    /// The number of its selection, rising from 1 (DEC-1759 lesson 13).
    request: RequestNumber,
    /// While `connected` is held for its first SELECT, when that ends.
    first_selection_due: Option<u64>,
    /// The sub-page its selection shows, as last sent.
    shown: Option<Shown>,
    /// The program running now was told of it with `connected`.
    announced: bool,
    /// The program answered that `connected`.
    replied: bool,
    /// How its selection has ended, or when it must (DOC-1921,
    /// REQ-1481).
    ending: Ending,
    /// When its input count must go alone, while no frame has carried it
    /// (REQ-1334).
    acknowledgement_due: Option<u64>,
    /// The index's sending, numbered for the receiver (DOC-1955).
    index_generation: u16,
    /// The sub-page list it holds of its selected page, and its version.
    list_held: Option<(u16, u16)>,
}

impl Connection {
    fn new(session: SessionId, cookies: Vec<CookiePair>, now: u64) -> Self {
        Self {
            session,
            cookies,
            welcomed: false,
            acked: 0,
            held: Held::new(),
            pointer: (0, 0),
            moves: Pacer::new(),
            record_id: 0,
            selection: None,
            parameters: None,
            request: FIRST_REQUEST,
            first_selection_due: Some(now.saturating_add(FIRST_SELECTION_WAIT_MS)),
            shown: None,
            announced: false,
            replied: false,
            ending: Ending::Answered,
            acknowledgement_due: None,
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
    /// Whether the program running has answered anything yet.
    answered: bool,
    /// The page the program was asked about to learn that it answers, until
    /// it does, which no visitor need be waiting for.
    readiness_page: Option<u16>,
    connections: BTreeMap<ConnectionId, Connection>,
    sessions: BTreeMap<SessionId, Session>,
    /// The sessions waiting for their viewer, by when each began to wait:
    /// the first is the next to end, by its grace or by
    /// [`MOST_WAITING_SESSIONS`]. Each is a session with `waiting_since`.
    waiting: BTreeSet<(u64, SessionId)>,
    sounds: Sounds,
    effects: Vec<Effect>,
    stats: ChannelStats,
    /// Stale input not yet told to the operator.
    stale_unreported: u64,
    /// When the operator was last told of stale input.
    stale_reported_at: Option<u64>,
    /// What the channel's pages show the web, for a short time (REQ-1728).
    descriptions: Descriptions,
}

impl Channel {
    /// A channel; its program is not running until
    /// [`Channel::program_started`].
    #[must_use]
    pub fn new(config: ChannelConfig) -> Self {
        Self {
            config,
            program: false,
            answered: false,
            readiness_page: None,
            connections: BTreeMap::new(),
            sessions: BTreeMap::new(),
            waiting: BTreeSet::new(),
            sounds: Sounds::default(),
            effects: Vec::new(),
            stats: ChannelStats::default(),
            stale_unreported: 0,
            stale_reported_at: None,
            descriptions: Descriptions::default(),
        }
    }

    /// The channel's description.
    #[must_use]
    pub const fn config(&self) -> &ChannelConfig {
        &self.config
    }

    /// The channel's `channel.ttx`, generated (DEC-1467 item 1): its name,
    /// its initial page and its push WebSocket, which takes input, always
    /// at `<channel>push` (REQ-1693 item 2). A receiver reads it first to
    /// know the channel is live (the engine's REQ-WEB-018), and then asks
    /// for nothing else over HTTP.
    #[must_use]
    pub fn descriptor(&self) -> Vec<u8> {
        let mut push = Writer::nested();
        push.boolean("input", true);
        let mut descriptor = Writer::nested();
        descriptor.text("name", &self.config.name).text("initial_page", &format!("{:03X}", self.config.initial_page)).object("push", push);
        let mut bytes = descriptor.line();
        bytes.pop();
        bytes
    }

    /// What a web browser shows at the channel's address (DEC-1669).
    #[must_use]
    pub fn page(&self, tail: &str) -> Vec<u8> {
        crate::landing::page(&self.config.name, &alloc::format!("{}{tail}", self.config.address)).into_bytes()
    }

    /// What a page's address answers when the page is not public, which
    /// names what was asked for and nothing else (REQ-1728 item 3).
    #[must_use]
    pub fn private_page(&self, tail: &str) -> Vec<u8> {
        neoteletext_web::small_page::private_page(&self.config.name, &alloc::format!("{}{tail}", self.config.address)).into_bytes()
    }

    /// The channel's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// The channel's address, `<channel>`.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.config.address
    }

    /// Whether the channel's program is running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.program
    }

    /// Whether the channel's program has answered since it started, so it
    /// can serve its viewers (requirement 8.4). One still loading has
    /// started and reads nothing: a viewer then would wait out
    /// [`PROGRAM_REPLY_LIMIT_MS`] and be denied.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.program && self.answered
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
            self.effects.push(Effect::ToProgram { line: protocol::line(message), about: message.connection() });
        }
    }

    /// A pointer's move for the program, as it left its pace.
    fn tell_move(&mut self, connection: ConnectionId, event: Event<'static>) {
        let item = Item::Event(event);
        let (Some(on), Some(moved)) = (input::stamp(&item), Input::of(&item).and_then(|input| input.event())) else { return };
        if self.program {
            count(&mut self.stats.lines_to_application);
            let message = InputMessage { connection, page: PageNumber(on.page), sub: protocol::SubCode(on.sub), lost: false, event: moved };
            self.effects.push(Effect::MoveToProgram { connection, line: protocol::line(&ServerMessage::Input(message)) });
        }
    }

    /// Any other event of `connection` is coming: the move waiting goes
    /// first, so that a release never comes before the move that ended the
    /// drag (DOC-1945).
    fn flush_moves(&mut self, connection: ConnectionId, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let mut waiting = Vec::new();
        state.moves.flush(microseconds(now), &mut |event| waiting.push(event));
        for event in waiting {
            self.tell_move(connection, event);
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
                description_text_characters: wide(protocol::DESCRIPTION_TEXT_CHARACTERS),
                description_limit_ms: DESCRIPTION_WAIT_MS,
            },
        };
        self.tell(&ServerMessage::Start(start));
        let open: Vec<ConnectionId> = self.connections.keys().copied().collect();
        for connection in open {
            self.announce(connection, now);
        }
    }

    /// The program ended or went: each connection is told to the next as it
    /// starts. A selection not yet answered keeps its deadline, so its viewer
    /// is told the channel is unavailable (REQ-1481).
    pub fn program_ended(&mut self) {
        self.program = false;
        self.answered = false;
        self.readiness_page = None;
        for state in self.connections.values_mut() {
            state.announced = false;
            state.replied = false;
            state.moves.clear();
        }
        for visitor in self.descriptions.forget() {
            self.effects.push(Effect::Described { visitor, answer: None });
        }
    }

    /// Asks the program something so that its answer shows it can answer
    /// ([`Channel::is_ready`]), for a host that reports readiness. With no
    /// viewer connected nothing else would bring an answer. `describe` is
    /// a question every program must already answer, and its answer is kept
    /// for the first visitor to that page, the one viewers arrive at.
    pub fn ask_whether_ready(&mut self) {
        let page = PageNumber(self.config.initial_page);
        self.readiness_page = Some(page.0);
        self.tell(&ServerMessage::Describe(Describe { page }));
    }

    /// A web visitor the host numbered `visitor` is at the address of `page`
    /// (REQ-1728). The answer comes as [`Effect::Described`], at once or
    /// within [`DESCRIPTION_WAIT_MS`], and is the same for anyone who asks.
    /// The program hears the page number and nothing else (REQ-1728 item 7):
    /// a reserved page, and a channel with no program, ask it nothing.
    pub fn describe(&mut self, visitor: VisitorId, page: PageNumber, now: u64) {
        if page.is_reserved() || !self.program {
            self.effects.push(Effect::Described { visitor, answer: None });
            return;
        }
        match self.descriptions.ask(page.0, visitor, now) {
            Asked::Answered(answer) => self.effects.push(Effect::Described { visitor, answer }),
            Asked::Waiting => {}
            // The program has this `describe` already, and answers it once.
            Asked::AskProgram if self.readiness_page == Some(page.0) => {}
            Asked::AskProgram => self.tell(&ServerMessage::Describe(Describe { page })),
        }
    }

    /// The program's `description`, kept and given to those who wait. One
    /// nobody asked for is refused, so a program cannot fill the cache; one
    /// asked for and late fills the entry that stood for it, for those after.
    fn described(&mut self, description: Description, now: u64) -> Result<(), Refusal> {
        let page = description.page.0;
        let for_readiness = self.readiness_page.take_if(|asked| *asked == page).is_some();
        if !for_readiness && !self.descriptions.awaited(page, now) {
            return Err(Refusal {
                message: Some(protocol::MessageType::Description),
                path: String::new(),
                rule: Rule::NotAwaited,
                detail: format!("no `describe` of {page:03X} is waiting"),
            });
        }
        let answer = public_page(description).map(Arc::new);
        for visitor in self.descriptions.answered(page, &answer, now) {
            self.effects.push(Effect::Described { visitor, answer: answer.clone() });
        }
        Ok(())
    }

    /// `connected` is held no longer: the receiver selected, cannot select,
    /// or did not in time. The selection, if any, then runs to its deadline.
    fn end_hold(&mut self, connection: ConnectionId, now: u64) {
        let due = self.answer_due(now);
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if state.first_selection_due.take().is_some() {
            state.ending = Ending::Due(state.ending.due().unwrap_or(due));
            self.announce(connection, now);
        }
    }

    /// When a selection made `now` must have ended: at the limit, or at once
    /// when no program is running, which nothing could answer (REQ-1481).
    const fn answer_due(&self, now: u64) -> u64 {
        if self.program { now.saturating_add(PROGRAM_REPLY_LIMIT_MS) } else { now }
    }

    /// Tells the program of `connection` with `connected`, awaiting its
    /// reply, if it runs and `connected` is not held (REQ-1471).
    fn announce(&mut self, connection: ConnectionId, now: u64) {
        if !self.program {
            return;
        }
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if state.first_selection_due.is_some() {
            return;
        }
        state.announced = true;
        state.replied = false;
        state.ending = Ending::Due(now.saturating_add(PROGRAM_REPLY_LIMIT_MS));
        let message = Connected {
            connection,
            session: state.session,
            request: state.request,
            cookies: state.cookies.clone(),
            selection: state.selection,
            path: state.selection.map(Selection::path),
            query: state.parameters.clone(),
        };
        self.tell(&ServerMessage::Connected(message));
    }

    /// A connection the host numbered `connection` opened for `session`,
    /// which it continues or starts (requirement 5.5), with the cookies its
    /// upgrade carried for the program ([`crate::session_cookie::program_cookies`]).
    /// The host never reuses a connection's number while the channel runs.
    /// Its program is told when its first selection comes, or
    /// [`FIRST_SELECTION_WAIT_MS`] passes.
    pub fn opened(&mut self, connection: ConnectionId, session: SessionId, cookies: Vec<CookiePair>, now: u64) {
        let state = self.sessions.entry(session).or_insert(Session { open: 0, waiting_since: None });
        if let Some(since) = state.waiting_since.take() {
            self.waiting.remove(&(since, session));
            self.stats.resumed();
        } else if state.open == 0 {
            self.stats.joined();
        }
        state.open = state.open.saturating_add(1);
        self.connections.insert(connection, Connection::new(session, cookies, now));
    }

    /// A connection closed: the program is given the releases of whatever
    /// it held, marked lost (REQ-207: the viewer cannot send them now, and
    /// a connection that resumes starts with nothing held), and told its
    /// selection left its page, and when it was its session's last, the
    /// session waits for its viewer for the channel's grace (requirement
    /// 5.5); the program is told it ended with `closed` only when the grace
    /// passes.
    pub fn closed(&mut self, connection: ConnectionId, now: u64) {
        self.flush_moves(connection, now);
        self.release_held(connection);
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
        if self.waiting.len() >= MOST_WAITING_SESSIONS
            && let Some((_, oldest)) = self.waiting.pop_first()
        {
            self.end_waiting(oldest);
        }
        self.waiting.insert((now, session));
    }

    /// A waiting session ends, its viewer not back: its program is told
    /// `closed` (requirement 5.5).
    fn end_waiting(&mut self, session: SessionId) {
        self.sessions.remove(&session);
        self.stats.expired();
        self.tell(&ServerMessage::Closed(Closed { session }));
    }

    /// A binary message from a connection: its input stream (DOC-1929).
    /// A message holds whole items (DOC-1929), so it is read alone: an
    /// item cut by its end is a protocol error, and the next message's bytes
    /// can never be read as the rest of it (DEC-1759 lesson 17). Input is
    /// acknowledged in its answer's frame, and alone only if nothing is
    /// published in [`ACKNOWLEDGEMENT_WAIT_MS`] from the first input that
    /// frame would have covered (REQ-1334).
    pub fn received(&mut self, connection: ConnectionId, bytes: &[u8], now: u64) {
        let Some(state) = self.connections.get(&connection) else { return };
        let acked = state.acked;
        if read_message(bytes, &mut |item| self.item(connection, &item, now)).is_err() {
            // A protocol error ends the session (DOC-1919).
            self.effects.push(Effect::Close { connection, code: CLOSE_BROKE_PROTOCOL });
            return;
        }
        let Some(state) = self.connections.get_mut(&connection) else { return };
        // A message of no input units is never acknowledged (DOC-1943).
        if state.acked != acked {
            state.acknowledgement_due.get_or_insert(now.saturating_add(ACKNOWLEDGEMENT_WAIT_MS));
        }
    }

    /// The time asked for has come: sessions whose grace passed end, and
    /// answers the program did not give in time are given up (REQ-1481).
    pub fn deadline(&mut self, now: u64) {
        let grace = self.config.session_grace_ms;
        while let Some(&(since, session)) = self.waiting.first()
            && since.saturating_add(grace) <= now
        {
            self.waiting.pop_first();
            self.end_waiting(session);
        }
        let held: Vec<ConnectionId> =
            self.connections.iter().filter(|(_, state)| state.first_selection_due.is_some_and(|due| due <= now)).map(|(&id, _)| id).collect();
        for connection in held {
            self.end_hold(connection, now);
        }
        let mut late: Vec<(ConnectionId, String)> = Vec::new();
        for (connection, state) in &mut self.connections {
            if state.ending.due().is_some_and(|due| due <= now) {
                state.ending = Ending::Answered;
                let unanswered = if state.replied { "no page for selected" } else { "no reply to connected" };
                late.push((*connection, format!("connection {connection}: {unanswered}")));
            }
        }
        // The viewer is told the channel is unavailable, and the connection
        // stays open (REQ-1481): DENIED carries status 503 (DEC-1730).
        for (connection, text) in late {
            self.log(Level::Warn, "program_late", format!("{text} within {PROGRAM_REPLY_LIMIT_MS} ms"));
            self.deny(connection, STATUS_UNAVAILABLE);
        }
        for visitor in self.descriptions.late(now) {
            self.effects.push(Effect::Described { visitor, answer: None });
        }
        let unacknowledged: Vec<ConnectionId> =
            self.connections.iter().filter(|(_, state)| state.acknowledgement_due.is_some_and(|due| due <= now)).map(|(&id, _)| id).collect();
        for connection in unacknowledged {
            self.acknowledge(connection);
        }
        let mut paced: Vec<(ConnectionId, Event<'static>)> = Vec::new();
        for (&connection, state) in &mut self.connections {
            state.moves.take_due(microseconds(now), &mut |event| paced.push((connection, event)));
        }
        for (connection, event) in paced {
            self.tell_move(connection, event);
        }
    }

    /// Tells `connection` its selection ended with `status` (DOC-1961):
    /// DENIED, naming what is selected, and the connection
    /// stays open.
    fn deny(&mut self, connection: ConnectionId, status: u16) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.ending = Ending::Denied;
        let place = state.selection.map(Selection::path).unwrap_or_default();
        let record = state.next_record();
        let mut bytes = Vec::new();
        let denied = DeniedRecord { status, place: &place };
        if encode_denied(denied, SERVICE, record, &mut |packet| bytes.extend_from_slice(&packet.bytes)).is_ok() {
            self.send(connection, Arc::from(bytes));
        }
    }

    /// When the channel next needs [`Channel::deadline`].
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        let grace = self.config.session_grace_ms;
        let expiry = self.waiting.first().map(|&(since, _)| since.saturating_add(grace));
        let dues = self.connections.values().flat_map(|state| state.ending.due().into_iter().chain(state.first_selection_due).chain(state.acknowledgement_due));
        // A move's time is in microseconds: this is the first millisecond at
        // or after it.
        let moves = self.connections.values().filter_map(|state| state.moves.due()).map(|due| due.div_ceil(MICROSECONDS_PER_MILLISECOND));
        dues.chain(expiry).chain(moves).chain(self.descriptions.next_due()).min()
    }

    fn item(&mut self, connection: ConnectionId, item: &Item<'_>, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.acked = state.acked.wrapping_add(u16::try_from(units(item) & 0xFFFF).unwrap_or(0));
        if !input::is_move(item) {
            self.flush_moves(connection, now);
        }
        match *item {
            Item::Event(Event::Hello { version, caps, .. }) => self.welcome(connection, version, caps, now),
            Item::Event(Event::Select(select, parameters)) => self.selected(connection, select.page, select.sub, parameters, now),
            _ => self.input(connection, item, now),
        }
    }

    /// Input from a viewer, checked against the page on screen (REQ-1555,
    /// REQ-1557). What is stamped with the connection's selection goes to
    /// the program. A press or anything else stamped with another page is
    /// dropped and counted: the viewer aimed it at a page no longer
    /// selected. A release is never dropped, since it lets go of what the
    /// connection holds, and focus is always acted on (REQ-1553: neither
    /// touches the selection). Raw text, which cannot carry a stamp, is for
    /// the selection as it stands.
    fn input(&mut self, connection: ConnectionId, item: &Item<'_>, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let selection = state.selection;
        let stamp = input::stamp(item);
        let elsewhere = match stamp {
            Some(on) => selection.is_none_or(|selection| selection.page.0 != on.page || selection.sub.0 != on.sub),
            None => matches!(item, Item::Text(_)) && selection.is_none(),
        };
        if elsewhere && !input::releases(item) {
            self.stale_input(now);
            return;
        }
        if let Some(cell) = input::pointer_cell(item) {
            state.pointer = cell;
        }
        if let Item::Event(Event::Mouse(mouse)) = *item {
            if mouse.action == MouseAction::Gone {
                state.moves.forget(0);
            } else if mouse.action == MouseAction::Move {
                let offered = state.moves.offer(Event::Mouse(mouse), microseconds(now));
                count(&mut self.stats.inputs);
                if let Offer::Now(event) = offered {
                    self.tell_move(connection, event);
                }
                return;
            }
        }
        let before = state.held;
        let mut owed = Vec::new();
        state.held.take(item, &mut |released, on| owed.push((released, on)));
        let let_go = before != state.held;
        self.lost(connection, owed);
        // A late release of what a selection change already let go.
        if elsewhere && !let_go {
            return;
        }
        let Some(input) = Input::of(item) else { return };
        let Some(event) = input.event() else { return };
        let on = stamp.map(|on| (on.page, on.sub)).or_else(|| selection.map(|selection| (selection.page.0, selection.sub.0)));
        let Some((page, sub)) = on else { return };
        count(&mut self.stats.inputs);
        if matches!(input, Input::Key { action: Action::Press, .. } | Input::Text(_)) {
            count(&mut self.stats.keys);
        }
        let lost = input::marked_lost(item);
        self.tell(&ServerMessage::Input(InputMessage { connection, page: PageNumber(page), sub: protocol::SubCode(sub), lost, event }));
    }

    /// Input dropped for its stamp: counted, and the operator told at most
    /// once in [`STALE_INPUT_REPORT_SECONDS`] (REQ-1557).
    fn stale_input(&mut self, now: u64) {
        count(&mut self.stats.stale_input);
        count(&mut self.stale_unreported);
        if self.stale_reported_at.is_none_or(|at| at.saturating_add(STALE_INPUT_REPORT_MS) <= now) {
            self.stale_reported_at = Some(now);
            let dropped = core::mem::take(&mut self.stale_unreported);
            self.log(Level::Warn, "stale_input", format!("dropped {dropped} input events stamped with a page no longer selected"));
        }
    }

    /// Everything `connection` holds is let go: the page lost the input.
    fn release_held(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let mut owed = Vec::new();
        state.held.release_all(&mut |released, on| owed.push((released, on)));
        self.lost(connection, owed);
    }

    /// Gives the program the releases `owed`, marked lost, each for the page
    /// that held it (REQ-207).
    fn lost(&mut self, connection: ConnectionId, owed: Vec<(Released, Stamp)>) {
        let Some(at) = self.connections.get(&connection).map(|state| state.pointer) else { return };
        for (released, on) in owed {
            let Some(event) = input::lost_release(released, at) else { continue };
            count(&mut self.stats.inputs);
            self.tell(&ServerMessage::Input(InputMessage { connection, page: PageNumber(on.page), sub: protocol::SubCode(on.sub), lost: true, event }));
        }
    }

    /// HELLO: WELCOME, or a close the receiver stops on when it speaks
    /// another version (DOC-1921). A receiver that cannot send
    /// SELECT is not waited for. Its capabilities are the only thing asked of
    /// it: nothing here reads its agent text (DEC-1759 lesson 19).
    fn welcome(&mut self, connection: ConnectionId, version: u32, caps: u32, now: u64) {
        if version != neoteletext_input::VERSION {
            self.effects.push(Effect::Close { connection, code: CLOSE_WRONG_VERSION });
            return;
        }
        if caps & SELECT_CAP == 0 {
            self.end_hold(connection, now);
        }
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let record = state.next_record();
        if let Ok(packet) = packet_of(&Control::Welcome { version, caps: caps & USED, resume: None }, SERVICE, record) {
            state.welcomed = true;
            self.send(connection, Arc::from(&packet.bytes[..]));
        }
    }

    /// SELECT (DOC-1753, REQ-INT-140): the tab selected a page
    /// with 3F7F, which asks again for its first sub-page even when the tab
    /// shows another of the page (DEC-1526 item 3), or one sub-page, or,
    /// with page 0, no page of the channel. The engine's decoder has
    /// checked the parameters.
    fn selected(&mut self, connection: ConnectionId, page: u16, sub: u16, parameters: &str, now: u64) {
        if page == 0 {
            // Selecting nothing is an answer to the wait for a first selection.
            self.end_hold(connection, now);
            self.release_held(connection);
            let Some(state) = self.connections.get_mut(&connection) else { return };
            let Some(before) = state.selection.take() else { return };
            state.parameters = None;
            state.shown = None;
            state.ending = Ending::Answered;
            if state.announced {
                self.tell(&ServerMessage::Left(Left { connection, page: before.page }));
            }
            return;
        }
        let parameters = (!parameters.is_empty()).then(|| parameters.into());
        self.select(connection, Selection { page: PageNumber(page), sub: protocol::SubCode(sub) }, parameters, now);
    }

    /// The connection selected a page (sub-code 3F7F) or one sub-page: the
    /// program is told it left the page it had, and asked for the new one
    /// (REQ-1472), after the releases of whatever it held (REQ-207). What was
    /// kept of the sub-page shown is dropped, so the first sub-page after a
    /// new selection goes whole (DEC-1499). Parameters belong to one
    /// selection, so the same page with others is a new selection, and one
    /// with none leaves them behind (REQ-1693 item 6). A selection has a
    /// number and a deadline from here, whether or not the program runs. The
    /// first one ends the wait for it, and goes in `connected`.
    ///
    /// Repeating a selection that stands is no new selection (DOC-1753),
    /// but one DENIED ended has nothing standing to wait on, so
    /// the repeat is a new one: every selection has an ending (DOC-1921).
    fn select(&mut self, connection: ConnectionId, selection: Selection, parameters: Option<String>, now: u64) {
        if self
            .connections
            .get(&connection)
            .is_none_or(|state| state.ending != Ending::Denied && state.selection == Some(selection) && state.parameters == parameters)
        {
            return;
        }
        self.release_held(connection);
        let due = self.answer_due(now);
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.ending = Ending::Due(due);
        let before = state.selection.replace(selection);
        state.parameters.clone_from(&parameters);
        state.shown = None;
        let announced = state.announced;
        if announced {
            state.request = state.request.saturating_add(1);
        }
        let request = state.request;
        self.stats.opened(selection.page.0);
        if !announced {
            // The program, when it is told, is told of it in `connected`.
            self.end_hold(connection, now);
            return;
        }
        if let Some(before) = before
            && before.page != selection.page
        {
            self.tell(&ServerMessage::Left(Left { connection, page: before.page }));
        }
        let path = selection.path();
        self.tell(&ServerMessage::Selected(Selected { connection, request, path, page: selection.page, sub: selection.sub, query: parameters }));
    }

    /// The cursor record says what was acted on, alone: nothing was published
    /// to carry it.
    fn acknowledge(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        state.acknowledgement_due = None;
        if !state.welcomed {
            return;
        }
        let page = state.selection.map_or(0, |selection| selection.page.0);
        if let Some(bytes) = state.cursor_packet(page) {
            self.send(connection, bytes);
        }
    }

    /// A line from the channel's program, without its newline: read,
    /// checked and carried out whole, or refused whole with `refused`
    /// (REQ-1475).
    pub fn from_program(&mut self, line: &[u8], now: u64) {
        count(&mut self.stats.lines_from_application);
        let acted = protocol::read(line).and_then(|message| {
            let answer = message.is_answer();
            self.act(message, now)?;
            self.answered |= answer;
            Ok(())
        });
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
            ProgramMessage::Reply(reply) => self.reply(reply)?,
            ProgramMessage::Page(message) => {
                let targets = self.targets(&message.connections);
                self.send_page(&targets, message.request, &message.page);
            }
            ProgramMessage::Subpages(list) => {
                let page = list.page.0;
                if list.request.is_some() && !self.answers_targets(&list.connections, list.request) {
                    return Ok(());
                }
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
            ProgramMessage::Redirect(redirect) => self.redirect(redirect, now)?,
            ProgramMessage::Description(description) => self.described(description, now)?,
            ProgramMessage::Denied(denied) => {
                if self.is_current(denied.connection, denied.request) {
                    self.deny(denied.connection, denied.status);
                }
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

    /// `reply`, the answer to `connected` (REQ-1471).
    fn reply(&mut self, reply: protocol::Reply) -> Result<(), Refusal> {
        let kind = protocol::MessageType::Reply;
        let refuse = |rule: Rule, detail: String| Refusal { message: Some(kind), path: "connection".to_string(), rule, detail };
        let Some(state) = self.connections.get_mut(&reply.connection) else {
            return Err(refuse(Rule::UnknownConnection, format!("{} is not open", reply.connection)));
        };
        if !state.announced || state.replied {
            return Err(refuse(Rule::NotAwaited, format!("{}'s connected was answered already", reply.connection)));
        }
        state.replied = true;
        // A reply to a selection replaced still gives the index; its
        // sub-page answers nothing, and the newer selection stands.
        let current = reply.request == state.request;
        // A connection with no selection asked for the initial page: the
        // sub-page given is its selection's.
        if let Some(page) = &reply.selected
            && current
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
            self.send_page(&[reply.connection], Some(reply.request), page);
        }
        Ok(())
    }

    /// `redirect`: a page redirect is itself a selection (DEC-1526 item 4):
    /// the receiver goes to the record's page, and the program is asked for
    /// it. A redirect to another channel changes no selection: the receiver
    /// leaves, or stays, by its own rules (REQ-1688), and it is the
    /// selection's answer.
    fn redirect(&mut self, redirect: protocol::Redirect, now: u64) -> Result<(), Refusal> {
        if !self.is_current(redirect.connection, redirect.request) {
            return Ok(());
        }
        self.send_records(&[redirect.connection], protocol::MessageType::Redirect, |state| records::redirect(&redirect, &mut state.record_id))?;
        count(&mut self.stats.redirects);
        match (redirect.page, redirect.sub, redirect.address) {
            (Some(page), Some(sub), _) => self.select(redirect.connection, Selection { page, sub }, redirect.query, now),
            (_, _, Some(address)) => {
                if let Some(state) = self.connections.get_mut(&redirect.connection) {
                    state.ending = Ending::Answered;
                }
                self.log(Level::Info, "redirect", format!("connection {} sent to {address}", redirect.connection));
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether `request` is `connection`'s selection now, and the connection
    /// open. An answer to a selection replaced is dropped and counted: the
    /// program did nothing wrong, the viewer moved on (DEC-1759 lesson 13).
    fn is_current(&mut self, connection: ConnectionId, request: RequestNumber) -> bool {
        let Some(state) = self.connections.get(&connection) else { return false };
        let current = state.request == request;
        if !current {
            count(&mut self.stats.stale_answers);
        }
        current
    }

    /// Whether a message numbered `request` names only connections it is
    /// current for: `check` has held it to one.
    fn answers_targets(&mut self, connections: &Connections, request: Option<RequestNumber>) -> bool {
        let (Connections::Listed(listed), Some(request)) = (connections, request) else { return true };
        listed.iter().all(|&connection| self.is_current(connection, request))
    }

    /// Sends the sub-page `page` to those of `targets` whose selection
    /// shows it: those that selected it, and those that selected its page
    /// with 3F7F, whose selection it then becomes (DEC-1526 item 3). Each
    /// is sent it whole, with the erase bit, unless its baseline is the
    /// same sub-code, when only what changed is sent (DEC-1499); every
    /// frame of a sub-page with Link Extension links is followed by its
    /// LINKS record (DEC-1517). It is encoded once, whatever the number of
    /// viewers.
    ///
    /// `request` is the selection it answers. An unnumbered sub-page is a
    /// change, for what connections already show, so one still awaiting its
    /// answer is not sent it: that answer will be current.
    fn send_page(&mut self, targets: &[ConnectionId], request: Option<RequestNumber>, page: &protocol::Page) {
        let encoded = crate::page::encode(page);
        let (number, sub) = (page.page.0, page.sub.0);
        let mut sends: Vec<(ConnectionId, Vec<u8>)> = Vec::new();
        let mut faults: Vec<String> = Vec::new();
        for &connection in targets {
            let answers = request.is_none_or(|request| self.is_current(connection, request));
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if !answers || (request.is_none() && state.ending.due().is_some()) || !state.shows(number, sub) {
                continue;
            }
            state.selection = Some(Selection { page: page.page, sub: page.sub });
            state.ending = Ending::Answered;
            let change = match &state.shown {
                Some(kept) if kept.page == number && kept.sub == sub => match crate::page::changes(&kept.packets, &encoded) {
                    // Links alone changed: a frame of the header without the
                    // erase bit, then its LINKS record, or none when every
                    // link was removed (DEC-1517).
                    Change::Nothing if kept.links != page.link_extension => crate::page::header_only(&encoded),
                    change => change,
                },
                _ => Change::Whole,
            };
            state.shown = Some(Shown { page: number, sub, packets: encoded.packets.clone(), cursor: encoded.cursor, links: page.link_extension.clone() });
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
                && let Some(cursor) = state.cursor_packet(number)
            {
                bytes.extend_from_slice(&cursor);
                state.acknowledgement_due = None;
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
/// What `description` shows the web: nothing unless it says public, which the
/// protocol's check has seen gives all three.
fn public_page(description: Description) -> Option<PublicPage> {
    let Description { public: true, title: Some(title), text: Some(text), content: Some(content), .. } = description else { return None };
    Some(PublicPage { title, text, picture: crate::page::picture(&content) })
}

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
/// none, with the input count. A place the record cannot hold is no cursor,
/// so that the count always has its record (REQ-1346).
fn cursor_of(page: u16, acked: u16, line: Option<&CursorLine>) -> Cursor {
    let on_rows = |line: &&CursorLine| (protocol::FIRST_CURSOR_ROW..=protocol::LAST_CURSOR_ROW).contains(&line.row);
    let shown = line.filter(on_rows);
    let place = shown.map(|line| Place { row: line.row, column: line.column });
    // Only BLOCK is a flag now: echo and masked echo are withdrawn (the
    // engine's E.134). No cursor takes no flags (DOC-1954).
    Cursor { page, acked, place, flags: shown.map_or(0, |line| line.flags & BLOCK) }
}

#[cfg(test)]
mod tests;
