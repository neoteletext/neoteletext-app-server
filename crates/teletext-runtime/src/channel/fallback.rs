//! The mosaic fallback (neoteletext-common `docs/renderer-parity.md`
//! PAR-24, PAR-25; REQ-6769): a connection that did not agree a picture's
//! language is sent the sub-page the program gave with rows 1 to 23
//! replaced by the picture drawn as Level 1 mosaics ([`crate::fallback`]).
//!
//! A picture is drawn again from its whole stream when it changes, at most
//! once in `FALLBACK_INTERVAL_MS`, its last change always drawn within that
//! interval; a kept picture's fallback is drawn once and shared by every
//! connection showing it. A channel draws at most
//! `MOST_FALLBACK_RENDERS_PER_SECOND` a second, sustained, and at most
//! `MOST_FALLBACK_BYTES_PER_SECOND` of stream, each ReGIS drawing counted as
//! its stream's bytes and `FALLBACK_DRAWING_FLOOR_BYTES` (TODO-6839), each
//! NAPLPS drawing as the pixel work it spends, `NAPLPS_WORK_PER_FALLBACK_BYTE`
//! units a byte (TODO-6845; the NAPLPS renderer's work allowance, DEC-6851,
//! bounds one drawing): a drawing over that budget is drawn late, never wrong
//! and never skipped, and one costlier than the whole budget is drawn alone
//! once the last second's drawings have passed. A NAPLPS drawing is let
//! start at what it may spend (its kept reader's count, or the allowance for
//! its bytes) and then counted at what it spent. For a stream
//! sent to such a connection itself, the server keeps the bytes it did not
//! send, at most `MOST_KEPT_STREAM_BYTES` a connection and
//! `MOST_FALLBACK_POOL_BYTES` for the channel; past either the stream is
//! dropped, its fallback shows what it had, and the author's log says
//! which bound was passed.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::vec::Vec;

use neoteletext_web::base64;

use super::{Channel, Connection, Level};
use crate::fallback::{FIRST_ROW, MosaicRows};
use crate::kept::{
    FALLBACK_DRAWING_FLOOR_BYTES, FALLBACK_INTERVAL_MS, MOST_FALLBACK_BYTES_PER_SECOND, MOST_FALLBACK_POOL_BYTES, MOST_FALLBACK_RENDERS_PER_SECOND,
    MOST_KEPT_STREAM_BYTES, NAPLPS_WORK_PER_FALLBACK_BYTE, PictureLanguage,
};
use crate::protocol::{self, ConnectionId, NaplpsObject};

/// The window `MOST_FALLBACK_RENDERS_PER_SECOND` and
/// `MOST_FALLBACK_BYTES_PER_SECOND` count over.
const RENDER_WINDOW_MS: u64 = 1_000;

// The longest stream a fallback draws fits the byte budget alone, so every
// drawing is made in the end.
const _: () = assert!(MOST_KEPT_STREAM_BYTES + FALLBACK_DRAWING_FLOOR_BYTES <= MOST_FALLBACK_BYTES_PER_SECOND);

/// What a ReGIS drawing of a stream of `length` bytes counts against
/// `MOST_FALLBACK_BYTES_PER_SECOND`.
const fn drawing_cost(length: usize) -> usize {
    length.saturating_add(FALLBACK_DRAWING_FLOOR_BYTES)
}

/// What a NAPLPS drawing that spends `work` pixel work units counts
/// against `MOST_FALLBACK_BYTES_PER_SECOND` (TODO-6845).
fn work_cost(work: u64) -> usize {
    usize::try_from(work.div_ceil(NAPLPS_WORK_PER_FALLBACK_BYTE)).unwrap_or(usize::MAX)
}

/// The most pixel work a NAPLPS stream of `length` bytes may spend: the
/// renderer's allowance, `WORK_CAP` and `WORK_PER_BYTE` a byte (DEC-6851).
fn most_naplps_work(length: usize) -> u64 {
    let bytes = u64::try_from(length).unwrap_or(u64::MAX);
    naplps::limits::WORK_CAP.saturating_add(naplps::limits::WORK_PER_BYTE.saturating_mul(bytes))
}

