//! Animation on a channel (neoteletext-common `docs/animation.md` ANIM-2,
//! ANIM-3, ANIM-17, ANIM-21; REQ-6533), in the second form (DEC-6587,
//! DEC-6599):
//!
//! - **Frames** (ANIM-2, ANIM-3): between a `frame` begin and its end the
//!   server holds everything it would send a connection that changes what
//!   the page shows, and at the end sends it between the two FRAME records
//!   in one message, so that a receiver has the whole frame at once. A
//!   frame not ended in two seconds, or left open when its program ends,
//!   is ended by the server; one that passes 262,144 bytes is sent
//!   unframed and refused `frame_too_large`.
//! - **Nothing is kept for a later viewer** (ANIM-21, page-rules.md
//!   RULE-40, RULE-111): an `animate`, `sprites`, `naplps_objects` or
//!   `sprite_sheet` message reaches the connections of its audience that
//!   have its page (number and sub-code) selected when it arrives, and no
//!   other; a program sends a page afresh at every selection. What the
//!   server holds of a sheet is the stream the connection is being sent
//!   for the page it has selected, so that the whole sheet so far is read
//!   at every message, and it is dropped when the connection leaves the
//!   page.
//! - **Timelines** (ANIM-21): played on the server's clock for the
//!   connections that had the page selected when the timeline was sent,
//!   each step sent as an ANIMATE record when its tick is due; a timeline
//!   ends for a connection that leaves the page or whose connection ends,
//!   and for every connection when the program's connection ends.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_core::limits::SUBCODE_ANY;
use neoteletext_rules::animate::Operation;
use neoteletext_web::consts::{MAX_FRAME_BYTES, MAX_FRAME_TICKS, MAX_SHEET_BYTES};

use super::{Channel, Connection, Level};
use crate::animation::{self, tick_ms};
use crate::protocol::{
    self, Animate, ConnectionId, FrameMarkName, FrameMessage, MOST_TIMELINES, MessageType, NaplpsObjects, ObjectPage, ProgramMessage, Refusal, Rule,
    ServerMessage, SheetKindName, SpriteSheet, Sprites, TimelineMessage, TimelineStop,
};

/// The most timelines the server plays for one connection, over all its
/// pages: four pages' worth of the sixteen a page may have (ANIM-21).
pub const MOST_TIMELINES_PER_CONNECTION: usize = MOST_TIMELINES * 4;
/// How long a frame may stay open before the server ends it itself: the
/// two seconds a receiver holds one (ANIM-2, `MAX_FRAME_TICKS`).
pub const FRAME_LIMIT_MS: u64 = tick_ms(MAX_FRAME_TICKS as u64);

/// A sheet a connection is being sent for the page it has selected
/// (ANIM-17): its stream's generation, and its bytes so far, which are
/// read whole at every `sprite_sheet` message.
struct SheetStream {
    page: u16,
    sub: u16,
    kind: SheetKindName,
    generation: u16,
    bytes: Vec<u8>,
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

/// One connection's sheet streams and open frame, all of the page it has
/// selected.
#[derive(Default)]
pub(super) struct Animated {
    /// The sheets it is being sent, of the page it has selected.
    sheets: Vec<SheetStream>,
    /// The number of its last sheet stream: a new one takes the next.
    sheet_generation: u16,
    /// The frame open for the page it shows.
    frame: Option<OpenFrame>,
    /// The number of its last frame.
    frame_identifier: u16,
}

impl Animated {
    /// The whole ReGIS sheet it has been sent of `page` and `sub`: what a
    /// receiver's renderer of that page is fed before the page's stream
    /// (ANIM-17), and so what the page's ReGIS is checked after.
    pub(super) fn regis_sheet(&self, page: u16, sub: u16) -> &[u8] {
        self.sheet_bytes(page, sub, SheetKindName::Regis)
    }

    /// The bytes of its sheet stream of `kind` for `page` and `sub` so far.
    fn sheet_bytes(&self, page: u16, sub: u16, kind: SheetKindName) -> &[u8] {
        self.sheets.iter().find(|stream| stream.page == page && stream.sub == sub && stream.kind == kind).map_or(&[], |stream| stream.bytes.as_slice())
    }

    /// Its selection left its page (RULE-111): its sheets are of a page it
    /// no longer has, and what was held of a frame of it is of a page no
    /// longer shown.
    pub(super) fn left(&mut self) {
        self.sheets.clear();
        self.frame = None;
    }

    /// When its open frame is ended by the server.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.frame.as_ref().map(|frame| frame.due)
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

    /// A connection's selection is now `page` and `sub`, or nothing: its
    /// timelines of any other page end (ANIM-21, RULE-111).
    pub(super) fn left(&mut self, connection: ConnectionId, stays: Option<(u16, u16)>) {
        self.playing.retain(|playing| playing.connection != connection || Some((playing.page, playing.sub)) == stays);
    }

    /// The program's connection ended: every timeline ends (RULE-111 case
    /// 3).
    pub(super) fn clear(&mut self) {
        self.playing.clear();
    }

