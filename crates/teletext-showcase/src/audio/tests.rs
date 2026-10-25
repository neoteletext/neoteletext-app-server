use alloc::string::ToString;
use alloc::vec::Vec;

use neoteletext_audio::MAX_BLOCK_SAMPLES;
use neoteletext_audio::idl::{Reassembler, fragment};
use neoteletext_audio::ima::decode_block;
use neoteletext_audio::port::{SoundFormat, SoundId, Start, VoiceSink};
use neoteletext_audio::receiver::Receiver;
use neoteletext_audio::record::{Operation, Record, parse};
use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_input::event::{Action, Event, Item, Key, KeyCode, write};
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use teletext_runtime::{Channel, ChannelConfig, Effect, Packet};

use super::radio::{BLOCK_BYTES, CHANNEL, HANDLE, LEAD_MS, TICK_MS, sample_at};
use super::synth::RATE;
use super::tracker::{MIX_PEAK, ROWS_PER_BAR, Tracker, melody_cell};
use super::tunes::PLAYLIST;
use super::{BOARD, DRUMS, MENU, PIANO, RADIO, applications, drums};

fn channel() -> Channel {
    let config = ChannelConfig { name: "Sound".to_string(), push_address: "ws://127.0.0.1:1/sound/push".to_string(), initial_page: MENU, epoch_at_start_ms: 0 };
    let mut channel = Channel::new(config, applications(&[]), 1);
    channel.start(0);
    channel
}

fn message(item: &Item<'_>) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; neoteletext_input::MAX_MESSAGE];
    let length = write(item, &mut bytes).unwrap();
    bytes.truncate(length);
    bytes
}

/// A session that said HELLO, with `page` open.
fn join(channel: &mut Channel, session: u64, page: u16, now: u64) {
    channel.opened(session, now);
    let hello = Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None };
    channel.received(session, &message(&Item::Event(hello)), now);
    open(channel, session, page, now);
}

fn open(channel: &mut Channel, session: u64, page: u16, now: u64) {
    channel.received(session, &message(&Item::Event(Event::Page(page))), now);
}

fn key(channel: &mut Channel, session: u64, action: Action, code: KeyCode, base: Option<char>, now: u64) {
    let key = Key { action, key: code, mods: 0, text: None, base };
    channel.received(session, &message(&Item::Event(Event::Key(key))), now);
}

/// Every packet sent to `session`.
fn packets(effects: &[Effect], session: u64) -> Vec<Packet> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { session: to, bytes } if *to == session => Some(bytes),
            _ => None,
        })
        .flat_map(|bytes| bytes.chunks(PACKET_BYTES).map(|chunk| Packet { bytes: chunk.try_into().unwrap() }).collect::<Vec<_>>())
        .collect()
}

