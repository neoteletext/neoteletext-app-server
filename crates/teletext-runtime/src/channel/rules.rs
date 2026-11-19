//! The page rule table on a channel, in its second form (neoteletext-common
//! `docs/page-rules.md` RULE-37 to RULE-40, RULE-46 to RULE-48, RULE-53,
//! RULE-111, RULE-112, RULE-114; REQ-6532, REQ-6615; DEC-6587, DEC-6599):
//!
//! - **Tables** (RULE-40): a table is sent at once to the connections of its
//!   audience that have its page (page number and sub-code) selected, and
//!   kept for none. At every selection, a change of sub-code among them,
//!   the connection is sent the channel's default table (RULE-53) or the
//!   RULES record that says the page has no table, before the program is
//!   told; the program's own table then replaces it.
//! - **What is kept for a connection**: the page whose record is in place,
//!   the names of the last four tables it was sent (by which its reports
//!   are read), one generation counter for the connection, and its state
//!   queries. All of it goes when the connection ends.
//! - **Commands** (RULE-114): `local_midi`, `variable`, `timer`, `report`,
//!   `run` and `sequence`, each one record, to the connections that have
//!   the page selected.
//! - **Reports** (RULE-37): a receiver's `RULE_EVENT` becomes the program's
//!   `input`, with the rule's name, its row, its tick and its target, or
//!   `rule_event`. A viewer's input reaches the server in no other way
//!   (REQ-6523).
//! - **State** (RULE-38): `query_state` is sent as `RULES_QUERY` and
//!   answered by `PAGE_STATE` as `page_state`, or refused.
//! - **Faults** (RULE-48): `RULES_FAULT` becomes `rules_fault`.
//!
//! Numbers become names again here, by the table the connection was sent
//! at the generation a receiver's message names (RULE-36).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_core::limits::SUBCODE_ANY;
use neoteletext_input::event::{Action, Event, Item, MouseAction, Pick, Stamp};
use neoteletext_input::held::Released;
use neoteletext_input::rules::{FaultCode, PageStateEvent, RuleBody, RuleEvent as ReceiverRuleEvent, RulesFault as ReceiverRulesFault};
use neoteletext_rules::limits::{NO_RULE, TIMERS};
use neoteletext_rules::names::TIMER_NAMES;
use neoteletext_rules::table::{Target, TargetKind};
use neoteletext_web::records::rules::{RulesQueryRecord, encode_query};

use super::{Channel, Level, SERVICE};
use crate::page::address;
use crate::protocol::{
    self, CommandMessage, ConnectionId, InputEvent, InputMessage, MessageType, PageNumber, PageState, ProgramMessage, QueryState, Refusal, Refused, Rule,
    RuleEvent, RuleEventBody, RuleName, RuleTarget, RulesFault, RulesFaultName, RulesMessage, SequenceState, ServerMessage, SubCode, TargetState, TimerState,
    ValueReading,
};
use crate::rules::{self as compiler, Compiled};
use crate::stats::count;

/// What a page has for a connection the program gave no table of its own
/// (RULE-53): `default_rules` of a channel's configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DefaultRules {
    /// No table: the page sends its channel no input.
    #[default]
    None,
    /// The default table, which sends every kind of input, as push
    /// protocol 8 sent it (RULE-53).
    SendAll,
}

/// How long a `query_state` waits for its answer before it is refused
/// `not_answered` (RULE-38).
pub const STATE_ANSWER_LIMIT_MS: u64 = 10_000;
/// The least time between two `query_state` for one connection and page
/// (RULE-38).
pub const STATE_QUERY_INTERVAL_MS: u64 = 1_000;
/// The tables last sent that a connection's reports are still read by: a
/// report may name the table before the one in force, or the table of the
/// page the connection just left.
const SENT_TABLES_KEPT: usize = 4;
/// The most things held whose press's rule is kept: every key the
/// receivers' evaluator tracks as down (RULE-57, `MAX_KEYS_DOWN`), three
/// buttons and every contact.
const MOST_PRESSES_KEPT: usize = neoteletext_rules::limits::MAX_KEYS_DOWN + 3 + neoteletext_input::MAX_CONTACTS;

/// The rule that sent an input event, as `input` names it (RULE-37).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct Sender {
    /// The rule's name.
    rule: RuleName,
    /// The place of the firing row within the rule's rows, when it has
    /// rows.
    row: Option<u8>,
    /// The page tick at which it fired.
    tick: u32,
    /// The target the pointer was inside, when the rule named one.
    target: Option<RuleTarget>,
}

