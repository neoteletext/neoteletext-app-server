//! The page rule table on a channel (neoteletext-common
//! `docs/page-rules.md` RULE-37 to RULE-40, RULE-46 to RULE-48, RULE-53;
//! REQ-6532):
//!
//! - **Tables** (RULE-40): for each connection the newest table the
//!   program gave it for each of at most [`MAX_KEPT_TABLES`] pages. A
//!   connection is sent its page's RULES record when it selects the page,
//!   before anything of the page, and at once when the program gives or
//!   removes the table of the page it has selected. A page with no table
//!   of the program's has the channel's default table, or none (RULE-53).
//! - **Reports** (RULE-37): a receiver's `RULE_EVENT` becomes the program's
//!   `input`, with the rule's name, its tick and its target, or
//!   `rule_event`. Under push protocol 9 a viewer's input reaches the
//!   server in no other way (REQ-6523).
//! - **State** (RULE-38): `query_state` is sent as `RULES_QUERY` and
//!   answered by `PAGE_STATE` as `page_state`, or refused.
//! - **Faults** (RULE-48): `RULES_FAULT` becomes `rules_fault`.
//!
//! Numbers become names again here, by the table the connection was sent
//! at the generation a receiver's message names (RULE-36).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_input::event::{Action, Event, Item, MouseAction, Pick, Stamp};
use neoteletext_input::held::Released;
use neoteletext_input::rules::{FaultCode, PageStateEvent, RuleBody, RuleEvent as ReceiverRuleEvent, RulesFault as ReceiverRulesFault};
use neoteletext_rules::table::{Target, TargetKind};
use neoteletext_web::records::rules::{RulesQueryRecord, encode as encode_rules, encode_query};

use super::{Channel, Level, SERVICE};
use crate::page::address;
use crate::protocol::{
    self, ConnectionId, InputEvent, InputMessage, MessageType, PageNumber, PageState, QueryState, Refusal, Refused, Rule, RuleEvent, RuleEventBody, RuleName,
    RuleTarget, RulesFault, RulesFaultName, RulesMessage, ServerMessage, SubCode, TargetState, TimerState, ValueReading,
};
use crate::rules::{Compiled, MAX_KEPT_TABLES, no_table};
use crate::stats::count;

/// What a page has for a connection the program gave no table of its own
/// (RULE-53): `default_rules` of a channel's configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DefaultRules {
    /// No table: the page sends its channel no input.
    #[default]
    None,
    /// The default table, which sends every kind of input, as push
    /// protocol 8 sent it.
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
    /// The page tick at which it fired.
    tick: u32,
    /// The target the pointer was inside, when the rule named one.
    target: Option<RuleTarget>,
}

impl Sender {
    /// The `input` message of `event`, which this rule sent.
    pub(super) fn input(&self, connection: ConnectionId, page: PageNumber, sub: SubCode, lost: bool, event: InputEvent) -> InputMessage {
        InputMessage { connection, page, sub, lost, rule: self.rule.clone(), tick: self.tick, target: self.target.clone(), event }
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
    generation: u16,
    table: Option<Arc<Compiled>>,
}

/// A `query_state` sent and not yet answered.
struct Query {
    /// The identifier in the `RULES_QUERY` record.
    identifier: u32,
    page: u16,
    /// The program's own number for it.
    request: u32,
    /// When it is refused `not_answered`.
    due: u64,
}

/// One connection's rule tables (RULE-40).
#[derive(Default)]
pub(super) struct PageTables {
    /// The newest table the program gave it for each page, the oldest
    /// first: at most [`MAX_KEPT_TABLES`].
    pub(super) kept: Vec<(u16, Arc<Compiled>)>,
    /// The generation of the last RULES record it was sent for each page.
    pub(super) generations: BTreeMap<u16, u16>,
    /// The records last sent, the oldest first.
    sent: Vec<Sent>,
    /// The page whose record is in place for its selection.
    placed: Option<u16>,
    /// Its state queries not yet answered.
    queries: Vec<Query>,
    /// When it was last asked each page's state.
    asked: Vec<(u16, u64)>,
    /// The last `RULES_QUERY` identifier it was sent.
    pub(super) identifier: u32,
}

impl PageTables {
    /// The table the program gave it for `page`.
    pub(super) fn kept(&self, page: u16) -> Option<&Arc<Compiled>> {
        self.kept.iter().find(|(held, _)| *held == page).map(|(_, table)| table)
    }

