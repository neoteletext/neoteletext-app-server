//! Animation on a channel (neoteletext-common `docs/animation.md` ANIM-2,
//! ANIM-3, ANIM-17, ANIM-21; REQ-6533):
//!
//! - **Frames** (ANIM-2, ANIM-3): between a `frame` begin and its end the
//!   server holds everything it would send a connection that changes what
//!   the page shows, and at the end sends it between the two FRAME records
//!   in one message, so that a receiver has the whole frame at once. A
//!   frame not ended in two seconds, or left open when its program ends,
//!   is ended by the server; one that passes 262,144 bytes is sent
//!   unframed and refused `frame_too_large`.
//! - **What a later viewer is sent** (ANIM-21): for each connection and
//!   page the newest sprites, NAPLPS objects and sheets, and the animation
//!   state that the `animate` messages and timeline steps amount to, sent
//!   when the connection selects the page: the sheets, the definitions,
//!   then the state.
//! - **Timelines** (ANIM-21): played on the server's clock, each step sent
//!   as an ANIMATE record when its tick is due.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_core::limits::SUBCODE_ANY;
use neoteletext_rules::animate::Operation;
use neoteletext_web::consts::{MAX_FRAME_BYTES, MAX_FRAME_TICKS, MAX_SHEET_BYTES};

use super::{Channel, Connection, Level};
use crate::animation::{self, Folded, Refused, tick_ms};
use crate::protocol::{
    self, Animate, ConnectionId, FrameMarkName, FrameMessage, MOST_TIMELINES, MessageType, NaplpsObject, NaplpsObjects, ProgramMessage, Refusal, Rule,
    ServerMessage, SheetKindName, Sprite, SpriteSheet, Sprites, TimelineMessage, TimelineStop,
};
use crate::rules::MAX_KEPT_TABLES;

/// The most pages whose sprites, objects, sheets and animation state the
/// server keeps for a connection (ANIM-21), the oldest dropped first: as
/// many as it keeps tables for (page-rules.md RULE-40).
pub const MAX_KEPT_ANIMATION_PAGES: usize = MAX_KEPT_TABLES;
/// The most sub-pages of one page whose sprites, objects and animation
/// state the server keeps for a connection, the oldest dropped first.
pub const MAX_KEPT_ANIMATION_SUBPAGES: usize = 16;
/// The most timelines the server plays for one connection, over all its
/// pages: four pages' worth of the sixteen a page may have (ANIM-21).
pub const MOST_TIMELINES_PER_CONNECTION: usize = MOST_TIMELINES * 4;
/// How long a frame may stay open before the server ends it itself: the
/// two seconds a receiver holds one (ANIM-2, `MAX_FRAME_TICKS`).
pub const FRAME_LIMIT_MS: u64 = tick_ms(MAX_FRAME_TICKS as u64);

/// A page's sheet of one kind, as the program gave it: its bytes in the
/// pieces they came in, shared between the connections they were for.
#[derive(Clone, Default)]
struct Sheet {
    pieces: Vec<Arc<[u8]>>,
    bytes: usize,
}

impl Sheet {
    /// The whole sheet.
    fn whole(&self) -> Vec<u8> {
        self.pieces.iter().flat_map(|piece| piece.iter().copied()).collect()
    }
}

/// What a connection is to hold of one sub-page (ANIM-21).
#[derive(Default)]
struct KeptSubpage {
    sub: u16,
    /// Its teletext sprites, when the program gave any.
    sprites: Option<Arc<[Sprite]>>,
    /// Its NAPLPS objects.
    objects: Option<Arc<[NaplpsObject]>>,
    /// What the operations sent for it amount to.
    folded: Folded,
}

/// What a connection is to hold of one page.
#[derive(Default)]
struct KeptPage {
    page: u16,
    regis_sheet: Option<Sheet>,
    naplps_sheet: Option<Sheet>,
    /// Its sub-pages, the oldest first; the sub-code 3F7F holds the
    /// operations sent for whichever sub-page is shown.
    subs: Vec<KeptSubpage>,
}