impl Sender {
    /// The `input` message of `event`, which this rule sent.
    pub(super) fn input(&self, connection: ConnectionId, page: PageNumber, sub: SubCode, lost: bool, event: InputEvent) -> InputMessage {
        InputMessage { connection, page, sub, lost, rule: self.rule.clone(), row: self.row, tick: self.tick, target: self.target.clone(), event }
    }
}

/// Keeps `sender` as the rule of `contact`'s latest move.
pub(super) fn keep(moved_by: &mut Vec<(u8, Sender)>, contact: u8, sender: &Sender) {
    moved_by.retain(|(held, _)| *held != contact);
    moved_by.push((contact, sender.clone()));
}

/// What an item presses or lets go of, and whether it presses it.
fn held_of(item: &Item<'_>) -> Option<(Released, bool)> {
    match item {
        Item::Event(Event::Key(key)) => match key.action {
            Action::Press => Some((Released::Key(key.key), true)),
            Action::Release => Some((Released::Key(key.key), false)),
            Action::Repeat => None,
        },
        Item::Event(Event::Mouse(mouse)) => match mouse.action {
            MouseAction::Press => Some((Released::Button(mouse.button), true)),
            MouseAction::Release => Some((Released::Button(mouse.button), false)),
            _ => None,
        },
        Item::Event(Event::Touch(touch)) => match touch.action {
            MouseAction::Press => Some((Released::Contact(touch.id), true)),
            MouseAction::Release | MouseAction::Gone => Some((Released::Contact(touch.id), false)),
            MouseAction::Move => None,
        },
        _ => None,
    }
}

/// Follows `item` in the list of what is held and the rule that sent its
/// press: a press is kept, a release forgets it.
pub(super) fn held_by(pressed_by: &mut Vec<(Released, Sender)>, item: &Item<'_>, sender: &Sender) {
    let Some((held, pressed)) = held_of(item) else { return };
    pressed_by.retain(|(kept, _)| *kept != held);
    if pressed {
        if pressed_by.len() >= MOST_PRESSES_KEPT {
            pressed_by.remove(0);
        }
        pressed_by.push((held, sender.clone()));
    }
}

/// The rule that sent the press of `released`, forgotten.
pub(super) fn take_held(pressed_by: &mut Vec<(Released, Sender)>, released: Released) -> Option<Sender> {
    let place = pressed_by.iter().position(|(kept, _)| *kept == released)?;
    Some(pressed_by.remove(place).1)
}

/// A RULES record sent to a connection: the table a report of that
/// generation is read by; `None` when the record said the page has none.
struct Sent {
    page: u16,
    sub: u16,
    generation: u16,
    table: Option<Arc<Compiled>>,
}

/// A `query_state` sent and not yet answered.
struct Query {
    /// The identifier in the `RULES_QUERY` record.
    identifier: u32,
    page: u16,
    sub: u16,
    /// The program's own number for it.
    request: u32,
    /// When it is refused `not_answered`.
    due: u64,
}

/// One connection's rule tables (RULE-40, RULE-111): nothing of a page
/// but what its reports are read by.
#[derive(Default)]
pub(super) struct PageTables {
    /// The generation of the last RULES record sent on the connection, for
    /// any page: each record carries the next (RULE-40).
    generation: u16,
    /// The records last sent, the oldest first: at most
    /// [`SENT_TABLES_KEPT`].
    sent: Vec<Sent>,
    /// The page whose record is in place for its selection.
    placed: Option<(u16, u16)>,
    /// Its state queries not yet answered.
    queries: Vec<Query>,
    /// When it was last asked each page's state.
    asked: Vec<((u16, u16), u64)>,
    /// The last `RULES_QUERY` identifier it was sent.
    pub(super) identifier: u32,
}

impl PageTables {
    /// The next generation: one above the last sent on the connection, from
    /// 1, passing from 65,535 to 1 (RULE-40).
    fn next_generation(&mut self) -> u16 {
        self.generation = self.generation.checked_add(1).unwrap_or(1);
        self.generation
    }

    /// The RULES records that give `page` and `sub` `table`, or with none
    /// say it has no table, at the next generation, noted as sent.
    fn record(&mut self, (page, sub): (u16, u16), table: Option<Arc<Compiled>>, record_id: &mut u8) -> Vec<u8> {
        let generation = self.next_generation();
        let bytes = match &table {
            Some(table) => table.records(generation, record_id),
            None => compiler::no_table_records(page, sub, generation, record_id),
        };
        if self.sent.len() >= SENT_TABLES_KEPT {
            self.sent.remove(0);
        }
        self.sent.push(Sent { page, sub, generation, table });
        self.placed = Some((page, sub));
        bytes
    }

