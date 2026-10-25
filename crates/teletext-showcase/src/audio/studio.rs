//! Pages 502 to 504, the instruments: the piano, the sound board and the
//! drum machine. They are one application so a viewer is sent the effects
//! library once for all three, and so the drum machines share one wake.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use neoteletext_input::event::Action;
use neoteletext_input::modes::{ALL_KEYS, BASE_KEY, KEY_RELEASE, MOUSE_BUTTONS};
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Input, Named, Owner, PageEntry, SessionId};

use super::piano::Held;
use super::{BOARD, DRUMS, MENU, PIANO, RADIO, board, drums, piano};
use crate::common::page;
use crate::sound::{Effect, Sounds};

/// A viewer's instruments.
#[derive(Default)]
struct Viewer {
    piano: Held,
    board: Option<usize>,
    machine: drums::Machine,
}

/// The instruments.
#[derive(Default)]
pub(crate) struct Studio {
    sounds: Sounds,
    viewers: BTreeMap<SessionId, Viewer>,
    /// Whether a wake is asked for, for the drum machines' next step.
    ticking: bool,
}

/// The page for `number`, with what `viewer` has.
fn screen(number: u16, viewer: &Viewer, playhead: Option<usize>) -> Screen {
    match number {
        PIANO => {
            let mut screen = page("Piano", Colour::Green, [("Sound", Some(MENU)), ("Radio", Some(RADIO)), ("Board", Some(BOARD)), ("Drums", Some(DRUMS))]);
            piano::draw(&mut screen, viewer.piano);
            screen.modes(ALL_KEYS | KEY_RELEASE | BASE_KEY | MOUSE_BUTTONS);
            screen
        }
        BOARD => {
            let mut screen =
                page("Sound board", Colour::Magenta, [("Sound", Some(MENU)), ("Radio", Some(RADIO)), ("Piano", Some(PIANO)), ("Drums", Some(DRUMS))]);
            board::draw(&mut screen, viewer.board);
            screen.modes(MOUSE_BUTTONS);
            screen
        }
        _ => {
            let mut screen =
                page("Drum machine", Colour::Blue, [("Sound", Some(MENU)), ("Radio", Some(RADIO)), ("Piano", Some(PIANO)), ("Board", Some(BOARD))]);
            drums::draw(&mut screen, &viewer.machine, playhead);
            screen.modes(MOUSE_BUTTONS);
            screen
        }
    }
}

/// The key's character by position: `base` when the receiver gives it, the
/// key's own character otherwise.
fn key_character(input: &Input<'_>) -> Option<char> {
    match *input {
        Input::Key { base: Some(base), .. } => Some(base),
        Input::Key { key: Named::Character(character), .. } => Some(character),
        _ => None,
    }
}

/// A cell of a mouse press, as columns and rows.
fn cell(column: u32, row: u32) -> Option<(usize, usize)> {
    Some((usize::try_from(column).ok()?, usize::try_from(row).ok()?))
}