impl KeptPage {
    fn sheet(&mut self, kind: SheetKindName) -> &mut Option<Sheet> {
        match kind {
            SheetKindName::Regis => &mut self.regis_sheet,
            SheetKindName::Naplps => &mut self.naplps_sheet,
        }
    }

    /// What is kept of `sub`, made if nothing is, the oldest sub-page
    /// dropped for it.
    fn sub(&mut self, sub: u16) -> Option<&mut KeptSubpage> {
        if !self.subs.iter().any(|kept| kept.sub == sub) {
            if self.subs.len() >= MAX_KEPT_ANIMATION_SUBPAGES {
                self.subs.remove(0);
            }
            self.subs.push(KeptSubpage { sub, ..KeptSubpage::default() });
        }
        self.subs.iter_mut().find(|kept| kept.sub == sub)
    }
}

/// A sheet a connection is being sent: where its stream stands.
struct SheetStream {
    page: u16,
    kind: SheetKindName,
    generation: u16,
    offset: u32,
}

/// A frame the program began and has not ended.
struct OpenFrame {
    page: u16,
    sub: u16,
    identifier: u16,
    /// What is held of it: the packets it would have been sent.
    held: Vec<u8>,
    /// When the server ends it itself.
    due: u64,
}

/// One connection's sprites, objects, sheets, frames and animation state.
#[derive(Default)]
pub(super) struct Animated {
    /// What it is to hold of each page, the oldest first.
    kept: Vec<KeptPage>,
    /// The sheets it is being sent, of the page it shows.
    sheets: Vec<SheetStream>,
    /// The number of its last sheet stream: a new one takes the next.
    sheet_generation: u16,
    /// The frame open for the page it shows.
    frame: Option<OpenFrame>,
    /// The number of its last frame.
    frame_identifier: u16,
    /// The page whose sheets it was sent for its selection.
    placed_page: Option<u16>,
    /// The sub-page whose definitions and state it was sent.
    placed_sub: Option<(u16, u16)>,
}

impl Animated {
    /// What is kept of `page`, made if nothing is, the oldest page
    /// dropped for it.
    fn page(&mut self, page: u16) -> Option<&mut KeptPage> {
        if !self.kept.iter().any(|kept| kept.page == page) {
            if self.kept.len() >= MAX_KEPT_ANIMATION_PAGES {
                self.kept.remove(0);
            }
            self.kept.push(KeptPage { page, ..KeptPage::default() });
        }
        self.kept.iter_mut().find(|kept| kept.page == page)
    }

    /// What is kept of the sub-page `sub` of `page`.
    fn subpage(&mut self, page: u16, sub: u16) -> Option<&mut KeptSubpage> {
        self.page(page)?.sub(sub)
    }

    /// The whole ReGIS sheet it is to hold of `page`: what a renderer of
    /// one of the page's sub-pages is fed before that sub-page's stream
    /// (ANIM-17).
    pub(super) fn regis_sheet(&self, page: u16) -> Vec<u8> {
        self.kept.iter().find(|kept| kept.page == page).and_then(|kept| kept.regis_sheet.as_ref()).map(Sheet::whole).unwrap_or_default()
    }

    /// Its selection left its page: nothing of that page is in place, and
    /// what was held of a frame of it is of a page no longer shown.
    pub(super) fn left(&mut self) {
        self.placed_page = None;
        self.placed_sub = None;
        self.sheets.clear();
        self.frame = None;
    }

