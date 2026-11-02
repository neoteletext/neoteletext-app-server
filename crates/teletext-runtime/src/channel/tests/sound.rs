//! A program's sound as a viewer receives it: Opus unchanged, as codec 2,
//! once the engine's own decoder holds it to the profile, and 16-bit samples
//! encoded as IMA ADPCM for receivers that decode no Opus.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_audio::encode::{Sound, packets};
use neoteletext_audio::{Codec, IMA_BLOCK_LARGE, IMA_BLOCK_SMALL, OPUS_RATE};
use serde_json::{Value, json};

use super::{CONNECTION, Channel, Effect, answered, line, sent_to, told};
use crate::channel::SERVICE;
use crate::protocol::tests::base64;

/// One second of a 440 Hz tone at 16 kbit/s at constant rate, as an encoder
/// writes it, with the command the engine's decision E.124 names, its 51
/// packets back to back, every one 40 bytes: made by
/// `ffmpeg -f lavfi -i sine=frequency=440:sample_rate=48000:duration=1 -ac 1
/// -c:a libopus -b:a 16k -vbr off -frame_duration 20 tone.opus` (libopus
/// 1.5) and the packets read out of the Ogg file after its two headers.
const TONE: &[u8] = include_bytes!("opus-tone.packets");
/// Its packets' length.
const TONE_PACKET_BYTES: u16 = 40;
/// Its packets.
const TONE_PACKETS: usize = 51;
/// Samples it holds: the 51 frames less the encoder's look-ahead, so the
/// last frame runs past the sound's end (decision E.124 item 2).
const TONE_SAMPLES: u32 = 48_648;
/// The handle and generation of the tests' sounds.
const HANDLE: u8 = 7;
const GENERATION: u16 = 2;
/// A sample rate of the profile.
const RATE: u32 = 8_000;

/// A sound message of `kind` for the test's connection.
fn sound(kind: &str, mut fields: Value) -> Vec<u8> {
    fields["type"] = json!("sound");
    fields["kind"] = json!(kind);
    fields["connections"] = json!([CONNECTION]);
    fields["handle"] = json!(HANDLE);
    fields["generation"] = json!(GENERATION);
    line(&fields)
}

/// What the viewer is sent for `records`, numbered from `first`.
fn audio(records: &[Vec<u8>], first: u8) -> Vec<u8> {
    let (packets, _) = packets(SERVICE, first, records);
    packets.iter().flat_map(|packet| packet.bytes).collect()
}

/// The record number the next record on a connection takes.
fn next_record(channel: &Channel) -> u8 {
    channel.connections[&CONNECTION].record_id
}

/// The refusals in `effects`, with their details.
fn refused(effects: &[Effect]) -> Vec<String> {
    told(effects).iter().filter(|line| line["type"] == "refused").map(|line| line["detail"].as_str().unwrap().into()).collect()
}

/// REQ-1060 and REQ-1061: a constant-rate encoder's stored sound reaches the
/// viewer as the engine carries Opus, codec 2, its packets unchanged: the
/// very records the engine's `Sound::opus` makes.
#[test]
fn an_encoders_opus_sound_reaches_the_viewer_as_codec_2() {
    assert_eq!(TONE.len(), TONE_PACKETS * usize::from(TONE_PACKET_BYTES));
    let mut channel = answered();
    let first = next_record(&channel);
    channel.from_program(&sound("opus", json!({"packet_bytes": TONE_PACKET_BYTES, "samples": TONE_SAMPLES, "packets": base64(TONE)})), 1);
    let effects = channel.take_effects();
    assert_eq!(refused(&effects), [] as [String; 0]);
    let sound = Sound::opus(HANDLE, GENERATION, TONE_PACKET_BYTES, TONE, TONE_SAMPLES).unwrap();
    assert_eq!(sound.codec, Codec::Opus);
    let mut records = alloc::vec![sound.object_record().unwrap()];
    records.extend(sound.data_records());
    assert_eq!(sent_to(&effects, CONNECTION), audio(&records, first));
}