    /// The selection left its page: no record is in place.
    pub(super) const fn left(&mut self) {
        self.placed = None;
    }

    /// The record it was sent for `page` and `sub` at `generation`.
    fn sent(&self, page: u16, sub: u16, generation: u16) -> Option<&Sent> {
        self.sent.iter().rev().find(|sent| sent.page == page && sent.sub == sub && sent.generation == generation)
    }

    /// The table in force for the page in place, when it is `page` and
    /// `sub`: the last record sent for it.
    fn in_force(&self, page: u16, sub: u16) -> Option<&Arc<Compiled>> {
        if self.placed != Some((page, sub)) {
            return None;
        }
        self.sent.iter().rev().find(|sent| sent.page == page && sent.sub == sub).and_then(|sent| sent.table.as_ref())
    }

    /// When its earliest state query is refused.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.queries.iter().map(|query| query.due).min()
    }
}

impl Channel {
    /// The table a page has when its program gives it none (RULE-53): the
    /// default table, for a channel whose configuration says `send_all`.
    fn default_table(&self, page: u16, sub: u16) -> Option<Arc<Compiled>> {
        self.default_wheel.and_then(|wheel| Compiled::send_all(wheel, page, sub)).map(Arc::new)
    }

    /// Sends `connection` the default table or the empty RULES record of
    /// `page` and `sub`, unless that page's record is in place: when it
    /// selects the page, before the program is told (RULE-40). A selection
    /// of 3F7F, whose sub-page is not yet known, is placed when its first
    /// sub-page is sent.
    pub(super) fn place_rules(&mut self, connection: ConnectionId, page: u16, sub: u16) {
        let bytes = self.placed_rules(connection, page, sub);
        if !bytes.is_empty() {
            self.send(connection, Arc::from(bytes));
        }
    }

    /// The records of [`Channel::place_rules`], to send with what follows.
    pub(super) fn placed_rules(&mut self, connection: ConnectionId, page: u16, sub: u16) -> Vec<u8> {
        if sub == SUBCODE_ANY {
            return Vec::new();
        }
        let default = self.default_table(page, sub);
        let Some(state) = self.connections.get_mut(&connection) else { return Vec::new() };
        if state.rules.placed == Some((page, sub)) {
            return Vec::new();
        }
        state.rules.record((page, sub), default, &mut state.record_id)
    }

    /// The program's connection ended (RULE-111 case 3): each connection is
    /// sent, for the page it has selected, the default table or the record
    /// that says the page has none, which drops what the receiver held of
    /// the program's table; nothing is held over for the program's return.
    pub(super) fn tables_dropped(&mut self) {
        let placed: Vec<(ConnectionId, (u16, u16))> =
            self.connections.iter().filter_map(|(&connection, state)| state.rules.placed.map(|placed| (connection, placed))).collect();
        for (connection, (page, sub)) in placed {
            let default = self.default_table(page, sub);
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let bytes = state.rules.record((page, sub), default, &mut state.record_id);
            if !bytes.is_empty() {
                self.send(connection, Arc::from(bytes));
            }
        }
    }