    /// Keeps `table` as its newest for `page`, or with none forgets the
    /// page's. Whether anything changed.
    fn keep(&mut self, page: u16, table: Option<&Arc<Compiled>>) -> bool {
        let same = match (self.kept(page), table) {
            (Some(held), Some(given)) => held.is_same(given),
            (None, None) => true,
            _ => false,
        };
        self.kept.retain(|(held, _)| *held != page);
        if let Some(table) = table {
            if self.kept.len() >= MAX_KEPT_TABLES {
                self.kept.remove(0);
            }
            self.kept.push((page, Arc::clone(table)));
        }
        !same
    }

    /// The RULES record that puts `page`'s table in place, or with none of
    /// the program's the `default`, or with neither the record that says
    /// the page has no table (RULE-40, RULE-53): numbered one above the
    /// last it was sent for the page, from 1, and never 0.
    fn record(&mut self, page: u16, default: Option<&Arc<Compiled>>, record_id: &mut u8) -> Vec<u8> {
        let table = self.kept(page).or(default).cloned();
        let generation = self.generations.get(&page).map_or(1, |last| last.checked_add(1).unwrap_or(1));
        let body = table.as_ref().map_or_else(|| no_table(page, generation), |table| table.body(page, generation));
        let mut bytes = Vec::new();
        if encode_rules(&body, SERVICE, *record_id, &mut |packet| bytes.extend_from_slice(&packet.bytes)).is_err() {
            return Vec::new();
        }
        *record_id = record_id.wrapping_add(1);
        self.generations.insert(page, generation);
        if self.sent.len() >= SENT_TABLES_KEPT {
            self.sent.remove(0);
        }
        self.sent.push(Sent { page, generation, table });
        self.placed = Some(page);
        bytes
    }

    /// The record for `page`, unless it is in place already.
    pub(super) fn place(&mut self, page: u16, default: Option<&Arc<Compiled>>, record_id: &mut u8) -> Vec<u8> {
        if self.placed == Some(page) { Vec::new() } else { self.record(page, default, record_id) }
    }

    /// The selection left its page: its table is in place no longer.
    pub(super) const fn left(&mut self) {
        self.placed = None;
    }

    /// The record it was sent for `page` at `generation`.
    fn sent(&self, page: u16, generation: u16) -> Option<&Sent> {
        self.sent.iter().rev().find(|sent| sent.page == page && sent.generation == generation)
    }

    /// When its earliest state query is refused.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.queries.iter().map(|query| query.due).min()
    }
}

