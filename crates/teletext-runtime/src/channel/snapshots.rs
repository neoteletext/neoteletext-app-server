//! Snapshot connections (REQ-6881, REQ-6882; neoteletext-common
//! `docs/tab-thumbnails.md` section 5; DEC-6875 version 2 item 3, DEC-6878
//! version 2 items 4 to 6): a connection whose HELLO offers SNAPSHOT asks
//! for one page as a stranger who has just arrived would see it, is sent
//! that page and its picture, or DENIED, and is closed.
//!
//! **Held apart.** When that HELLO is read, the connection leaves the
//! channel's table of connections, the viewers' table, and is held only
//! here: its address, when it closes, the capabilities agreed and what it
//! waits for. No path that reads the viewers' table can reach it, so by
//! construction it is sent no `STATE_CHALLENGE`, makes and counts no viewer,
//! is never told to the program as `connected`, `selected`, `input`, `left`
//! or `gone`, is in no message to `"all"` or to a page's viewers, runs no
//! rule table, animation, timeline, sound or synthesizer, and is counted as
//! a snapshot (`stats.snapshots`), never as a viewer. A flag on
//! `Connection` was considered and not chosen: every viewer path would
//! have to remember to check it (DEC-6878 version 2 item 6).
//!
//! **The answer**, in REQ-6881 item 4's order: no program, DENIED 503; a
//! public description already kept that shows the place; an answer already
//! kept for the place; else the program is asked with `snapshot`, which
//! names no connection, once for every connection waiting on the place,
//! within its rate and the number awaited; when it declines, answers with
//! something refused, does not answer in time or was not asked for its
//! rate, the page's description is asked for as a web visitor asks; else
//! DENIED 404, or 429 when the program was not asked because of its rate.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::net::IpAddr;

use neoteletext_input::caps::{NAPLPS_CAP, REGIS_CAP, SELECT_CAP, SNAPSHOT_CAP};
use neoteletext_input::control::{Control, packet_of};
use neoteletext_input::event::{Event, Item};
use neoteletext_web::consts::CLOSE_NORMAL;
use neoteletext_web::records::denied::{DeniedRecord, STATUS_NOT_FOUND, STATUS_TOO_MANY, STATUS_UNAVAILABLE, encode as encode_denied};

use super::{Channel, Effect, Level, SERVICE};
use crate::descriptions::{Answer, Asked, Waiter};
use crate::kept::{Checker, PictureLanguage};
use crate::protocol::{
    self, ANY_SUBPAGE, ConnectionId, Describe, MessageType, NaplpsObject, PageNumber, Refusal, Rule, Selection, ServerMessage, Snapshot, SnapshotNumber,
    SnapshotPicture, SubCode,
};
use crate::snapshot::{
    Bucket, Cache, Counter, Kept, MOST_SNAPSHOTS_AWAITED, Over, Place, SNAPSHOT_ANSWER_MS, SNAPSHOT_CONNECTION_MS, ShownPicture, SnapshotContent,
};
use crate::stats::count;

/// What WELCOME agrees on a snapshot connection, of what it offered:
/// SNAPSHOT, SELECT and the picture languages, never CURSOR, TOUCH or
/// RESUME (REQ-6881 item 1).
pub(super) const SNAPSHOT_AGREES: u32 = SNAPSHOT_CAP | SELECT_CAP | REGIS_CAP | NAPLPS_CAP;

/// A snapshot's picture as it is sent: its language, its stream, its
/// NAPLPS objects and their title.
type Resolved = (PictureLanguage, Vec<u8>, Option<Vec<NaplpsObject>>, Option<String>);

/// The generation of the one picture stream a snapshot connection is sent.
const FIRST_GENERATION: u16 = 1;

/// What a snapshot connection waits for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stands {
    /// Its one SELECT.
    Select,
    /// An answer for its place, from the program or the description;
    /// `limited` when the program was not asked because of its rate.
    Answer { place: Place, limited: bool },
    /// It was answered or refused and is closing: nothing more is sent.
    Done,
}

/// One snapshot connection: everything the channel holds of it.
struct SnapshotConnection {
    /// Its network address, as VS-304 takes it, for its counts alone.
    address: Option<IpAddr>,
    /// Whether the counts admitted it, so that its close releases them.
    admitted: bool,
    /// When `SNAPSHOT_CONNECTION_MS` from its opening ends.
    closes_at: u64,
    /// The capabilities WELCOME agreed.
    agreed: u32,
    stands: Stands,
    /// The next record number on the audio and input service.
    record_id: u8,
}

