//! Page 501, NeoTeletext Radio: one synthesized music stream for every
//! listener (decision 7). Each block is made once and sent, the same
//! records, to every session with the page open; the runtime packs them
//! per session. A listener is a session with the page open: the stream
//! starts for the first and stops for the last, so nothing is made while
//! nobody listens.
//!
//! The stream is the engine's live sound (its REQ-AUD-014, `audio.md`
//! section 8.3) on the profile's channel 3, which the showcase's effects
//! leave to it. A listener is sent the live object and a PLAY bound to this
//! page, then the blocks not yet heard, so it joins the stream where the
//! others are, in the same generation. Blocks go whole, ahead of real time
//! by [`LEAD_MS`]; the page, shared, shows what is playing and the voices'
//! levels, redrawn once a tick for everyone.

use alloc::collections::{BTreeSet, VecDeque};
use alloc::format;
use alloc::vec::Vec;

use neoteletext_audio::encode::{Command, Sound};
use neoteletext_audio::{Codec, IMA_BLOCK_LARGE};
use teletext_runtime::screen::{Colour, SIXELS_ACROSS, SIXELS_DOWN, Screen, Sixels};
use teletext_runtime::{Application, Audience, Context, Owner, PageEntry, SessionId};

use super::draw::{badge, column_bar, sixels, track_bar};
use super::synth::{ENVELOPE_ONE, MILLISECONDS, RATE, scaled_level};
use super::tracker::{ARPEGGIO, BASS, HATS, KICK, LEAD, SNARE, Tracker, VOICES, note_name, share};
use super::tunes::PLAYLIST;
use super::{BOARD, DRUMS, MENU, PIANO, RADIO};
use crate::common::page;

/// The stream's handle in the audio profile: clear of the effects
/// library's 1 to 28.
pub(crate) const HANDLE: u8 = 40;
/// The profile's channel the radio plays on: the last, which the
/// showcase's effects (`sound.rs`, channels 0 to 2) leave to it.
pub(crate) const CHANNEL: u8 = 3;
/// A command's operations (the engine's REQ-AUD-006).
pub(crate) const PLAY: u8 = 1;
/// Stop.
pub(crate) const STOP: u8 = 2;
/// How loud the radio plays, 0 to 100.
const GAIN: u8 = 85;
/// Its fade in and out, in milliseconds.
const FADE_MS: u16 = 20;
/// The block size: 512 bytes, 1 017 samples, 64 ms at 16 kHz.
pub(crate) const BLOCK_BYTES: u16 = IMA_BLOCK_LARGE;
/// How far ahead of real time blocks are sent, in milliseconds: more than
/// the receiver mixer's buffer of a live sound (400 ms, the engine's
/// `audio.md` section 8.3) plus a [`TICK_MS`] between sends, well inside
/// its ring of 32 blocks (about 2 s at this rate).
pub(crate) const LEAD_MS: u64 = 800;
/// How often the stream is topped up and the page redrawn: four times a
/// second.
pub(crate) const TICK_MS: u64 = 250;
/// The most blocks one tick sends, a ring's worth (the engine's
/// `LIVE_RING_BLOCKS`): after a stall the stream skips ahead instead.
const MOST_BLOCKS: usize = 32;

/// The stream sample heard at `milliseconds` of the host's clock: the
/// stream runs on the channel's clock, so every listener hears the same
/// moment of the playlist.
pub(crate) const fn sample_at(milliseconds: u64) -> u64 {
    milliseconds.saturating_mul(RATE as u64) / MILLISECONDS
}

/// The live sound now streaming.
struct Stream {
    sound: Sound,
    /// The next block's number.
    next_block: u16,
    /// The stream sample the next block starts at.
    sent: u64,
    /// The blocks sent and not yet heard, with the sample each starts at,
    /// for a listener who joins.
    recent: VecDeque<(u64, Vec<u8>)>,
}

impl Stream {
    fn samples_per_block(&self) -> u64 {
        u64::from(self.sound.samples_per_block().max(1))
    }
}

/// The radio.
pub(crate) struct Radio {
    tracker: Tracker,
    listeners: BTreeSet<SessionId>,
    stream: Option<Stream>,
    /// The live sound's generation: each start is a new one.
    generation: u16,
    /// The last command's number on the radio's channel.
    sequence: u16,
}

