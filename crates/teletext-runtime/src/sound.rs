//! A program's `sound` (the engine's audio profile, DOC-1785) as the
//! profile's records: Opus carried unchanged once each packet is held to
//! the profile by the engine's own decoder (decision 21), 16-bit samples
//! encoded here as IMA ADPCM, commands numbered per channel so the latest
//! wins, and the synthesizers' sounds, banks and live sounds built through
//! the engine ([`synth`]; neoteletext-common's `docs/audio/server.md`). The
//! message has been checked whole ([`crate::protocol::check`]) before it
//! gets here.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use neoteletext_audio::encode::{Command, LiveEncoder, Sound as Encoded};
use neoteletext_audio::record::{FLAG_AFTER_CURRENT, FLAG_LOOP, SYNTH6581_CLOCK_NTSC};
use neoteletext_audio::{CHANNELS, Codec, IMA_BLOCK_LARGE, IMA_BLOCK_SMALL, OPUS_RATE};
use neoteletext_opus::{DecodeError, Decoder, FRAME_SAMPLES, PacketError};
use neoteletext_web::base64;

use crate::protocol::{
    BlocksSound, ConnectionId, GainSound, LiveSound, NoteFamily, ObjectSound, OpusLiveSound, OpusPacketsSound, OpusSound, PlaySound, Rule, Sound, SoundKind,
    StopSound, Synth7Family, Synth8Family, Synth8KeyMode, Synth53Family, Synth60Family, Synth80Family, Synth106Family, Synth6581Clock, Synth6581Event,
    Synth6581Music, SynthDFamily,
};
use synth::{FamilyCalls, Settings, Stored, refuse};
pub(crate) use synth::{Refused, Sent};

mod family;
mod midi_file;
pub(crate) mod synth;
mod tracker;
mod wire;

/// A command's operations (the engine's `audio.md` section 4.4).
pub(crate) const PLAY: u8 = 1;
/// Stop.
pub(crate) const STOP: u8 = 2;
/// Set the gain.
pub(crate) const GAIN: u8 = 3;
/// The profile's channels, as a count of slots.
const CHANNEL_SLOTS: usize = CHANNELS as usize;

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
    /// The synthesizers' banks and live sounds (SRV-59, SRV-72).
    synths: synth::Synths,
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
    refuse("", Rule::OutsideProfile, format!("{error:?}"))
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
                return Err(refuse("", Rule::OutsideProfile, format!("packet {index} is outside the Opus profile: {}", packet_refusal(refused))));
            }
        }
        Ok(())
    }

    /// What `sound` sends to `targets`, the open connections it names, at
    /// `now_ms`.
    pub(crate) fn act(&mut self, sound: &Sound, targets: &[ConnectionId], now_ms: u64) -> Result<Sent, Refused> {
        if let Some(sent) = self.synth(sound, targets, now_ms) {
            return sent;
        }
        let records = self.records(sound)?;
        // SRV-59, SRV-72: an Opus or PCM sound on a handle replaces its bank
        // and live sound.
        if let Some(handle) = replacing_handle(sound) {
            self.synths.replaced(handle);
        }
        let mut sent = Sent::default();
        for &connection in targets {
            sent.records.push((connection, records.clone()));
        }
        Ok(sent)
    }

    /// The live synthesizer blocks due at `now_ms` (SRV-77).
    pub(crate) fn due(&mut self, now_ms: u64) -> Vec<(ConnectionId, Vec<Vec<u8>>)> {
        self.synths.due(now_ms)
    }

    /// When a live synthesizer block is next due.
    pub(crate) fn next_due(&self) -> Option<u64> {
        self.synths.next_due()
    }

    /// Forgets what a closed connection was sent.
    pub(crate) fn forget(&mut self, connection: ConnectionId) {
        self.synths.forget(connection);
    }

    /// The records of an Opus or PCM sound or a command.
    fn records(&mut self, sound: &Sound) -> Result<Vec<Vec<u8>>, Refused> {
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
                // Each message's blocks are encoded from its own samples, the
                // last padded with silence, as before the engine kept an
                // encoder's state between calls (its DEC-2467).
                let mut encoder = LiveEncoder::new(&sound, *first).map_err(encoding)?;
                let mut records = encoder.records(&samples(pcm));
                records.extend(encoder.finish());
                Ok(records)
            }
            Sound::Play(PlaySound { channel, handle, generation, gain, fade_ms, r#loop, after, duck, page, .. }) => {
                if self.synths.is_bank(*handle) {
                    return Err(refuse("handle", Rule::NotListed, "MIDI-55: a bank is never played"));
                }
                // RULE-60, RULE-61: a local sound belongs to a page, ends
                // when the page is no longer open, and has no end of its
                // own to loop at or to follow.
                if self.synths.is_local(*handle, *generation) {
                    let held = |given: bool, path: &str, detail: &str| if given { Err(refuse(path, Rule::OutOfRange, detail)) } else { Ok(()) };
                    held(page.is_none(), "page", "RULE-61: a local sound is played with a page")?;
                    held(*r#loop, "loop", "RULE-61: a local sound is played with loop false")?;
                    held(*after, "after", "RULE-61: a local sound is played with after false")?;
                }
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
                    magazine: None,
                };
                Ok(alloc::vec![command.record()])
            }
            Sound::Stop(StopSound { channel, fade_ms, .. }) => Ok(alloc::vec![channel_command(*channel, self.next(*channel)?, STOP, 0, *fade_ms).record()]),
            Sound::Gain(GainSound { channel, gain, fade_ms, .. }) => {
                Ok(alloc::vec![channel_command(*channel, self.next(*channel)?, GAIN, *gain, *fade_ms).record()])
            }
            // Every synthesizer kind is taken by `synth` first.
            _ => Ok(Vec::new()),
        }
    }

    /// The next command number on `channel`.
    fn next(&mut self, channel: u8) -> Result<u16, Refused> {
        let slot = self.sequences.get_mut(usize::from(channel)).ok_or_else(|| refuse("channel", Rule::OutOfRange, "channel is 0 to 3"))?;
        *slot = slot.wrapping_add(1);
        Ok(*slot)
    }
}