impl Channel {
    /// Sends `connection` the RULES record of `page`, unless it is in
    /// place: when it selects the page (RULE-40).
    pub(super) fn place_rules(&mut self, connection: ConnectionId, page: u16) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if state.rules.placed.is_some_and(|placed| placed != page) {
            state.rules.left();
        }
        let bytes = state.rules.place(page, self.default_table.as_ref(), &mut state.record_id);
        if !bytes.is_empty() {
            self.send(connection, Arc::from(bytes));
        }
    }

    /// `rules`: a page's table for the connections named, or with null its
    /// removal (RULE-35, RULE-40). Each keeps it, and one that has the page
    /// selected is sent it at once.
    pub(super) fn rules(&mut self, message: &RulesMessage) -> Result<(), Refusal> {
        let table = protocol::check::compiled(message)?.map(Arc::new);
        let page = message.page.0;
        for connection in self.targets(&message.connections) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let changed = state.rules.keep(page, table.as_ref());
            // The same table given again changes nothing a receiver holds:
            // sending it would reset the page's variables and timers and
            // fire its `open` rules again (RULE-2).
            if !changed || state.rules.placed != Some(page) {
                continue;
            }
            let bytes = state.rules.record(page, self.default_table.as_ref(), &mut state.record_id);
            if !bytes.is_empty() {
                self.send(connection, Arc::from(bytes));
            }
        }
        Ok(())
    }

    /// `query_state` (RULE-38): a `RULES_QUERY` record to the connection,
    /// which must have the page selected, at most once a second for the
    /// page.
    pub(super) fn query_state(&mut self, query: QueryState, now: u64) -> Result<(), Refusal> {
        let refuse = |path: &str, rule: Rule, detail: String| Refusal { message: Some(MessageType::QueryState), path: path.into(), rule, detail };
        let (connection, page) = (query.connection, query.page.0);
        let Some(state) = self.connections.get_mut(&connection) else {
            return Err(refuse("connection", Rule::UnknownConnection, format!("{connection} is not open")));
        };
        if state.selection.is_none_or(|selection| selection.page.0 != page) {
            return Err(refuse("page", Rule::WrongPage, format!("connection {connection} has not selected {page:03X}")));
        }
        let tables = &mut state.rules;
        tables.asked.retain(|(_, at)| at.saturating_add(STATE_QUERY_INTERVAL_MS) > now);
        if tables.asked.iter().any(|(asked, _)| *asked == page) {
            return Err(refuse("", Rule::TooFrequent, format!("{page:03X} was asked of connection {connection} within {STATE_QUERY_INTERVAL_MS} ms")));
        }
        tables.identifier = tables.identifier.wrapping_add(1);
        let identifier = tables.identifier;
        let mut bytes = Vec::new();
        let record = RulesQueryRecord { page: address(page), identifier };
        encode_query(record, SERVICE, state.record_id, &mut |packet| bytes.extend_from_slice(&packet.bytes))
            .map_err(|fault| refuse("", Rule::WrongType, format!("{fault:?}")))?;
        state.record_id = state.record_id.wrapping_add(1);
        let tables = &mut state.rules;
        tables.asked.push((page, now));
        let request = u32::try_from(query.request.get()).unwrap_or(u32::MAX);
        tables.queries.push(Query { identifier, page, request, due: now.saturating_add(STATE_ANSWER_LIMIT_MS) });
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
            detail: format!("connection {connection} gave no state of {:03X}: {why}", query.page),
            request: Some(u64::from(query.request)),
            sound: None,
        }));
    }

    /// The state queries past their time are refused (RULE-38).
    pub(super) fn late_queries(&mut self, now: u64) {
        let mut late = Vec::new();
        for (&connection, state) in &mut self.connections {
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
    pub(super) fn unanswered_queries(&mut self, connection: ConnectionId, stays: Option<u16>) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let (kept, gone) = core::mem::take(&mut state.rules.queries).into_iter().partition(|query| Some(query.page) == stays);
        state.rules.queries = kept;
        for query in Vec::into_iter(gone) {
            self.not_answered(connection, &query, "the connection ended or left the page");
        }
    }

    /// `RULE_EVENT` (RULE-37, RULE-46): a rule's `send` or `echo`. `carried`
    /// is its input event, when it has one, with the PICK that follows a
    /// pointer event on a page drawn in ReGIS.
    pub(super) fn rule_event(&mut self, connection: ConnectionId, event: &ReceiverRuleEvent<'_>, carried: Option<(Item<'_>, Option<Pick>)>, now: u64) {
        let Some(state) = self.connections.get(&connection) else { return };
        // The names are those of the table the receiver holds: the record
        // it was sent at the generation the report names.
        let named = state.rules.sent(event.on.page, event.generation).and_then(|sent| sent.table.as_ref()).and_then(|table| {
            let rule = table.rule(usize::from(event.rule))?;
            let target = if event.target == Target::NONE { None } else { Some(table.target(event.target)?) };
            Some((Arc::clone(table), rule, target))
        });
        let Some((table, rule, target)) = named else {
            // A report of a table the connection no longer has, or was
            // never sent: dropped and counted, as input for a page no
            // longer selected is.
            self.stale_input(now);
            return;
        };
        let body = match event.body {
            RuleBody::Input(_) => {
                let Some((item, pick)) = carried else { return };
                let sender = Sender { rule, tick: event.tick, target };
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
        let Stamp { page, sub } = event.on;
        self.tell(&ServerMessage::RuleEvent(RuleEvent { connection, page: PageNumber(page), sub: SubCode(sub), rule, tick: event.tick, event: body }));
    }

    /// `PAGE_STATE` (RULE-38, RULE-47): the answer to a state query, given
    /// to the program with every number as the name its table gave it.
    pub(super) fn page_state(&mut self, connection: ConnectionId, answer: &PageStateEvent<'_>) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(reported) = answer.state() else { return };
        let Some(place) = state.rules.queries.iter().position(|query| query.identifier == reported.identifier && query.page == answer.on.page) else {
            // An answer to nothing asked, or asked and already refused.
            self.log(Level::Debug, "page_state_unasked", format!("connection {connection} answered a state query that is not waiting"));
            return;
        };
        let query = state.rules.queries.remove(place);
        let (page, sub) = (PageNumber(answer.on.page), SubCode(answer.on.sub));
        let mut message = PageState {
            connection,
            page,
            sub,
            request: query.request,
            tick: 0,
            table: false,
            variables: BTreeMap::new(),
            timers: BTreeMap::new(),
            enabled: Vec::new(),
            targets: Vec::new(),
            values: Vec::new(),
        };
        if let Some(held) = reported.table {
            let Some(table) = state.rules.sent(query.page, held.generation).and_then(|sent| sent.table.clone()) else {
                // The receiver holds a table the server did not send it at
                // that generation: nothing in the answer can be named.
                self.not_answered(connection, &query, "it answered for a table it was not sent");
                return;
            };
            let names = table.names();
            let name = |name: &String| RuleName(name.clone());
            message.table = true;
            message.tick = held.tick;
            message.variables = names.variables.iter().map(name).zip(held.variables.iter().copied()).collect();
            message.timers =
                names.timers.iter().map(name).zip(held.timers.iter().map(|timer| TimerState { running: timer.running, elapsed: timer.elapsed })).collect();
            message.enabled = names
                .rules
                .iter()
                .zip(0_u32..)
                .filter(|(_, bit)| 1_u64.checked_shl(*bit).is_some_and(|bit| held.enabled & bit != 0))
                .map(|(rule, _)| name(rule))
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
                        look: crate::rules::look_name(report.look),
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
            FaultCode::TickOverrun => RulesFaultName::TickOverrun,
            FaultCode::SendsDropped => RulesFaultName::SendsDropped,
            FaultCode::EventsDropped => RulesFaultName::EventsDropped,
            FaultCode::ActionsSkipped => RulesFaultName::ActionsSkipped,
            FaultCode::QueryDropped => RulesFaultName::QueryDropped,
            FaultCode::LocalOverrun => RulesFaultName::LocalOverrun,
            FaultCode::AnimationSlowed => RulesFaultName::AnimationSlowed,
        };
        let page = fault.on.page;
        if name == RulesFaultName::Undecodable {
            self.log(
                Level::Error,
                "rules_undecodable",
                format!(
                    "connection {connection} could not decode the rule table of {page:03X} at generation {}: the page has no table there",
                    fault.generation
                ),
            );
        }
        self.tell(&ServerMessage::RulesFault(RulesFault { connection, page: PageNumber(page), generation: fault.generation, fault: name, count: fault.count }));
    }
}

#[cfg(test)]
impl Channel {
    /// The generation of the RULES record `connection` was last sent for
    /// `page`: what a receiver's reports of that page's table carry.
    pub(super) fn rules_generation(&self, connection: ConnectionId, page: u16) -> Option<u16> {
        self.connections.get(&connection)?.rules.generations.get(&page).copied()
    }

    /// What `connection` has selected: the stamp of what it sends.
    pub(super) fn selection_of(&self, connection: ConnectionId) -> Option<Stamp> {
        let selection = self.connections.get(&connection)?.selection?;
        Some(Stamp { page: selection.page.0, sub: selection.sub.0 })
    }
}
