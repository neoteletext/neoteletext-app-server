//! Viewer state on a channel (neoteletext-common's `docs/viewer-state.md`,
//! adopted by DEC-6471): the challenges a connection is sent, the proofs
//! and answers it sends back, the table of the viewers the channel knows,
//! and what the program is told and may ask.
//!
//! - **The exchange** (VS-202, VS-205): every connection is challenged once
//!   after WELCOME, without `create`, and `connected` waits for the answer
//!   or [`STATE_ANSWER_MS`]. Later challenges come only from a program's
//!   `request_state` (with `create`, and only within [`STATE_GESTURE_MS`]
//!   of the viewer's input, VS-212), from the receiver's `STATE_OFFER`, or
//!   from a rotation of the handle secret; at most one is outstanding, and
//!   at most [`MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE`] are sent.
//! - **What is kept** (VS-302, VS-303): a key identifier, never a key, a
//!   proof or a challenge once answered; the table is in memory only,
//!   bounded at [`MOST_VIEWERS_PER_CHANNEL`], and an entry goes
//!   [`VIEWER_IDLE_MS`] after its last connection closed.
//! - **What the program is told**: a handle (VS-305), never anything else
//!   of the mechanism; and nothing of it is logged (VS-209).

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::net::IpAddr;

use neoteletext_input::event::Event;
use neoteletext_input::state::{Lifetime, NoneReason, StateProof};
use neoteletext_web::records::state::{StateChallenge, StateEnd, encode_challenge, encode_end, encode_keep_offer};
use neoteletext_web::viewer_state::{ChannelIdentity, KeyId};

use super::{Channel, Connection, Level, SERVICE};
use crate::protocol::{
    ConnectionId, EndState, Handle, MessageType, OfferRemember, Refusal, RequestState, Rule, ServerMessage, StateRefusal, StateRefused, ViewerBegan,
    ViewerLeft, ViewersReset,
};
use crate::stats::count;
use crate::viewer::{
    self, CHALLENGE_BYTES, CHALLENGE_WINDOW_MS, HandleKey, HandleSecret, MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE, MOST_COUNTED_ADDRESSES,
    MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR, MOST_VIEWERS_PER_CHANNEL, NEW_VIEWER_WINDOW_MS, RandomSource, STATE_ANSWER_MS, STATE_GESTURE_MS,
    STATE_OFFER_INTERVAL_MS, VIEWER_IDLE_MS,
};

/// Why a challenge was sent, which decides what its answer tells the
/// program.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// The challenge after WELCOME: its answer goes in `connected`.
    Opening,
    /// A program's `request_state`: its answer is `viewer_began` or
    /// `state_refused`.
    Request,
    /// The receiver's `STATE_OFFER`, or a rotation: a proof is
    /// `viewer_began`, anything else is nothing.
    Offer,
}

/// A challenge sent and not yet answered (VS-205).
struct Outstanding {
    challenge: [u8; CHALLENGE_BYTES],
    /// When it is no longer valid.
    due: u64,
    purpose: Purpose,
}

/// The viewer a connection presents.
#[derive(Clone, Copy)]
struct Present {
    key_id: KeyId,
    lifetime: Lifetime,
}

/// One connection's part in viewer state.
pub(super) struct Slot {
    /// The network address it came from, as the host sees it or as its
    /// trusted proxy states it (VS-304).
    address: Option<IpAddr>,
    outstanding: Option<Outstanding>,
    /// `connected` waits for the opening challenge's answer (VS-202).
    opening: bool,
    present: Option<Present>,
    /// When the viewer's last input that is a gesture came (VS-212).
    last_gesture: Option<u64>,
    /// When each challenge of the last [`CHALLENGE_WINDOW_MS`] was sent.
    sent: Vec<u64>,
    /// When the receiver last sent `STATE_NONE` or `STATE_OFFER` unasked.
    last_unprompted: Option<u64>,
    /// `STATE_KEEP_OFFER` was sent (VS-208: at most once a connection).
    keep_offered: bool,
}

impl Slot {
    pub(super) const fn new(address: Option<IpAddr>) -> Self {
        Self { address, outstanding: None, opening: false, present: None, last_gesture: None, sent: Vec::new(), last_unprompted: None, keep_offered: false }
    }

    /// The network address it came from, as VS-304 takes it: what a
    /// snapshot connection is counted by (REQ-6882).
    pub(super) const fn address(&self) -> Option<IpAddr> {
        self.address
    }