impl Default for Radio {
    fn default() -> Self {
        Self::new()
    }
}

impl Radio {
    /// The radio, off air.
    pub(crate) fn new() -> Self {
        Self { tracker: Tracker::new(), listeners: BTreeSet::new(), stream: None, generation: 0, sequence: 0 }
    }

    fn command(&mut self, operation: u8) -> Vec<u8> {
        self.sequence = self.sequence.wrapping_add(1);
        Command {
            channel: CHANNEL,
            sequence: self.sequence,
            operation,
            handle: if operation == PLAY { HANDLE } else { 0 },
            generation: if operation == PLAY { self.generation } else { 0 },
            gain: if operation == PLAY { GAIN } else { 0 },
            fade_ms: FADE_MS,
            flags: 0,
            duck: 0,
            // Bound to the page: the receiver fades it out when the
            // viewer leaves, before the server hears (`audio.md` 7.3).
            page: (operation == PLAY).then_some(RADIO),
        }
        .record()
    }

    /// Starts a new generation of the stream at `now`.
    fn start(&mut self, now: u64) {
        self.generation = self.generation.wrapping_add(1);
        let Ok(sound) = Sound::live(HANDLE, self.generation, Codec::ImaAdpcm, RATE, BLOCK_BYTES) else { return };
        self.stream = Some(Stream { sound, next_block: 0, sent: sample_at(now), recent: VecDeque::new() });
    }

    /// `session` joins: the live object, the play, and the blocks it has
    /// yet to hear, so it joins at the first of them.
    fn join(&mut self, session: SessionId, now: u64, context: &mut Context<'_>) {
        let Some(object) = self.stream.as_ref().and_then(|stream| stream.sound.object_record().ok()) else { return };
        let play = self.command(PLAY);
        let mut records = alloc::vec![object, play];
        if let Some(stream) = &self.stream {
            let heard = sample_at(now);
            let block = stream.samples_per_block();
            records.extend(stream.recent.iter().filter(|(start, _)| start.saturating_add(block) > heard).map(|(_, record)| record.clone()));
        }
        context.sound(Audience::Session(session), records);
    }

    /// Makes the blocks due by `now` plus the lead, once, and sends them to
    /// every listener.
    fn pump(&mut self, now: u64, context: &mut Context<'_>) {
        let Some(stream) = self.stream.as_mut() else { return };
        let block = stream.samples_per_block();
        let heard = sample_at(now);
        let due = sample_at(now.saturating_add(LEAD_MS));
        // After a stall the blocks already past are skipped, not sent late.
        stream.sent = stream.sent.max(heard);
        let whole = due.saturating_sub(stream.sent).checked_div(block).unwrap_or(0);
        let count = usize::try_from(whole).unwrap_or(MOST_BLOCKS).min(MOST_BLOCKS);
        // One call for the tick's blocks: the encoder carries its step
        // size from block to block within a call, and starts each call at
        // the smallest, which costs 2 to 7 dB at every restart (measured on
        // this music).
        let pcm = self.tracker.render(stream.sent, count.saturating_mul(usize::try_from(block).unwrap_or(0)));
        let records = if count == 0 { Vec::new() } else { stream.sound.live_records(stream.next_block, &pcm) };
        for record in &records {
            stream.recent.push_back((stream.sent, record.clone()));
            stream.next_block = stream.next_block.wrapping_add(1);
            stream.sent = stream.sent.saturating_add(block);
        }
        while stream.recent.front().is_some_and(|(start, _)| start.saturating_add(block) <= heard) {
            stream.recent.pop_front();
        }
        context.sound(Audience::Viewers(RADIO), records);
    }

    /// The next tick, on the tick's grid.
    fn schedule(context: &mut Context<'_>) {
        let now = context.now();
        context.wake_at(now.saturating_add(TICK_MS).saturating_sub(now % TICK_MS));
    }

    fn leave(&mut self, session: SessionId, context: &mut Context<'_>) {
        if self.listeners.remove(&session) && self.listeners.is_empty() {
            self.stream = None;
            self.draw(context);
        }
    }