impl SnapshotConnection {
    const fn next_record(&mut self) -> u8 {
        let id = self.record_id;
        self.record_id = id.wrapping_add(1);
        id
    }
}

/// A `snapshot` sent and not yet answered.
struct Awaited {
    place: Place,
    /// The connections waiting on it: every one that selected its place
    /// while it was awaited.
    waiters: Vec<ConnectionId>,
    /// When it counts as declined.
    due: u64,
}

/// A channel's snapshot connections, and its snapshots awaited and kept.
#[derive(Default)]
pub(super) struct Snapshots {
    table: BTreeMap<ConnectionId, SnapshotConnection>,
    /// Each connection by when it closes.
    closing: BTreeSet<(u64, ConnectionId)>,
    counter: Counter,
    bucket: Bucket,
    cache: Cache,
    awaited: BTreeMap<SnapshotNumber, Awaited>,
    /// The last snapshot number used: the next is one more (REQ-6900 item 1).
    last_number: SnapshotNumber,
}

impl Snapshots {
    /// Whether `connection` is a snapshot connection.
    pub(super) fn holds(&self, connection: ConnectionId) -> bool {
        self.table.contains_key(&connection)
    }

    /// When the channel next needs its deadline for them: a connection's
    /// life, or an awaited snapshot's.
    pub(super) fn next_wake(&self) -> Option<u64> {
        let closing = self.closing.first().map(|&(at, _)| at);
        closing.into_iter().chain(self.awaited.values().map(|awaited| awaited.due)).min()
    }

    /// The snapshot connections open, and the snapshots awaited.
    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.table.len(), self.awaited.len())
    }

    /// The last snapshot number sent.
    #[cfg(test)]
    pub(super) const fn last_number(&self) -> SnapshotNumber {
        self.last_number
    }
}

/// A place's path, as SELECT and DENIED name it: `302` or `302/0002`.
fn path((page, sub): Place) -> String {
    Selection { page: PageNumber(page), sub: SubCode(sub) }.path()
}

/// Whether `content` shows `place`: its sub-page is the one selected, or
/// the page was selected as a whole.
const fn shows(content: &SnapshotContent, (page, sub): Place) -> bool {
    content.page.page.0 == page && (sub == ANY_SUBPAGE.0 || content.page.sub.0 == sub)
}

impl Channel {
    /// HELLO offering SNAPSHOT (REQ-6881 items 1 and 2): the connection is
    /// taken out of the viewers' table and held here, WELCOME agrees exactly
    /// what a snapshot may use, and no challenge or viewer follows. Past a
    /// limit of REQ-6882 it is told DENIED 429 with no place and closed. A
    /// HELLO without SELECT breaks the protocol, and so does one the
    /// program was already told of (it said nothing for
    /// `FIRST_SELECTION_WAIT_MS`), which is no stranger's arrival: false
    /// for those, a protocol error.
    pub(super) fn snapshot_hello(&mut self, connection: ConnectionId, version: u32, caps: u32, now: u64) -> bool {
        let Some(state) = self.connections.get(&connection) else { return true };
        if caps & SELECT_CAP == 0 || state.announced {
            return false;
        }
        let Some(state) = self.connections.remove(&connection) else { return true };
        let address = state.viewer.address();
        let closes_at = state.opened_at.saturating_add(SNAPSHOT_CONNECTION_MS);
        let admitted = self.snapshots.counter.admit(address, now);
        count(&mut self.stats.snapshots);
        let agreed = caps & SNAPSHOT_AGREES;
        let mut held = SnapshotConnection { address, admitted: admitted.is_ok(), closes_at, agreed, stands: Stands::Select, record_id: 0 };
        let welcome = packet_of(&Control::Welcome { version, caps: agreed, resume: None }, SERVICE, held.next_record());
        self.snapshots.table.insert(connection, held);
        self.snapshots.closing.insert((closes_at, connection));
        if let Ok(packet) = welcome {
            self.send(connection, Arc::from(&packet.bytes[..]));
        }
        if let Err(over) = admitted {
            let bound = match over {
                Over::PerMinute => "MOST_SNAPSHOTS_PER_ADDRESS_PER_MINUTE",
                Over::OpenFromAddress => "MOST_OPEN_SNAPSHOTS_PER_ADDRESS",
                Over::Open => "MOST_OPEN_SNAPSHOTS",
            };
            self.log(Level::Info, "snapshot_refused", format!("a snapshot connection was refused: {bound}"));
            self.deny_snapshot(connection, STATUS_TOO_MANY, "");
        }
        true
    }