    /// When its open frame is ended by the server.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.frame.as_ref().map(|frame| frame.due)
    }

    /// The records of a sheet from its start, as a new stream.
    fn sheet_from_start(&mut self, page: u16, kind: SheetKindName, sheet: &Sheet, record_id: &mut u8) -> Result<Vec<u8>, Refused> {
        self.sheet_generation = self.sheet_generation.checked_add(1).unwrap_or(1);
        let generation = self.sheet_generation;
        let whole = sheet.whole();
        let bytes = animation::sheet_records(page, kind, generation, 0, &whole, record_id)?;
        self.sheets.retain(|stream| !(stream.page == page && stream.kind == kind));
        self.sheets.push(SheetStream { page, kind, generation, offset: u32::try_from(whole.len()).unwrap_or(u32::MAX) });
        Ok(bytes)
    }

    /// The records that put in place what it is to hold of `page`, and of
    /// its sub-page `sub` unless that is 3F7F: the sheets, then the
    /// definitions, then the animation state (ANIM-17, ANIM-21). Nothing
    /// for what is in place already. `naplps_generation` is the generation
    /// of the NAPLPS stream the objects belong to.
    pub(super) fn place(&mut self, page: u16, sub: u16, naplps_generation: u16, record_id: &mut u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        if self.placed_page != Some(page) {
            self.left();
            self.placed_page = Some(page);
            let sheets = self.kept.iter().find(|kept| kept.page == page).map(|kept| (kept.regis_sheet.clone(), kept.naplps_sheet.clone()));
            if let Some((regis, naplps)) = sheets {
                for (kind, sheet) in [(SheetKindName::Regis, regis), (SheetKindName::Naplps, naplps)] {
                    if let Some(sheet) = sheet {
                        bytes.extend(self.sheet_from_start(page, kind, &sheet, record_id).unwrap_or_default());
                    }
                }
            }
        }
        if sub == SUBCODE_ANY || self.placed_sub == Some((page, sub)) {
            return bytes;
        }
        self.placed_sub = Some((page, sub));
        let Some(kept) = self.kept.iter().find(|kept| kept.page == page) else { return bytes };
        let of = |sub: u16| kept.subs.iter().find(|held| held.sub == sub);
        if let Some(held) = of(sub) {
            if let Some(sprites) = &held.sprites {
                bytes.extend(animation::sprites_record(page, sub, sprites, record_id).unwrap_or_default());
            }
            if let Some(objects) = &held.objects {
                bytes.extend(animation::naplps_objects_record(page, sub, naplps_generation, objects, record_id).unwrap_or_default());
            }
        }
        // The state sent for this sub-page, then for whichever is shown.
        for of_sub in [sub, SUBCODE_ANY] {
            if let Some(held) = of(of_sub).filter(|held| !held.folded.is_empty()) {
                bytes.extend(animation::animate_records(page, of_sub, &held.folded.operations(), record_id).unwrap_or_default());
            }
        }
        bytes
    }
}

/// One timeline the server is playing for one connection (ANIM-21).
struct Playing {
    connection: ConnectionId,
    page: u16,
    sub: u16,
    id: u16,
    /// The steps, in ascending order of their ticks.
    steps: Arc<[(u64, Vec<Operation>)]>,
    /// The ticks after which it begins again.
    loop_ticks: Option<u64>,
    /// When it began: the arrival of its message.
    started_ms: u64,
    /// How many times it has begun again.
    loops: u64,
    /// The next step to send.
    next: usize,
}

impl Playing {
    /// When its next step is due.
    fn due(&self) -> Option<u64> {
        let (tick, _) = self.steps.get(self.next)?;
        let ticks = self.loop_ticks.unwrap_or(0).saturating_mul(self.loops).saturating_add(*tick);
        Some(self.started_ms.saturating_add(tick_ms(ticks)))
    }
}

/// The timelines the server is playing.
#[derive(Default)]
pub(super) struct Timelines {
    playing: Vec<Playing>,
}

impl Timelines {
    /// A connection closed: nothing is played for it.
    pub(super) fn forget(&mut self, connection: ConnectionId) {
        self.playing.retain(|playing| playing.connection != connection);
    }

    /// When the next step of any is due.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.playing.iter().filter_map(Playing::due).min()
    }
}

impl Connection {
    /// The generation of the NAPLPS stream a sub-page's objects belong to
    /// (ANIM-11): the stream it has for the sub-page, or the one its next
    /// `naplps` message with `restart` begins.
    fn naplps_generation_for(&self, page: u16, sub: u16) -> u16 {
        self.naplps
            .as_ref()
            .filter(|stream| stream.page == page && stream.sub == sub)
            .map_or_else(|| self.naplps_generation.wrapping_add(1), |stream| stream.generation)
    }

    /// The records that put its page's animation in place (ANIM-21).
    pub(super) fn place_animation(&mut self, page: u16, sub: u16) -> Vec<u8> {
        let generation = self.naplps_generation_for(page, sub);
        self.animation.place(page, sub, generation, &mut self.record_id)
    }