/// The audio records sent to `session`, whole, in order.
fn records(effects: &[Effect], session: u64) -> Vec<Vec<u8>> {
    let mut reassembler = Reassembler::<8>::new();
    let mut out = Vec::new();
    for packet in packets(effects, session) {
        let Some(Ok(fragment)) = fragment(&packet) else { continue };
        if let Some((_, record)) = reassembler.push(&fragment)
            && parse(record).is_ok()
        {
            out.push(record.to_vec());
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Object { handle: u8, generation: u16, live: bool, rate: u32, block: u16 },
    Play { channel: u8, handle: u8, generation: u16, page: Option<u16> },
    Stop { channel: u8 },
    Data { handle: u8, generation: u16, first: u16 },
    Other,
}

fn kind(record: &[u8]) -> Kind {
    match parse(record).unwrap() {
        Record::Object(object) => {
            Kind::Object { handle: object.handle, generation: object.generation, live: object.samples == 0, rate: object.rate_hz, block: object.block_bytes }
        }
        Record::Control(control) if control.operation == Operation::Play => {
            Kind::Play { channel: control.channel, handle: control.handle, generation: control.generation, page: control.page }
        }
        Record::Control(control) if control.operation == Operation::Stop => Kind::Stop { channel: control.channel },
        Record::Data(data) => Kind::Data { handle: data.handle, generation: data.generation, first: data.first_block },
        Record::Control(_) => Kind::Other,
    }
}

fn kinds(effects: &[Effect], session: u64) -> Vec<Kind> {
    records(effects, session).iter().map(|record| kind(record)).collect()
}

/// The radio's data records among `records`.
fn blocks(records: &[Vec<u8>]) -> Vec<Vec<u8>> {
    records.iter().filter(|record| matches!(kind(record), Kind::Data { handle: HANDLE, .. })).cloned().collect()
}

/// Whole blocks due in the lead from a start.
fn blocks_in_lead() -> usize {
    let per_block = u64::from(neoteletext_audio::Codec::ImaAdpcm.samples_per_block(BLOCK_BYTES));
    usize::try_from(sample_at(LEAD_MS).checked_div(per_block).unwrap()).unwrap()
}

/// Opening the radio sends that session, and only it, the live object,
/// a PLAY bound to the page on channel 3, then the lead's blocks.
#[test]
fn a_listener_is_sent_the_live_object_a_play_and_blocks() {
    let mut channel = channel();
    join(&mut channel, 2, MENU, 0);
    join(&mut channel, 1, RADIO, 0);
    let effects = channel.take_effects();
    let kinds = kinds(&effects, 1);
    assert_eq!(kinds.first(), Some(&Kind::Object { handle: HANDLE, generation: 1, live: true, rate: RATE, block: 512 }));
    assert_eq!(kinds.get(1), Some(&Kind::Play { channel: CHANNEL, handle: HANDLE, generation: 1, page: Some(RADIO) }));
    let data: Vec<&Kind> = kinds.iter().skip(2).collect();
    assert_eq!(data.len(), blocks_in_lead(), "{data:?}");
    for (index, block) in data.iter().enumerate() {
        assert_eq!(**block, Kind::Data { handle: HANDLE, generation: 1, first: u16::try_from(index).unwrap() });
    }
    assert!(records(&effects, 2).is_empty(), "the menu's viewer hears nothing");
    assert_eq!(channel.next_wake(), Some(TICK_MS), "the stream tops up on the tick");
}

/// Two listeners: the second joins the same generation with the blocks
/// not yet heard, and from then on both get the same records.
#[test]
fn two_listeners_get_the_same_blocks() {
    let mut channel = channel();
    join(&mut channel, 1, RADIO, 0);
    let first = blocks(&records(&channel.take_effects(), 1));
    join(&mut channel, 2, RADIO, 100);
    let effects = channel.take_effects();
    let joined = records(&effects, 2);
    let kinds: Vec<Kind> = joined.iter().map(|record| kind(record)).collect();
    assert!(matches!(kinds.first(), Some(Kind::Object { generation: 1, .. })));
    assert!(matches!(kinds.get(1), Some(Kind::Play { generation: 1, .. })));
    assert!(records(&effects, 1).is_empty(), "the first listener is sent nothing again");
    let caught_up = blocks(&joined);
    assert!(!caught_up.is_empty() && caught_up.len() < first.len(), "{} of {}", caught_up.len(), first.len());
    assert_eq!(caught_up[..], first[first.len() - caught_up.len()..], "the newest blocks, the same bytes");
    for tick in 1..=8 {
        channel.deadline(tick * TICK_MS);
        let effects = channel.take_effects();
        let (one, two) = (blocks(&records(&effects, 1)), blocks(&records(&effects, 2)));
        assert!(!one.is_empty(), "tick {tick} sent blocks");
        assert_eq!(one, two, "tick {tick}: one stream for both");
    }
}

/// Leaving the page stops the radio for that listener; with nobody left
/// nothing is made, and no wake is asked for.
#[test]
fn leaving_stops_and_nobody_listening_makes_nothing() {
    let mut channel = channel();
    assert_eq!(channel.next_wake(), None, "off air at the start");
    join(&mut channel, 1, RADIO, 0);
    channel.take_effects();
    open(&mut channel, 1, MENU, 100);
    let effects = channel.take_effects();
    assert_eq!(kinds(&effects, 1), [Kind::Stop { channel: CHANNEL }]);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Shared { page: RADIO, .. })), "the page goes off air");
    for now in (TICK_MS..=2_000).step_by(100) {
        if channel.next_wake().is_some_and(|at| at <= now) {
            channel.deadline(now);
        }
    }
    let effects = channel.take_effects();
    assert!(effects.is_empty(), "nothing made or sent: {effects:?}");
    assert_eq!(channel.next_wake(), None);
}

/// A listener whose connection ends is gone; the next listener starts a
/// new generation.
#[test]
fn a_closed_listener_stops_the_stream() {
    let mut channel = channel();
    join(&mut channel, 1, RADIO, 0);
    channel.closed(1, 50);
    channel.take_effects();
    channel.deadline(TICK_MS);
    assert_eq!(channel.take_effects(), []);
    join(&mut channel, 2, RADIO, 1_000);
    let kinds = kinds(&channel.take_effects(), 2);
    assert!(matches!(kinds.first(), Some(Kind::Object { generation: 2, .. })), "{kinds:?}");
}

#[derive(Default)]
struct Sink {
    streams: Vec<(SoundId, u32)>,
    starts: Vec<(u8, Start)>,
    stops: Vec<u8>,
    samples: Vec<i16>,
}