/// The handle an Opus or PCM sound gives a new object (SRV-59, SRV-72).
const fn replacing_handle(sound: &Sound) -> Option<u8> {
    match sound {
        Sound::Opus(OpusSound { handle, .. })
        | Sound::OpusLive(OpusLiveSound { handle, .. })
        | Sound::Object(ObjectSound { handle, .. })
        | Sound::Live(LiveSound { handle, .. }) => Some(*handle),
        _ => None,
    }
}

/// synth8's key mode as tag 7 writes it (MIDI-48).
const fn keys(mode: Synth8KeyMode, split_key: u8) -> [u8; 2] {
    let mode = match mode {
        Synth8KeyMode::Whole => neoteletext_audio::record::SYNTH8_KEYS_WHOLE,
        Synth8KeyMode::Dual => neoteletext_audio::record::SYNTH8_KEYS_DUAL,
        Synth8KeyMode::Split => neoteletext_audio::record::SYNTH8_KEYS_SPLIT,
    };
    [mode, split_key]
}

/// Tag 3's chip and clock byte (MIDI-48): the chip from the kind.
const fn chip(variant: u8, clock: Synth6581Clock) -> u8 {
    match clock {
        Synth6581Clock::Pal => variant,
        Synth6581Clock::Ntsc => variant | SYNTH6581_CLOCK_NTSC,
    }
}

/// The live kind an events message's kind adds to (SRV-72).
const fn live_kind_of(kind: SoundKind) -> SoundKind {
    use SoundKind as K;
    match kind {
        K::Synth6581Events => K::Synth6581Live,
        K::Synth8580Events => K::Synth8580Live,
        K::Synth7Events => K::Synth7Live,
        K::Synth106Events => K::Synth106Live,
        K::Synth60Events => K::Synth60Live,
        K::SynthDEvents => K::SynthDLive,
        K::Synth53Events => K::Synth53Live,
        K::Synth52Events => K::Synth52Live,
        K::Synth51Events => K::Synth51Live,
        K::Synth8Events => K::Synth8Live,
        K::Synth80Events => K::Synth80Live,
        other => other,
    }
}

/// A note family's stored sound, live sound, events message or bank.
macro_rules! note_sound {
    ($self:ident, $sound:ident, $targets:ident, $now:ident, $kind:ident, $synth:ident, $family:ty, $music:ident, $keys:expr) => {{
        let settings = Settings { chip: None, keys: $keys, banks: $sound.banks.clone() };
        let (timed, end_tick, dropped) = match &$sound.music {
            crate::protocol::$music::Events(list) => {
                let mut timed = Vec::new();
                synth::place_notes::<$family>($synth, &list.events, "music.events", &mut timed)?;
                (timed, None, crate::protocol::DroppedCounts::default())
            }
            crate::protocol::$music::MidiFile(file) => {
                let (timed, end, dropped) =
                    synth::place_file($synth, &crate::protocol::check::bytes_of(&file.bytes), file.channel_map.as_ref(), &file.cues, "music.bytes")?;
                (timed, Some(end), dropped)
            }
        };
        let stored = Stored {
            kind: $kind,
            synth: $synth,
            handle: $sound.handle,
            generation: $sound.generation,
            settings,
            lift: &$sound.lift_banks,
            length: $sound.length,
            timed,
            end_tick,
            dropped,
        };
        $self.synths.stored(stored, $targets)
    }};
}

