//! A channel's pictures (DEC-6335; the NAPLPS renderer's REQ-6408 item 3;
//! neoteletext-common `docs/renderer-parity.md` PAR-16 to PAR-24): each
//! connection's stream of ReGIS or NAPLPS for the sub-page it shows, read
//! by the receiver's own reader before anything is sent; the pictures
//! kept for a page and sub-code and given to every connection that shows
//! it ([`crate::kept`]); the links and cursor of either language; and only
//! to connections that agreed the language's capability bit.
//!
//! **A kept picture reaches a connection** right after the frame of the
//! page that answers its selection, as a new generation of its stream of
//! the language from offset 0, then the objects, links and cursor
//! (PAR-18). It goes on taking the kept picture's later messages until its
//! selection changes, unless a message naming connections begins a stream
//! of its own, which takes no kept message until the page is selected
//! again.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_input::caps::{NAPLPS_CAP, REGIS_CAP};

use super::{Channel, Connection, Level};
use crate::kept::{self, Checker, Companion, KeptPicture, KeptRefusal, Notes, PictureLanguage, StreamChange};
use crate::protocol::{
    self, AuthorLevel, AuthorLog, ConnectionId, Connections, DrawingLink, MessageType, NaplpsObjects, PictureAudience, ProgramMessage, Refusal, Rule,
    ServerMessage,
};
use crate::records;

/// One connection's stream of a picture language for the sub-page it
/// shows.
pub(super) struct PictureStream {
    pub(super) page: u16,
    pub(super) sub: u16,
    pub(super) generation: u16,
    /// The bytes sent of it.
    pub(super) offset: u32,
    /// The receiver's reader of a stream the program sent this connection
    /// itself; none for a kept picture's, which its own reader checks.
    pub(super) checker: Option<Checker>,
    /// It is the kept picture of its page and sub-code (PAR-18).
    pub(super) kept: bool,
    /// The bytes of the message being acted on were read and are to be sent.
    pub(super) pending: bool,
    /// Its objects were logged as naming a macro it has not defined
    /// (PAR-23: once a generation).
    pub(super) macro_logged: bool,
}

/// The capability bit of a picture language (PAR-1).
const fn capability(language: PictureLanguage) -> u32 {
    match language {
        PictureLanguage::Regis => REGIS_CAP,
        PictureLanguage::Naplps => NAPLPS_CAP,
    }
}

/// The message types of a language's stream, links and cursor.
const fn stream_type(language: PictureLanguage) -> MessageType {
    match language {
        PictureLanguage::Regis => MessageType::Regis,
        PictureLanguage::Naplps => MessageType::Naplps,
    }
}

/// A refusal of what cannot be kept.
fn kept_refusal(kind: MessageType, refused: KeptRefusal) -> Refusal {
    Refusal { message: Some(kind), path: refused.path.to_string(), rule: refused.rule, detail: refused.detail }
}

/// The links of `naplps_links` as the record writes `regis_links`'s: a
/// number is an object when the sub-page's picture is NAPLPS (PAR-2).
fn object_links(links: &[protocol::ObjectLink]) -> Vec<DrawingLink> {
    links.iter().map(|link| DrawingLink { drawing: link.object, target: link.target.clone() }).collect()
}

impl Connection {
    /// Whether WELCOME agreed `language`'s capability bit (PAR-24).
    pub(super) fn agrees(&self, language: PictureLanguage) -> bool {
        self.agreed.is_some_and(|agreed| agreed & capability(language) != 0)
    }

    /// Its stream of `language`.
    pub(super) const fn stream_mut(&mut self, language: PictureLanguage) -> &mut Option<PictureStream> {
        match language {
            PictureLanguage::Regis => &mut self.regis,
            PictureLanguage::Naplps => &mut self.naplps,
        }
    }