    /// The page, shared, as it is now.
    fn draw(&self, context: &mut Context<'_>) {
        let mut screen =
            page("NeoTeletext Radio", Colour::Blue, [("Sound", Some(MENU)), ("Piano", Some(PIANO)), ("Board", Some(BOARD)), ("Drums", Some(DRUMS))]);
        if self.stream.is_some() {
            self.on_air(&mut screen, context.now());
        } else {
            Self::off_air(&mut screen);
        }
        context.publish(Owner::Channel, RADIO, &screen);
    }

    fn off_air(screen: &mut Screen) {
        badge(screen, 3, 0, Colour::Blue, Colour::White, " OFF AIR ");
        screen.coloured(3, 14, Colour::Cyan, "nobody is listening");
        screen.centred(5, Colour::White, "Open this page to tune in: the radio");
        screen.centred(6, Colour::White, "starts for its first listener, and");
        screen.centred(7, Colour::White, "every listener hears one stream.");
        screen.coloured(9, 1, Colour::Yellow, "On the playlist");
        let mut row = 11_usize;
        for tune in PLAYLIST.iter().filter(|tune| !tune.ident) {
            screen.coloured(row, 2, Colour::White, tune.title);
            screen.coloured(row, 20, Colour::Cyan, tune.style);
            row = row.saturating_add(2);
        }
        stream_line(screen);
    }

    fn on_air(&self, screen: &mut Screen, now: u64) {
        let heard = sample_at(now);
        badge(screen, 3, 0, Colour::Red, Colour::White, " ON AIR ");
        let listeners = self.listeners.len();
        let noun = if listeners == 1 { "listener" } else { "listeners" };
        screen.coloured(3, 13, Colour::Cyan, &format!("{listeners} {noun}, one stream"));
        if let Some(place) = self.tracker.place(heard) {
            screen.coloured(4, 0, Colour::Yellow, "Now");
            screen.coloured(4, 5, Colour::White, place.tune.title);
            screen.coloured(5, 5, Colour::Cyan, &format!("{}, {} bpm", place.tune.style, place.tune.tempo));
            let lead = place.lead.map_or_else(|| alloc::string::String::from("rest"), note_name);
            screen.coloured(6, 0, Colour::Green, &format!("Bar {:>2} of {:<2} Chord {:<4} Lead {lead}", place.bar.saturating_add(1), place.bars, place.chord));
            let progress = usize::try_from(share(scale_thousandths(place.thousandths), PROGRESS_SIXELS)).unwrap_or(0);
            track_bar(screen, 7, METER_COLUMN, PROGRESS_CELLS, progress, Colour::Yellow);
            screen.coloured(22, 0, Colour::Yellow, "Next");
            screen.coloured(22, 5, Colour::White, place.next.title);
        }
        let window = self.window(heard);
        for (index, ((name, colour), level)) in BARS.iter().zip(window.levels).enumerate() {
            let column = BARS_COLUMN.saturating_add(index.saturating_mul(BAR_STEP));
            let height = usize::try_from(share(level, u32::try_from(BAR_ROWS.saturating_mul(SIXELS_DOWN)).unwrap_or(0))).unwrap_or(0);
            column_bar(screen, BARS_ROW, column, (BAR_CELLS, BAR_ROWS), height, *colour);
            screen.coloured(BARS_ROW.saturating_add(BAR_ROWS), column.saturating_sub(1), *colour, name);
        }
        screen.coloured(LEVEL_ROW, 0, Colour::White, "Level");
        level_meter(screen, LEVEL_ROW, window.loudness);
        self.scope(screen, heard);
        stream_line(screen);
    }

    /// The voices' loudest levels, and the mix's loudness, over the tick
    /// just heard.
    fn window(&self, heard: u64) -> Window {
        let span = sample_at(TICK_MS);
        let first = heard.saturating_sub(span);
        let mut levels = [0_i32; VOICES];
        for probe in 1..=LEVEL_PROBES {
            let at = first.saturating_add(span.saturating_mul(probe) / LEVEL_PROBES);
            // A level counts less the longer ago it was: a peak meter's
            // fall, over a tick, so a drum hit shows and then dies away.
            let recency = i32::try_from(u64::try_from(ENVELOPE_ONE).unwrap_or(0).saturating_mul(probe) / LEVEL_PROBES).unwrap_or(0);
            for (slot, voice) in levels.iter_mut().zip(self.tracker.voices(at)) {
                *slot = (*slot).max(scaled_level(voice.level, recency));
            }
        }
        let (mut total, mut count) = (0_u64, 0_u64);
        let mut at = first;
        while at < heard {
            total = total.saturating_add(u64::from(self.tracker.sample(at).unsigned_abs()));
            count = count.saturating_add(1);
            at = at.saturating_add(LOUDNESS_STRIDE);
        }
        Window { levels, loudness: u32::try_from(total.checked_div(count).unwrap_or(0)).unwrap_or(0) }
    }