    /// `rules`: a page's table for the connections named that have the
    /// page selected, or with null its removal, which puts the channel's
    /// default back (RULE-35, RULE-40, RULE-53). Nothing is kept for a
    /// connection that selects the page later.
    pub(super) fn rules(&mut self, message: &RulesMessage) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let table = protocol::check::compiled(message)?.map(Arc::new);
        let table = table.or_else(|| self.default_table(page, sub));
        for connection in self.showing(&message.connections, page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            // Its NAPLPS objects for the stream this connection has (ANIM-21).
            let table = table.as_ref().map(|table| table.for_naplps_generation(state.naplps_generation_for(page, sub)));
            let bytes = state.rules.record((page, sub), table, &mut state.record_id);
            // Inside a frame open for the page, it waits for the frame's
            // end with the rest (ANIM-2).
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// A command sent directly (RULE-114): one record to each connection
    /// named that has the page selected. A `run` or a `sequence` names a
    /// group or a sequence of the table in force there, and a connection
    /// whose table has no such name is sent nothing and named in the
    /// refusal (`unknown_name`), the others being sent it.
    pub(super) fn command(&mut self, message: &CommandMessage) -> Result<(), Refusal> {
        let (page, sub) = message.page();
        let kind = message.kind();
        let mut lacking: Vec<ConnectionId> = Vec::new();
        let mut detail = String::new();
        for connection in self.showing(message.connections(), page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let table = state.rules.in_force(page, sub).cloned();
            let bytes = match compiler::direct(message, table.as_deref(), &mut state.record_id) {
                Ok(bytes) => bytes,
                Err(refusal) if refusal.rule == Rule::UnknownName => {
                    lacking.push(connection);
                    detail = refusal.detail;
                    continue;
                }
                Err(refusal) => return Err(refusal),
            };
            self.send_for_page(connection, page, bytes);
        }
        if lacking.is_empty() {
            return Ok(());
        }
        Err(Refusal {
            message: Some(kind),
            path: message.name_path().into(),
            rule: Rule::UnknownName,
            detail: format!("{detail}, for connections {lacking:?}"),
        })
    }

    /// The messages of the rule table (RULE-35, RULE-38, RULE-114), and,
    /// beside them, those of animation.
    pub(super) fn act_on_rules(&mut self, message: ProgramMessage, now: u64) -> Result<(), Refusal> {
        match message {
            ProgramMessage::Rules(message) => self.rules(&message),
            ProgramMessage::QueryState(query) => self.query_state(query, now),
            ProgramMessage::LocalMidi(message) => self.command(&CommandMessage::LocalMidi(message)),
            ProgramMessage::Variable(message) => self.command(&CommandMessage::Variable(message)),
            ProgramMessage::Timer(message) => self.command(&CommandMessage::Timer(message)),
            ProgramMessage::Report(message) => self.command(&CommandMessage::Report(message)),
            ProgramMessage::Run(message) => self.command(&CommandMessage::Run(message)),
            ProgramMessage::Sequence(message) => self.command(&CommandMessage::Sequence(message)),
            ProgramMessage::ObjectPage(message) => self.object_page(&message),
            other => self.act_on_animation(other, now),
        }
    }

    /// `query_state` (RULE-38): a `RULES_QUERY` record to the connection,
    /// which must have the page selected, at most once a second for the
    /// page.
    pub(super) fn query_state(&mut self, query: QueryState, now: u64) -> Result<(), Refusal> {
        let refuse = |path: &str, rule: Rule, detail: String| Refusal { message: Some(MessageType::QueryState), path: path.into(), rule, detail };
        let (connection, page, sub) = (query.connection, query.page.0, query.sub.0);
        let Some(state) = self.connections.get_mut(&connection) else {
            return Err(refuse("connection", Rule::UnknownConnection, format!("{connection} is not open")));
        };
        if state.rules.placed != Some((page, sub)) {
            return Err(refuse("page", Rule::WrongPage, format!("connection {connection} has not selected {page:03X}/{sub:04X}")));
        }
        let tables = &mut state.rules;
        tables.asked.retain(|(_, at)| at.saturating_add(STATE_QUERY_INTERVAL_MS) > now);
        if tables.asked.iter().any(|(asked, _)| *asked == (page, sub)) {
            return Err(refuse(
                "",
                Rule::TooFrequent,
                format!("{page:03X}/{sub:04X} was asked of connection {connection} within {STATE_QUERY_INTERVAL_MS} ms"),
            ));
        }
        tables.identifier = tables.identifier.wrapping_add(1);
        let identifier = tables.identifier;
        let mut bytes = Vec::new();
        let record = RulesQueryRecord { page: address(page), subcode: neoteletext_core::types::SubCode::new(sub), identifier };
        encode_query(record, SERVICE, state.record_id, &mut |packet| bytes.extend_from_slice(&packet.bytes))
            .map_err(|fault| refuse("", Rule::WrongType, format!("{fault:?}")))?;
        state.record_id = state.record_id.wrapping_add(1);
        let tables = &mut state.rules;
        tables.asked.push(((page, sub), now));
        let request = u32::try_from(query.request.get()).unwrap_or(u32::MAX);
        tables.queries.push(Query { identifier, page, sub, request, due: now.saturating_add(STATE_ANSWER_LIMIT_MS) });
        self.send(connection, Arc::from(bytes));
        Ok(())
    }

    /// Tells the program a state query got no answer (RULE-38), so that a
    /// missing answer is never mistaken for an empty state.
    fn not_answered(&mut self, connection: ConnectionId, query: &Query, why: &str) {
        self.tell(&ServerMessage::Refused(Refused {
            message: Some(MessageType::QueryState),
            path: String::new(),
            rule: Rule::NotAnswered,
            detail: format!("connection {connection} gave no state of {:03X}/{:04X}: {why}", query.page, query.sub),
            request: Some(u64::from(query.request)),
            sound: None,
        }));
    }

    /// The state queries past their time are refused (RULE-38).
    pub(super) fn late_queries(&mut self, now: u64) {
        let mut late = Vec::new();
        for connection in self.connections.due(now) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let (due, waiting) = core::mem::take(&mut state.rules.queries).into_iter().partition(|query| query.due <= now);
            state.rules.queries = waiting;
            late.extend(Vec::into_iter(due).map(|query| (connection, query)));
        }
        for (connection, query) in late {
            self.not_answered(connection, &query, "it did not answer in time");
        }
    }