/// What a drawing is let start at: a ReGIS one at its bytes; a NAPLPS one
/// at the work its stream's reader counted (`known`, exact for a kept
/// picture's) or else the most its bytes allow, and the most a composing of
/// its objects may spend (`MAX_COMPOSE_WORK`) if it has any.
fn start_cost(language: PictureLanguage, length: usize, known: Option<u64>, objects: bool) -> usize {
    match language {
        PictureLanguage::Regis => drawing_cost(length),
        PictureLanguage::Naplps => {
            let composing = if objects { naplps::limits::MAX_COMPOSE_WORK } else { 0 };
            work_cost(known.unwrap_or_else(|| most_naplps_work(length)).saturating_add(composing))
        }
    }
}

/// What a drawing made is counted at: a ReGIS one at its bytes, a NAPLPS
/// one at the work it spent (one that failed, at the most it could have).
fn spent_cost(language: PictureLanguage, length: usize, work: Option<u64>) -> usize {
    match (language, work) {
        (PictureLanguage::Naplps, Some(work)) => work_cost(work),
        (PictureLanguage::Naplps, None) => work_cost(most_naplps_work(length).saturating_add(naplps::limits::MAX_COMPOSE_WORK)),
        (PictureLanguage::Regis, _) => drawing_cost(length),
    }
}

/// When one picture's fallback is drawn.
impl Connection {
    /// When its own picture's fallback is next to be drawn, if it has one.
    pub(super) fn own_fallback_due(&self) -> Option<u64> {
        self.own_picture.as_ref().and_then(|own| own.drawing.due)
    }
}

#[derive(Default)]
pub(super) struct Drawing {
    /// Its rows as last drawn.
    rows: Option<MosaicRows>,
    /// The picture changed since.
    changed: bool,
    /// When it was last drawn.
    last_drawn: Option<u64>,
    /// When it is next to be drawn.
    due: Option<u64>,
}

impl Drawing {
    /// The picture changed at `now`: drawn now, or once the interval since
    /// the last drawing has passed (PAR-25).
    fn changed(&mut self, now: u64) {
        self.changed = true;
        let earliest = self.last_drawn.map_or(now, |last| last.saturating_add(FALLBACK_INTERVAL_MS));
        self.due = Some(self.due.map_or(earliest.max(now), |due| due.min(earliest.max(now))));
    }
}

/// A stream sent to a connection that did not agree its language, kept for
/// its fallback.
pub(super) struct OwnPicture {
    pub(super) language: PictureLanguage,
    pub(super) page: u16,
    pub(super) sub: u16,
    pub(super) stream: Vec<u8>,
    pub(super) objects: Option<Vec<NaplpsObject>>,
    /// A bound was passed: nothing more is kept of it.
    pub(super) dropped: bool,
    pub(super) drawing: Drawing,
}

/// The channel's fallbacks of kept pictures, and its drawings.
#[derive(Default)]
pub(super) struct Fallbacks {
    /// Each kept picture's, by page and sub-code.
    kept: BTreeMap<(u16, u16), Drawing>,
    /// When the channel drew within the last second, and what each drawing
    /// counted.
    renders: VecDeque<(u64, usize)>,
    /// Every drawing made, for the operator and the tests.
    pub(super) drawn: u64,
}

impl Fallbacks {
    /// The earliest drawing due.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.kept.values().filter_map(|drawing| drawing.due).min()
    }

    /// Whether the channel may make a drawing counted as `cost` at `now`,
    /// and if not when it may try again.
    fn budget(&mut self, now: u64, cost: usize) -> Result<(), u64> {
        while self.renders.front().is_some_and(|&(at, _)| now.saturating_sub(at) >= RENDER_WINDOW_MS) {
            self.renders.pop_front();
        }
        let most = usize::try_from(MOST_FALLBACK_RENDERS_PER_SECOND).unwrap_or(usize::MAX);
        let spent = self.renders.iter().fold(0_usize, |sum, &(_, counted)| sum.saturating_add(counted));
        match self.renders.front() {
            Some(&(oldest, _)) if self.renders.len() >= most || spent.saturating_add(cost) > MOST_FALLBACK_BYTES_PER_SECOND => {
                Err(oldest.saturating_add(RENDER_WINDOW_MS))
            }
            _ => Ok(()),
        }
    }

    fn spend(&mut self, now: u64, cost: usize) {
        self.renders.push_back((now, cost));
        self.drawn = self.drawn.saturating_add(1);
    }
}

