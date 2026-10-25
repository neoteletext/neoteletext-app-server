//! `teletext-pages`: what a viewer of each channel would receive, as T42
//! files to look at with the engine's `neottx render` or `neottx text`,
//! with no network and no window.
//!
//! ```text
//! teletext-pages --out FOLDER [--pictures FOLDER] [--seconds N]
//!                [--type PAGE=TEXT]... [--press PAGE=KEY,KEY,...]...
//! ```
//!
//! For each channel it opens one session, says HELLO, opens each page
//! (PAGE), types and presses what was asked on that page, and runs the
//! channel's time on for `--seconds` (default 2) in steps of 100
//! milliseconds. `FOLDER/<channel>.t42` then holds the shared pages
//! followed by everything sent to the session, the last of each page
//! winning, as a receiver keeps it. Keys are INPUT.md's names, such as
//! `ArrowLeft`, `Enter` or ` ` for a character.

#![forbid(unsafe_code)]
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use std::path::PathBuf;
use std::process::ExitCode;

use neoteletext_input::event::{Action, Event, Item, Key, KeyCode, write};
use neoteletext_input::keys;
use neoteletext_input::modes::{ALL_MODES, CURSOR_CAP, ECHO_CAP, PAGE_CAP};
use teletext_runtime::{Channel, ChannelConfig, Effect};

/// The virtual clock's step, in milliseconds.
const STEP: u64 = 100;

struct Arguments {
    out: PathBuf,
    pictures: Option<PathBuf>,
    seconds: u64,
    typed: Vec<(u16, String)>,
    pressed: Vec<(u16, Vec<String>)>,
}

fn page_value(value: &str) -> Result<(u16, String), String> {
    let (page, rest) = value.split_once('=').ok_or("PAGE=VALUE")?;
    Ok((u16::from_str_radix(page, 16).map_err(|_| "a hexadecimal page")?, rest.to_owned()))
}

fn arguments() -> Result<Arguments, String> {
    let mut parsed = Arguments { out: PathBuf::from("tmp/pages"), pictures: None, seconds: 2, typed: Vec::new(), pressed: Vec::new() };
    let mut list = std::env::args().skip(1);
    while let Some(argument) = list.next() {
        let value = list.next().ok_or_else(|| format!("{argument} needs a value"))?;
        match argument.as_str() {
            "--out" => parsed.out = PathBuf::from(value),
            "--pictures" => parsed.pictures = Some(PathBuf::from(value)),
            "--seconds" => parsed.seconds = value.parse().map_err(|_| "--seconds is a number")?,
            "--type" => parsed.typed.push(page_value(&value)?),
            "--press" => {
                let (page, keys) = page_value(&value)?;
                parsed.pressed.push((page, keys.split(',').map(str::to_owned).collect()));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

/// One INPUT.md message holding `item`.
fn message(item: &Item<'_>) -> Vec<u8> {
    let mut bytes = vec![0_u8; neoteletext_input::MAX_MESSAGE];
    let length = write(item, &mut bytes).unwrap_or(0);
    bytes.truncate(length);
    bytes
}

/// A key by INPUT.md name, or a single character.
fn key_code(name: &str) -> Option<KeyCode> {
    let mut characters = name.chars();
    match (characters.next(), characters.next()) {
        (Some(character), None) => Some(KeyCode::Character(character)),
        _ => keys::code(name).map(KeyCode::Functional),
    }
}

fn main() -> ExitCode {
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("teletext-pages: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&arguments.out) {
        eprintln!("teletext-pages: {}: {error}", arguments.out.display());
        return ExitCode::FAILURE;
    }
    let pictures = arguments.pictures.as_deref().map(teletext_native::pictures::load).unwrap_or_default();
    for spec in teletext_showcase::channels(&pictures) {
        let config = ChannelConfig {
            name: spec.name.to_owned(),
            push_address: format!("ws://127.0.0.1/{}/push", spec.folder),
            initial_page: 0x100,
            epoch_at_start_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(0)),
        };
        let mut channel = Channel::new(config, spec.applications, 1);
        let mut now = 0_u64;
        channel.start(now);
        let mut received: Vec<u8> = Vec::new();
        let mut owned: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
        let take = |channel: &mut Channel, received: &mut Vec<u8>, owned: &mut std::collections::BTreeSet<u16>| {
            for effect in channel.take_effects() {
                match effect {
                    Effect::Send { bytes, .. } => received.extend_from_slice(&bytes),
                    Effect::Own { page, .. } => {
                        owned.insert(page);
                    }
                    Effect::Disown { page, .. } => {
                        owned.remove(&page);
                    }
                    _ => {}
                }
            }
        };
        channel.opened(1, now);
        channel.received(
            1,
            &message(&Item::Event(Event::Hello { version: neoteletext_input::VERSION, caps: PAGE_CAP | CURSOR_CAP | ECHO_CAP | ALL_MODES, agent: None })),
            now,
        );
        take(&mut channel, &mut received, &mut owned);
        let pages: Vec<u16> = channel.entries().map(|entry| entry.number).collect();
        for page in pages {
            channel.received(1, &message(&Item::Event(Event::Page(page))), now);
            for (_, text) in arguments.typed.iter().filter(|(on, _)| *on == page) {
                channel.received(1, &message(&Item::Text(text)), now);
            }
            for (_, names) in arguments.pressed.iter().filter(|(on, _)| *on == page) {
                for name in names {
                    let Some(code) = key_code(name) else { continue };
                    for action in [Action::Press, Action::Release] {
                        let key = Key { action, key: code, mods: 0, text: None, base: None };
                        channel.received(1, &message(&Item::Event(Event::Key(key))), now);
                    }
                    now = now.saturating_add(STEP);
                    channel.deadline(now);
                }
            }
            take(&mut channel, &mut received, &mut owned);
        }
        let end = now.saturating_add(arguments.seconds.saturating_mul(1_000));
        while now < end {
            now = now.saturating_add(STEP);
            if channel.next_wake().is_some_and(|at| at <= now) {
                channel.deadline(now);
            }
        }
        // What the session was sent, then each shared page as it ended,
        // except those the session owns: the last frame of a page wins, as
        // a receiver keeps it.
        take(&mut channel, &mut received, &mut owned);
        let mut file: Vec<u8> = received;
        for (page, frame) in channel.shared_frames() {
            if !owned.contains(&page) {
                file.extend_from_slice(frame);
            }
        }
        let path = arguments.out.join(format!("{}.t42", spec.folder));
        match std::fs::write(&path, &file) {
            Ok(()) => println!("{}: {} pages, {} bytes", path.display(), channel.entries().count(), file.len()),
            Err(error) => eprintln!("teletext-pages: {}: {error}", path.display()),
        }
    }
    ExitCode::SUCCESS
}
