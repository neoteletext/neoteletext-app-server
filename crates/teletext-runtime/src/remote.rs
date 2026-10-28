//! An application in another process, in any language (`SERVER.md`
//! section 7; `docs/applications.md`): the channel's calls become lines of
//! JSON for the process, and its lines become the channel's actions. Pages
//! arrive as TTI, read by the engine's reader (section 7.3); sounds as
//! 16-bit samples, encoded here with the engine's IMA ADPCM encoder.
//!
//! Everything the process sends is untrusted: a line that is not valid is
//! refused whole and reported through [`Context::log`], never acted on in
//! part.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_audio::encode::{Command, Sound};
use neoteletext_audio::record::{FLAG_AFTER_CURRENT, FLAG_LOOP};
use neoteletext_audio::{CHANNELS, Codec, IMA_BLOCK_LARGE, IMA_BLOCK_SMALL, MAX_GAIN, OPUS_FRAME_SAMPLES, OPUS_RATE};
use neoteletext_formats::item::{Item, Reader};
use neoteletext_formats::page::PageFile;
use neoteletext_formats::tti::TtiReader;
use neoteletext_input::event::Action;
use neoteletext_input::keys;
use neoteletext_opus::{DecodeError, Decoder, FRAME_SAMPLES, PacketError};
use neoteletext_web::base64;

use crate::application::{Application, Audience, Context, Level, Owner, PageEntry};
use crate::channel::SessionId;
use crate::input::{Input, Named};
use crate::json::{self, Value, Writer};
use crate::screen::address;

/// The protocol's version, in `start` (`docs/applications.md`).
pub const PROTOCOL: i64 = 1;
/// The most sub-pages one publication may hold.
pub const MOST_SUBPAGES: usize = 64;
/// The most pages an application may declare.
pub const MOST_PAGES: usize = 800;
/// The longest page title: the engine's `contents.txt` rule (its
/// REQ-WEB-025, decision E.120).
const TITLE_LIMIT: usize = neoteletext_web::contents::MAX_TITLE;
/// A command's operations (the engine's `audio.md` section 4.4).
const PLAY: u8 = 1;
/// Stop.
const STOP: u8 = 2;
/// Set the gain.
const GAIN: u8 = 3;
/// The highest duck step (`audio.md` section 4.4: 0 to 6).
const MOST_DUCK: u8 = 6;
/// The profile's channels, as a count of slots.
const CHANNEL_SLOTS: usize = CHANNELS as usize;
/// The longest sound one message may carry: two minutes at 48 kHz, the
/// browser's own limit for a sound.
const MOST_SAMPLES: usize = 48_000 * 120;
/// The Opus profile's average rate (the engine's decision E.123): 16 kbit/s.
const OPUS_BITS_PER_SECOND: u32 = 16_000;
/// Opus frames a second: 48 kHz in frames of 960 samples, 20 ms.
const OPUS_FRAMES_PER_SECOND: u32 = OPUS_RATE / OPUS_FRAME_SAMPLES;
/// Bits in a byte.
const BITS_PER_BYTE: u32 = 8;
/// The longest packet a constant-rate sound may have and keep to the
/// profile's average: 40 bytes, the 16 kbit/s of 20 ms (decision 21; the
/// engine's E.123 checks the average where a sound is published, not in
/// its decoder).
const OPUS_MOST_PACKET_BYTES: u32 = OPUS_BITS_PER_SECOND / OPUS_FRAMES_PER_SECOND / BITS_PER_BYTE;
const _: () = assert!(OPUS_MOST_PACKET_BYTES * BITS_PER_BYTE * OPUS_FRAMES_PER_SECOND == OPUS_BITS_PER_SECOND);
/// The most packets one message may carry: a stored sound's two minutes.
const MOST_OPUS_PACKETS: usize = MOST_SAMPLES / FRAME_SAMPLES;

/// Why a line from the process was refused.
type Refusal = String;

