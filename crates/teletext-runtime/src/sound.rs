//! A program's `sound` (the engine's audio profile, `SERVER.md` section 9)
//! as the profile's records: Opus carried unchanged once each packet is
//! held to the profile by the engine's own decoder (decision 21), 16-bit
//! samples encoded here as IMA ADPCM, and commands numbered per channel so
//! the latest wins. The message has been checked whole
//! ([`crate::protocol::check`]) before it gets here.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_audio::encode::{Command, Sound as Encoded};
use neoteletext_audio::record::{FLAG_AFTER_CURRENT, FLAG_LOOP};
use neoteletext_audio::{CHANNELS, Codec, IMA_BLOCK_LARGE, IMA_BLOCK_SMALL, OPUS_RATE};
use neoteletext_opus::{DecodeError, Decoder, FRAME_SAMPLES, PacketError};
use neoteletext_web::base64;

use crate::protocol::{BlocksSound, GainSound, LiveSound, ObjectSound, OpusLiveSound, OpusPacketsSound, OpusSound, PlaySound, Rule, Sound, StopSound};

/// A command's operations (the engine's `audio.md` section 4.4).
const PLAY: u8 = 1;
/// Stop.
const STOP: u8 = 2;
/// Set the gain.
const GAIN: u8 = 3;
/// The profile's channels, as a count of slots.
const CHANNEL_SLOTS: usize = CHANNELS as usize;

/// Why a sound could not be made: the rule, and the detail.
pub(crate) type Refused = (Rule, alloc::string::String);

/// What the channel keeps to make sounds: each audio channel's last
/// command number, and the Opus check.
#[derive(Default)]
pub(crate) struct Sounds {
    /// The last command's number on each of the profile's four channels:
    /// a receiver acts only on a newer one (`audio.md` section 7.1).
    sequences: [u16; CHANNEL_SLOTS],
    /// The engine's Opus decoder, about 28 KB, made at the first Opus
    /// sound only.
    opus: Option<Box<Decoder>>,
}

/// The bytes of base64 already checked.
fn bytes_of(text: &str) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(text.len())];
    let length = base64::decode(text.as_bytes(), &mut bytes).unwrap_or(0);
    bytes.truncate(length);
    bytes
}

/// 16-bit little-endian samples.
fn samples(text: &str) -> Vec<i16> {
    bytes_of(text).as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect()
}

/// Why the engine's decoder refused a packet, in words.
const fn packet_refusal(refused: PacketError) -> &'static str {
    match refused {
        PacketError::Empty => "empty",
        PacketError::TooLong => "over 160 bytes",
        PacketError::Stereo => "stereo",
        PacketError::SeveralFrames => "more than one frame",
        PacketError::Malformed => "malformed",
        PacketError::NotTwentyMs => "a frame other than 20 ms",
    }
}

fn encoding<E: core::fmt::Debug>(error: E) -> Refused {
    (Rule::OutsideProfile, format!("{error:?}"))
}

impl Sounds {
    /// Refuses unless every packet is in the profile the receivers decode:
    /// mono, one 20 ms frame, at most 160 bytes, bare or padded (the
    /// engine's E.123 and E.124), each judged by the engine's own decoder.
    fn check_opus(&mut self, packet_bytes: u16, packets: &[u8]) -> Result<(), Refused> {
        let decoder = self.opus.get_or_insert_with(|| Box::new(Decoder::new()));
        let mut pcm = [0_i16; FRAME_SAMPLES];
        for (index, packet) in packets.chunks_exact(usize::from(packet_bytes)).enumerate() {
            decoder.reset();
            if let Err(DecodeError::Packet(refused)) = decoder.decode(packet, &mut pcm) {
                return Err((Rule::OutsideProfile, format!("packet {index} is outside the Opus profile: {}", packet_refusal(refused))));
            }
        }
        Ok(())
    }