/// `page` with rows 1 to 23 replaced by `rows`, and no local enhancement
/// data, which would place characters over the mosaics.
fn with_rows(page: &protocol::Page, rows: &MosaicRows) -> protocol::Page {
    let mut replaced = page.clone();
    for (slot, codes) in replaced.rows.iter_mut().skip(usize::from(FIRST_ROW)).zip(rows.iter()) {
        let mut text = alloc::vec![0_u8; base64::encoded_len(codes.len())];
        let written = base64::encode(codes, &mut text).unwrap_or(0);
        text.truncate(written);
        *slot = Some(protocol::Base64(alloc::string::String::from_utf8(text).unwrap_or_default()));
    }
    replaced.enhancements.clear();
    replaced
}

impl Channel {
    /// Whether `connection` lacks a picture language's bit: what makes the
    /// server keep the program's sub-page for it, to give it a fallback.
    pub(super) fn may_need_fallback(&self, connection: ConnectionId) -> bool {
        self.connections.get(&connection).is_some_and(|state| !state.agrees(PictureLanguage::Regis) || !state.agrees(PictureLanguage::Naplps))
    }

    /// The fallback rows `connection` is to be shown for `page`, `sub`:
    /// its own stream's, or the kept picture's when it did not agree its
    /// language; none when it has no fallback, or none drawn yet.
    pub(super) fn fallback_rows(&self, connection: ConnectionId, page: u16, sub: u16) -> Option<MosaicRows> {
        let state = self.connections.get(&connection)?;
        if let Some(own) = state.own_picture.as_ref().filter(|own| own.page == page && own.sub == sub) {
            return own.drawing.rows;
        }
        let picture = self.kept.get(page, sub)?;
        if state.agrees(picture.language) {
            return None;
        }
        self.fallbacks.kept.get(&(page, sub)).and_then(|drawing| drawing.rows)
    }

    /// The sub-page `page` with `connection`'s fallback rows, if it has
    /// any, and whether a kept picture's fallback it needs is not drawn.
    pub(super) fn page_for(&mut self, connection: ConnectionId, page: &protocol::Page) -> Option<protocol::Page> {
        let (number, sub) = (page.page.0, page.sub.0);
        let rows = self.fallback_rows(connection, number, sub);
        if rows.is_none() {
            self.want_kept_fallback(connection, number, sub);
        }
        rows.map(|rows| with_rows(page, &rows))
    }

    /// A connection without the bit shows a kept picture whose fallback is
    /// not drawn: it is drawn now, or as soon as it may be.
    fn want_kept_fallback(&mut self, connection: ConnectionId, page: u16, sub: u16) {
        let Some(picture) = self.kept.get(page, sub) else { return };
        if self.connections.get(&connection).is_none_or(|state| state.agrees(picture.language)) {
            return;
        }
        let drawing = self.fallbacks.kept.entry((page, sub)).or_default();
        if drawing.rows.is_none() && drawing.due.is_none() {
            drawing.changed(self.now);
        }
    }

    /// A kept picture changed (PAR-25): its fallback is drawn again if a
    /// connection showing it did not agree its language.
    pub(super) fn kept_changed(&mut self, page: u16, sub: u16) {
        let Some(language) = self.kept.get(page, sub).map(|picture| picture.language) else {
            self.fallbacks.kept.remove(&(page, sub));
            return;
        };
        let wanted = self.connections.values().any(|state| state.has_shown(page, sub) && !state.agrees(language));
        let drawing = self.fallbacks.kept.entry((page, sub)).or_default();
        if wanted {
            drawing.changed(self.now);
        } else {
            // Drawn when a connection that needs it comes.
            drawing.rows = None;
            drawing.due = None;
        }
    }