/// The pages a process declares in its first line, `{"type":"pages",…}`.
///
/// # Errors
/// A line that is not a valid declaration, with the reason.
pub fn declared_pages(line: &[u8]) -> Result<Vec<PageEntry>, Refusal> {
    let value = json::parse(line).map_err(|fault| format!("not JSON: {fault:?}"))?;
    if value.get("type").and_then(Value::text) != Some("pages") {
        return Err("the first line must be {\"type\":\"pages\",…}".to_string());
    }
    let items = value.get("pages").and_then(Value::items).ok_or("pages: an array is needed")?;
    if items.len() > MOST_PAGES {
        return Err(format!("pages: at most {MOST_PAGES}"));
    }
    let mut entries: Vec<PageEntry> = Vec::new();
    for item in items {
        let number = page_number(item.get("page")).ok_or("pages: a page is a number such as \"100\"")?;
        let title = title_of(item.get("title"), number)?;
        let section = match item.get("section") {
            None | Some(Value::Null) => None,
            some => {
                Some(title_of(some, number).map_err(|_| format!("pages: the section {number:03X} starts is over {TITLE_LIMIT} characters or has controls"))?)
            }
        };
        if entries.iter().any(|entry| entry.number == number) {
            return Err(format!("pages: {number:03X} twice"));
        }
        let entry = PageEntry::new(number, title);
        entries.push(match section {
            Some(name) if !name.is_empty() => entry.starting(name),
            _ => entry,
        });
    }
    Ok(entries)
}

/// Whether `title` may be a page's title or a section's name: at most
/// [`TITLE_LIMIT`] characters, none a control.
pub(crate) fn is_title(title: &str) -> bool {
    title.chars().count() <= TITLE_LIMIT && !title.chars().any(char::is_control)
}

/// A page's title, none being empty: at most [`TITLE_LIMIT`] characters, none a control.
fn title_of(value: Option<&Value>, number: u16) -> Result<&str, Refusal> {
    let title = value.and_then(Value::text).unwrap_or("");
    if !is_title(title) {
        return Err(format!("the title of {number:03X} is over {TITLE_LIMIT} characters or has controls"));
    }
    Ok(title)
}

/// A page number as the protocol writes it: hexadecimal text, `"100"` to
/// `"8FF"`, magazine 1 to 8 (EN 300 706 clause 3.1).
fn page_number(value: Option<&Value>) -> Option<u16> {
    let text = value?.text()?;
    if text.len() != 3 {
        return None;
    }
    let number = u16::from_str_radix(text, 16).ok()?;
    (0x100..=0x8FF).contains(&number).then_some(number)
}

/// A page number written as the protocol writes it.
fn page_text(number: u16) -> String {
    format!("{number:03X}")
}

/// An application in another process.
#[derive(Debug)]
pub struct Remote {
    pages: Vec<PageEntry>,
    /// The last command's number on each of the profile's four channels:
    /// a receiver acts only on a newer one (`audio.md` section 7.1).
    sequences: [u16; CHANNEL_SLOTS],
    /// The engine's Opus decoder, made at the first Opus sound, which holds
    /// each packet to the profile the receivers decode (decision 21).
    opus: Option<OpusCheck>,
}

/// The decoder that checks Opus packets: about 28 KB, so made only for a
/// program that sends Opus.
struct OpusCheck(Box<Decoder>);

impl core::fmt::Debug for OpusCheck {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("OpusCheck")
    }
}

impl Remote {
    /// The application, serving the pages it declared.
    #[must_use]
    pub const fn new(pages: Vec<PageEntry>) -> Self {
        Self { pages, sequences: [0; CHANNEL_SLOTS], opus: None }
    }