    /// Its stream of `language`.
    pub(super) const fn stream(&self, language: PictureLanguage) -> Option<&PictureStream> {
        match language {
            PictureLanguage::Regis => self.regis.as_ref(),
            PictureLanguage::Naplps => self.naplps.as_ref(),
        }
    }

    /// The next generation of its stream of `language`.
    const fn next_generation(&mut self, language: PictureLanguage) -> u16 {
        let counter = match language {
            PictureLanguage::Regis => &mut self.regis_generation,
            PictureLanguage::Naplps => &mut self.naplps_generation,
        };
        *counter = counter.wrapping_add(1);
        *counter
    }

    /// Whether the sub-page whose frame it was sent last is `page`, `sub`.
    fn has_shown(&self, page: u16, sub: u16) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.page == page && shown.sub == sub)
    }

    /// Whether it takes the kept picture of `page`, `sub` in `language`:
    /// its stream of it is the kept one.
    fn takes_kept(&self, language: PictureLanguage, page: u16, sub: u16) -> bool {
        self.stream(language).is_some_and(|stream| stream.kept && stream.page == page && stream.sub == sub)
    }

    /// Whether a message naming connections began a stream of `page`, `sub`
    /// in `language` for it, which takes no kept message (PAR-18).
    fn has_own_stream(&self, language: PictureLanguage, page: u16, sub: u16) -> bool {
        self.stream(language).is_some_and(|stream| !stream.kept && stream.page == page && stream.sub == sub)
    }

    /// Reads a message's bytes as its receiver will, starting the
    /// sub-page's stream again when asked; a connection with no stream of
    /// its own of the sub-page is passed over until one is started. A
    /// ReGIS stream begins with the page's sheet (ANIM-17).
    ///
    /// # Errors
    /// What the receiver would refuse (REQ-6332, PAR-23).
    fn read_own(&mut self, language: PictureLanguage, page: u16, sub: u16, restart: bool, bytes: &[u8]) -> Result<Notes, String> {
        if restart {
            let generation = self.next_generation(language);
            let mut checker = Checker::new(language);
            if language == PictureLanguage::Regis {
                // The page's sheet is fed to its renderer before the page's
                // own stream (ANIM-17; a sheet is one page's, DEC-6599): it
                // was read when it was given, so what it reports is not this
                // message's.
                let _ = checker.feed(self.animation.regis_sheet(page, sub));
            }
            *self.stream_mut(language) =
                Some(PictureStream { page, sub, generation, offset: 0, checker: Some(checker), kept: false, pending: false, macro_logged: false });
        }
        let Some(stream) = self.stream_mut(language).as_mut().filter(|stream| stream.page == page && stream.sub == sub && !stream.kept) else {
            return Ok(Vec::new());
        };
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        if stream.offset.checked_add(length).is_none() {
            return Err(format!("the sub-page's {} passed 4,294,967,295 bytes; start it again", language.name()));
        }
        let notes = match stream.checker.as_mut() {
            Some(checker) => checker.feed(bytes).map_err(|refused| refused.detail)?,
            None => Vec::new(),
        };
        stream.pending = true;
        Ok(notes)
    }

    /// The records of the bytes [`Connection::read_own`] just read, or
    /// nothing for a connection it passed over.
    fn own_records(&mut self, language: PictureLanguage, bytes: &[u8]) -> Result<Vec<u8>, records::Refused> {
        let mut record_id = self.record_id;
        let Some(stream) = self.stream_mut(language).as_mut().filter(|stream| stream.pending) else { return Ok(Vec::new()) };
        stream.pending = false;
        let sent = stream_records(language, stream, bytes, &mut record_id)?;
        self.record_id = record_id;
        Ok(sent)
    }

    /// The records that begin `kept`, the kept picture of `page`, `sub`, for
    /// this connection (PAR-18): its stream as a new generation from offset
    /// 0, then its objects, its links and its cursor; its stream of the
    /// language is the kept one from then.
    pub(super) fn begin_kept(&mut self, page: u16, sub: u16, kept: &KeptPicture) -> Result<Vec<u8>, records::Refused> {
        let language = kept.language;
        let generation = self.next_generation(language);
        let mut stream = PictureStream { page, sub, generation, offset: 0, checker: None, kept: true, pending: false, macro_logged: false };
        let mut bytes = stream_records(language, &mut stream, &kept.stream, &mut self.record_id)?;
        *self.stream_mut(language) = Some(stream);
        if let Some(objects) = &kept.objects {
            let set = crate::animation::NaplpsSet { objects: &objects.objects, title: objects.title.as_ref().map(|title| title.0.as_str()), one: false };
            bytes.extend_from_slice(&crate::animation::naplps_objects_record(page, sub, generation, set, &mut self.record_id)?);
        }
        if let Some(links) = &kept.links {
            bytes.extend_from_slice(&records::drawing_links(page, sub, links, &mut self.record_id)?);
        }
        if let Some(cursor) = kept.cursor {
            bytes.extend_from_slice(&records::regis_cursor(page, sub, cursor, &mut self.record_id)?);
        }
        Ok(bytes)
    }
}