    /// The waveform just heard, as a trace in mosaics, scaled to its own
    /// peak so a quiet passage shows its shape too.
    fn scope(&self, screen: &mut Screen, heard: u64) {
        let (width, height) = sixels(SCOPE_CELLS, SCOPE_ROWS);
        let mut canvas = Sixels::new(width, height);
        let span = u64::try_from(width).unwrap_or(0).saturating_mul(SCOPE_STRIDE);
        let first = heard.saturating_sub(span);
        let trace: Vec<i64> =
            (0..width).map(|x| i64::from(self.tracker.sample(first.saturating_add(u64::try_from(x).unwrap_or(0).saturating_mul(SCOPE_STRIDE))))).collect();
        let peak = trace.iter().map(|sample| sample.abs()).max().unwrap_or(0).max(SCOPE_QUIET);
        let bottom = i64::try_from(height.saturating_sub(1)).unwrap_or(0);
        let mut previous: Option<usize> = None;
        for (x, sample) in trace.iter().enumerate() {
            // From −peak at the bottom to +peak at the top.
            let lifted = peak.saturating_sub(*sample).clamp(0, peak.saturating_mul(2));
            let y = usize::try_from(lifted.saturating_mul(bottom).checked_div(peak.saturating_mul(2)).unwrap_or(0)).unwrap_or(0);
            let (from, to) = previous.map_or((y, y), |last| (last.min(y), last.max(y)));
            for joined in from..=to {
                canvas.set(x, joined, true);
            }
            previous = Some(y);
        }
        canvas.draw(screen, SCOPE_ROW, METER_COLUMN, Colour::Green);
    }
}

/// What the meters show of a tick.
struct Window {
    levels: [i32; VOICES],
    /// The mean absolute sample.
    loudness: u32,
}

/// The column the horizontal meters start at.
const METER_COLUMN: usize = 3;
/// The progress bar's cells.
const PROGRESS_CELLS: usize = 36;
/// Its sixels across.
const PROGRESS_SIXELS: u32 = 72;
const _: () = assert!(PROGRESS_SIXELS as usize == PROGRESS_CELLS * SIXELS_ACROSS);
/// The voices' bars: their labels and colours, in the tracker's order.
const BARS: [(&str, Colour); VOICES] = {
    let mut bars = [("", Colour::White); VOICES];
    bars[BASS] = ("Bass", Colour::Red);
    bars[KICK] = ("Kick", Colour::Yellow);
    bars[LEAD] = ("Lead", Colour::Green);
    bars[ARPEGGIO] = ("Arp", Colour::Cyan);
    bars[SNARE] = ("Snare", Colour::Magenta);
    bars[HATS] = ("Hats", Colour::White);
    bars
};
/// The first bar's first cell, after its colour.
const BARS_COLUMN: usize = 3;
/// A bar's width in cells.
const BAR_CELLS: usize = 5;
/// From one bar to the next: its cells and the next one's colour.
const BAR_STEP: usize = BAR_CELLS + 1;
/// The bars' top row.
const BARS_ROW: usize = 8;
/// Their height in rows: the bars and their labels fill rows 8 to 16.
const BAR_ROWS: usize = 8;
/// Probes of the voices' levels over a tick: one every 8 ms, short enough
/// to catch a closed hi-hat (35 ms).
const LEVEL_PROBES: u64 = 32;
/// Samples between the loudness's probes.
const LOUDNESS_STRIDE: u64 = 8;
/// The oscilloscope's row, cells and rows.
const SCOPE_ROW: usize = 19;
/// Its width in cells.
const SCOPE_CELLS: usize = 36;
/// Its height in rows.
const SCOPE_ROWS: usize = 3;
/// Samples between its columns: 72 columns cover 13.5 ms.
const SCOPE_STRIDE: u64 = 3;
/// The smallest peak the trace scales to, so near silence stays flat.
const SCOPE_QUIET: i64 = 1_000;