    /// One item of a snapshot connection's input: one SELECT of a display
    /// page after HELLO, whose query is ignored, and nothing else
    /// (REQ-6881 item 3); false for anything else, a protocol error. After
    /// it was answered or refused it is closing, and what it sent meanwhile
    /// is not read.
    pub(super) fn snapshot_item(&mut self, connection: ConnectionId, item: &Item<'_>, now: u64) -> bool {
        let Some(held) = self.snapshots.table.get(&connection) else { return true };
        match (held.stands, item) {
            (Stands::Done, _) => true,
            (Stands::Select, Item::Event(Event::Select(select, _))) if select.page != 0 && !PageNumber(select.page).is_reserved() => {
                self.snapshot_select(connection, (select.page, select.sub), now);
                true
            }
            _ => {
                if let Some(held) = self.snapshots.table.get_mut(&connection) {
                    held.stands = Stands::Done;
                }
                false
            }
        }
    }

    /// The SELECT of a snapshot connection: answered in REQ-6881 item 4's
    /// order.
    fn snapshot_select(&mut self, connection: ConnectionId, place: Place, now: u64) {
        self.snapshot_waits(connection, place, false);
        if !self.program {
            self.deny_snapshot(connection, STATUS_UNAVAILABLE, &path(place));
            return;
        }
        // Step 2: a public description at hand, whose sub-page is the one
        // selected, answers without asking the program anything.
        if let Some(content) = self.public_content(place, now) {
            self.send_snapshot(connection, &content);
            return;
        }
        // Step 3: an answer already kept for the place.
        match self.snapshots.cache.get(place, now) {
            Some(Kept::Drawn(content)) => {
                self.send_snapshot(connection, &content);
                return;
            }
            Some(Kept::Declined) => {
                self.snapshot_from_description(connection, place, false, now);
                return;
            }
            None => {}
        }
        // Step 4: the program, asked once for every connection on the place.
        if let Some(awaited) = self.snapshots.awaited.values_mut().find(|awaited| awaited.place == place) {
            awaited.waiters.push(connection);
            return;
        }
        if self.snapshots.awaited.len() < MOST_SNAPSHOTS_AWAITED && self.snapshots.bucket.take(now) {
            let snapshot = self.snapshots.last_number.saturating_add(1).min(protocol::SAFE_INTEGER_LIMIT);
            self.snapshots.last_number = snapshot;
            let due = now.saturating_add(SNAPSHOT_ANSWER_MS);
            self.snapshots.awaited.insert(snapshot, Awaited { place, waiters: alloc::vec![connection], due });
            let (page, sub) = place;
            self.tell(&ServerMessage::Snapshot(Snapshot { snapshot, page: PageNumber(page), sub: SubCode(sub), path: path(place) }));
            return;
        }
        self.snapshot_from_description(connection, place, true, now);
    }

    /// Marks `connection` as waiting for an answer for `place`.
    fn snapshot_waits(&mut self, connection: ConnectionId, place: Place, limited: bool) {
        if let Some(held) = self.snapshots.table.get_mut(&connection).filter(|held| held.stands != Stands::Done) {
            held.stands = Stands::Answer { place, limited };
        }
    }

    /// The content of the public description kept for `place`'s page, if
    /// one is and its sub-page shows the place.
    fn public_content(&self, place: Place, now: u64) -> Option<Arc<SnapshotContent>> {
        let public = self.descriptions.kept(place.0, now)?.as_ref()?;
        shows(&public.snapshot, place).then(|| Arc::clone(&public.snapshot))
    }

    /// Step 5: the page's description, asked for exactly as a web visitor
    /// asks (REQ-1728), the program hearing the page number and nothing
    /// else.
    fn snapshot_from_description(&mut self, connection: ConnectionId, place: Place, limited: bool, now: u64) {
        self.snapshot_waits(connection, place, limited);
        let page = PageNumber(place.0);
        match self.descriptions.ask(place.0, Waiter::Snapshot(connection), now) {
            Asked::Answered(answer) => self.snapshot_described(connection, &answer),
            Asked::Waiting => {}
            // The program has this `describe` already, and answers it once.
            Asked::AskProgram if self.readiness_page == Some(place.0) => {}
            Asked::AskProgram => self.tell(&ServerMessage::Describe(Describe { page })),
        }
    }