    /// Refuses unless `packets` are whole packets of `packet_bytes` each, at
    /// most 40 bytes, every one in the profile the receivers decode: mono,
    /// one 20 ms frame, at most 160 bytes, bare or padded (the engine's
    /// E.123 and E.124). The engine's own decoder judges each packet, from
    /// its start, so the profile is the engine's alone (requirement 10.1).
    fn check_opus(&mut self, packet_bytes: u16, packets: &[u8]) -> Result<(), Refusal> {
        if packet_bytes == 0 || u32::from(packet_bytes) > OPUS_MOST_PACKET_BYTES {
            return Err(format!("sound: packet_bytes is 1 to {OPUS_MOST_PACKET_BYTES}, the profile's 16 kbit/s at constant rate"));
        }
        let size = usize::from(packet_bytes);
        if packets.is_empty() || !packets.len().is_multiple_of(size) {
            return Err(format!("sound: packets are whole packets of {packet_bytes} bytes each"));
        }
        let OpusCheck(decoder) = self.opus.get_or_insert_with(|| OpusCheck(Box::new(Decoder::new())));
        let mut pcm = [0_i16; FRAME_SAMPLES];
        for (index, packet) in packets.chunks_exact(size).enumerate() {
            decoder.reset();
            if let Err(DecodeError::Packet(refused)) = decoder.decode(packet, &mut pcm) {
                return Err(format!("sound: packet {index} is outside the Opus profile: {}", packet_refusal(refused)));
            }
        }
        Ok(())
    }

    fn serves(&self, number: u16) -> bool {
        self.pages.iter().any(|entry| entry.number == number)
    }

    /// Carries out one line from the process.
    fn act(&mut self, line: &[u8], context: &mut Context<'_>) -> Result<(), Refusal> {
        let value = json::parse(line).map_err(|fault| format!("not JSON: {fault:?}"))?;
        match value.get("type").and_then(Value::text) {
            Some("publish") => self.publish(&value, context),
            Some("goto") => {
                let session = session_of(value.get("session")).ok_or("goto: a session is needed")?;
                let page = page_number(value.get("page")).ok_or("goto: a page such as \"100\" is needed")?;
                context.goto(session, page);
                Ok(())
            }
            Some("sound") => self.sound(&value, context),
            Some("release") => {
                let session = session_of(value.get("session")).ok_or("release: a session is needed")?;
                let page = page_number(value.get("page")).ok_or("release: a page such as \"100\" is needed")?;
                context.release(session, page);
                Ok(())
            }
            Some("title") => {
                let number = page_number(value.get("page")).ok_or("title: a page such as \"100\" is needed")?;
                if !self.serves(number) {
                    return Err(format!("title: {number:03X} was not declared"));
                }
                context.retitle(number, title_of(value.get("title"), number)?);
                Ok(())
            }
            Some("log") => {
                context.log(Level::Info, "program_log", value.get("text").and_then(Value::text).unwrap_or(""));
                Ok(())
            }
            Some("pages") => Err("pages: declared once, in the first line".to_string()),
            Some(other) => Err(format!("unknown type {other:?}")),
            None => Err("no type".to_string()),
        }
    }

    fn publish(&self, value: &Value, context: &mut Context<'_>) -> Result<(), Refusal> {
        let number = page_number(value.get("page")).ok_or("publish: a page such as \"100\" is needed")?;
        if !self.serves(number) {
            return Err(format!("publish: {number:03X} was not declared"));
        }
        let owner = match value.get("owner") {
            Some(Value::Text(text)) if text == "channel" => Owner::Channel,
            other => Owner::Session(session_of(other).ok_or("publish: owner is \"channel\" or a session")?),
        };
        let tti = value.get("tti").and_then(Value::text).ok_or("publish: tti is needed")?;
        let subpages = read_tti(tti, number)?;
        context.publish_subpages(owner, number, subpages);
        Ok(())
    }