macro_rules! live_sound {
    ($self:ident, $sound:ident, $targets:ident, $kind:ident, $synth:ident, $keys:expr) => {{
        let settings = Settings { chip: None, keys: $keys, banks: $sound.banks.clone() };
        $self.synths.announce($kind, $synth, $sound.handle, $sound.generation, settings, $targets)
    }};
}

/// A note family's local sound (page-rules.md RULE-61): its setup's
/// events placed, all at time 0.
macro_rules! local_sound {
    ($self:ident, $sound:ident, $targets:ident, $synth:ident, $family:ty, $keys:expr) => {{
        let settings = Settings { chip: None, keys: $keys, banks: $sound.banks.clone() };
        let mut timed = Vec::new();
        synth::place_notes::<$family>($synth, &$sound.setup, "setup", &mut timed)?;
        $self.synths.local($synth, $sound.handle, $sound.generation, &settings, timed, $targets)
    }};
}

macro_rules! events_sound {
    ($self:ident, $sound:ident, $targets:ident, $now:ident, $kind:ident, $synth:ident, $family:ty) => {{
        let mut timed = Vec::new();
        synth::place_notes::<$family>($synth, &$sound.events, "events", &mut timed)?;
        let first = $sound.events.first().map(<$family as NoteFamily>::time_us);
        $self.synths.events(live_kind_of($kind), $sound.handle, $sound.generation, $sound.until_us.get(), timed, (first, None), $targets, $now)
    }};
}

macro_rules! bank_sound {
    ($self:ident, $sound:ident, $targets:ident, $synth:ident, $family:ty) => {{
        let image = <$family as FamilyCalls>::bank_image($synth, &$sound.bank).map_err(|fault| refuse("bank", fault.rule, fault.detail))?;
        $self.synths.bank($synth, $sound.handle, $sound.generation, &image, $targets)
    }};
}