    /// `connected` waits for the opening challenge's answer.
    pub(super) const fn holds(&self) -> bool {
        self.opening
    }

    /// When the challenge outstanding expires.
    pub(super) fn due(&self) -> Option<u64> {
        self.outstanding.as_ref().map(|outstanding| outstanding.due)
    }
}

/// One viewer the channel knows (VS-303).
struct Entry {
    handle: Handle,
    /// The connections that present it now.
    connections: BTreeSet<ConnectionId>,
    /// The lifetime its last proof claimed.
    lifetime: Lifetime,
    /// When it was last proven, or its last connection closed: the order
    /// in which the table's bound removes entries.
    seen: u64,
    /// When its last connection closed, while it has none.
    idle_since: Option<u64>,
}

/// How many new viewers one address brought in its window (VS-304).
struct Counted {
    since: u64,
    new: u32,
}

/// The channel's part in viewer state.
pub(super) struct Viewers {
    key: HandleKey,
    /// The identities a proof is verified under (VS-301).
    identities: Vec<ChannelIdentity>,
    random: Box<dyn RandomSource>,
    table: BTreeMap<KeyId, Entry>,
    by_handle: BTreeMap<Handle, KeyId>,
    /// Every entry by when it was seen, for the table's bound.
    seen: BTreeSet<(u64, KeyId)>,
    /// The entries with no connection, by when their last closed.
    idle: BTreeSet<(u64, KeyId)>,
    counted: BTreeMap<IpAddr, Counted>,
    /// Each counted address by when its window began.
    counted_order: BTreeSet<(u64, IpAddr)>,
    /// The handle secret was rotated while no program ran: the next is told
    /// `viewers_reset` after `start` (VS-306).
    reset_untold: bool,
}

impl Viewers {
    /// The viewer state of a channel at `address` and `others`, under the
    /// server's handle secret; the addresses that are no channel identity,
    /// for the operator's log.
    pub(super) fn new<'a>(
        address: &'a str,
        others: impl Iterator<Item = &'a str>,
        secret: &HandleSecret,
        random: Box<dyn RandomSource>,
    ) -> (Self, Vec<&'a str>) {
        let (identities, refused) = viewer::identities(core::iter::once(address).chain(others));
        let folder = identities.first().map_or("/", viewer::folder_of);
        let key = HandleKey::derive(secret, folder);
        let viewers = Self {
            key,
            identities,
            random,
            table: BTreeMap::new(),
            by_handle: BTreeMap::new(),
            seen: BTreeSet::new(),
            idle: BTreeSet::new(),
            counted: BTreeMap::new(),
            counted_order: BTreeSet::new(),
            reset_untold: false,
        };
        (viewers, refused)
    }

    /// The viewers the table holds.
    pub(super) fn len(&self) -> usize {
        self.table.len()
    }

    /// When the next entry goes idle.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.idle.first().map(|&(since, _)| since.saturating_add(VIEWER_IDLE_MS))
    }

    /// Removes `key_id`'s entry, with every index of it.
    fn remove(&mut self, key_id: &KeyId) -> Option<Entry> {
        let entry = self.table.remove(key_id)?;
        self.by_handle.remove(&entry.handle);
        self.seen.remove(&(entry.seen, *key_id));
        if let Some(since) = entry.idle_since {
            self.idle.remove(&(since, *key_id));
        }
        Some(entry)
    }

    /// Whether `address` may bring another new viewer now (VS-304), counted
    /// if so.
    fn admit(&mut self, address: Option<IpAddr>, now: u64) -> bool {
        let Some(address) = address else { return true };
        if let Some(counted) = self.counted.get_mut(&address) {
            if counted.since.saturating_add(NEW_VIEWER_WINDOW_MS) <= now {
                self.counted_order.remove(&(counted.since, address));
                counted.since = now;
                counted.new = 0;
                self.counted_order.insert((now, address));
            }
            if counted.new >= MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR {
                return false;
            }
            counted.new = counted.new.saturating_add(1);
            return true;
        }
        while self.counted.len() >= MOST_COUNTED_ADDRESSES
            && let Some((_, oldest)) = self.counted_order.pop_first()
        {
            self.counted.remove(&oldest);
        }
        self.counted.insert(address, Counted { since: now, new: 1 });
        self.counted_order.insert((now, address));
        true
    }
}

