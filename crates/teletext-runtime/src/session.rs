//! One connection's session (INPUT.md section 4): its input stream, the
//! input count it has acted on, the page open with the keys, its record
//! numbers, and the pages it owns privately. A reconnection is a new
//! session.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;

use neoteletext_input::event::{MAX_STREAM_ITEM, StreamDecoder};

use crate::channel::Held;

/// A session's state.
pub(crate) struct Session {
    /// The input stream's decoder, made at the first input: most viewers
    /// of a channel never type, and the decoder holds a paste's worth of
    /// bytes (decision 7 item 4).
    pub decoder: Option<Box<StreamDecoder<MAX_STREAM_ITEM>>>,
    /// WELCOME was sent: the receiver acts on cursor and mode records.
    pub welcomed: bool,
    /// The input count acted on, modulo 65 536 (INPUT.md section 7.1).
    pub acked: u16,
    /// The page open with the keys, as PAGE last said; 0 for none.
    pub page: u16,
    /// A page the channel sent the viewer to, until PAGE says it is open:
    /// acknowledgements name it meanwhile, since the receiver goes to the
    /// page of the last cursor record it reads (INPUT.md section 7.1).
    pub went: Option<u16>,
    /// The next record number on the audio and input service.
    pub record_id: u8,
    /// Pages this session owns: each viewer's own game, typed form or
    /// adventure (`SERVER.md` section 2).
    pub own: BTreeMap<u16, Held>,
}

impl Session {
    pub(crate) const fn new() -> Self {
        Self { decoder: None, welcomed: false, acked: 0, page: 0, went: None, record_id: 0, own: BTreeMap::new() }
    }

    /// The next record number, advanced.
    pub(crate) fn next_record(&mut self) -> u8 {
        let id = self.record_id;
        self.record_id = id.wrapping_add(1);
        id
    }
}