    /// Bytes of a stream sent to `connection`, which did not agree its
    /// language: kept for its fallback within its bounds (PAR-25).
    pub(super) fn own_fallback_stream(&mut self, connection: ConnectionId, language: PictureLanguage, page: u16, sub: u16, restart: bool, bytes: &[u8]) {
        let pool: usize =
            self.connections.iter().filter(|(id, _)| **id != connection).filter_map(|(_, state)| state.own_picture.as_ref()).map(|own| own.stream.len()).sum();
        let now = self.now;
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let same = state.own_picture.as_ref().is_some_and(|own| own.page == page && own.sub == sub && own.language == language);
        if restart || !same {
            if !restart && !same {
                // As for a receiver: a message without restart adds to a
                // stream begun, and there is none.
                return;
            }
            state.own_picture = Some(OwnPicture { language, page, sub, stream: Vec::new(), objects: None, dropped: false, drawing: Drawing::default() });
        }
        let Some(own) = state.own_picture.as_mut() else { return };
        if own.dropped {
            return;
        }
        let length = own.stream.len().saturating_add(bytes.len());
        let passed = if length > MOST_KEPT_STREAM_BYTES {
            Some(format!("MOST_KEPT_STREAM_BYTES ({MOST_KEPT_STREAM_BYTES})"))
        } else if pool.saturating_add(length) > MOST_FALLBACK_POOL_BYTES {
            Some(format!("MOST_FALLBACK_POOL_BYTES ({MOST_FALLBACK_POOL_BYTES})"))
        } else {
            None
        };
        if let Some(bound) = passed {
            own.dropped = true;
            let note = format!("{} for the mosaic fallback passed {bound}: the fallback shows what it had", language.name());
            self.author_notes(&[connection], page, alloc::vec![note.clone()]);
            self.log(Level::Warn, "fallback_dropped", format!("connection {connection}, {page:03X}/{sub:04X}: {note}"));
        } else {
            own.stream.extend_from_slice(bytes);
            own.drawing.changed(now);
        }
    }

    /// NAPLPS objects sent to `connection`, which did not agree NAPLPS:
    /// kept with its stream for its fallback.
    pub(super) fn own_fallback_objects(&mut self, connection: ConnectionId, page: u16, sub: u16, objects: &[NaplpsObject], one: bool) {
        let now = self.now;
        let Some(own) = self
            .connections
            .get_mut(&connection)
            .and_then(|state| state.own_picture.as_mut())
            .filter(|own| own.page == page && own.sub == sub && own.language == PictureLanguage::Naplps)
        else {
            return;
        };
        let held = own.objects.get_or_insert_with(Vec::new);
        if !one {
            held.clear();
        }
        for object in objects {
            match held.iter_mut().find(|kept| kept.number == object.number) {
                Some(kept) => *kept = object.clone(),
                None => held.push(object.clone()),
            }
        }
        own.drawing.changed(now);
    }