impl VoiceSink for Sink {
    fn define(&mut self, _: SoundId, _: SoundFormat) {}
    fn samples(&mut self, _: SoundId, _: u32, _: &[i16]) {}
    fn forget(&mut self, _: SoundId) {}
    fn stream(&mut self, sound: SoundId, rate_hz: u32) {
        self.streams.push((sound, rate_hz));
    }
    fn stream_samples(&mut self, _: SoundId, pcm: &[i16]) {
        self.samples.extend_from_slice(pcm);
    }
    fn start(&mut self, channel: u8, start: Start) {
        self.starts.push((channel, start));
    }
    fn stop(&mut self, channel: u8, _: u16) {
        self.stops.push(channel);
    }
    fn gain(&mut self, _: u8, _: u8, _: u16) {}
}

/// The power of `reference` over that of its difference from `decoded`:
/// 100 is 20 dB.
fn power_ratio(reference: &[i16], decoded: &[i16]) -> f64 {
    let (mut signal, mut noise) = (0.0_f64, 0.0_f64);
    for (&expected, &got) in reference.iter().zip(decoded) {
        let difference = f64::from(expected) - f64::from(got);
        signal += f64::from(expected) * f64::from(expected);
        noise += difference * difference;
    }
    signal / noise.max(1.0)
}

/// The engine's own receiver plays what a listener is sent: a live sound
/// at 16 kHz started on channel 3 with the page open, its samples the
/// tracker's through IMA ADPCM, none lost.
#[test]
fn the_engine_receiver_plays_the_stream() {
    let mut channel = channel();
    let opened_at = 1_000;
    join(&mut channel, 1, RADIO, opened_at);
    let mut arena = alloc::vec![0_u8; 1 << 20];
    let mut receiver = Receiver::<16, 8>::new(&mut arena);
    let mut sink = Sink::default();
    receiver.open_page(Some(RADIO), &mut sink);
    let mut now = opened_at;
    let mut sent = Vec::new();
    loop {
        receiver.tick(now, &mut sink);
        let effects = channel.take_effects();
        for packet in packets(&effects, 1) {
            receiver.push_packet(&packet, &mut sink);
        }
        sent.extend(blocks(&records(&effects, 1)));
        if now >= opened_at + 2_000 {
            break;
        }
        now += TICK_MS;
        channel.deadline(now);
    }
    assert_eq!(sink.streams.first().map(|&(sound, rate)| (sound.handle, rate)), Some((HANDLE, RATE)));
    assert_eq!(sink.starts.first().map(|&(on, start)| (on, start.sound.handle)), Some((CHANNEL, HANDLE)));
    assert!(sink.stops.iter().all(|&stopped| stopped != CHANNEL), "never stalled: {:?}", sink.stops);
    let statistics = receiver.statistics();
    assert_eq!((statistics.records_refused, statistics.conflicts, statistics.concealed, statistics.decode_failures), (0, 0, 0, 0));
    assert!(sink.samples.len() >= usize::try_from(sample_at(2_000)).unwrap(), "{} samples", sink.samples.len());
    // Exactly the blocks sent, decoded, in order: nothing lost or late.
    let mut decoded = Vec::new();
    for record in &sent {
        let Ok(Record::Data(data)) = parse(record) else { panic!("a data record") };
        let mut block = [0_i16; MAX_BLOCK_SAMPLES];
        let length = decode_block(data.blocks, &mut block).unwrap();
        decoded.extend_from_slice(&block[..length]);
    }
    assert_eq!(sink.samples, decoded[..sink.samples.len()]);
    let reference = Tracker::new().render(sample_at(opened_at), sink.samples.len());
    // IMA ADPCM keeps about 30 dB (the engine's `audio.md` section 11);
    // this music's noise drums keep less, and the whole over 20 dB.
    let ratio = power_ratio(&reference, &sink.samples);
    assert!(ratio > 100.0, "signal to noise as a power ratio {ratio}");
}

/// The piano plays by key position: Q on a French keyboard is A's key,
/// middle C, the library's handle 1; a repeat or a release sends no sound.
#[test]
fn the_piano_plays_a_note_by_key_position() {
    let mut channel = channel();
    join(&mut channel, 1, PIANO, 0);
    let loaded = kinds(&channel.take_effects(), 1);
    assert!(loaded.iter().filter(|kind| matches!(kind, Kind::Object { .. })).count() > 13, "the library first");
    key(&mut channel, 1, Action::Press, KeyCode::Character('q'), Some('a'), 10);
    let effects = channel.take_effects();
    let played = kinds(&effects, 1);
    assert!(matches!(played.as_slice(), [Kind::Play { handle: 1, page: None, channel: 0..=2, .. }]), "{played:?}");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send { session: 1, bytes } if bytes.len() > 20 * PACKET_BYTES)), "the page lights the key");
    key(&mut channel, 1, Action::Repeat, KeyCode::Character('q'), Some('a'), 20);
    assert!(records(&channel.take_effects(), 1).is_empty(), "a repeat plays nothing");
    key(&mut channel, 1, Action::Release, KeyCode::Character('q'), Some('a'), 30);
    assert!(records(&channel.take_effects(), 1).is_empty(), "a release plays nothing");
    key(&mut channel, 1, Action::Press, KeyCode::Character('k'), None, 40);
    let played = kinds(&channel.take_effects(), 1);
    assert!(matches!(played.as_slice(), [Kind::Play { handle: 13, .. }]), "the C above, without base: {played:?}");
}