    /// The description `connection` waited for: sent when it is public and
    /// shows the place; else DENIED 404, or 429 when the program was not
    /// asked because of its rate (REQ-6881 item 4 step 6).
    pub(super) fn snapshot_described(&mut self, connection: ConnectionId, answer: &Answer) {
        let Some(&SnapshotConnection { stands: Stands::Answer { place, limited }, .. }) = self.snapshots.table.get(&connection) else { return };
        match answer.as_ref().filter(|public| shows(&public.snapshot, place)) {
            Some(public) => {
                let content = Arc::clone(&public.snapshot);
                self.send_snapshot(connection, &content);
            }
            None => self.deny_snapshot(connection, if limited { STATUS_TOO_MANY } else { STATUS_NOT_FOUND }, &path(place)),
        }
    }

    /// `snapshot_page` (REQ-6900 items 2 and 5): checked against what was
    /// asked, kept and sent to every connection waiting on it. One refused
    /// is taken as a decline.
    ///
    /// # Errors
    /// `not_awaited`, `wrong_page`, `not_kept`, or the receiver's refusal
    /// of the picture's bytes.
    pub(super) fn snapshot_page(&mut self, answer: protocol::SnapshotPage, now: u64) -> Result<(), Refusal> {
        let number = answer.snapshot;
        let Some(awaited) = self.snapshots.awaited.remove(&number) else { return Err(not_awaited(MessageType::SnapshotPage, number)) };
        match self.snapshot_content(answer, awaited.place) {
            Ok(content) => {
                let content = Arc::new(content);
                self.snapshots.cache.keep(awaited.place, Kept::Drawn(Arc::clone(&content)), now);
                for connection in awaited.waiters {
                    self.send_snapshot(connection, &content);
                }
                Ok(())
            }
            Err(refusal) => {
                self.snapshot_not_drawn(awaited, now);
                Err(refusal)
            }
        }
    }

    /// `snapshot_declined` (REQ-6900 item 3): kept as the place's answer,
    /// and those waiting go to the description.
    ///
    /// # Errors
    /// `not_awaited`.
    pub(super) fn snapshot_declined(&mut self, declined: protocol::SnapshotDeclined, now: u64) -> Result<(), Refusal> {
        let Some(awaited) = self.snapshots.awaited.remove(&declined.snapshot) else {
            return Err(not_awaited(MessageType::SnapshotDeclined, declined.snapshot));
        };
        self.snapshot_not_drawn(awaited, now);
        Ok(())
    }

    /// The program did not draw an awaited snapshot: declined, refused or
    /// late. The decline is kept for the place, so the program is asked
    /// again only after `SNAPSHOT_CACHE_MS`, and those waiting go to the
    /// description.
    fn snapshot_not_drawn(&mut self, awaited: Awaited, now: u64) {
        self.snapshots.cache.keep(awaited.place, Kept::Declined, now);
        for connection in awaited.waiters {
            if self.snapshots.table.get(&connection).is_some_and(|held| held.stands != Stands::Done) {
                self.snapshot_from_description(connection, awaited.place, false, now);
            }
        }
    }

