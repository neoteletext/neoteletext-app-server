//! What a host keeps of a connection while the channel's memory is lost
//! and the connection stays open: the Cloudflare host's Durable Object,
//! whose memory goes at each hibernation while its connections stay open
//! (DEC-1755 option 1, TODO-4579). The host keeps a [`Resume`] with each
//! connection, as its WebSocket's attachment, and gives it back to a new
//! [`Channel`] with [`Channel::resumed`]; nothing of it outlives the
//! connection, since the attachment goes with its WebSocket.
//!
//! What it holds is what the receiver and the program cannot ask for
//! again: the numbers the receiver counts on (its input count acted on,
//! the next record number, the index's and each picture stream's
//! generation, DOC-1943, DOC-1955), WELCOME's agreement, the selection
//! with its query and number, and the viewer it proved (VS-202). The rest
//! (the sub-page shown, the page's rule table, its animation and
//! pictures) the program sends again when it is told the connection with
//! `connected` after `start`, as after a program's restart (REQ-1481).

use alloc::string::String;

use neoteletext_input::MAX_PAGE_PARAMETERS;
use neoteletext_input::state::Lifetime;
use neoteletext_web::viewer_state::KeyId;
use serde::{Deserialize, Serialize};

use super::{Channel, Connection, Ending};
use crate::protocol::{ConnectionId, PageNumber, RequestNumber, Selection, SubCode};

/// One connection's state as a host keeps it across the loss of the
/// channel's memory. Its fields are the runtime's own: a host stores it
/// whole and gives it back unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    /// WELCOME was sent.
    welcomed: bool,
    /// The capabilities WELCOME agreed.
    agreed: Option<u32>,
    /// The input count acted on (DOC-1943).
    acked: u16,
    /// The next record number on the audio and input service.
    record_id: u8,
    /// The index's sending number (DOC-1955).
    index_generation: u16,
    /// The last ReGIS and NAPLPS streams' numbers.
    regis_generation: u16,
    naplps_generation: u16,
    /// The selection's number.
    request: RequestNumber,
    /// The page and sub-code selected.
    selection: Option<(u16, u16)>,
    /// The selection's query (REQ-1693 item 6).
    parameters: Option<String>,
    /// The viewer it proved: its key identifier and whether the viewer is
    /// remembered (VS-302: an identifier, never a key).
    viewer: Option<(KeyId, bool)>,
}

impl Channel {
    /// What a host keeps of `connection` to give it back after the
    /// channel's memory is lost, or none for a connection the channel does
    /// not hold as a viewer's (a snapshot connection starts again).
    #[must_use]
    pub fn resume_of(&self, connection: ConnectionId) -> Option<Resume> {
        let state = self.connections.get(&connection)?;
        Some(Resume {
            welcomed: state.welcomed,
            agreed: state.agreed,
            acked: state.acked,
            record_id: state.record_id,
            index_generation: state.index_generation,
            regis_generation: state.regis_generation,
            naplps_generation: state.naplps_generation,
            request: state.request,
            selection: state.selection.map(|selection| (selection.page.0, selection.sub.0)),
            parameters: state.parameters.clone(),
            viewer: state.viewer.presented().map(|(key_id, lifetime)| (key_id, lifetime == Lifetime::Remembered)),
        })
    }

    /// A connection that was open when the channel's memory was lost, as
    /// [`Channel::resume_of`] gave it, opened again in this channel. It is
    /// as it was to its receiver: no WELCOME or challenge again, and its
    /// numbers run on. Its program is told it with `connected` when
    /// [`Channel::program_started`] next tells it the connections open,
    /// with its selection, query and viewer, and sends its page again.
    /// A query longer than SELECT carries is not taken (the stored state is
    /// read as untrusted).
    pub fn resumed(&mut self, connection: ConnectionId, address: Option<core::net::IpAddr>, resume: Resume, now: u64) {
        let mut state = Connection::new(address, now);
        state.welcomed = resume.welcomed;
        state.agreed = resume.agreed;
        state.acked = resume.acked;
        state.record_id = resume.record_id;
        state.index_generation = resume.index_generation;
        state.regis_generation = resume.regis_generation;
        state.naplps_generation = resume.naplps_generation;
        state.request = resume.request;
        state.selection = resume.selection.map(|(page, sub)| Selection { page: PageNumber(page), sub: SubCode(sub) });
        state.parameters = resume.parameters.filter(|parameters| parameters.len() <= MAX_PAGE_PARAMETERS);
        // Its first selection and its opening challenge were over before
        // the memory was lost, or are given up now: `connected` goes when
        // the program starts.
        state.first_selection_due = None;
        state.ending = Ending::Answered;
        if state.agreed.is_some() {
            self.stats.joined();
        }
        self.connections.insert(connection, state);
        if let Some((key_id, remembered)) = resume.viewer {
            self.viewer_resumed(connection, key_id, if remembered { Lifetime::Remembered } else { Lifetime::Visit }, now);
        }
    }
}