/// A digit on the board plays its effect: 3 is the hit, handle 22.
#[test]
fn the_board_plays_an_effect_for_a_digit() {
    let mut channel = channel();
    join(&mut channel, 1, BOARD, 0);
    channel.take_effects();
    channel.received(1, &message(&Item::Text("3")), 10);
    let played = kinds(&channel.take_effects(), 1);
    assert!(matches!(played.as_slice(), [Kind::Play { handle: 22, .. }]), "{played:?}");
}

/// Enter starts the drum machine; each step plays that step's sounds to
/// its viewer alone, on one clock.
#[test]
fn the_drum_machine_plays_its_steps_in_time() {
    let mut channel = channel();
    join(&mut channel, 1, DRUMS, 0);
    join(&mut channel, 2, DRUMS, 0);
    channel.take_effects();
    let enter = neoteletext_input::keys::code("Enter").unwrap();
    key(&mut channel, 1, Action::Press, KeyCode::Functional(enter), None, 10);
    channel.take_effects();
    assert_eq!(channel.next_wake(), Some(drums::STEP_MS));
    let machine = drums::Machine::default();
    let mut played_any = false;
    for step in 1..=u64::try_from(drums::STEPS).unwrap() {
        let now = step * drums::STEP_MS;
        channel.deadline(now);
        let effects = channel.take_effects();
        let played = kinds(&effects, 1).iter().filter(|kind| matches!(kind, Kind::Play { .. })).count();
        let index = drums::step_at(now);
        let expected = (0..drums::TRACKS.len()).filter(|&track| machine.on(track, index)).count();
        assert_eq!(played, expected, "step {index}");
        played_any |= played > 0;
        assert!(records(&effects, 2).is_empty(), "the other viewer's machine is stopped");
    }
    assert!(played_any);
}

/// The synthesis is the same every time, stays inside 16 bits, loops, and
/// is loud enough to hear.
#[test]
fn synthesis_is_deterministic_and_in_range() {
    let (one, two) = (Tracker::new(), Tracker::new());
    let cycle = one.cycle();
    assert!(cycle > u64::from(RATE) * 60, "a playlist of minutes: {cycle}");
    let mut loudest = 0_i32;
    for second in (0..cycle / u64::from(RATE)).step_by(7) {
        let first = second * u64::from(RATE);
        let block = one.render(first, 4_000);
        assert_eq!(block, two.render(first, 4_000), "second {second}");
        for &sample in &block {
            assert!(i32::from(sample).abs() <= MIX_PEAK, "second {second}: {sample}");
            loudest = loudest.max(i32::from(sample).abs());
        }
    }
    assert!(loudest > MIX_PEAK / 3, "loud enough: {loudest}");
    assert_eq!(one.render(cycle + 12_345, 500), one.render(12_345, 500), "the playlist loops");
}

/// Every tune reads cleanly: sixteen good tokens a bar, a chord a bar,
/// drum bars of sixteen rows, titles that fit the page.
#[test]
fn the_tunes_are_well_formed() {
    let rows = usize::try_from(ROWS_PER_BAR).unwrap();
    for tune in PLAYLIST {
        assert_eq!(tune.chords.len(), tune.melody.len(), "{}", tune.title);
        for (bar, text) in tune.melody.iter().enumerate() {
            let tokens: Vec<&str> = text.split_whitespace().collect();
            assert_eq!(tokens.len(), rows, "{} bar {}", tune.title, bar + 1);
            assert!(tokens.iter().all(|token| melody_cell(token).is_some()), "{} bar {}: {text}", tune.title, bar + 1);
        }
        assert_eq!(tune.bass.split_whitespace().count(), rows, "{}", tune.title);
        assert!(tune.bass.split_whitespace().all(|token| ["R", "T", "F", "S", "O", "-", "."].contains(&token)));
        for drum in [tune.kick, tune.snare, tune.hats] {
            assert_eq!(drum.chars().count(), rows, "{}", tune.title);
        }
        assert!(tune.fill.is_empty() || tune.fill.chars().count() == rows);
        let line = alloc::format!("{}, {} bpm", tune.style, tune.tempo);
        assert!(tune.title.len() <= 32 && line.len() <= 34, "{line} fits its row");
    }
}