impl Sounds {
    /// A synthesizer sound's records, or `None` for a sound of another
    /// kind.
    fn synth(&mut self, sound: &Sound, targets: &[ConnectionId], now: u64) -> Option<Result<Sent, Refused>> {
        let kind = sound.kind();
        let synth = family::Synth::of(kind)?;
        Some((|| match sound {
            Sound::Synth6581(stored) | Sound::Synth8580(stored) => {
                let (timed, end_tick) = match &stored.music {
                    Synth6581Music::Events(list) => {
                        synth::check_opening(&list.events, "music.events")?;
                        let mut timed = Vec::new();
                        synth::place_synth6581(&list.events, "music.events", &mut timed)?;
                        (timed, None)
                    }
                    Synth6581Music::MidiFile(file) => {
                        let (timed, end, _) = synth::place_file(synth, &crate::protocol::check::bytes_of(&file.bytes), None, &file.cues, "music.bytes")?;
                        (timed, Some(end))
                    }
                };
                let settings = Settings { chip: Some(chip(synth.variant, stored.clock)), keys: None, banks: Vec::new() };
                let stored = Stored {
                    kind,
                    synth,
                    handle: stored.handle,
                    generation: stored.generation,
                    settings,
                    lift: &[],
                    length: stored.length,
                    timed,
                    end_tick,
                    dropped: crate::protocol::DroppedCounts::default(),
                };
                self.synths.stored(stored, targets)
            }
            Sound::Synth6581Live(live) | Sound::Synth8580Live(live) => {
                let settings = Settings { chip: Some(chip(synth.variant, live.clock)), keys: None, banks: Vec::new() };
                self.synths.announce(kind, synth, live.handle, live.generation, settings, targets)
            }
            Sound::Synth6581Events(events) | Sound::Synth8580Events(events) => {
                let mut timed = Vec::new();
                synth::place_synth6581(&events.events, "events", &mut timed)?;
                let first = events.events.first();
                let opening = first.map(|first| synth::tick_of(first.time_us()) == 0 && family::synth6581::is_full_update(first));
                let firsts = (first.map(Synth6581Event::time_us), opening);
                self.synths.events(live_kind_of(kind), events.handle, events.generation, events.until_us.get(), timed, firsts, targets, now)
            }
            Sound::Synth7(stored) => note_sound!(self, stored, targets, now, kind, synth, Synth7Family, Synth7Music, None),
            Sound::Synth106(stored) => note_sound!(self, stored, targets, now, kind, synth, Synth106Family, Synth106Music, None),
            Sound::Synth60(stored) => note_sound!(self, stored, targets, now, kind, synth, Synth60Family, Synth60Music, None),
            Sound::SynthD(stored) => note_sound!(self, stored, targets, now, kind, synth, SynthDFamily, SynthDMusic, None),
            Sound::Synth53(stored) | Sound::Synth52(stored) | Sound::Synth51(stored) => {
                note_sound!(self, stored, targets, now, kind, synth, Synth53Family, Synth53Music, None)
            }
            Sound::Synth8(stored) => {
                note_sound!(self, stored, targets, now, kind, synth, Synth8Family, Synth8Music, Some(keys(stored.key_mode, stored.split_key.get())))
            }
            Sound::Synth80(stored) => note_sound!(self, stored, targets, now, kind, synth, Synth80Family, Synth80Music, None),
            Sound::Synth7Live(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::Synth106Live(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::Synth60Live(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::SynthDLive(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::Synth53Live(live) | Sound::Synth52Live(live) | Sound::Synth51Live(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::Synth8Live(live) => live_sound!(self, live, targets, kind, synth, Some(keys(live.key_mode, live.split_key.get()))),
            Sound::Synth80Live(live) => live_sound!(self, live, targets, kind, synth, None),
            Sound::Synth7Local(local) => local_sound!(self, local, targets, synth, Synth7Family, None),
            Sound::Synth106Local(local) => local_sound!(self, local, targets, synth, Synth106Family, None),
            Sound::Synth60Local(local) => local_sound!(self, local, targets, synth, Synth60Family, None),
            Sound::SynthDLocal(local) => local_sound!(self, local, targets, synth, SynthDFamily, None),
            Sound::Synth53Local(local) | Sound::Synth52Local(local) | Sound::Synth51Local(local) => {
                local_sound!(self, local, targets, synth, Synth53Family, None)
            }
            Sound::Synth8Local(local) => local_sound!(self, local, targets, synth, Synth8Family, Some(keys(local.key_mode, local.split_key.get()))),
            Sound::Synth80Local(local) => local_sound!(self, local, targets, synth, Synth80Family, None),
            Sound::Synth7Events(events) => events_sound!(self, events, targets, now, kind, synth, Synth7Family),
            Sound::Synth106Events(events) => events_sound!(self, events, targets, now, kind, synth, Synth106Family),
            Sound::Synth60Events(events) => events_sound!(self, events, targets, now, kind, synth, Synth60Family),
            Sound::SynthDEvents(events) => events_sound!(self, events, targets, now, kind, synth, SynthDFamily),
            Sound::Synth53Events(events) | Sound::Synth52Events(events) | Sound::Synth51Events(events) => {
                events_sound!(self, events, targets, now, kind, synth, Synth53Family)
            }
            Sound::Synth8Events(events) => events_sound!(self, events, targets, now, kind, synth, Synth8Family),
            Sound::Synth80Events(events) => events_sound!(self, events, targets, now, kind, synth, Synth80Family),
            Sound::Synth7Bank(bank) => bank_sound!(self, bank, targets, synth, Synth7Family),
            Sound::Synth106Bank(bank) => bank_sound!(self, bank, targets, synth, Synth106Family),
            Sound::Synth60Bank(bank) => bank_sound!(self, bank, targets, synth, Synth60Family),
            Sound::SynthDBank(bank) => bank_sound!(self, bank, targets, synth, SynthDFamily),
            Sound::Synth53Bank(bank) | Sound::Synth52Bank(bank) | Sound::Synth51Bank(bank) => bank_sound!(self, bank, targets, synth, Synth53Family),
            Sound::Synth8Bank(bank) => bank_sound!(self, bank, targets, synth, Synth8Family),
            Sound::Synth80Bank(bank) => bank_sound!(self, bank, targets, synth, Synth80Family),
            _ => Ok(Sent::default()),
        })())
    }
}

/// A command that belongs to its audio channel alone, a stop or a set gain:
/// it names no sound, no page and no magazine.
const fn channel_command(channel: u8, sequence: u16, operation: u8, gain: u8, fade_ms: u16) -> Command {
    Command { channel, sequence, operation, handle: 0, generation: 0, gain, fade_ms, flags: 0, duck: 0, page: None, magazine: None }
}