impl Studio {
    /// The instruments, with the effects library made once.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn draw(&self, session: SessionId, number: u16, context: &mut Context<'_>) {
        let Some(viewer) = self.viewers.get(&session) else { return };
        let playhead = viewer.machine.playing.then(|| drums::step_at(context.now()));
        context.publish(Owner::Session(session), number, &screen(number, viewer, playhead));
    }

    fn piano(&mut self, session: SessionId, input: &Input<'_>, context: &mut Context<'_>) -> bool {
        let Some(viewer) = self.viewers.get_mut(&session) else { return false };
        let held = &mut viewer.piano;
        let (note, pressed) = match *input {
            Input::Key { action: Action::Repeat, .. } => return false,
            Input::Key { action, .. } => {
                let Some(note) = key_character(input).and_then(piano::note_of) else { return false };
                (note, action == Action::Press)
            }
            // Text comes only from a receiver that did not take ALL_KEYS.
            Input::Text(_) => {
                let Some(note) = input.character().and_then(piano::note_of) else { return false };
                held.last = Some(note);
                self.sounds.play(session, Effect::Note(note), context);
                return true;
            }
            Input::Mouse { pressed: true, button: 0, column, row } => {
                let Some(note) = cell(column, row).and_then(|(column, row)| piano::note_at(column, row)) else { return false };
                held.mouse = Some(note);
                held.last = Some(note);
                self.sounds.play(session, Effect::Note(note), context);
                return true;
            }
            Input::Mouse { pressed: false, .. } => return held.mouse.take().is_some(),
            Input::FocusOut => {
                *held = Held { last: held.last, ..Held::default() };
                return true;
            }
            _ => return false,
        };
        if pressed {
            held.keys |= 1 << note;
            held.last = Some(note);
            self.sounds.play(session, Effect::Note(note), context);
        } else {
            held.keys &= !(1 << note);
        }
        true
    }

    fn board(&mut self, session: SessionId, input: &Input<'_>, context: &mut Context<'_>) -> bool {
        let button = match *input {
            Input::Mouse { pressed: true, button: 0, column, row } => cell(column, row).and_then(|(column, row)| board::button_at(column, row)),
            _ => input.character().and_then(board::button_of),
        };
        let (Some(index), Some(viewer)) = (button, self.viewers.get_mut(&session)) else { return false };
        viewer.board = Some(index);
        if let Some(&(effect, _)) = board::BUTTONS.get(index) {
            self.sounds.play(session, effect, context);
        }
        true
    }

    fn drums(&mut self, session: SessionId, input: &Input<'_>, context: &mut Context<'_>) -> bool {
        let Some(viewer) = self.viewers.get_mut(&session) else { return false };
        let machine = &mut viewer.machine;
        let moves = [(Named::Up, -1, 0), (Named::Down, 1, 0), (Named::Left, 0, -1), (Named::Right, 0, 1)];
        if let Some(&(_, down, across)) = moves.iter().find(|(key, _, _)| input.pressed(*key)) {
            machine.go(down, across);
        } else if input.pressed(Named::Enter) {
            machine.playing = !machine.playing;
        } else if let Input::Mouse { pressed: true, button: 0, column, row } = *input {
            let Some(place) = cell(column, row).and_then(|(column, row)| drums::step_at_cell(column, row)) else { return false };
            machine.cursor = place;
            machine.toggle();
        } else {
            match input.character().map(|character| character.to_ascii_lowercase()) {
                Some(' ') => machine.toggle(),
                Some('c') => machine.clear(),
                _ => return false,
            }
        }
        if machine.playing {
            self.tick(context);
        }
        true
    }

    /// Asks for the next step's wake if a machine plays and none is asked.
    fn tick(&mut self, context: &mut Context<'_>) {
        if !self.ticking && self.viewers.values().any(|viewer| viewer.machine.playing) {
            let now = context.now();
            context.wake_at(now.saturating_add(drums::STEP_MS).saturating_sub(now % drums::STEP_MS));
            self.ticking = true;
        }
    }
}

impl Application for Studio {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(PIANO, "Piano"), PageEntry::new(BOARD, "Sound board"), PageEntry::new(DRUMS, "Drum machine")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        let viewer = Viewer::default();
        for number in [PIANO, BOARD, DRUMS] {
            context.publish(Owner::Channel, number, &screen(number, &viewer, None));
        }
    }

    fn opened(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        self.viewers.entry(session).or_default();
        // The library before the first press, so the first note is heard
        // at once.
        self.sounds.load(session, context);
        self.draw(session, page, context);
    }

    fn left_page(&mut self, session: SessionId, page: u16, _context: &mut Context<'_>) {
        if let Some(viewer) = self.viewers.get_mut(&session) {
            match page {
                PIANO => viewer.piano = Held { last: viewer.piano.last, ..Held::default() },
                DRUMS => viewer.machine.playing = false,
                _ => {}
            }
        }
    }

    fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let changed = match page {
            PIANO => self.piano(session, input, context),
            BOARD => self.board(session, input, context),
            DRUMS => self.drums(session, input, context),
            _ => false,
        };
        if changed {
            self.draw(session, page, context);
        }
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        self.ticking = false;
        let step = drums::step_at(context.now());
        let playing: Vec<SessionId> = self.viewers.iter().filter(|(_, viewer)| viewer.machine.playing).map(|(&session, _)| session).collect();
        for session in playing {
            let Some(machine) = self.viewers.get(&session).map(|viewer| viewer.machine) else { continue };
            for (track, &(_, effect, _)) in drums::TRACKS.iter().enumerate() {
                if machine.on(track, step) {
                    self.sounds.play(session, effect, context);
                }
            }
            self.draw(session, DRUMS, context);
        }
        self.tick(context);
    }

    fn ended(&mut self, session: SessionId, _context: &mut Context<'_>) {
        self.viewers.remove(&session);
        self.sounds.forget(session);
    }
}