    /// When the next step of any is due.
    pub(super) fn next_due(&self) -> Option<u64> {
        self.playing.iter().filter_map(Playing::due).min()
    }
}

impl Connection {
    /// The generation of the NAPLPS stream a page's objects belong to
    /// (ANIM-11): the stream it has for the page, or the one its next
    /// `naplps` message with `restart` begins.
    pub(super) fn naplps_generation_for(&self, page: u16, sub: u16) -> u16 {
        self.naplps
            .as_ref()
            .filter(|stream| stream.page == page && stream.sub == sub)
            .map_or_else(|| self.naplps_generation.wrapping_add(1), |stream| stream.generation)
    }
}

/// A refusal of an animation message as a whole.
fn refuse(kind: MessageType, path: &str, rule: Rule, detail: String) -> Refusal {
    Refusal { message: Some(kind), path: path.into(), rule, detail }
}

/// The sub-code 3F7F names no page (DEC-6599, RULE-112): an animation
/// message for it is refused.
fn one_page(kind: MessageType, sub: u16) -> Result<(), Refusal> {
    if sub == SUBCODE_ANY {
        return Err(refuse(kind, "sub", Rule::OutOfRange, String::from("3F7F names no page: a sub-page is a page, named by its own sub-code (DEC-6599)")));
    }
    Ok(())
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

    /// The messages of animation (ANIM-21).
    pub(super) fn act_on_animation(&mut self, message: ProgramMessage, now: u64) -> Result<(), Refusal> {
        let sub = match &message {
            ProgramMessage::Frame(frame) => Some(frame.sub.0),
            ProgramMessage::Animate(animate) => Some(animate.sub.0),
            ProgramMessage::Sprites(sprites) => Some(sprites.sub.0),
            ProgramMessage::NaplpsObjects(objects) => Some(objects.sub.0),
            ProgramMessage::SpriteSheet(sheet) => Some(sheet.sub.0),
            ProgramMessage::Timeline(timeline) => Some(timeline.sub.0),
            ProgramMessage::TimelineStop(stop) => Some(stop.sub.0),
            _ => None,
        };
        if let Some(sub) = sub {
            one_page(message.kind(), sub)?;
        }
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
                // An end whose begin found no connection showing the page,
                // because each had moved on, is passed over as its begin
                // was: only a connection that shows the page with no frame
                // of it open is a fault of order (TODO-6675).
                if targets.is_empty() && !self.showing(&message.connections, page, sub).is_empty() {
                    return Err(refuse(
                        kind,
                        "mark",
                        Rule::FrameOrder,
                        format!("no connection named that shows {page:03X}/{sub:04X} has a frame of it open: an end follows its begin"),
                    ));
                }
                for connection in targets {
                    self.end_frame(connection);
                }
            }
        }
        Ok(())
    }

    /// Operations for one page: sent to each of `targets` that has it
    /// selected, and kept for none (ANIM-6, ANIM-21).
    fn operate(&mut self, targets: &[ConnectionId], page: u16, sub: u16, operations: &[Operation]) {
        for &connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if !state.shows(page, sub) {
                continue;
            }
            let bytes = animation::animate_records(page, sub, operations, &mut state.record_id).unwrap_or_default();
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

    /// `sprites` (ANIM-7): sent to each connection named that has the page
    /// selected, kept for none.
    fn sprites(&mut self, message: &Sprites) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        for connection in self.showing(&message.connections, page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let bytes = animation::sprites_record(page, sub, &message.sprites, &mut state.record_id)
                .map_err(|detail| refuse(MessageType::Sprites, "sprites", Rule::InvalidSprites, detail))?;
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `naplps_objects` (ANIM-11): as `sprites`, for the NAPLPS stream the
    /// connection has of the page.
    fn naplps_objects(&mut self, message: &NaplpsObjects) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        for connection in self.showing(&message.connections, page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let generation = state.naplps_generation_for(page, sub);
            let bytes = animation::naplps_objects_record(page, sub, generation, &message.objects, &mut state.record_id)
                .map_err(|detail| refuse(MessageType::NaplpsObjects, "objects", Rule::InvalidNaplpsObjects, detail))?;
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `sprite_sheet` (ANIM-17): more bytes of one page's sheet, for each
    /// connection named that has the page selected. The whole sheet so far
    /// is read, as its renderer will read it, for every one of them before
    /// anything is sent, and one that would refuse it, or that it would
    /// pass `MAX_SHEET_BYTES` for, refuses the message. With `restart`, or
    /// for a connection that has no stream of it, a new stream begins.
    fn sprite_sheet(&mut self, message: &SpriteSheet) -> Result<(), Refusal> {
        let kind = MessageType::SpriteSheet;
        let (page, sub, of) = (message.page.0, message.sub.0, message.kind);
        let piece = protocol::check::bytes_of(&message.data);
        let targets = self.showing(&message.connections, page, sub);
        let restart = message.restart;
        let held = move |state: &Connection| -> bool {
            !restart && state.animation.sheets.iter().any(|stream| stream.page == page && stream.sub == sub && stream.kind == of)
        };
        let mut read: Vec<Vec<u8>> = Vec::new();
        for connection in &targets {
            let Some(state) = self.connections.get(connection) else { continue };
            let before = if held(state) { state.animation.sheet_bytes(page, sub, of) } else { &[] };
            if before.len().saturating_add(piece.len()) > MAX_SHEET_BYTES {
                let detail = format!("{} bytes after {}: a page's sheet is at most {MAX_SHEET_BYTES}", piece.len(), before.len());
                return Err(refuse(kind, "data", Rule::SheetTooLarge, detail));
            }
            let mut whole = before.to_vec();
            whole.extend_from_slice(&piece);
            // The same sheet for many connections is read once.
            if !read.contains(&whole) {
                animation::read_sheet(of, &whole).map_err(|detail| refuse(kind, "data", Rule::InvalidSheet, detail))?;
                read.push(whole);
            }
        }
        for connection in targets {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            let continuing = held(state);
            let bytes = if continuing {
                let Some(stream) = state.animation.sheets.iter_mut().find(|stream| stream.page == page && stream.sub == sub && stream.kind == of) else {
                    continue;
                };
                let offset = u32::try_from(stream.bytes.len()).unwrap_or(u32::MAX);
                stream.bytes.extend_from_slice(&piece);
                animation::sheet_records((page, sub), of, stream.generation, offset, &piece, &mut state.record_id)
            } else {
                state.animation.sheet_generation = state.animation.sheet_generation.checked_add(1).unwrap_or(1);
                let generation = state.animation.sheet_generation;
                state.animation.sheets.retain(|stream| !(stream.page == page && stream.sub == sub && stream.kind == of));
                state.animation.sheets.push(SheetStream { page, sub, kind: of, generation, bytes: piece.clone() });
                animation::sheet_records((page, sub), of, generation, 0, &piece, &mut state.record_id)
            };
            // A new ReGIS sheet makes a receiver start the page's renderer
            // again (ANIM-17): the ReGIS it was sent is not of this sheet.
            if !continuing && of == SheetKindName::Regis && state.regis.as_ref().is_some_and(|stream| stream.page == page && stream.sub == sub) {
                state.regis = None;
            }
            let bytes = bytes.map_err(|detail| refuse(kind, "data", Rule::InvalidSheet, detail))?;
            self.send_for_page(connection, page, bytes);
        }
        Ok(())
    }

    /// `object_page` (ANIM-7, ANIM-17, TODO-6569): one sub-page of a
    /// teletext object page that a page's sprites take their frames from,
    /// written from its objects (EN 300 706 clause 10.5.1) and sent to the
    /// connections named that have that page selected, before the page
    /// itself. It is no selection's answer: what the connection shows does
    /// not change, and nothing of it is kept (RULE-40), so a program sends
    /// it at each selection, before the page.
    pub(super) fn object_page(&mut self, message: &ObjectPage) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let bytes =
            crate::object_page::packets(&message.object).map_err(|detail| refuse(MessageType::ObjectPage, "object", Rule::InvalidObjectPage, detail))?;
        for connection in self.showing(&message.connections, page, sub) {
            self.send_for_page(connection, page, bytes.clone());
        }
        Ok(())
    }

    /// `timeline` (ANIM-21): played for each connection named, from now.
    fn timeline(&mut self, message: &TimelineMessage, now: u64) -> Result<(), Refusal> {
        let kind = MessageType::Timeline;
        let steps = protocol::check::timeline_steps(message)?;
        let (page, id) = (message.page.0, message.id.get());
        let sub = message.sub.0;
        // Played for the connections that have the page selected now
        // (ANIM-21): it ends for one when it leaves the page.
        let targets = self.showing(&message.connections, page, sub);
        for &connection in &targets {
            let others = |playing: &&Playing| playing.connection == connection && !(playing.page == page && playing.sub == sub && playing.id == id);
            let of_page = self.timelines.playing.iter().filter(others).filter(|playing| playing.page == page && playing.sub == sub).count();
            if of_page >= MOST_TIMELINES {
                return Err(refuse(
                    kind,
                    "id",
                    Rule::TooManyTimelines,
                    format!("connection {connection} has {MOST_TIMELINES} timelines playing for {page:03X}/{sub:04X}"),
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
            self.timelines.playing.retain(|playing| !(playing.connection == connection && playing.page == page && playing.sub == sub && playing.id == id));
            self.timelines.playing.push(Playing { connection, page, sub, id, steps: Arc::clone(&steps), loop_ticks, started_ms: now, loops: 0, next: 0 });
        }
        self.play_timelines(now);
        Ok(())
    }

    /// `timeline_stop`: a timeline that is not playing is passed over.
    fn timeline_stop(&mut self, message: &TimelineStop) {
        let targets = self.targets(&message.connections);
        let (page, sub, id) = (message.page.0, message.sub.0, message.id.get());
        self.timelines.playing.retain(|playing| !(targets.contains(&playing.connection) && playing.page == page && playing.sub == sub && playing.id == id));
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