    /// Draws every fallback that is due at `now` and the budget allows, and
    /// sends each connection showing it its rows (PAR-25).
    pub(super) fn draw_fallbacks(&mut self, now: u64) {
        let due_kept: Vec<(u16, u16)> =
            self.fallbacks.kept.iter().filter(|(_, drawing)| drawing.due.is_some_and(|due| due <= now)).map(|(key, _)| *key).collect();
        for (page, sub) in due_kept {
            let cost = self.kept.get(page, sub).map_or(0, |picture| {
                let objects = picture.objects.as_ref().is_some_and(|objects| !objects.objects.is_empty());
                start_cost(picture.language, picture.stream.len(), picture.work_spent(), objects)
            });
            if let Err(later) = self.fallbacks.budget(now, cost) {
                if let Some(drawing) = self.fallbacks.kept.get_mut(&(page, sub)) {
                    drawing.due = Some(later);
                }
                continue;
            }
            let Some(picture) = self.kept.get(page, sub) else {
                self.fallbacks.kept.remove(&(page, sub));
                continue;
            };
            let objects = picture.objects.as_ref().map(|objects| objects.objects.as_slice());
            let drawn = crate::fallback::rows(picture.language, &picture.stream, objects);
            let language = picture.language;
            let length = picture.stream.len();
            self.fallbacks.spend(now, spent_cost(language, length, drawn.as_ref().ok().and_then(|(_, work)| *work)));
            let rows = match drawn {
                Ok((rows, _)) => rows,
                Err(fault) => {
                    self.log(Level::Error, "fallback_refused", format!("{page:03X}/{sub:04X}: {fault}"));
                    continue;
                }
            };
            if let Some(drawing) = self.fallbacks.kept.get_mut(&(page, sub)) {
                *drawing = Drawing { rows: Some(rows), changed: false, last_drawn: Some(now), due: None };
            }
            let viewers: Vec<ConnectionId> = self
                .connections
                .iter()
                .filter(|(_, state)| {
                    state.has_shown(page, sub) && !state.agrees(language) && state.own_picture.as_ref().is_none_or(|own| own.page != page || own.sub != sub)
                })
                .map(|(&id, _)| id)
                .collect();
            for connection in viewers {
                self.show_fallback(connection, &rows);
            }
        }
        let due_own: Vec<ConnectionId> = self
            .connections
            .due(now)
            .into_iter()
            .filter(|id| self.connections.get(id).and_then(Connection::own_fallback_due).is_some_and(|due| due <= now))
            .collect();
        for connection in due_own {
            let cost = self.connections.get(&connection).and_then(|state| state.own_picture.as_ref()).map_or(0, |own| {
                let objects = own.objects.as_ref().is_some_and(|objects| !objects.is_empty());
                start_cost(own.language, own.stream.len(), None, objects)
            });
            if let Err(later) = self.fallbacks.budget(now, cost) {
                if let Some(own) = self.connections.get_mut(&connection).and_then(|state| state.own_picture.as_mut()) {
                    own.drawing.due = Some(later);
                }
                continue;
            }
            let Some(own) = self.connections.get(&connection).and_then(|state| state.own_picture.as_ref()) else { continue };
            let (page, sub) = (own.page, own.sub);
            let drawn = crate::fallback::rows(own.language, &own.stream, own.objects.as_deref());
            self.fallbacks.spend(now, spent_cost(own.language, own.stream.len(), drawn.as_ref().ok().and_then(|(_, work)| *work)));
            let rows = match drawn {
                Ok((rows, _)) => rows,
                Err(fault) => {
                    self.log(Level::Error, "fallback_refused", format!("connection {connection}, {page:03X}/{sub:04X}: {fault}"));
                    continue;
                }
            };
            if let Some(own) = self.connections.get_mut(&connection).and_then(|state| state.own_picture.as_mut()) {
                own.drawing = Drawing { rows: Some(rows), changed: false, last_drawn: Some(now), due: None };
            }
            if self.connections.get(&connection).is_some_and(|state| state.has_shown(page, sub)) {
                self.show_fallback(connection, &rows);
            }
        }
    }

    /// Sends `connection` its sub-page shown with `rows`: what changed of
    /// it against what it was sent.
    fn show_fallback(&mut self, connection: ConnectionId, rows: &MosaicRows) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(shown) = state.shown.as_mut() else { return };
        let Some(program) = shown.program.as_ref() else { return };
        let encoded = crate::page::encode(&with_rows(program, rows));
        let change = crate::page::changes(&shown.packets, &encoded);
        shown.packets.clone_from(&encoded.packets);
        let page = shown.page;
        let bytes = match change {
            crate::page::Change::Nothing => return,
            crate::page::Change::Whole => encoded.packets.concat(),
            crate::page::Change::Rows(rows) => rows,
        };
        self.send_for_page(connection, page, bytes);
    }

    /// `connection`'s sub-page shown, with the program's own rows again:
    /// its fallback's picture was forgotten.
    pub(super) fn show_program_rows(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(shown) = state.shown.as_mut() else { return };
        let Some(program) = shown.program.as_ref() else { return };
        let encoded = crate::page::encode(program);
        let change = crate::page::changes(&shown.packets, &encoded);
        shown.packets.clone_from(&encoded.packets);
        let page = shown.page;
        let bytes = match change {
            crate::page::Change::Nothing => return,
            crate::page::Change::Whole => encoded.packets.concat(),
            crate::page::Change::Rows(rows) => rows,
        };
        self.send_for_page(connection, page, bytes);
    }

    /// How many fallbacks the channel has drawn.
    #[cfg(test)]
    pub(super) const fn fallbacks_drawn(&self) -> u64 {
        self.fallbacks.drawn
    }
}

/// The program's sub-page kept with what a connection was shown, for a
/// connection that may need a fallback.
pub(super) type ProgramPage = Option<Box<protocol::Page>>;