    fn sound(&mut self, value: &Value, context: &mut Context<'_>) -> Result<(), Refusal> {
        let to = value.get("to").ok_or("sound: to is needed")?;
        let audience = if let Some(session) = to.get("session") {
            Audience::Session(session_of(Some(session)).ok_or("sound: to.session is a session")?)
        } else {
            Audience::Viewers(page_number(to.get("page")).ok_or("sound: to is {\"session\":n} or {\"page\":\"100\"}")?)
        };
        let records = if let Some(object) = value.get("object") {
            let (handle, generation) = handle_of(object)?;
            let rate = rate_of(object)?;
            let pcm = samples(object)?;
            let sound = Sound::new(handle, generation, Codec::ImaAdpcm, rate, IMA_BLOCK_SMALL, &pcm).map_err(|error| format!("sound.object: {error:?}"))?;
            let mut records = alloc::vec![sound.object_record().map_err(|error| format!("sound.object: {error:?}"))?];
            records.extend(sound.data_records());
            records
        } else if let Some(opus) = value.get("opus") {
            let (handle, generation) = handle_of(opus)?;
            let packet_bytes = packet_bytes_of(opus)?;
            let packets = opus_packets(opus)?;
            self.check_opus(packet_bytes, &packets)?;
            let samples =
                opus.get("samples").and_then(Value::integer).and_then(|samples| u32::try_from(samples).ok()).ok_or("sound.opus: samples is a count")?;
            let sound = Sound::opus(handle, generation, packet_bytes, &packets, samples).map_err(|error| format!("sound.opus: {error:?}"))?;
            let mut records = alloc::vec![sound.object_record().map_err(|error| format!("sound.opus: {error:?}"))?];
            records.extend(sound.data_records());
            records
        } else if let Some(live) = value.get("opus_live") {
            let (handle, generation) = handle_of(live)?;
            let packet_bytes = packet_bytes_of(live)?;
            if u32::from(packet_bytes) > OPUS_MOST_PACKET_BYTES {
                return Err(format!("sound.opus_live: packet_bytes is 1 to {OPUS_MOST_PACKET_BYTES}, the profile's 16 kbit/s at constant rate"));
            }
            let sound = Sound::live(handle, generation, Codec::Opus, OPUS_RATE, packet_bytes).map_err(|error| format!("sound.opus_live: {error:?}"))?;
            alloc::vec![sound.object_record().map_err(|error| format!("sound.opus_live: {error:?}"))?]
        } else if let Some(live) = value.get("opus_packets") {
            let (handle, generation) = handle_of(live)?;
            let packet_bytes = packet_bytes_of(live)?;
            let first =
                live.get("first").and_then(Value::integer).and_then(|first| u16::try_from(first).ok()).ok_or("sound.opus_packets: first is a block number")?;
            let packets = opus_packets(live)?;
            self.check_opus(packet_bytes, &packets)?;
            let sound = Sound::live(handle, generation, Codec::Opus, OPUS_RATE, packet_bytes).map_err(|error| format!("sound.opus_packets: {error:?}"))?;
            sound.packet_records(first, &packets).map_err(|error| format!("sound.opus_packets: {error:?}"))?
        } else if let Some(live) = value.get("live") {
            let (handle, generation) = handle_of(live)?;
            let sound = Sound::live(handle, generation, Codec::ImaAdpcm, rate_of(live)?, IMA_BLOCK_LARGE).map_err(|error| format!("sound.live: {error:?}"))?;
            alloc::vec![sound.object_record().map_err(|error| format!("sound.live: {error:?}"))?]
        } else if let Some(blocks) = value.get("blocks") {
            let (handle, generation) = handle_of(blocks)?;
            let first =
                blocks.get("first").and_then(Value::integer).and_then(|first| u16::try_from(first).ok()).ok_or("sound.blocks: first is a block number")?;
            let sound =
                Sound::live(handle, generation, Codec::ImaAdpcm, rate_of(blocks)?, IMA_BLOCK_LARGE).map_err(|error| format!("sound.blocks: {error:?}"))?;
            sound.live_records(first, &samples(blocks)?)
        } else if let Some(play) = value.get("play") {
            let (handle, generation) = handle_of(play)?;
            let mut flags = 0;
            if play.get("loop").and_then(Value::boolean) == Some(true) {
                flags |= FLAG_LOOP;
            }
            if play.get("after").and_then(Value::boolean) == Some(true) {
                flags |= FLAG_AFTER_CURRENT;
            }
            let duck = small(play.get("duck"), 0, MOST_DUCK).ok_or("sound.play: duck is 0 to 6")?;
            let page = match play.get("page") {
                None | Some(Value::Null) => None,
                some => Some(page_number(some).ok_or("sound.play: page is a page such as \"100\"")?),
            };
            alloc::vec![self.command(play, PLAY, handle, generation, flags, duck, page)?]
        } else if let Some(stop) = value.get("stop") {
            alloc::vec![self.command(stop, STOP, 0, 0, 0, 0, None)?]
        } else if let Some(gain) = value.get("gain") {
            alloc::vec![self.command(gain, GAIN, 0, 0, 0, 0, None)?]
        } else {
            return Err("sound: one of opus, opus_live, opus_packets, object, live, blocks, play, stop or gain is needed".to_string());
        };
        context.sound(audience, records);
        Ok(())
    }