    /// Whether its selection shows `sub` of `page`, or with 3F7F any
    /// sub-page of `page`.
    fn shows_any(&self, page: u16, sub: u16) -> bool {
        if sub == SUBCODE_ANY { self.selection.is_some_and(|selection| selection.page.0 == page) } else { self.shows(page, sub) }
    }
}

/// A refusal of an animation message as a whole.
fn refuse(kind: MessageType, path: &str, rule: Rule, detail: String) -> Refusal {
    Refusal { message: Some(kind), path: path.into(), rule, detail }
}

impl Channel {
    /// Sends `connection` bytes that change what `page` shows: at once, or
    /// into the frame it has open for the page (ANIM-2). A frame that
    /// would pass `MAX_FRAME_BYTES` is given up: what it held and these
    /// bytes go unframed, so that a fault never hides a picture, and the
    /// program is told `frame_too_large` (ANIM-3, ANIM-21).
    pub(super) fn send_for_page(&mut self, connection: ConnectionId, page: u16, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(frame) = state.animation.frame.as_mut().filter(|frame| frame.page == page) else {
            self.send(connection, Arc::from(bytes));
            return;
        };
        if frame.held.len().saturating_add(bytes.len()) <= MAX_FRAME_BYTES {
            frame.held.extend_from_slice(&bytes);
            return;
        }
        let mut unframed = core::mem::take(&mut frame.held);
        unframed.extend_from_slice(&bytes);
        state.animation.frame = None;
        self.send(connection, Arc::from(unframed));
        let detail = format!("connection {connection}'s frame of {page:03X} passed {MAX_FRAME_BYTES} bytes: what it held was sent as it is, with no frame");
        self.log(Level::Warn, "frame_too_large", detail.clone());
        self.tell(&ServerMessage::Refused(refuse(MessageType::Frame, "", Rule::FrameTooLarge, detail).refused()));
    }