    /// The connection ended, or its selection left its page for `stays`
    /// (none when it selects nothing): the state queries of the pages it
    /// left are refused (RULE-38).
    pub(super) fn unanswered_queries(&mut self, connection: ConnectionId, stays: Option<(u16, u16)>) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let (kept, gone) = core::mem::take(&mut state.rules.queries).into_iter().partition(|query| Some((query.page, query.sub)) == stays);
        state.rules.queries = kept;
        for query in Vec::into_iter(gone) {
            self.not_answered(connection, &query, "the connection ended or left the page");
        }
    }

    /// `RULE_EVENT` (RULE-37, RULE-46): a `REPORT` a receiver applied.
    /// `carried` is its input event, when it has one, with the PICK that
    /// follows a pointer event on a page drawn in ReGIS.
    pub(super) fn rule_event(&mut self, connection: ConnectionId, event: &ReceiverRuleEvent<'_>, carried: Option<(Item<'_>, Option<Pick>)>, now: u64) {
        let Some(state) = self.connections.get(&connection) else { return };
        // The names are those of the table the receiver holds: the record
        // it was sent at the generation the report names.
        let Stamp { page, sub } = event.on;
        let Some(table) = state.rules.sent(page, sub, event.generation).and_then(|sent| sent.table.clone()) else {
            // A report of a table the connection no longer has, or was
            // never sent: dropped and counted, as input for a page no
            // longer selected is.
            self.stale_input(now);
            return;
        };
        let rule = if event.rule == NO_RULE { None } else { table.rule(usize::from(event.rule)) };
        if event.rule != NO_RULE && rule.is_none() {
            self.stale_input(now);
            return;
        }
        let row = (event.row != NO_RULE).then_some(event.row);
        let target = if event.target == Target::NONE { None } else { table.target(event.target) };
        let body = match event.body {
            RuleBody::Input(_) => {
                // A report of an input event is made only by a rule's run
                // (RULE-22).
                let (Some(rule), Some((item, pick))) = (rule, carried) else {
                    self.stale_input(now);
                    return;
                };
                let sender = Sender { rule, row, tick: event.tick, target };
                self.input(connection, &item, &sender, now);
                if let Some(pick) = pick {
                    self.input(connection, &Item::Event(Event::Pick(pick)), &sender, now);
                }
                return;
            }
            RuleBody::Fired => RuleEventBody::Fired,
            RuleBody::Value { value, reading } => {
                let Some(read) = table.value(value) else {
                    self.stale_input(now);
                    return;
                };
                RuleEventBody::Value { read, value: reading }
            }
        };
        count(&mut self.stats.inputs);
        self.tell(&ServerMessage::RuleEvent(RuleEvent { connection, page: PageNumber(page), sub: SubCode(sub), rule, row, tick: event.tick, event: body }));
    }

    /// `PAGE_STATE` (RULE-38, RULE-47): the answer to a state query, given
    /// to the program with every number as the name its table gave it.
    pub(super) fn page_state(&mut self, connection: ConnectionId, answer: &PageStateEvent<'_>) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(reported) = answer.state() else { return };
        let (page, sub) = (answer.on.page, answer.on.sub);
        let Some(place) = state.rules.queries.iter().position(|query| query.identifier == reported.identifier && query.page == page && query.sub == sub) else {
            // An answer to nothing asked, or asked and already refused.
            self.log(Level::Debug, "page_state_unasked", format!("connection {connection} answered a state query that is not waiting"));
            return;
        };
        let query = state.rules.queries.remove(place);
        let mut message = PageState {
            connection,
            page: PageNumber(page),
            sub: SubCode(sub),
            request: query.request,
            tick: 0,
            table: false,
            variables: alloc::vec![0; neoteletext_rules::limits::VARIABLES],
            timers: BTreeMap::new(),
            sequences: BTreeMap::new(),
            targets: Vec::new(),
            values: Vec::new(),
        };
        if let Some(held) = reported.table {
            let Some(table) = state.rules.sent(page, sub, held.generation).and_then(|sent| sent.table.clone()) else {
                // The receiver holds a table the server did not send it at
                // that generation: nothing in the answer can be named.
                self.not_answered(connection, &query, "it answered for a table it was not sent");
                return;
            };
            message.table = true;
            message.tick = held.tick;
            message.variables = held.variables.to_vec();
            message.timers = TIMER_NAMES
                .iter()
                .zip(held.timers.iter())
                .take(TIMERS)
                .filter(|(_, timer)| timer.running)
                .map(|(name, timer)| ((*name).to_string(), TimerState { running: timer.running, elapsed: timer.elapsed }))
                .collect();
            message.sequences = held
                .sequences
                .iter()
                .enumerate()
                .filter_map(|(number, sequence)| {
                    Some((
                        table.sequence_name(number)?,
                        SequenceState { running: sequence.running, passes: sequence.passes, position: sequence.position, waiting: sequence.waiting },
                    ))
                })
                .collect();
            message.targets = held
                .targets
                .iter()
                .filter(|report| matches!(report.target.kind, TargetKind::Control | TargetKind::Object))
                .filter_map(|report| {
                    Some(TargetState {
                        target: table.target(report.target)?,
                        exists: report.exists,
                        shown: report.shown,
                        look: compiler::look_name(report.look),
                        frame: report.frame,
                        x: report.x,
                        y: report.y,
                        layer: report.layer,
                        order: report.order,
                        cycling: report.cycling,
                    })
                })
                .collect();
            message.values = held.values.iter().filter_map(|(value, reading)| Some(ValueReading { read: table.value(*value)?, value: *reading })).collect();
        }
        self.tell(&ServerMessage::PageState(alloc::boxed::Box::new(message)));
    }

    /// `RULES_FAULT` (RULE-48): a receiver's report of a fault of a page's
    /// table, given to the program by name; a table the receiver could not
    /// decode is the server's and the receiver's disagreement, and the
    /// operator is told.
    pub(super) fn rules_fault(&mut self, connection: ConnectionId, fault: ReceiverRulesFault) {
        let name = match fault.fault {
            FaultCode::Undecodable => RulesFaultName::Undecodable,
            FaultCode::ReportsDropped => RulesFaultName::ReportsDropped,
            FaultCode::EventsDropped => RulesFaultName::EventsDropped,
            FaultCode::CommandsSkipped => RulesFaultName::CommandsSkipped,
            FaultCode::QueryDropped => RulesFaultName::QueryDropped,
            FaultCode::LocalOverrun => RulesFaultName::LocalOverrun,
            FaultCode::AnimationSlowed => RulesFaultName::AnimationSlowed,
            FaultCode::EvaluationLate => RulesFaultName::EvaluationLate,
            FaultCode::QueueFull => RulesFaultName::QueueFull,
        };
        let Stamp { page, sub } = fault.on;
        if name == RulesFaultName::Undecodable {
            self.log(
                Level::Error,
                "rules_undecodable",
                format!(
                    "connection {connection} could not decode the rule table of {page:03X}/{sub:04X} at generation {}: the page has no table there",
                    fault.generation
                ),
            );
        }
        self.tell(&ServerMessage::RulesFault(RulesFault {
            connection,
            page: PageNumber(page),
            sub: SubCode(sub),
            generation: fault.generation,
            fault: name,
            count: fault.count,
        }));
    }
}

#[cfg(test)]
impl Channel {
    /// The generation of the RULES record `connection` was last sent for
    /// `page` and `sub`: what a receiver's reports of that page's table
    /// carry.
    pub(super) fn rules_generation(&self, connection: ConnectionId, page: u16, sub: u16) -> Option<u16> {
        let state = self.connections.get(&connection)?;
        state.rules.sent.iter().rev().find(|sent| sent.page == page && sent.sub == sub).map(|sent| sent.generation)
    }

    /// What `connection` has selected: the stamp of what it sends.
    pub(super) fn selection_of(&self, connection: ConnectionId) -> Option<Stamp> {
        let selection = self.connections.get(&connection)?.selection?;
        Some(Stamp { page: selection.page.0, sub: selection.sub.0 })
    }
}