    /// A command on the channel `fields` names, numbered after the last.
    #[expect(clippy::too_many_arguments, reason = "the fields of one CONTROL record (audio.md section 4.4)")]
    fn command(&mut self, fields: &Value, operation: u8, handle: u8, generation: u16, flags: u8, duck: u8, page: Option<u16>) -> Result<Vec<u8>, Refusal> {
        let channel = fields
            .get("channel")
            .and_then(Value::integer)
            .and_then(|channel| usize::try_from(channel).ok())
            .filter(|&channel| channel < CHANNEL_SLOTS)
            .ok_or("sound: channel is 0 to 3")?;
        let gain = if operation == STOP { 0 } else { small(fields.get("gain"), MAX_GAIN, MAX_GAIN).ok_or("sound: gain is 0 to 100")? };
        let fade_ms =
            fields.get("fade_ms").map_or(Some(0), |fade| fade.integer().and_then(|fade| u16::try_from(fade).ok())).ok_or("sound: fade_ms is 0 to 65535")?;
        let slot = self.sequences.get_mut(channel).ok_or("sound: channel is 0 to 3")?;
        *slot = slot.wrapping_add(1);
        let command = Command { channel: u8::try_from(channel).unwrap_or(0), sequence: *slot, operation, handle, generation, gain, fade_ms, flags, duck, page };
        Ok(command.record())
    }

    fn message(context: &mut Context<'_>, writer: Writer) {
        context.message(writer.line());
    }
}

/// A session as the protocol writes it: a whole number.
fn session_of(value: Option<&Value>) -> Option<SessionId> {
    value?.integer().and_then(|session| SessionId::try_from(session).ok())
}

/// A small number from `value`, `default` when absent, at most `most`.
fn small(value: Option<&Value>, default: u8, most: u8) -> Option<u8> {
    match value {
        None => Some(default),
        Some(value) => value.integer().and_then(|number| u8::try_from(number).ok()).filter(|&number| number <= most),
    }
}

fn handle_of(fields: &Value) -> Result<(u8, u16), Refusal> {
    let handle = fields.get("handle").and_then(Value::integer).and_then(|handle| u8::try_from(handle).ok()).ok_or("sound: handle is 0 to 255")?;
    let generation = fields
        .get("generation")
        .map_or(Some(0), |generation| generation.integer().and_then(|generation| u16::try_from(generation).ok()))
        .ok_or("sound: generation is 0 to 65535")?;
    Ok((handle, generation))
}

fn rate_of(fields: &Value) -> Result<u32, Refusal> {
    fields
        .get("rate")
        .and_then(Value::integer)
        .and_then(|rate| u32::try_from(rate).ok())
        .ok_or_else(|| "sound: rate is one of the profile's sample rates".to_string())
}

