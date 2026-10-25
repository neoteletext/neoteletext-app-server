//! The showcase's sounds, synthesized: effects for the games and a
//! chromatic octave for the piano, in the engine's audio profile
//! (decisions E.84 to E.86). A session is sent the library once, as sound
//! objects; each sound after that is a 42-byte command.
//!
//! The synthesis is integer arithmetic only (square, triangle and noise
//! waves with linear envelopes), so it runs on every host without a
//! floating-point library.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use neoteletext_audio::encode::{Command, Sound};
use neoteletext_audio::{Codec, IMA_BLOCK_SMALL};
use teletext_runtime::{Audience, Context, SessionId};

/// The library's sample rate: 8 kHz is enough for chip-style effects.
pub const RATE: u32 = 8_000;
/// A command's play operation (the engine's `Command::operation`).
const PLAY: u8 = 1;
/// Commands rotate over the profile's channels 0 to 2; channel 3 is the
/// radio's.
const EFFECT_CHANNELS: u8 = 3;
/// How loud effects play, 0 to 100.
const GAIN: u8 = 80;
/// The fade at each end of a sound, in milliseconds, against clicks.
const FADE_MS: u16 = 5;
/// The loudest sample a sound reaches: a quarter of full scale, so mixed
/// sounds do not clip.
const PEAK: i32 = 8_000;

/// A sound of the library.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// A falling zap.
    Laser,
    /// A burst of noise, fading.
    Explosion,
    /// A short blip.
    Hit,
    /// Two rising notes.
    Coin,
    /// A rising sweep.
    Jump,
    /// A rising arpeggio.
    Win,
    /// A falling slide.
    Lose,
    /// A tick, for keys.
    Click,
    /// A bell-like tone.
    Bell,
    /// A note of the chromatic octave from middle C: 0 to 12.
    Note(u8),
}

/// The first note's handle; the twelve after it follow.
const NOTE_HANDLE: u8 = 1;
/// The notes of the octave.
pub const NOTES: u8 = 13;
/// Middle C to the C above, in hundredths of a hertz (equal temperament,
/// A4 = 440 Hz).
const NOTE_CENTIHERTZ: [u32; 13] = [26_163, 27_718, 29_366, 31_113, 32_963, 34_923, 36_999, 39_200, 41_530, 44_000, 46_616, 49_388, 52_325];

impl Effect {
    fn handle(self) -> u8 {
        match self {
            Self::Note(note) => NOTE_HANDLE.saturating_add(note.min(NOTES.saturating_sub(1))),
            Self::Laser => 20,
            Self::Explosion => 21,
            Self::Hit => 22,
            Self::Coin => 23,
            Self::Jump => 24,
            Self::Win => 25,
            Self::Lose => 26,
            Self::Click => 27,
            Self::Bell => 28,
        }
    }

    const ALL: [Self; 9] = [Self::Laser, Self::Explosion, Self::Hit, Self::Coin, Self::Jump, Self::Win, Self::Lose, Self::Click, Self::Bell];