    /// The records of `sound`.
    pub(crate) fn records(&mut self, sound: &Sound) -> Result<Vec<Vec<u8>>, Refused> {
        match sound {
            Sound::Opus(OpusSound { handle, generation, packet_bytes, samples, packets, .. }) => {
                let packets = bytes_of(packets);
                self.check_opus(*packet_bytes, &packets)?;
                let sound = Encoded::opus(*handle, *generation, *packet_bytes, &packets, *samples).map_err(encoding)?;
                let mut records = alloc::vec![sound.object_record().map_err(encoding)?];
                records.extend(sound.data_records());
                Ok(records)
            }
            Sound::OpusLive(OpusLiveSound { handle, generation, packet_bytes, .. }) => {
                let sound = Encoded::live(*handle, *generation, Codec::Opus, OPUS_RATE, *packet_bytes).map_err(encoding)?;
                Ok(alloc::vec![sound.object_record().map_err(encoding)?])
            }
            Sound::OpusPackets(OpusPacketsSound { handle, generation, packet_bytes, first, packets, .. }) => {
                let packets = bytes_of(packets);
                self.check_opus(*packet_bytes, &packets)?;
                let sound = Encoded::live(*handle, *generation, Codec::Opus, OPUS_RATE, *packet_bytes).map_err(encoding)?;
                sound.packet_records(*first, &packets).map_err(encoding)
            }
            Sound::Object(ObjectSound { handle, generation, rate, pcm, .. }) => {
                let sound = Encoded::new(*handle, *generation, Codec::ImaAdpcm, *rate, IMA_BLOCK_SMALL, &samples(pcm)).map_err(encoding)?;
                let mut records = alloc::vec![sound.object_record().map_err(encoding)?];
                records.extend(sound.data_records());
                Ok(records)
            }
            Sound::Live(LiveSound { handle, generation, rate, .. }) => {
                let sound = Encoded::live(*handle, *generation, Codec::ImaAdpcm, *rate, IMA_BLOCK_LARGE).map_err(encoding)?;
                Ok(alloc::vec![sound.object_record().map_err(encoding)?])
            }
            Sound::Blocks(BlocksSound { handle, generation, rate, first, pcm, .. }) => {
                let sound = Encoded::live(*handle, *generation, Codec::ImaAdpcm, *rate, IMA_BLOCK_LARGE).map_err(encoding)?;
                Ok(sound.live_records(*first, &samples(pcm)))
            }
            Sound::Play(PlaySound { channel, handle, generation, gain, fade_ms, r#loop, after, duck, page, .. }) => {
                let mut flags = 0;
                if *r#loop {
                    flags |= FLAG_LOOP;
                }
                if *after {
                    flags |= FLAG_AFTER_CURRENT;
                }
                let command = Command {
                    channel: *channel,
                    sequence: self.next(*channel)?,
                    operation: PLAY,
                    handle: *handle,
                    generation: *generation,
                    gain: *gain,
                    fade_ms: *fade_ms,
                    flags,
                    duck: *duck,
                    page: page.map(|page| page.0),
                };
                Ok(alloc::vec![command.record()])
            }
            Sound::Stop(StopSound { channel, fade_ms, .. }) => {
                let command = Command {
                    channel: *channel,
                    sequence: self.next(*channel)?,
                    operation: STOP,
                    handle: 0,
                    generation: 0,
                    gain: 0,
                    fade_ms: *fade_ms,
                    flags: 0,
                    duck: 0,
                    page: None,
                };
                Ok(alloc::vec![command.record()])
            }
            Sound::Gain(GainSound { channel, gain, fade_ms, .. }) => {
                let command = Command {
                    channel: *channel,
                    sequence: self.next(*channel)?,
                    operation: GAIN,
                    handle: 0,
                    generation: 0,
                    gain: *gain,
                    fade_ms: *fade_ms,
                    flags: 0,
                    duck: 0,
                    page: None,
                };
                Ok(alloc::vec![command.record()])
            }
        }
    }

    /// The next command number on `channel`.
    fn next(&mut self, channel: u8) -> Result<u16, Refused> {
        let slot = self.sequences.get_mut(usize::from(channel)).ok_or((Rule::OutOfRange, "channel is 0 to 3".to_string()))?;
        *slot = slot.wrapping_add(1);
        Ok(*slot)
    }
}

/// Who a sound is for.
pub(crate) const fn audience(sound: &Sound) -> &crate::protocol::Connections {
    match sound {
        Sound::Opus(OpusSound { connections, .. })
        | Sound::OpusLive(OpusLiveSound { connections, .. })
        | Sound::OpusPackets(OpusPacketsSound { connections, .. })
        | Sound::Object(ObjectSound { connections, .. })
        | Sound::Live(LiveSound { connections, .. })
        | Sound::Blocks(BlocksSound { connections, .. })
        | Sound::Play(PlaySound { connections, .. })
        | Sound::Stop(StopSound { connections, .. })
        | Sound::Gain(GainSound { connections, .. }) => connections,
    }
}