/// The 16-bit little-endian samples of `pcm`, base64 (RFC 4648 section 4).
fn samples(fields: &Value) -> Result<Vec<i16>, Refusal> {
    let text = fields.get("pcm").and_then(Value::text).ok_or("sound: pcm is base64 of 16-bit little-endian samples")?;
    let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(text.len())];
    let length = base64::decode(text.as_bytes(), &mut bytes).map_err(|error| format!("sound: pcm is not base64: {error:?}"))?;
    bytes.truncate(length);
    if !bytes.len().is_multiple_of(2) || bytes.len() / 2 > MOST_SAMPLES {
        return Err(format!("sound: pcm is whole samples, at most {MOST_SAMPLES}"));
    }
    Ok(bytes.as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect())
}

/// An Opus sound's packet length.
fn packet_bytes_of(fields: &Value) -> Result<u16, Refusal> {
    fields
        .get("packet_bytes")
        .and_then(Value::integer)
        .and_then(|bytes| u16::try_from(bytes).ok())
        .filter(|&bytes| bytes > 0)
        .ok_or_else(|| format!("sound: packet_bytes is 1 to {OPUS_MOST_PACKET_BYTES}"))
}

/// An Opus sound's packets, back to back, base64 (RFC 4648 section 4): at
/// most two minutes of them, bounded before anything is decoded.
fn opus_packets(fields: &Value) -> Result<Vec<u8>, Refusal> {
    let text = fields.get("packets").and_then(Value::text).ok_or("sound: packets is base64 of the Opus packets, back to back")?;
    let most_bytes = MOST_OPUS_PACKETS.saturating_mul(usize::try_from(OPUS_MOST_PACKET_BYTES).unwrap_or(usize::MAX));
    if text.len() > base64::encoded_len(most_bytes) {
        return Err(format!("sound: packets are at most {MOST_OPUS_PACKETS}, two minutes"));
    }
    let mut bytes = alloc::vec![0_u8; base64::decoded_len_max(text.len())];
    let length = base64::decode(text.as_bytes(), &mut bytes).map_err(|error| format!("sound: packets are not base64: {error:?}"))?;
    bytes.truncate(length);
    Ok(bytes)
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

/// The sub-pages of a TTI text, each addressed to `number` whatever its
/// `PN` line says: the publication's page is the one the process named.
fn read_tti(tti: &str, number: u16) -> Result<Vec<PageFile>, Refusal> {
    let mut reader = TtiReader::new();
    let mut subpages: Vec<PageFile> = Vec::new();
    let mut fault: Option<String> = None;
    let mut collect = |item: Item<'_>| match item {
        Item::Page(page) => {
            if subpages.len() < MOST_SUBPAGES {
                let mut page = *page;
                page.address = address(number);
                subpages.push(page);
            } else if fault.is_none() {
                fault = Some(format!("publish: at most {MOST_SUBPAGES} sub-pages"));
            }
        }
        Item::Diagnostic(diagnostic) if fault.is_none() => fault = Some(format!("publish: TTI: {diagnostic:?}")),
        _ => {}
    };
    reader.feed(tti.as_bytes(), &mut collect);
    reader.finish(&mut collect);
    if let Some(fault) = fault {
        return Err(fault);
    }
    if subpages.is_empty() {
        return Err("publish: the TTI holds no page (a PN line starts one)".to_string());
    }
    Ok(subpages)
}

/// A key's name as the protocol writes it: W3C `KeyboardEvent.key` names
/// (INPUT.md section 5.6), or the character for a character key.
fn key_name(key: Named) -> String {
    match key {
        Named::Up => "ArrowUp".to_string(),
        Named::Down => "ArrowDown".to_string(),
        Named::Left => "ArrowLeft".to_string(),
        Named::Right => "ArrowRight".to_string(),
        Named::Enter => "Enter".to_string(),
        Named::Escape => "Escape".to_string(),
        Named::Backspace => "Backspace".to_string(),
        Named::Delete => "Delete".to_string(),
        Named::Tab => "Tab".to_string(),
        Named::Home => "Home".to_string(),
        Named::End => "End".to_string(),
        Named::PageUp => "PageUp".to_string(),
        Named::PageDown => "PageDown".to_string(),
        Named::Character(character) => character.to_string(),
        Named::Other(code) => keys::name(code).unwrap_or("Unidentified").to_string(),
    }
}

