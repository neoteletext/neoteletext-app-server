//! What the fuzz targets of the page rule table and of animation share
//! (page-rules.md RULE-83, animation.md ANIM-23): a channel with one
//! connection that shows page 101, sub-page 0001, and the records it is
//! sent, read by the engine's reader.

use neoteletext_input::caps::{CURSOR_CAP, SELECT_CAP};
use neoteletext_input::event::{Event, Item, Select, write};
use neoteletext_web::records::{Read, RecordReader};
use teletext_runtime::channel::DefaultRules;
use teletext_runtime::{Channel, ChannelConfig, Effect, HandleSecret, InitialPage, RandomSource, RandomUnavailable};

/// The connection.
pub const CONNECTION: u64 = 1;
/// The page it shows.
pub const PAGE: u16 = 0x101;
/// Bytes of a packet 4/31 (EN 300 706 clause 7.1.2).
const PACKET_BYTES: usize = 42;

/// Random bytes that count, so that a run repeats.
struct Counting(u8);

impl RandomSource for Counting {
    fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable> {
        for byte in bytes {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// A channel whose one connection shows [`PAGE`], sub-page 0001.
pub fn channel() -> Channel {
    let config = ChannelConfig {
        name: "Fuzz".into(),
        address: "http://127.0.0.1/fuzz/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: DefaultRules::None,
    };
    let mut channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(Counting(0)));
    channel.program_started(0);
    channel.opened(CONNECTION, None, 0);
    let mut session = Vec::new();
    for item in [
        Item::Event(Event::Hello { version: neoteletext_input::VERSION, caps: CURSOR_CAP | SELECT_CAP, agent: None }),
        Item::Event(Event::Select(Select::new(PAGE, 1).unwrap(), "")),
    ] {
        let mut one = [0; 64];
        let length = write(&item, &mut one).unwrap();
        session.extend_from_slice(&one[..length]);
    }
    channel.received(CONNECTION, &session, 0);
    let _ = channel.take_effects();
    channel
}

/// Whether the program was told `refused`.
pub fn refused(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| matches!(effect, Effect::ToProgram { line, .. } if line.starts_with(br#"{"type":"refused""#)))
}

/// Reads every record `effects` send with the engine's reader, which must
/// take each: the server sends nothing a receiver refuses. Gives the body
/// of each RULES record.
pub fn records(effects: &[Effect]) -> Vec<Vec<u8>> {
    let mut reader = RecordReader::new();
    let mut tables = Vec::new();
    for effect in effects {
        let Effect::Send { bytes, .. } = effect else { continue };
        for packet in bytes.chunks_exact(PACKET_BYTES) {
            let packet = neoteletext_core::packet::Packet { bytes: packet.try_into().unwrap() };
            match reader.push(&packet) {
                Read::Invalid(_) => panic!("the engine's reader refuses a record the server sent"),
                Read::Record(neoteletext_web::records::ChannelRecord::Rules(record)) => tables.push(record.body.to_vec()),
                Read::Record(_) | Read::Nothing => {}
            }
        }
    }
    tables
}