    /// The sound's samples.
    fn samples(self) -> Vec<i16> {
        match self {
            Self::Laser => sweep(1_800, 300, 160, Wave::Square),
            Self::Explosion => noise(450),
            Self::Hit => sweep(900, 600, 60, Wave::Square),
            Self::Coin => [tone(98_800, 70, Wave::Square), tone(131_800, 140, Wave::Square)].concat(),
            Self::Jump => sweep(300, 900, 150, Wave::Triangle),
            Self::Win => [52_325, 65_926, 78_399, 104_650].iter().flat_map(|&note| tone(note, 110, Wave::Square)).collect(),
            Self::Lose => sweep(500, 120, 500, Wave::Triangle),
            Self::Click => tone(200_000, 12, Wave::Square),
            Self::Bell => tone(88_000, 500, Wave::Triangle),
            Self::Note(note) => {
                let index = usize::from(note.min(NOTES.saturating_sub(1)));
                tone(NOTE_CENTIHERTZ.get(index).copied().unwrap_or(26_163), 400, Wave::Triangle)
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Wave {
    Square,
    Triangle,
}

/// Samples in `milliseconds`.
fn samples_in(milliseconds: u32) -> u32 {
    RATE.saturating_mul(milliseconds) / 1_000
}

/// One sample of `wave` at `phase`, 0 to 65 535 over a cycle.
fn wave_at(wave: Wave, phase: u32) -> i32 {
    let phase = i32::try_from(phase & 0xFFFF).unwrap_or(0);
    match wave {
        Wave::Square => {
            if phase < 0x8000 {
                PEAK
            } else {
                PEAK.saturating_neg()
            }
        }
        // Up from −peak to +peak over the first half, down over the second.
        Wave::Triangle => {
            let rising = if phase < 0x8000 { phase } else { 0xFFFF_i32.saturating_sub(phase) };
            rising.saturating_mul(PEAK.saturating_mul(2)).checked_div(0x8000).unwrap_or(0).saturating_sub(PEAK)
        }
    }
}

/// A linear fade out over the sound's length: sample `index` of `total`.
fn envelope(sample: i32, index: u32, total: u32) -> i16 {
    let remaining = i64::from(total.saturating_sub(index));
    let scaled = i64::from(sample).saturating_mul(remaining).checked_div(i64::from(total.max(1))).unwrap_or(0);
    i16::try_from(scaled).unwrap_or(0)
}

/// A tone of `centihertz` for `milliseconds`.
fn tone(centihertz: u32, milliseconds: u32, wave: Wave) -> Vec<i16> {
    let total = samples_in(milliseconds);
    // The phase step per sample, in 1/65 536ths of a cycle.
    let step = u32::try_from(u64::from(centihertz).saturating_mul(65_536).checked_div(u64::from(RATE).saturating_mul(100)).unwrap_or(0)).unwrap_or(0);
    let mut phase = 0_u32;
    (0..total)
        .map(|index| {
            let sample = envelope(wave_at(wave, phase), index, total);
            phase = phase.wrapping_add(step);
            sample
        })
        .collect()
}

/// A sweep from `from` to `to` hertz over `milliseconds`.
fn sweep(from: u32, to: u32, milliseconds: u32, wave: Wave) -> Vec<i16> {
    let total = samples_in(milliseconds);
    let mut phase = 0_u32;
    (0..total)
        .map(|index| {
            let span = i64::from(to).saturating_sub(i64::from(from));
            let hertz = i64::from(from).saturating_add(span.saturating_mul(i64::from(index)).checked_div(i64::from(total.max(1))).unwrap_or(0));
            let step = u32::try_from(hertz.max(1).saturating_mul(65_536).checked_div(i64::from(RATE)).unwrap_or(0)).unwrap_or(0);
            let sample = envelope(wave_at(wave, phase), index, total);
            phase = phase.wrapping_add(step);
            sample
        })
        .collect()
}

/// Noise for `milliseconds`, from a 16-bit linear feedback shift register
/// (taps 16, 14, 13, 11: maximal length).
fn noise(milliseconds: u32) -> Vec<i16> {
    let total = samples_in(milliseconds);
    let mut register = 0xACE1_u16;
    (0..total)
        .map(|index| {
            let bit = (register ^ (register >> 2) ^ (register >> 3) ^ (register >> 5)) & 1;
            register = (register >> 1) | (bit << 15);
            let sample = if register & 1 == 1 { PEAK } else { PEAK.saturating_neg() };
            envelope(sample, index, total)
        })
        .collect()
}

/// The records of every sound of the library: objects and their data.
fn library_records() -> Vec<Vec<u8>> {
    let effects = Effect::ALL.iter().copied().chain((0..NOTES).map(Effect::Note));
    let mut records = Vec::new();
    for effect in effects {
        let Ok(sound) = Sound::new(effect.handle(), 1, Codec::ImaAdpcm, RATE, IMA_BLOCK_SMALL, &effect.samples()) else { continue };
        let Ok(object) = sound.object_record() else { continue };
        records.push(object);
        records.extend(sound.data_records());
    }
    records
}

/// Which sessions hold the library, and the commands' numbering.
pub struct Sounds {
    library: Vec<Vec<u8>>,
    loaded: BTreeSet<SessionId>,
    sequence: u16,
    channel: u8,
}

impl Default for Sounds {
    fn default() -> Self {
        Self::new()
    }
}

impl Sounds {
    /// The library, synthesized once.
    #[must_use]
    pub fn new() -> Self {
        Self { library: library_records(), loaded: BTreeSet::new(), sequence: 0, channel: 0 }
    }

    /// Sends the library to `session` if it does not hold it yet: once a
    /// session, before its first sound.
    pub fn load(&mut self, session: SessionId, context: &mut Context<'_>) {
        if self.loaded.insert(session) {
            context.sound(Audience::Session(session), self.library.clone());
        }
    }

    /// Plays `effect` for `session`, sending the library first if needed.
    pub fn play(&mut self, session: SessionId, effect: Effect, context: &mut Context<'_>) {
        self.load(session, context);
        self.sequence = self.sequence.wrapping_add(1);
        self.channel = self.channel.wrapping_add(1) % EFFECT_CHANNELS;
        let command = Command {
            channel: self.channel,
            sequence: self.sequence,
            operation: PLAY,
            handle: effect.handle(),
            generation: 1,
            gain: GAIN,
            fade_ms: FADE_MS,
            flags: 0,
            duck: 0,
            page: None,
        };
        context.sound(Audience::Session(session), alloc::vec![command.record()]);
    }

    /// Forgets `session`, which ended.
    pub fn forget(&mut self, session: SessionId) {
        self.loaded.remove(&session);
    }
}