/// `STATE_NONE`'s reason as the program is told it (VS-351 item 4): a key
/// just forgotten is no key.
const fn refusal_of(reason: NoneReason) -> StateRefusal {
    match reason {
        NoneReason::NoKey | NoneReason::Forgotten => StateRefusal::NoKey,
        NoneReason::Declined => StateRefusal::Declined,
        NoneReason::Stateless => StateRefusal::Stateless,
        NoneReason::NoGesture => StateRefusal::NoGesture,
    }
}

/// The packets `write` gives, back to back.
fn packets(write: impl FnOnce(&mut dyn FnMut(neoteletext_core::packet::Packet)) -> bool) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    write(&mut |packet| bytes.extend_from_slice(&packet.bytes)).then_some(bytes)
}

impl Channel {
    /// The viewer on `connection` as `connected` names it.
    pub(super) fn viewer_of(&self, state: &Connection) -> (Option<Handle>, bool) {
        let Some(present) = state.viewer.present else { return (None, false) };
        let handle = self.viewers.table.get(&present.key_id).map(|entry| entry.handle);
        (handle, handle.is_some() && present.lifetime == Lifetime::Remembered)
    }

    /// Sends `connection` a challenge, `create` set for a program's state
    /// request (VS-205).
    fn challenge(&mut self, connection: ConnectionId, create: bool, purpose: Purpose, now: u64) -> Result<(), StateRefusal> {
        let Some(state) = self.connections.get_mut(&connection) else { return Err(StateRefusal::NoKey) };
        let slot = &mut state.viewer;
        if slot.outstanding.is_some() {
            return Err(StateRefusal::Rate);
        }
        slot.sent.retain(|&sent| sent.saturating_add(CHALLENGE_WINDOW_MS) > now);
        if slot.sent.len() >= MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE {
            return Err(StateRefusal::Rate);
        }
        let mut challenge = [0_u8; CHALLENGE_BYTES];
        if self.viewers.random.fill(&mut challenge).is_err() {
            self.effects.push(super::Effect::Log {
                level: Level::Error,
                event: "random_unavailable",
                text: String::from("no random bytes for a viewer-state challenge; the connection has no viewer"),
            });
            return Err(StateRefusal::Rate);
        }
        let record = state.next_record();
        let Some(bytes) = packets(|out| encode_challenge(StateChallenge { challenge, create }, SERVICE, record, out).is_ok()) else {
            return Err(StateRefusal::Rate);
        };
        let slot = &mut state.viewer;
        slot.sent.push(now);
        slot.outstanding = Some(Outstanding { challenge, due: now.saturating_add(STATE_ANSWER_MS), purpose });
        self.send(connection, Arc::from(bytes));
        Ok(())
    }

    /// WELCOME was sent: the opening challenge, whose answer `connected`
    /// waits for (VS-202).
    pub(super) fn opening_challenge(&mut self, connection: ConnectionId, now: u64) {
        if self.challenge(connection, false, Purpose::Opening, now).is_ok()
            && let Some(state) = self.connections.get_mut(&connection)
        {
            state.viewer.opening = true;
        }
    }