/// REQ-1061: a live Opus sound goes as its object, then its packets' records
/// as they come, unchanged, numbered from the block named.
#[test]
fn a_live_opus_sound_goes_as_its_packets_come() {
    /// The first block of the test's packets.
    const BLOCK: u16 = 100;
    /// How many of the tone's packets it sends.
    const SENT: usize = 10;
    let mut channel = answered();
    let first = next_record(&channel);
    let packets = &TONE[..SENT * usize::from(TONE_PACKET_BYTES)];
    channel.from_program(&sound("opus_live", json!({"packet_bytes": TONE_PACKET_BYTES})), 1);
    let object = channel.take_effects();
    let after = next_record(&channel);
    channel.from_program(&sound("opus_packets", json!({"packet_bytes": TONE_PACKET_BYTES, "first": BLOCK, "packets": base64(packets)})), 2);
    let rest = channel.take_effects();
    assert_eq!(refused(&object).len() + refused(&rest).len(), 0);
    let live = Sound::live(HANDLE, GENERATION, Codec::Opus, OPUS_RATE, TONE_PACKET_BYTES).unwrap();
    assert_eq!(sent_to(&object, CONNECTION), audio(&[live.object_record().unwrap()], first));
    assert_eq!(sent_to(&rest, CONNECTION), audio(&live.packet_records(BLOCK, packets).unwrap(), after));
}

/// REQ-1060: packets outside the profile are refused whole, with the reason,
/// and nothing is sent: over 40 bytes (above 16 kbit/s at constant rate),
/// stereo, two frames, a 10 ms frame, empty, or not whole packets (the engine's
/// E.123 and REQ-AUD-022); and a stored sound its packets cannot hold.
#[test]
fn opus_outside_the_profile_is_refused() {
    // TOC bytes (RFC 6716 section 3.1, table 2): Hybrid fullband 20 ms, mono,
    // one frame; then its stereo, two-frame and 10 ms forms.
    let toc: u8 = 15 << 3;
    let cases: [(u16, Vec<u8>); 6] = [
        (41, alloc::vec![toc; 41]),
        (2, alloc::vec![toc | 0x04, 0]),
        (3, alloc::vec![toc | 1, 0, 0]),
        (2, alloc::vec![14 << 3, 0]),
        (2, alloc::vec![toc, 0, toc]),
        (2, Vec::new()),
    ];
    for (packet_bytes, packets) in cases {
        let mut channel = answered();
        channel.from_program(&sound("opus_packets", json!({"packet_bytes": packet_bytes, "first": 0, "packets": base64(&packets)})), 1);
        let effects = channel.take_effects();
        assert_eq!(refused(&effects).len(), 1, "{packet_bytes}: {packets:?}: {:?}", told(&effects));
        assert_eq!(sent_to(&effects, CONNECTION).len(), 0, "{packet_bytes}: {packets:?}");
    }
    let mut channel = answered();
    channel.from_program(&sound("opus", json!({"packet_bytes": TONE_PACKET_BYTES, "samples": OPUS_RATE, "packets": base64(TONE)})), 1);
    let effects = channel.take_effects();
    assert_eq!(refused(&effects).len(), 1, "a length its packets do not hold: {:?}", told(&effects));
    assert_eq!(sent_to(&effects, CONNECTION).len(), 0);
}

/// REQ-1062: a program's samples go as IMA ADPCM, whole and live, the records
/// the engine's encoder makes, and commands are numbered per audio channel so
/// that the latest wins.
#[test]
fn samples_are_encoded_as_ima_adpcm_and_commands_numbered() {
    /// A few samples, as little-endian bytes.
    const PCM: [u8; 8] = [0, 0, 16, 0, 32, 0, 48, 0];
    let samples: Vec<i16> = PCM.as_chunks::<2>().0.iter().map(|bytes| i16::from_le_bytes(*bytes)).collect();
    let mut channel = answered();
    let first = next_record(&channel);
    channel.from_program(&sound("object", json!({"rate": RATE, "pcm": base64(&PCM)})), 1);
    let effects = channel.take_effects();
    assert_eq!(refused(&effects), [] as [String; 0]);
    let stored = Sound::new(HANDLE, GENERATION, Codec::ImaAdpcm, RATE, IMA_BLOCK_SMALL, &samples).unwrap();
    assert_eq!(stored.codec, Codec::ImaAdpcm);
    let mut records = alloc::vec![stored.object_record().unwrap()];
    records.extend(stored.data_records());
    assert_eq!(sent_to(&effects, CONNECTION), audio(&records, first));

    let after = next_record(&channel);
    channel.from_program(&sound("blocks", json!({"rate": RATE, "first": 3, "pcm": base64(&PCM)})), 2);
    let live = Sound::live(HANDLE, GENERATION, Codec::ImaAdpcm, RATE, IMA_BLOCK_LARGE).unwrap();
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), audio(&live.live_records(3, &samples), after));

    let stop = line(&json!({"type": "sound", "kind": "stop", "connections": [CONNECTION], "channel": 1, "fade_ms": 0}));
    let command = |channel: &mut Channel| {
        channel.from_program(&stop, 3);
        sent_to(&channel.take_effects(), CONNECTION)
    };
    assert_ne!(command(&mut channel)[2..], command(&mut channel)[2..], "each command its own number");
}