/// An input event as the protocol writes it.
fn event(input: &Input<'_>) -> Writer {
    let mut event = Writer::nested();
    match *input {
        Input::Text(text) => {
            event.text("kind", "text").text("text", text);
        }
        Input::Paste(text) => {
            event.text("kind", "paste").text("text", text);
        }
        Input::Key { action, key, text, base } => {
            let action = match action {
                Action::Press => "press",
                Action::Repeat => "repeat",
                Action::Release => "release",
            };
            event.text("kind", "key").text("action", action).text("key", &key_name(key));
            if let Some(text) = text {
                event.text("text", text);
            }
            if let Some(base) = base {
                event.text("base", &base.to_string());
            }
        }
        Input::Mouse { pressed, button, column, row } => {
            event
                .text("kind", "mouse")
                .text("action", if pressed { "press" } else { "release" })
                .boolean("pressed", pressed)
                .integer("button", i64::from(button))
                .integer("column", i64::from(column))
                .integer("row", i64::from(row));
        }
        Input::Moved { buttons, column, row } => {
            event
                .text("kind", "mouse")
                .text("action", "move")
                .integer("buttons", i64::from(buttons))
                .integer("column", i64::from(column))
                .integer("row", i64::from(row));
        }
        Input::Wheel { dx, dy, column, row } => {
            event
                .text("kind", "wheel")
                .integer("dx", i64::from(dx))
                .integer("dy", i64::from(dy))
                .integer("column", i64::from(column))
                .integer("row", i64::from(row));
        }
        Input::Hover { entered, column, row } => {
            event
                .text("kind", "hover")
                .text("action", if entered { "enter" } else { "leave" })
                .boolean("entered", entered)
                .integer("column", i64::from(column))
                .integer("row", i64::from(row));
        }
        Input::FocusIn => {
            event.text("kind", "focus");
        }
        Input::FocusOut => {
            event.text("kind", "blur");
        }
    }
    event
}

fn session_line(kind: &str, session: SessionId) -> Writer {
    let mut writer = Writer::new(kind);
    writer.integer("session", i64::try_from(session).unwrap_or(i64::MAX));
    writer
}

impl Application for Remote {
    fn pages(&self) -> Vec<PageEntry> {
        self.pages.clone()
    }

    fn declared(&mut self, pages: &[PageEntry]) {
        self.pages = pages.to_vec();
    }

    fn start(&mut self, context: &mut Context<'_>) {
        let mut writer = Writer::new("start");
        writer.integer("protocol", PROTOCOL).integer("epoch_ms", i64::try_from(context.epoch_ms()).unwrap_or(i64::MAX));
        Self::message(context, writer);
    }

    fn joined(&mut self, session: SessionId, context: &mut Context<'_>) {
        Self::message(context, session_line("join", session));
    }

    fn opened(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        let mut writer = session_line("open", session);
        writer.text("page", &page_text(page));
        Self::message(context, writer);
    }

    fn left_page(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        let mut writer = session_line("leave", session);
        writer.text("page", &page_text(page));
        Self::message(context, writer);
    }

    fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let mut writer = session_line("input", session);
        writer.text("page", &page_text(page)).object("event", event(input));
        Self::message(context, writer);
    }

    fn ended(&mut self, session: SessionId, context: &mut Context<'_>) {
        Self::message(context, session_line("close", session));
    }

    fn process_line(&mut self, line: &[u8], context: &mut Context<'_>) {
        if let Err(refusal) = self.act(line, context) {
            context.log(Level::Warn, "line_refused", &format!("refused a line from the application: {refusal}"));
        }
    }
}

#[cfg(test)]
mod tests;