    /// Ends `connection`'s open frame: what it held goes between its
    /// begin and its end, in one message.
    fn end_frame(&mut self, connection: ConnectionId) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let Some(frame) = state.animation.frame.take() else { return };
        let mark = |mark, record_id: &mut u8| animation::frame_record(frame.page, frame.sub, mark, frame.identifier, record_id).unwrap_or_default();
        let mut bytes = mark(FrameMarkName::Begin, &mut state.record_id);
        bytes.extend_from_slice(&frame.held);
        bytes.extend(mark(FrameMarkName::End, &mut state.record_id));
        self.send(connection, Arc::from(bytes));
    }

    /// The frames open past their time are ended by the server (ANIM-2):
    /// a frame delays a picture and never hides one.
    pub(super) fn late_frames(&mut self, now: u64) {
        let late: Vec<ConnectionId> =
            self.connections.iter().filter(|(_, state)| state.animation.next_due().is_some_and(|due| due <= now)).map(|(&connection, _)| connection).collect();
        for connection in late {
            self.log(Level::Warn, "frame_not_ended", format!("connection {connection}'s frame was not ended within {FRAME_LIMIT_MS} ms: the server ended it"));
            self.end_frame(connection);
        }
    }

    /// The program ended with frames open: the server ends them itself
    /// (ANIM-21).
    pub(super) fn end_open_frames(&mut self) {
        let open: Vec<ConnectionId> = self.connections.iter().filter(|(_, state)| state.animation.frame.is_some()).map(|(&connection, _)| connection).collect();
        for connection in open {
            self.end_frame(connection);
        }
    }

    /// Sends `connection` what it is to hold of its page's animation
    /// (ANIM-21), when it selects the page.
    pub(super) fn place_animation(&mut self, connection: ConnectionId, page: u16, sub: u16) {
        let Some(state) = self.connections.get_mut(&connection) else { return };
        let bytes = state.place_animation(page, sub);
        if !bytes.is_empty() {
            self.send(connection, Arc::from(bytes));
        }
    }

    /// The messages of animation (ANIM-21).
    pub(super) fn act_on_animation(&mut self, message: ProgramMessage, now: u64) -> Result<(), Refusal> {
        match message {
            ProgramMessage::Frame(frame) => self.frame(&frame, now),
            ProgramMessage::Animate(animate) => self.animate(&animate),
            ProgramMessage::Sprites(sprites) => self.sprites(&sprites),
            ProgramMessage::NaplpsObjects(objects) => self.naplps_objects(&objects),
            ProgramMessage::SpriteSheet(sheet) => self.sprite_sheet(&sheet),
            ProgramMessage::Timeline(timeline) => self.timeline(&timeline, now),
            ProgramMessage::TimelineStop(stop) => {
                self.timeline_stop(&stop);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// `frame` (ANIM-2, ANIM-21): begins a frame for the connections named
    /// that show the sub-page, or ends the frame each has open for it.
    fn frame(&mut self, message: &FrameMessage, now: u64) -> Result<(), Refusal> {
        let kind = MessageType::Frame;
        let (page, sub) = (message.page.0, message.sub.0);
        match message.mark {
            FrameMarkName::Begin => {
                let targets = self.showing(&message.connections, page, sub);
                if let Some(inside) = targets.iter().find(|connection| self.connections.get(connection).is_some_and(|state| state.animation.frame.is_some())) {
                    return Err(refuse(
                        kind,
                        "mark",
                        Rule::FrameOrder,
                        format!("connection {inside} has a frame open: a frame begins after the last has ended"),
                    ));
                }
                for connection in targets {
                    let Some(state) = self.connections.get_mut(&connection) else { continue };
                    state.animation.frame_identifier = state.animation.frame_identifier.wrapping_add(1);
                    let identifier = state.animation.frame_identifier;
                    state.animation.frame = Some(OpenFrame { page, sub, identifier, held: Vec::new(), due: now.saturating_add(FRAME_LIMIT_MS) });
                }
            }
            FrameMarkName::End => {
                let open = |state: &Connection| state.animation.frame.as_ref().is_some_and(|frame| frame.page == page && frame.sub == sub);
                let targets: Vec<ConnectionId> =
                    self.targets(&message.connections).into_iter().filter(|connection| self.connections.get(connection).is_some_and(open)).collect();
                if targets.is_empty() {
                    return Err(refuse(
                        kind,
                        "mark",
                        Rule::FrameOrder,
                        format!("no connection named has a frame of {page:03X}/{sub:04X} open: an end follows its begin"),
                    ));
                }
                for connection in targets {
                    self.end_frame(connection);
                }
            }
        }
        Ok(())
    }

    /// Operations for a sub-page of `page`, or with 3F7F for whichever is
    /// shown: each of `targets` keeps what they amount to, and one that
    /// shows the sub-page is sent them (ANIM-6, ANIM-21).
    fn operate(&mut self, targets: &[ConnectionId], page: u16, sub: u16, operations: &[Operation]) {
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let kept = state.animation.subpage(page, sub).is_some_and(|held| held.folded.apply(operations));
            let bytes = if state.shows_any(page, sub) {
                animation::animate_records(page, sub, operations, &mut state.record_id).unwrap_or_default()
            } else {
                Vec::new()
            };
            if !kept {
                self.log(
                    Level::Debug,
                    "animation_not_kept",
                    format!("connection {connection}: an operation on {page:03X}/{sub:04X} found no room in the state kept"),
                );
            }
            self.send_for_page(connection, page, bytes);
        }
    }

    /// `animate` (ANIM-6).
    fn animate(&mut self, message: &Animate) -> Result<(), Refusal> {
        let operations = protocol::check::operations(MessageType::Animate, "ops", &message.ops)?;
        let targets = self.targets(&message.connections);
        self.operate(&targets, message.page.0, message.sub.0, &operations);
        Ok(())
    }

    /// `sprites` (ANIM-7): each connection named keeps them, forgetting
    /// the animation state of the sprites they replace, and one that shows
    /// the sub-page is sent them.
    fn sprites(&mut self, message: &Sprites) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let sprites: Arc<[Sprite]> = Arc::from(message.sprites.clone());
        for connection in self.targets(&message.connections) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if let Some(kept) = state.animation.subpage(page, sub) {
                kept.sprites = Some(Arc::clone(&sprites));
                kept.folded.forget_objects();
            }
            let bytes = if state.shows(page, sub) {
                animation::sprites_record(page, sub, &sprites, &mut state.record_id)
                    .map_err(|detail| refuse(MessageType::Sprites, "sprites", Rule::InvalidSprites, detail))?
            } else {
                Vec::new()
            };
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `naplps_objects` (ANIM-11): as `sprites`, for the NAPLPS stream the
    /// connection has of the sub-page.
    fn naplps_objects(&mut self, message: &NaplpsObjects) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let objects: Arc<[NaplpsObject]> = Arc::from(message.objects.clone());
        for connection in self.targets(&message.connections) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let generation = state.naplps_generation_for(page, sub);
            if let Some(kept) = state.animation.subpage(page, sub) {
                kept.objects = Some(Arc::clone(&objects));
                kept.folded.forget_objects();
            }
            let bytes = if state.shows(page, sub) {
                animation::naplps_objects_record(page, sub, generation, &objects, &mut state.record_id)
                    .map_err(|detail| refuse(MessageType::NaplpsObjects, "objects", Rule::InvalidNaplpsObjects, detail))?
            } else {
                Vec::new()
            };
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `sprite_sheet` (ANIM-17): more bytes of a page's sheet. Each
    /// connection named keeps the sheet; it is read whole, as its renderer
    /// will read it, for every one of them before anything changes or is
    /// sent, and one that would refuse it, or that it would pass 65,536
    /// bytes for, refuses the message.
    fn sprite_sheet(&mut self, message: &SpriteSheet) -> Result<(), Refusal> {
        let kind = MessageType::SpriteSheet;
        let (page, of) = (message.page.0, message.kind);
        let piece: Arc<[u8]> = Arc::from(protocol::check::bytes_of(&message.data));
        let targets = self.targets(&message.connections);
        let mut read: Vec<Vec<u8>> = Vec::new();
        for connection in &targets {
            let Some(state) = self.connections.get_mut(connection) else { continue };
            let kept = state.animation.page(page).and_then(|kept| kept.sheet(of).clone());
            let held = if message.restart { Sheet::default() } else { kept.unwrap_or_default() };
            if held.bytes.saturating_add(piece.len()) > MAX_SHEET_BYTES {
                let detail = format!("{} bytes after {}: a page's sheet is at most {MAX_SHEET_BYTES}", piece.len(), held.bytes);
                return Err(refuse(kind, "data", Rule::SheetTooLarge, detail));
            }
            let mut whole = held.whole();
            whole.extend_from_slice(&piece);
            // The same sheet for many connections is read once.
            if !read.contains(&whole) {
                animation::read_sheet(of, &whole).map_err(|detail| refuse(kind, "data", Rule::InvalidSheet, detail))?;
                read.push(whole);
            }
        }
        for connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let Some(kept) = state.animation.page(page) else { continue };
            let sheet = kept.sheet(of);
            if message.restart {
                *sheet = None;
            }
            let sheet = sheet.get_or_insert_with(Sheet::default);
            sheet.pieces.push(Arc::clone(&piece));
            sheet.bytes = sheet.bytes.saturating_add(piece.len());
            let whole = sheet.clone();
            if state.selection.is_none_or(|selection| selection.page.0 != page) {
                continue;
            }
            let streaming = state.animation.sheets.iter_mut().find(|stream| stream.page == page && stream.kind == of);
            let sent = match streaming {
                Some(stream) if !message.restart => {
                    let sent = animation::sheet_records(page, of, stream.generation, stream.offset, &piece, &mut state.record_id);
                    stream.offset = stream.offset.saturating_add(u32::try_from(piece.len()).unwrap_or(u32::MAX));
                    sent
                }
                // A sheet begun again, or one the connection has nothing
                // of: a new stream, from its start.
                _ => state.animation.sheet_from_start(page, of, &whole, &mut state.record_id),
            };
            // A new ReGIS sheet makes a receiver start the page's renderer
            // again (ANIM-17): the ReGIS it was sent is not of this sheet.
            if message.restart && of == SheetKindName::Regis && state.regis.as_ref().is_some_and(|stream| stream.page == page) {
                state.regis = None;
            }
            let bytes = sent.map_err(|detail| refuse(kind, "data", Rule::InvalidSheet, detail))?;
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `timeline` (ANIM-21): played for each connection named, from now.
    fn timeline(&mut self, message: &TimelineMessage, now: u64) -> Result<(), Refusal> {
        let kind = MessageType::Timeline;
        let steps = protocol::check::timeline_steps(message)?;
        let (page, id) = (message.page.0, message.id.get());
        let targets = self.targets(&message.connections);
        for &connection in &targets {
            let others = |playing: &&Playing| playing.connection == connection && !(playing.page == page && playing.id == id);
            let of_page = self.timelines.playing.iter().filter(others).filter(|playing| playing.page == page).count();
            if of_page >= MOST_TIMELINES {
                return Err(refuse(
                    kind,
                    "id",
                    Rule::TooManyTimelines,
                    format!("connection {connection} has {MOST_TIMELINES} timelines playing for {page:03X}"),
                ));
            }
            if self.timelines.playing.iter().filter(others).count() >= MOST_TIMELINES_PER_CONNECTION {
                let detail = format!("connection {connection} has {MOST_TIMELINES_PER_CONNECTION} timelines playing over all its pages");
                return Err(refuse(kind, "id", Rule::TooManyTimelines, detail));
            }
        }
        let steps: Arc<[(u64, Vec<Operation>)]> = Arc::from(steps);
        let loop_ticks = message.loop_ticks.map(protocol::U64In::get);
        for connection in targets {
            // A timeline with a playing one's number replaces it.
            self.timelines.playing.retain(|playing| !(playing.connection == connection && playing.page == page && playing.id == id));
            self.timelines.playing.push(Playing {
                connection,
                page,
                sub: message.sub.0,
                id,
                steps: Arc::clone(&steps),
                loop_ticks,
                started_ms: now,
                loops: 0,
                next: 0,
            });
        }
        self.play_timelines(now);
        Ok(())
    }

    /// `timeline_stop`: a timeline that is not playing is passed over.
    fn timeline_stop(&mut self, message: &TimelineStop) {
        let targets = self.targets(&message.connections);
        let (page, id) = (message.page.0, message.id.get());
        self.timelines.playing.retain(|playing| !(targets.contains(&playing.connection) && playing.page == page && playing.id == id));
    }

    /// Sends each step that is due (ANIM-21), on the server's clock.
    pub(super) fn play_timelines(&mut self, now: u64) {
        let mut due: Vec<(ConnectionId, u16, u16, Vec<Operation>)> = Vec::new();
        for playing in &mut self.timelines.playing {
            // One timeline sends at most each of its steps once in one
            // wake: a host that slept through many of a loop's turns does
            // not send them all.
            for _ in 0..playing.steps.len() {
                if playing.due().is_none_or(|at| at > now) {
                    break;
                }
                if let Some((_, operations)) = playing.steps.get(playing.next) {
                    due.push((playing.connection, playing.page, playing.sub, operations.clone()));
                }
                playing.next = playing.next.saturating_add(1);
                if playing.next >= playing.steps.len() && playing.loop_ticks.is_some() {
                    playing.next = 0;
                    playing.loops = playing.loops.saturating_add(1);
                }
            }
            // The turns slept through are passed over: it goes on at its
            // next turn, in step with its start.
            if let Some(loop_ticks) = playing.loop_ticks.filter(|_| playing.next == 0 && playing.due().is_some_and(|at| at <= now)) {
                let turn = tick_ms(loop_ticks).max(1);
                let turns = now.saturating_sub(playing.started_ms).checked_div(turn).unwrap_or(playing.loops);
                playing.loops = turns.saturating_add(1).max(playing.loops);
            }
        }
        // One that played its last step once has ended.
        self.timelines.playing.retain(|playing| playing.next < playing.steps.len());
        for (connection, page, sub, operations) in due {
            self.operate(&[connection], page, sub, &operations);
        }
    }
}