    /// What a `snapshot_page` gives to send, once it is of the place asked
    /// for and its picture is one the receiver would read.
    fn snapshot_content(&self, answer: protocol::SnapshotPage, (page, sub): Place) -> Result<SnapshotContent, Refusal> {
        let kind = MessageType::SnapshotPage;
        let number = answer.snapshot;
        let refuse =
            |at: &str, rule: Rule, detail: String| Refusal { message: Some(kind), path: at.to_string(), rule, detail: format!("snapshot {number}: {detail}") };
        let given = &answer.page;
        let asked = path((page, sub));
        if given.page.0 != page {
            return Err(refuse("page.page", Rule::WrongPage, format!("{:03X} answers {asked}, not that page", given.page.0)));
        }
        if sub != ANY_SUBPAGE.0 && given.sub.0 != sub {
            return Err(refuse("page.sub", Rule::WrongPage, format!("{:04X} answers {asked}, not that sub-code", given.sub.0)));
        }
        let picture = match answer.picture {
            None => None,
            Some(SnapshotPicture::Kept(kept)) => {
                if self.kept.get(page, kept.kept.0).is_none() {
                    return Err(refuse("picture.kept", Rule::NotKept, format!("nothing is kept of {page:03X}/{:04X}", kept.kept.0)));
                }
                Some(ShownPicture::Kept(kept.kept.0))
            }
            Some(SnapshotPicture::Bytes(bytes)) => {
                let language = match bytes.language {
                    protocol::PictureLanguageName::Regis => PictureLanguage::Regis,
                    protocol::PictureLanguageName::Naplps => PictureLanguage::Naplps,
                };
                // The reader has written any parts into `data` (PAR-21).
                let data = bytes.data.as_ref().map(|data| protocol::check::bytes_of(data)).unwrap_or_default();
                Checker::new(language, self.kept.fonts()).feed(&data).map_err(|refused| refuse("picture.data", refused.rule, refused.detail))?;
                Some(ShownPicture::Bytes { language, data: Arc::from(data), objects: bytes.objects })
            }
        };
        Ok(SnapshotContent { page: answer.page, picture })
    }

    /// Sends a snapshot connection that waits its answer (REQ-6881 item 5):
    /// the sub-page whole as page packets, with no index, LINKS, SUBPAGES,
    /// cursor, rule table, sound or animation record; then its picture as
    /// the REGIS or NAPLPS record when the connection agreed its language,
    /// a NAPLPS picture's objects as one `NAPLPS_OBJECTS` record, never
    /// `DRAWING_LINKS` or `REGIS_CURSOR`; or, in a language not agreed, the
    /// sub-page with the picture's mosaic fallback (REQ-6769). Then a
    /// normal close.
    fn send_snapshot(&mut self, connection: ConnectionId, content: &SnapshotContent) {
        let Some(held) = self.snapshots.table.get(&connection).filter(|held| matches!(held.stands, Stands::Answer { .. })) else { return };
        let agreed = held.agreed;
        let (page, sub) = (content.page.page.0, content.page.sub.0);
        let picture: Option<Resolved> = match &content.picture {
            None => None,
            // A kept picture as its kept stream stands now (REQ-6881 item 5);
            // one forgotten since leaves the page alone.
            Some(ShownPicture::Kept(kept)) => self.kept.get(page, *kept).map(|kept| {
                let objects = kept.objects.as_ref().map(|objects| objects.objects.clone());
                let title = kept.objects.as_ref().and_then(|objects| objects.title.as_ref()).map(|title| title.0.clone());
                (kept.language, kept.stream.clone(), objects, title)
            }),
            Some(ShownPicture::Bytes { language, data, objects }) => Some((*language, data.to_vec(), objects.clone(), None)),
        };
        let takes = |language: PictureLanguage| agreed & super::pictures::capability(language) != 0;
        let shown = match &picture {
            Some((language, stream, objects, _)) if !takes(*language) => crate::fallback::rows(*language, stream, objects.as_deref()).map_or_else(
                |fault| {
                    self.log(Level::Warn, "snapshot_fallback", format!("{page:03X}/{sub:04X}: its picture could not be drawn as mosaics: {fault}"));
                    content.page.clone()
                },
                |(rows, _)| super::fallback::with_rows(&content.page, &rows),
            ),
            _ => content.page.clone(),
        };
        let mut bytes = crate::page::encode(&shown).packets.concat();
        let Some(held) = self.snapshots.table.get_mut(&connection) else { return };
        held.stands = Stands::Done;
        let mut record_id = held.record_id;
        let records = match &picture {
            Some((language, stream, objects, title)) if takes(*language) => {
                let records = match language {
                    PictureLanguage::Regis => crate::records::regis(page, sub, FIRST_GENERATION, 0, stream, &mut record_id),
                    PictureLanguage::Naplps => crate::records::naplps(page, sub, FIRST_GENERATION, 0, stream, &mut record_id),
                };
                records.and_then(|mut records| {
                    if let (PictureLanguage::Naplps, Some(objects)) = (language, objects) {
                        let set = crate::animation::NaplpsSet { objects, title: title.as_deref(), one: false };
                        records.extend_from_slice(&crate::animation::naplps_objects_record(page, sub, FIRST_GENERATION, set, &mut record_id)?);
                    }
                    Ok(records)
                })
            }
            _ => Ok(Vec::new()),
        };
        held.record_id = record_id;
        match records {
            Ok(records) => bytes.extend_from_slice(&records),
            Err(fault) => self.log(Level::Error, "snapshot_refused", format!("{page:03X}/{sub:04X}: the engine refused its picture's records: {fault}")),
        }
        self.send(connection, Arc::from(bytes));
        self.effects.push(Effect::Close { connection, code: CLOSE_NORMAL });
    }