/// Thousandths as an envelope level, for [`share`].
fn scale_thousandths(thousandths: u32) -> i32 {
    i32::try_from(u64::from(thousandths.min(1_000)).saturating_mul(u64::try_from(ENVELOPE_ONE).unwrap_or(0)) / 1_000).unwrap_or(0)
}

/// The meter's three colours, each with its cells: green to a loud mix,
/// yellow above it, red at the top.
const LEVEL_SEGMENTS: [(Colour, usize); 3] = [(Colour::Green, 18), (Colour::Yellow, 7), (Colour::Red, 5)];
/// The level meter's row, under the bars' labels.
const LEVEL_ROW: usize = 17;
/// The meter's first colour cell.
const LEVEL_COLUMN: usize = 6;
/// The quietest loudness the meter shows, in quarter octaves: 2⁹, a
/// mean of 512, 36 dB under full scale.
const LEVEL_FLOOR: u32 = 9 * 4;
/// The loudest, in quarter octaves: 2¹³, which the loudest passages
/// reach (a full mix's mean is 4 000 to 8 000).
const LEVEL_TOP: u32 = 13 * 4;

/// Loudness as quarter octaves: four a doubling, as a meter in decibels
/// is read (a quarter octave is 1.5 dB).
fn quarter_octaves(loudness: u32) -> u32 {
    let Some(octave) = loudness.checked_ilog2() else { return 0 };
    // The two bits under the top one.
    let quarter = if octave >= 2 { (loudness >> octave.saturating_sub(2)) & 0b11 } else { 0 };
    octave.saturating_mul(4).saturating_add(quarter)
}

/// The level meter on `row`: loudness as lit sixels of the segments.
fn level_meter(screen: &mut Screen, row: usize, loudness: u32) {
    let total: usize = LEVEL_SEGMENTS.iter().map(|&(_, cells)| cells.saturating_mul(SIXELS_ACROSS)).sum();
    let range = LEVEL_TOP.saturating_sub(LEVEL_FLOOR);
    let above = quarter_octaves(loudness).saturating_sub(LEVEL_FLOOR).min(range);
    let lit = usize::try_from(above).unwrap_or(0).saturating_mul(total).checked_div(usize::try_from(range).unwrap_or(1)).unwrap_or(0);
    let mut column = LEVEL_COLUMN.saturating_add(1);
    let mut before = 0_usize;
    for &(colour, cells) in &LEVEL_SEGMENTS {
        track_bar(screen, row, column, cells, lit.saturating_sub(before), colour);
        before = before.saturating_add(cells.saturating_mul(SIXELS_ACROSS));
        column = column.saturating_add(cells).saturating_add(1);
    }
}

/// What the stream is, on row 23.
fn stream_line(screen: &mut Screen) {
    let bits = u64::from(RATE).saturating_mul(u64::from(BLOCK_BYTES)).saturating_mul(8);
    let kilobits = bits.checked_div(u64::from(Codec::ImaAdpcm.samples_per_block(BLOCK_BYTES))).unwrap_or(0) / 1_000;
    screen.coloured(23, 0, Colour::Blue, &format!("Live IMA ADPCM, {} kHz, {kilobits} kbit/s", RATE / 1_000));
}

impl Application for Radio {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(RADIO, "NeoTeletext Radio")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        self.draw(context);
    }

    fn opened(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        if page != RADIO {
            return;
        }
        let now = context.now();
        self.listeners.insert(session);
        let starting = self.stream.is_none();
        if starting {
            self.start(now);
        }
        self.join(session, now, context);
        if starting {
            self.pump(now, context);
            Self::schedule(context);
        }
        self.draw(context);
    }

    fn left_page(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        if page == RADIO && self.listeners.contains(&session) {
            let stop = self.command(STOP);
            context.sound(Audience::Session(session), alloc::vec![stop]);
            self.leave(session, context);
        }
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        if self.stream.is_none() {
            return;
        }
        let now = context.now();
        self.pump(now, context);
        self.draw(context);
        Self::schedule(context);
    }

    fn ended(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.leave(session, context);
    }
}