/// The records of `bytes` on at `stream`'s offset, which moves on.
fn stream_records(language: PictureLanguage, stream: &mut PictureStream, bytes: &[u8], record_id: &mut u8) -> Result<Vec<u8>, records::Refused> {
    let sent = match language {
        PictureLanguage::Regis => records::regis(stream.page, stream.sub, stream.generation, stream.offset, bytes, record_id)?,
        PictureLanguage::Naplps => records::naplps(stream.page, stream.sub, stream.generation, stream.offset, bytes, record_id)?,
    };
    stream.offset = stream.offset.saturating_add(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
    Ok(sent)
}

/// What a kept message changed, for the connections taking the picture.
enum Delta<'a> {
    /// The stream began again with these bytes.
    Restarted(&'a [u8]),
    /// These bytes were added to the stream.
    Appended(&'a [u8]),
    /// The objects changed.
    Objects(crate::animation::NaplpsSet<'a>),
    /// The links changed.
    Links(&'a [DrawingLink]),
    /// The cursor changed.
    Cursor(u16),
}

impl Channel {
    /// A picture message (PAR-16 to PAR-22).
    pub(super) fn act_on_picture(&mut self, message: &ProgramMessage) -> Result<(), Refusal> {
        match message {
            ProgramMessage::Regis(message) => self.regis(message),
            ProgramMessage::RegisLinks(message) => self.regis_links(message),
            ProgramMessage::RegisCursor(message) => self.regis_cursor(message),
            ProgramMessage::Naplps(message) => self.naplps(message),
            ProgramMessage::NaplpsLinks(message) => self.naplps_links(message),
            ProgramMessage::NaplpsCursor(message) => self.naplps_cursor(message),
            ProgramMessage::ForgetPicture(message) => self.forget_picture(message),
            ProgramMessage::MeasurePictureText(measure) => {
                // PAR-22: the encoder's figures, sent to no viewer.
                let measured = crate::picture_text::measure(measure)?;
                self.tell(&ServerMessage::MeasuredPictureText(measured));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The open connections whose sub-page shown is `page`, `sub`: who a
    /// kept picture of it goes to.
    fn viewers_of(&self, page: u16, sub: u16) -> Vec<ConnectionId> {
        self.connections.iter().filter(|(_, state)| state.has_shown(page, sub)).map(|(&id, _)| id).collect()
    }

    /// The records of a kept picture that begin it for `connection`, if it
    /// shows the page now, agreed the picture's language and takes nothing
    /// of it yet, and has no stream of its own of the page: what goes right
    /// after the frame that answers its selection (PAR-18).
    pub(super) fn kept_opening(&mut self, connection: ConnectionId, page: u16, sub: u16) -> Vec<u8> {
        let Some(picture) = self.kept.get(page, sub) else { return Vec::new() };
        let language = picture.language;
        let Some(state) = self.connections.get_mut(&connection) else { return Vec::new() };
        if !state.agrees(language) || state.takes_kept(language, page, sub) || state.has_own_stream(language, page, sub) {
            return Vec::new();
        }
        match state.begin_kept(page, sub, picture) {
            Ok(bytes) => bytes,
            Err(fault) => {
                self.log(Level::Error, "kept_refused", format!("{page:03X}/{sub:04X}: the engine refused the kept picture's records: {fault}"));
                Vec::new()
            }
        }
    }

    /// Gives a kept picture's change to every connection showing it that
    /// agreed its language (PAR-18, PAR-24): the change to those that take
    /// it, the whole picture to those that take nothing of the page yet.
    fn deliver_kept(&mut self, page: u16, sub: u16, delta: &Delta<'_>) {
        let Some(language) = self.kept.get(page, sub).map(|picture| picture.language) else { return };
        let mut faults: Vec<String> = Vec::new();
        for connection in self.viewers_of(page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if !state.agrees(language) || state.has_own_stream(language, page, sub) {
                continue;
            }
            let bytes = if state.takes_kept(language, page, sub) {
                kept_change(state, language, page, sub, delta)
            } else if let Some(picture) = self.kept.get(page, sub) {
                state.begin_kept(page, sub, picture)
            } else {
                continue;
            };
            match bytes {
                Ok(bytes) => self.send_for_page(connection, page, bytes),
                Err(fault) => faults.push(fault),
            }
        }
        for fault in faults {
            self.log(Level::Error, "kept_refused", format!("{page:03X}/{sub:04X}: the engine refused a kept picture's records: {fault}"));
        }
    }

    /// Lines for the author's log of the connections showing `page` and
    /// `sub` (PAR-23), each held to the record's length.
    pub(super) fn author_notes(&mut self, targets: &[ConnectionId], page: u16, notes: Notes) {
        if targets.is_empty() {
            return;
        }
        for note in notes {
            let mut text = String::new();
            for character in note.chars() {
                if text.len().saturating_add(character.len_utf8()) > neoteletext_formats::log::MAX_LOG_TEXT_BYTES {
                    break;
                }
                text.push(character);
            }
            let line = AuthorLog { connections: Connections::Listed(targets.to_vec()), level: AuthorLevel::W, text, page: Some(protocol::PageNumber(page)) };
            let _ = self.send_records(targets, MessageType::AuthorLog, None, |state| records::author_log(&line, &mut state.record_id));
        }
    }

    /// `regis` or `naplps` (DEC-6335, REQ-6408 item 3, PAR-16, PAR-23):
    /// kept, or to the connections named that show the sub-page and agreed
    /// the language. Those named are read before any is sent; one whose
    /// receiver would refuse the bytes refuses the message, and the streams
    /// of every target, which read it, are started again.
    pub(super) fn picture_stream(
        &mut self,
        language: PictureLanguage,
        audience: &PictureAudience,
        page: u16,
        sub: u16,
        restart: bool,
        bytes: &[u8],
    ) -> Result<(), Refusal> {
        let kind = stream_type(language);
        let Some(connections) = audience.connections() else {
            let (change, notes) = self.kept.stream(page, sub, language, restart, bytes).map_err(|refused| kept_refusal(kind, refused))?;
            let viewers = self.viewers_of(page, sub);
            self.author_notes(&viewers, page, notes);
            let delta = match change {
                StreamChange::Restarted => Delta::Restarted(bytes),
                StreamChange::Appended => Delta::Appended(bytes),
            };
            self.deliver_kept(page, sub, &delta);
            return Ok(());
        };
        let targets: Vec<ConnectionId> = self
            .showing(connections, page, sub)
            .into_iter()
            .filter(|connection| self.connections.get(connection).is_some_and(|state| state.agrees(language)))
            .collect();
        let mut notes: Notes = Vec::new();
        for connection in &targets {
            let Some(state) = self.connections.get_mut(connection) else { continue };
            match state.read_own(language, page, sub, restart, bytes) {
                Ok(found) => {
                    for note in found {
                        if !notes.contains(&note) {
                            notes.push(note);
                        }
                    }
                }
                Err(detail) => {
                    for connection in &targets {
                        if let Some(state) = self.connections.get_mut(connection) {
                            *state.stream_mut(language) = None;
                        }
                    }
                    let rule = match language {
                        PictureLanguage::Regis => Rule::InvalidRegis,
                        PictureLanguage::Naplps => Rule::InvalidNaplps,
                    };
                    return Err(Refusal { message: Some(kind), path: "data".to_string(), rule, detail });
                }
            }
        }
        self.send_records(&targets, kind, Some(page), |state| state.own_records(language, bytes))?;
        self.author_notes(&targets, page, notes);
        Ok(())
    }

    /// `regis`.
    pub(super) fn regis(&mut self, message: &protocol::Regis) -> Result<(), Refusal> {
        let bytes = message.data.as_ref().map(|data| protocol::check::bytes_of(data)).unwrap_or_default();
        self.picture_stream(PictureLanguage::Regis, &message.connections, message.page.0, message.sub.0, message.restart, &bytes)
    }

    /// `naplps`.
    pub(super) fn naplps(&mut self, message: &protocol::Naplps) -> Result<(), Refusal> {
        let bytes = message.data.as_ref().map(|data| protocol::check::bytes_of(data)).unwrap_or_default();
        self.picture_stream(PictureLanguage::Naplps, &message.connections, message.page.0, message.sub.0, message.restart, &bytes)
    }

    /// A companion of a kept picture (PAR-16): kept, then given to the
    /// connections taking the picture.
    fn kept_companion(&mut self, kind: MessageType, language: PictureLanguage, page: u16, sub: u16, companion: Companion) -> Result<(), Refusal> {
        let line_bytes = self.line_bytes;
        self.kept.companion(page, sub, language, companion, line_bytes).map_err(|refused| kept_refusal(kind, refused))?;
        let Some(picture) = self.kept.get(page, sub) else { return Ok(()) };
        let (links, cursor) = (picture.links.clone(), picture.cursor);
        match kind {
            MessageType::RegisLinks | MessageType::NaplpsLinks => self.deliver_kept(page, sub, &Delta::Links(links.as_deref().unwrap_or_default())),
            MessageType::RegisCursor | MessageType::NaplpsCursor => self.deliver_kept(page, sub, &Delta::Cursor(cursor.unwrap_or(0))),
            _ => {}
        }
        Ok(())
    }

    /// Links of a picture (REQ-6372, PAR-20): kept, or to the connections
    /// named that show the sub-page and agreed the language.
    fn picture_links(
        &mut self,
        kind: MessageType,
        language: PictureLanguage,
        audience: &PictureAudience,
        page: u16,
        sub: u16,
        links: Vec<DrawingLink>,
    ) -> Result<(), Refusal> {
        let Some(connections) = audience.connections() else { return self.kept_companion(kind, language, page, sub, Companion::Links(links)) };
        let targets = self.agreeing(connections, language, page, sub);
        self.send_records(&targets, kind, Some(page), |state| records::drawing_links(page, sub, &links, &mut state.record_id))
    }

    /// A picture's cursor (REQ-6372, PAR-20): kept, or to the connections
    /// named that show the sub-page and agreed the language.
    fn picture_cursor(
        &mut self,
        kind: MessageType,
        language: PictureLanguage,
        audience: &PictureAudience,
        page: u16,
        sub: u16,
        number: u16,
    ) -> Result<(), Refusal> {
        let Some(connections) = audience.connections() else { return self.kept_companion(kind, language, page, sub, Companion::Cursor(number)) };
        let targets = self.agreeing(connections, language, page, sub);
        self.send_records(&targets, kind, Some(page), |state| records::regis_cursor(page, sub, number, &mut state.record_id))
    }

    /// The connections named that show the sub-page and agreed `language`.
    pub(super) fn agreeing(&self, connections: &Connections, language: PictureLanguage, page: u16, sub: u16) -> Vec<ConnectionId> {
        self.showing(connections, page, sub)
            .into_iter()
            .filter(|connection| self.connections.get(connection).is_some_and(|state| state.agrees(language)))
            .collect()
    }

    /// `regis_links`.
    pub(super) fn regis_links(&mut self, message: &protocol::RegisLinks) -> Result<(), Refusal> {
        self.picture_links(MessageType::RegisLinks, PictureLanguage::Regis, &message.connections, message.page.0, message.sub.0, message.links.clone())
    }

    /// `regis_cursor`.
    pub(super) fn regis_cursor(&mut self, message: &protocol::RegisCursor) -> Result<(), Refusal> {
        self.picture_cursor(MessageType::RegisCursor, PictureLanguage::Regis, &message.connections, message.page.0, message.sub.0, message.drawing)
    }

    /// `naplps_links` (PAR-20): as `regis_links`, of objects.
    pub(super) fn naplps_links(&mut self, message: &protocol::NaplpsLinks) -> Result<(), Refusal> {
        let links = object_links(&message.links);
        self.picture_links(MessageType::NaplpsLinks, PictureLanguage::Naplps, &message.connections, message.page.0, message.sub.0, links)
    }

    /// `naplps_cursor` (PAR-20): as `regis_cursor`, of an object.
    pub(super) fn naplps_cursor(&mut self, message: &protocol::NaplpsCursor) -> Result<(), Refusal> {
        self.picture_cursor(MessageType::NaplpsCursor, PictureLanguage::Naplps, &message.connections, message.page.0, message.sub.0, message.object)
    }

    /// `naplps_objects` to `"kept"` (PAR-16): kept with the picture, and
    /// given to the connections taking it; logged once a generation when
    /// they name a macro the stream has not defined (PAR-23).
    pub(super) fn kept_naplps_objects(&mut self, message: &NaplpsObjects) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let companion = Companion::Objects { objects: message.objects.clone(), title: message.title.clone(), one: message.one };
        self.kept_companion(MessageType::NaplpsObjects, PictureLanguage::Naplps, page, sub, companion)?;
        let missing = self
            .kept
            .get(page, sub)
            .filter(|picture| !picture.undefined_macro_logged)
            .and_then(|picture| picture.defined_macros().map(|defined| kept::undefined_macros(&message.objects, &defined)))
            .unwrap_or_default();
        if !missing.is_empty() {
            self.kept.mark_logged(page, sub);
            let viewers = self.viewers_of(page, sub);
            self.author_notes(&viewers, page, alloc::vec![undefined_note(&missing)]);
        }
        self.deliver_kept(page, sub, &Delta::Objects(message.into()));
        Ok(())
    }

    /// For `naplps_objects` to connections (PAR-23): a line for the
    /// author's log of each whose stream has not defined a macro the
    /// objects name, once a generation.
    pub(super) fn undefined_macro_notes(&mut self, targets: &[ConnectionId], message: &NaplpsObjects) {
        let (page, sub) = (message.page.0, message.sub.0);
        let mut told: Vec<(ConnectionId, Vec<u8>)> = Vec::new();
        for connection in targets {
            let Some(state) = self.connections.get_mut(connection) else { continue };
            let Some(stream) = state.naplps.as_mut().filter(|stream| stream.page == page && stream.sub == sub && !stream.macro_logged) else { continue };
            let Some(defined) = stream.checker.as_ref().and_then(Checker::defined_macros) else { continue };
            let missing = kept::undefined_macros(&message.objects, &defined);
            if !missing.is_empty() {
                stream.macro_logged = true;
                told.push((*connection, missing));
            }
        }
        for (connection, missing) in told {
            self.author_notes(&[connection], page, alloc::vec![undefined_note(&missing)]);
        }
    }

    /// `forget_picture` (PAR-19): the kept picture dropped, and every
    /// connection that took it sent an empty picture: a new generation of
    /// no bytes, no links and no cursor.
    pub(super) fn forget_picture(&mut self, message: &protocol::ForgetPicture) -> Result<(), Refusal> {
        let (page, sub) = (message.page.0, message.sub.0);
        let forgotten = self.kept.forget(page, sub).map_err(|refused| kept_refusal(MessageType::ForgetPicture, refused))?;
        let language = forgotten.language;
        let mut faults = Vec::new();
        for connection in self.viewers_of(page, sub) {
            let Some(state) = self.connections.get_mut(&connection) else { continue };
            if !state.takes_kept(language, page, sub) {
                continue;
            }
            let emptied = empty_picture(state, language, page, sub, &forgotten);
            *state.stream_mut(language) = None;
            match emptied {
                Ok(bytes) => self.send_for_page(connection, page, bytes),
                Err(fault) => faults.push(fault),
            }
        }
        for fault in faults {
            self.log(Level::Error, "kept_refused", format!("{page:03X}/{sub:04X}: the engine refused a forgotten picture's records: {fault}"));
        }
        Ok(())
    }
}

/// The author's line for objects that name macros not yet defined.
fn undefined_note(missing: &[u8]) -> String {
    let codes: Vec<String> = missing.iter().map(|code| format!("0x{code:02X}")).collect();
    format!("NAPLPS objects name macros the stream has not defined yet: {}", codes.join(", "))
}

/// The records of a kept change for a connection taking the picture.
fn kept_change(state: &mut Connection, language: PictureLanguage, page: u16, sub: u16, delta: &Delta<'_>) -> Result<Vec<u8>, records::Refused> {
    match delta {
        Delta::Restarted(bytes) => {
            let generation = state.next_generation(language);
            let mut stream = PictureStream { page, sub, generation, offset: 0, checker: None, kept: true, pending: false, macro_logged: false };
            let sent = stream_records(language, &mut stream, bytes, &mut state.record_id)?;
            *state.stream_mut(language) = Some(stream);
            Ok(sent)
        }
        Delta::Appended(bytes) => {
            let mut record_id = state.record_id;
            let Some(stream) = state.stream_mut(language).as_mut() else { return Ok(Vec::new()) };
            let sent = stream_records(language, stream, bytes, &mut record_id)?;
            state.record_id = record_id;
            Ok(sent)
        }
        Delta::Objects(set) => {
            let generation = state.stream(language).map_or(0, |stream| stream.generation);
            crate::animation::naplps_objects_record(page, sub, generation, *set, &mut state.record_id)
        }
        Delta::Links(links) => records::drawing_links(page, sub, links, &mut state.record_id),
        Delta::Cursor(number) => records::regis_cursor(page, sub, *number, &mut state.record_id),
    }
}

/// An empty picture for a connection that took a forgotten one (PAR-19): a
/// new generation of no bytes, and the links and cursor it had removed.
fn empty_picture(state: &mut Connection, language: PictureLanguage, page: u16, sub: u16, forgotten: &KeptPicture) -> Result<Vec<u8>, records::Refused> {
    let generation = state.next_generation(language);
    let mut stream = PictureStream { page, sub, generation, offset: 0, checker: None, kept: true, pending: false, macro_logged: false };
    let mut bytes = stream_records(language, &mut stream, &[], &mut state.record_id)?;
    if forgotten.links.is_some() {
        bytes.extend_from_slice(&records::drawing_links(page, sub, &[], &mut state.record_id)?);
    }
    if forgotten.cursor.is_some() {
        bytes.extend_from_slice(&records::regis_cursor(page, sub, 0, &mut state.record_id)?);
    }
    Ok(bytes)
}