    /// DENIED with `status`, naming `place`, then a normal close.
    fn deny_snapshot(&mut self, connection: ConnectionId, status: u16, place: &str) {
        let Some(held) = self.snapshots.table.get_mut(&connection).filter(|held| held.stands != Stands::Done) else { return };
        held.stands = Stands::Done;
        let record = held.next_record();
        let mut bytes = Vec::new();
        if encode_denied(DeniedRecord { status, place }, SERVICE, record, &mut |packet| bytes.extend_from_slice(&packet.bytes)).is_ok() {
            self.send(connection, Arc::from(bytes));
        }
        self.effects.push(Effect::Close { connection, code: CLOSE_NORMAL });
    }

    /// A snapshot connection broke the protocol: it is closing, and is sent
    /// nothing more.
    pub(super) fn snapshot_broke(&mut self, connection: ConnectionId) {
        if let Some(held) = self.snapshots.table.get_mut(&connection) {
            held.stands = Stands::Done;
        }
    }

    /// A connection closed; true when it was a snapshot connection, which
    /// the program is not told of. A snapshot awaited for it stays awaited,
    /// and its answer is kept (REQ-6881 item 6).
    pub(super) fn snapshot_closed(&mut self, connection: ConnectionId) -> bool {
        let Some(held) = self.snapshots.table.remove(&connection) else { return false };
        self.snapshots.closing.remove(&(held.closes_at, connection));
        if held.admitted {
            self.snapshots.counter.release(held.address);
        }
        true
    }

    /// The time asked for has come: snapshots the program did not answer in
    /// `SNAPSHOT_ANSWER_MS` count as declined, and snapshot connections at
    /// `SNAPSHOT_CONNECTION_MS` are closed, with DENIED 503 when a SELECT
    /// stands unanswered (REQ-6881 item 6).
    pub(super) fn snapshot_deadline(&mut self, now: u64) {
        let late: Vec<SnapshotNumber> = self.snapshots.awaited.iter().filter(|(_, awaited)| awaited.due <= now).map(|(&number, _)| number).collect();
        for number in late {
            let Some(awaited) = self.snapshots.awaited.remove(&number) else { continue };
            self.log(Level::Warn, "program_late", format!("no answer to snapshot {number} within {SNAPSHOT_ANSWER_MS} ms"));
            self.snapshot_not_drawn(awaited, now);
        }
        while let Some(&(at, connection)) = self.snapshots.closing.first()
            && at <= now
        {
            self.snapshots.closing.remove(&(at, connection));
            let Some(stands) = self.snapshots.table.get(&connection).map(|held| held.stands) else { continue };
            match stands {
                Stands::Answer { place, .. } => self.deny_snapshot(connection, STATUS_UNAVAILABLE, &path(place)),
                Stands::Select => {
                    self.snapshot_broke(connection);
                    self.effects.push(Effect::Close { connection, code: CLOSE_NORMAL });
                }
                Stands::Done => {}
            }
        }
    }

    /// The program ended: nothing it was asked will be answered, and what
    /// it answered goes with it. Those waiting are told the channel is
    /// unavailable.
    pub(super) fn snapshots_program_ended(&mut self) {
        self.snapshots.awaited.clear();
        self.snapshots.cache.clear();
        let waiting: Vec<(ConnectionId, Place)> = self
            .snapshots
            .table
            .iter()
            .filter_map(|(&connection, held)| if let Stands::Answer { place, .. } = held.stands { Some((connection, place)) } else { None })
            .collect();
        for (connection, place) in waiting {
            self.deny_snapshot(connection, STATUS_UNAVAILABLE, &path(place));
        }
    }
}

/// The refusal of an answer to a snapshot the server does not await: never
/// sent, answered already, or past `SNAPSHOT_ANSWER_MS` (REQ-6900 item 5).
fn not_awaited(kind: MessageType, number: SnapshotNumber) -> Refusal {
    Refusal { message: Some(kind), path: "snapshot".to_string(), rule: Rule::NotAwaited, detail: format!("snapshot {number} is not awaited") }
}
