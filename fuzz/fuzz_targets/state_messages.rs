//! Viewer state's readers and verifier on the server (DEC-6471, VS-204,
//! VS-301, VS-352): a channel whose two connections send any input
//! streams, proofs that verify for the challenge outstanding or not, and
//! whose program sends any line, viewer state's requests among them, with
//! time passing. No panic, whatever the bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use neoteletext_input::event::{Event, Item, write};
use neoteletext_input::state::{Algorithm, CHALLENGE_BYTES, Lifetime, StateProof};
use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
use neoteletext_web::viewer_state::{self, ChannelIdentity, MAX_SIGNED_BYTES};
use ring::signature::{Ed25519KeyPair, KeyPair};
use teletext_runtime::{Channel, ChannelConfig, Effect, HandleSecret, InitialPage, Packet, RandomSource, RandomUnavailable};

/// The channel's address, which proofs name.
const ADDRESS: &str = "http://127.0.0.1/fuzz/";
/// The time between messages, in milliseconds: a few steps cross the
/// gesture's 7,000 ms and the answer's 10,000 ms.
const STEP: u64 = 1_500;
/// What each message is, by its first byte: a connection's bytes, a proof
/// of a connection's challenge, or the program's line.
const KINDS: u8 = 5;
/// The kind byte that is the program's line.
const PROGRAM: u8 = 4;
/// The bytes of a packet.
const PACKET_BYTES: usize = 42;
/// The most an item takes, written.
const MOST_ITEM_BYTES: usize = 256;
/// Past every viewer's idle time, so the table empties at the end.
const LATER_MS: u64 = 10 * 60 * 1_000;
/// HELLO with version 8 and the CURSOR and SELECT capabilities.
const HELLO: [u8; 7] = *b"\xFF\x09\x08\x80\x80\x88\x02";

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

/// The latest challenge each connection was sent.
fn note_challenges(effects: &[Effect], latest: &mut [Option<[u8; CHALLENGE_BYTES]>; 2], readers: &mut [RecordReader; 2]) {
    for effect in effects {
        let Effect::Send { connection, bytes } = effect else { continue };
        let Some(slot) = connection.checked_sub(1).and_then(|index| usize::try_from(index).ok()).filter(|index| *index < 2) else { continue };
        for packet in bytes.as_chunks::<PACKET_BYTES>().0 {
            let packet = Packet { bytes: *packet };
            if let Read::Record(ChannelRecord::StateChallenge(record)) = readers[slot].push(&packet) {
                latest[slot] = Some(record.challenge);
            }
        }
    }
}

/// A proof of `challenge` by the key of `seed`, the lifetime `lifetime`,
/// as one message.
fn proof(seed: [u8; 32], lifetime: Lifetime, challenge: &[u8; CHALLENGE_BYTES]) -> Vec<u8> {
    let pair = Ed25519KeyPair::from_seed_unchecked(&seed).unwrap();
    let identity = ChannelIdentity::read(ADDRESS).unwrap();
    let mut signed = [0_u8; MAX_SIGNED_BYTES];
    let length = viewer_state::signed_bytes(Algorithm::Ed25519, lifetime, challenge, &identity, &mut signed).unwrap();
    let signature: [u8; 64] = pair.sign(&signed[..length]).as_ref().try_into().unwrap();
    let public_key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
    let item = Item::Event(Event::StateProof(StateProof { algorithm: Algorithm::Ed25519, lifetime, public_key: &public_key, signature: &signature }));
    let mut bytes = [0_u8; MOST_ITEM_BYTES];
    let written = write(&item, &mut bytes).unwrap();
    bytes[..written].to_vec()
}

fuzz_target!(|data: &[u8]| {
    let config = ChannelConfig {
        name: "Fuzz".into(),
        address: ADDRESS.into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        // Every page has the default table, so that input is acted on
        // (page-rules.md RULE-53).
        default_rules: teletext_runtime::channel::DefaultRules::SendAll,
    };
    let mut channel = Channel::new(config, &HandleSecret::new([0; 32]), Box::new(Counting(0)));
    let mut latest = [None; 2];
    let mut readers = [RecordReader::new(), RecordReader::new()];
    channel.program_started(0);
    for connection in [1, 2] {
        channel.opened(connection, None, 0);
        channel.received(connection, &HELLO, 0);
    }
    note_challenges(&channel.take_effects(), &mut latest, &mut readers);
    let mut now = 0_u64;
    // Each message: a kind byte, a length byte, then that many bytes. A
    // proof's bytes are the seed's first byte and the lifetime's.
    let mut rest = data;
    while let [kind, length, tail @ ..] = rest {
        let take = usize::from(*length).min(tail.len());
        let (message, after) = tail.split_at(take);
        match kind % KINDS {
            PROGRAM => channel.from_program(message, now),
            sent @ (0 | 1) => channel.received(u64::from(sent) + 1, message, now),
            proven => {
                let index = usize::from(proven - 2);
                let seed = [message.first().copied().unwrap_or(0); 32];
                let lifetime = if message.get(1).is_some_and(|byte| byte & 1 == 1) { Lifetime::Remembered } else { Lifetime::Visit };
                if let Some(challenge) = latest[index] {
                    channel.received(u64::try_from(index).unwrap() + 1, &proof(seed, lifetime, &challenge), now);
                }
            }
        }
        now += STEP;
        channel.deadline(now);
        note_challenges(&channel.take_effects(), &mut latest, &mut readers);
        rest = after;
    }
    channel.closed(1, now);
    channel.closed(2, now);
    channel.deadline(now + LATER_MS);
});