    /// The opening challenge is answered or expired: `connected` waits no
    /// longer for it.
    fn opening_over(&mut self, connection: ConnectionId, now: u64) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if !core::mem::take(&mut state.viewer.opening) {
            return;
        }
        if state.first_selection_due.is_none() && !state.announced {
            self.announce(connection, now);
        }
    }

    /// The viewer acted on `connection` (VS-212).
    pub(super) fn gesture(&mut self, connection: ConnectionId, now: u64) {
        if let Some(state) = self.connections.get_mut(&connection) {
            state.viewer.last_gesture = Some(now);
        }
    }

    /// A receiver selected before it answered the opening challenge, which
    /// is answering it with no key (VS-202).
    pub(super) fn selected_before_answer(&mut self, connection: ConnectionId, now: u64) {
        let opening = self
            .connections
            .get(&connection)
            .and_then(|state| state.viewer.outstanding.as_ref())
            .is_some_and(|outstanding| outstanding.purpose == Purpose::Opening);
        if opening {
            self.refused(connection, StateRefusal::NoKey, now);
        }
    }

    /// One of viewer state's messages from a receiver (VS-204).
    pub(super) fn state_message(&mut self, connection: ConnectionId, event: &Event<'_>, now: u64) {
        match *event {
            Event::StateProof(proof) => self.proof(connection, &proof, now),
            Event::StateNone(reason) => {
                let prompted = self.connections.get(&connection).is_some_and(|state| state.viewer.outstanding.is_some());
                if prompted {
                    self.refused(connection, refusal_of(reason), now);
                } else if reason == NoneReason::Forgotten && self.unprompted(connection, now) {
                    // VS-113: the key on this connection was forgotten.
                    self.leave(connection, now);
                }
            }
            Event::StateOffer => {
                let idle = self.connections.get(&connection).is_some_and(|state| state.viewer.outstanding.is_none());
                if idle && self.unprompted(connection, now) {
                    // VS-111: a key is held now; a refusal of the challenge
                    // is told to nobody, since nobody asked.
                    let _ = self.challenge(connection, false, Purpose::Offer, now);
                }
            }
            _ => {}
        }
    }

    /// Whether a message the receiver sent unasked is within its bound
    /// (VS-204: once in [`STATE_OFFER_INTERVAL_MS`]), counted if so.
    fn unprompted(&mut self, connection: ConnectionId, now: u64) -> bool {
        let Some(state) = self.connections.get_mut(&connection) else { return false };
        let slot = &mut state.viewer;
        if slot.last_unprompted.is_some_and(|at| at.saturating_add(STATE_OFFER_INTERVAL_MS) > now) {
            return false;
        }
        slot.last_unprompted = Some(now);
        true
    }

    /// A `STATE_PROOF`: verified against the challenge outstanding, which it
    /// uses whether or not it verifies (VS-205, VS-301). One with no
    /// challenge outstanding is refused and told to nobody.
    fn proof(&mut self, connection: ConnectionId, proof: &StateProof<'_>, now: u64) {
        let Some((version, outstanding)) = self.connections.get(&connection).and_then(|state| Some((state.version, state.viewer.outstanding.as_ref()?))) else {
            count(&mut self.stats.proofs_refused);
            return;
        };
        // Under the version the connection's HELLO named, so a proof made
        // under one version is no proof under another (VS-203 item 2,
        // DEC-6953 item 2).
        if let Ok((key_id, lifetime)) = viewer::verify(version, proof, &outstanding.challenge, &self.viewers.identities) {
            self.proven(connection, Present { key_id, lifetime }, now);
        } else {
            count(&mut self.stats.proofs_refused);
            self.refused(connection, StateRefusal::Invalid, now);
        }
    }

    /// The challenge outstanding ends without a viewer, for `reason`.
    fn refused(&mut self, connection: ConnectionId, reason: StateRefusal, now: u64) {
        let Some(outstanding) = self.connections.get_mut(&connection).and_then(|state| state.viewer.outstanding.take()) else { return };
        match outstanding.purpose {
            Purpose::Opening => self.opening_over(connection, now),
            Purpose::Request => self.tell(&ServerMessage::StateRefused(StateRefused { connection, reason })),
            Purpose::Offer => {}
        }
    }

    /// The challenge outstanding was answered by a key that verified: the
    /// connection presents its viewer, unless a bound refuses a new one
    /// (VS-303, VS-304).
    fn proven(&mut self, connection: ConnectionId, present: Present, now: u64) {
        let Some(state) = self.connections.get(&connection) else { return };
        let address = state.viewer.address;
        let before = state.viewer.present;
        let known = self.viewers.table.contains_key(&present.key_id);
        if !known && !self.viewers.admit(address, now) {
            count(&mut self.stats.proofs_refused);
            self.refused(connection, StateRefusal::Rate, now);
            return;
        }
        count(&mut self.stats.proofs_accepted);
        let Some(outstanding) = self.connections.get_mut(&connection).and_then(|state| state.viewer.outstanding.take()) else { return };
        if before.is_some_and(|before| before.key_id != present.key_id) {
            self.leave(connection, now);
        }
        if !known {
            self.make_room();
            let handle = self.viewers.key.handle(&present.key_id);
            let entry = Entry { handle, connections: BTreeSet::new(), lifetime: present.lifetime, seen: now, idle_since: None };
            self.viewers.table.insert(present.key_id, entry);
            self.viewers.by_handle.insert(handle, present.key_id);
            self.viewers.seen.insert((now, present.key_id));
        }
        let viewers = &mut self.viewers;
        let Some(entry) = viewers.table.get_mut(&present.key_id) else { return };
        viewers.seen.remove(&(entry.seen, present.key_id));
        entry.seen = now;
        viewers.seen.insert((now, present.key_id));
        if let Some(since) = entry.idle_since.take() {
            viewers.idle.remove(&(since, present.key_id));
        }
        entry.lifetime = present.lifetime;
        entry.connections.insert(connection);
        let handle = entry.handle;
        let began = before.is_none_or(|before| before.key_id != present.key_id);
        if let Some(state) = self.connections.get_mut(&connection) {
            state.viewer.present = Some(present);
        }
        match outstanding.purpose {
            Purpose::Opening => self.opening_over(connection, now),
            Purpose::Request | Purpose::Offer if began => {
                let remembered = present.lifetime == Lifetime::Remembered;
                self.tell(&ServerMessage::ViewerBegan(ViewerBegan { connection, viewer: handle, remembered }));
            }
            Purpose::Request | Purpose::Offer => {}
        }
    }

    /// Room for one more viewer: at the bound, the viewer seen longest ago
    /// is removed (VS-303), and each connection that presented it told
    /// `viewer_left`. It loses nothing: its next proof gives the same handle.
    fn make_room(&mut self) {
        while self.viewers.table.len() >= MOST_VIEWERS_PER_CHANNEL
            && let Some(&(_, oldest)) = self.viewers.seen.first()
        {
            let Some(entry) = self.viewers.remove(&oldest) else {
                // An index with no entry is a fault of this module; it goes,
                // so the loop ends.
                self.viewers.seen.pop_first();
                continue;
            };
            for connection in entry.connections {
                self.unpresent(connection);
            }
        }
    }

    /// `connection` no longer presents a viewer: the program is told.
    fn unpresent(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        if state.viewer.present.take().is_some() {
            self.tell(&ServerMessage::ViewerLeft(ViewerLeft { connection }));
        }
    }

    /// The viewer on `connection` left it (VS-113, VS-351 item 3): its entry
    /// keeps its other connections, and waits idle without any.
    fn leave(&mut self, connection: ConnectionId, now: u64) {
        let Some(present) = self.connections.get(&connection).and_then(|state| state.viewer.present) else { return };
        self.release(connection, &present.key_id, now);
        self.unpresent(connection);
    }

    /// Takes `connection` from `key_id`'s entry, which goes idle when it was
    /// its last.
    fn release(&mut self, connection: ConnectionId, key_id: &KeyId, now: u64) {
        let viewers = &mut self.viewers;
        let Some(entry) = viewers.table.get_mut(key_id) else { return };
        entry.connections.remove(&connection);
        if entry.connections.is_empty() && entry.idle_since.is_none() {
            entry.idle_since = Some(now);
            viewers.seen.remove(&(entry.seen, *key_id));
            entry.seen = now;
            viewers.seen.insert((now, *key_id));
            viewers.idle.insert((now, *key_id));
        }
    }

    /// `connection` closed, holding `slot`.
    pub(super) fn viewer_closed(&mut self, connection: ConnectionId, slot: &Slot, now: u64) {
        if let Some(present) = slot.present {
            self.release(connection, &present.key_id, now);
        }
    }

    /// The time has come: challenges not answered in time end (VS-202,
    /// VS-205), and idle viewers past [`VIEWER_IDLE_MS`] go (VS-303).
    pub(super) fn viewer_deadline(&mut self, now: u64) {
        let expired: Vec<ConnectionId> = self
            .connections
            .due(now)
            .into_iter()
            .filter(|connection| self.connections.get(connection).is_some_and(|state| state.viewer.due().is_some_and(|due| due <= now)))
            .collect();
        for connection in expired {
            self.refused(connection, StateRefusal::Timeout, now);
        }
        while let Some(&(since, key_id)) = self.viewers.idle.first()
            && since.saturating_add(VIEWER_IDLE_MS) <= now
        {
            self.viewers.remove(&key_id);
        }
    }

    /// `request_state` (VS-352 item 1, VS-353).
    pub(super) fn request_state(&mut self, request: RequestState, now: u64) -> Result<(), Refusal> {
        let RequestState { connection } = request;
        let refuse = |rule: Rule, detail: String| Refusal { message: Some(MessageType::RequestState), path: String::from("connection"), rule, detail };
        let Some(state) = self.connections.get(&connection) else {
            return Err(refuse(Rule::UnknownConnection, format!("connection {connection} is not open")));
        };
        if state.viewer.present.is_some() {
            return Err(refuse(Rule::HasViewer, format!("connection {connection} already has a viewer")));
        }
        if state.viewer.last_gesture.is_none_or(|at| at.saturating_add(STATE_GESTURE_MS) < now) {
            return Err(refuse(Rule::NoGesture, format!("no input on connection {connection} in the last {STATE_GESTURE_MS} ms")));
        }
        if let Err(reason) = self.challenge(connection, true, Purpose::Request, now) {
            self.tell(&ServerMessage::StateRefused(StateRefused { connection, reason }));
        }
        Ok(())
    }

    /// `end_state` (VS-352 item 2): `STATE_END` to every connection that
    /// presents the viewer, `viewer_left` for each, and its entry removed.
    pub(super) fn end_state(&mut self, end: EndState) -> Result<(), Refusal> {
        let Some(key_id) = self.viewers.by_handle.get(&end.viewer).copied() else {
            return Err(Refusal {
                message: Some(MessageType::EndState),
                path: String::from("viewer"),
                rule: Rule::UnknownViewer,
                detail: String::from("the channel holds no such viewer"),
            });
        };
        let Some(entry) = self.viewers.remove(&key_id) else { return Ok(()) };
        for connection in entry.connections {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let record = state.next_record();
            if let Some(bytes) = packets(|out| encode_end(StateEnd { key_id }, SERVICE, record, out).is_ok()) {
                self.send(connection, Arc::from(bytes));
            }
            self.unpresent(connection);
        }
        Ok(())
    }

    /// `offer_remember` (VS-352 item 3, VS-208): `STATE_KEEP_OFFER` once to
    /// a connection with a visit key; `state_refused` otherwise.
    pub(super) fn offer_remember(&mut self, offer: OfferRemember) -> Result<(), Refusal> {
        let OfferRemember { connection } = offer;
        let Some(state) = self.connections.get_mut(&connection) else {
            return Err(Refusal {
                message: Some(MessageType::OfferRemember),
                path: String::from("connection"),
                rule: Rule::UnknownConnection,
                detail: format!("connection {connection} is not open"),
            });
        };
        let refusal = match state.viewer.present {
            None => Some(StateRefusal::NoKey),
            Some(present) if present.lifetime == Lifetime::Remembered => Some(StateRefusal::Declined),
            Some(_) if state.viewer.keep_offered => Some(StateRefusal::Rate),
            Some(_) => None,
        };
        if let Some(reason) = refusal {
            self.tell(&ServerMessage::StateRefused(StateRefused { connection, reason }));
            return Ok(());
        }
        state.viewer.keep_offered = true;
        let record = state.next_record();
        if let Some(bytes) = packets(|out| encode_keep_offer(SERVICE, record, out).is_ok()) {
            self.send(connection, Arc::from(bytes));
        }
        Ok(())
    }

    /// Rotates the server's handle secret on this channel (VS-306): the
    /// program is told `viewers_reset` first (or the next one, after its
    /// `start`, when none runs), then every entry is forgotten and each
    /// connection that presented a viewer is challenged again, so that its
    /// viewer comes back under a new handle no program can relate to the
    /// old one.
    pub fn rotate_viewers(&mut self, secret: &HandleSecret, now: u64) {
        if self.program {
            self.tell(&ServerMessage::ViewersReset(ViewersReset {}));
        } else {
            self.viewers.reset_untold = true;
        }
        let folder = self.viewers.identities.first().map_or("/", viewer::folder_of);
        self.viewers.key = HandleKey::derive(secret, folder);
        let viewers = &mut self.viewers;
        viewers.table.clear();
        viewers.by_handle.clear();
        viewers.seen.clear();
        viewers.idle.clear();
        let presented: Vec<ConnectionId> =
            self.connections.iter_mut().filter_map(|(&connection, state)| state.viewer.present.take().map(|_| connection)).collect();
        for connection in presented {
            // A bound reached leaves the connection without a viewer until
            // its browser offers again.
            let _ = self.challenge(connection, false, Purpose::Offer, now);
        }
    }

    /// After `start`: `viewers_reset`, if the secret was rotated while no
    /// program ran.
    pub(super) fn tell_untold_reset(&mut self) {
        if core::mem::take(&mut self.viewers.reset_untold) {
            self.tell(&ServerMessage::ViewersReset(ViewersReset {}));
        }
    }
}
