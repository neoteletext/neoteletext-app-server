//! The hall of fame: the best scores of every game and every player, one
//! shared page (`SERVER.md` section 2). It changes when a game ends with a
//! score good enough, and is published once for every viewer.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::SessionId;
use teletext_runtime::screen::{Colour, Screen};

use super::look::digits;
use super::{Game, MENU};
use crate::common::page;
use crate::index::INDEX;

/// The scores kept: as many as the page has rows for.
const PLACES: usize = 12;
/// The table: its heading under the title band, the scores from two rows
/// below it, each after its colour in column 1.
const HEADING_ROW: usize = 3;
const FIRST_ROW: usize = 5;
const TABLE_COLUMN: usize = 1;
/// Where an empty table says so.
const EMPTY_ROW: usize = 9;
/// The note at the foot, under the twelve places.
const FOOT_ROW: usize = 19;
/// Players are named by their session's number, cut to four digits: no
/// name is asked for, and the number is enough to tell players apart on
/// one page.
const PLAYER_NUMBERS: u64 = 10_000;

/// One score.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entry {
    /// The score.
    pub score: u32,
    /// The game.
    pub game: Game,
    /// The player's session.
    pub player: SessionId,
}

/// The best scores, best first.
#[derive(Default)]
pub struct Hall {
    entries: Vec<Entry>,
}

/// A player's name on the page.
#[must_use]
pub fn player_name(session: SessionId) -> String {
    format!("Player {:04}", session.checked_rem(PLAYER_NUMBERS).unwrap_or(0))
}

impl Hall {
    /// Records `score`; whether it made the table, so the page changed. A
    /// score that ties one already there goes under it: the first to reach
    /// it keeps the place.
    pub fn record(&mut self, game: Game, player: SessionId, score: u32) -> bool {
        if score == 0 {
            return false;
        }
        let place = self.entries.iter().position(|entry| entry.score < score).unwrap_or(self.entries.len());
        if place >= PLACES {
            return false;
        }
        self.entries.insert(place, Entry { score, game, player });
        self.entries.truncate(PLACES);
        true
    }

    /// The best score of `game` and who made it.
    #[must_use]
    pub fn best(&self, game: Game) -> Option<(u32, String)> {
        self.entries.iter().find(|entry| entry.game == game).map(|entry| (entry.score, player_name(entry.player)))
    }

    /// The scores, best first.
    #[cfg(test)]
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The page.
    #[must_use]
    pub fn screen(&self) -> Screen {
        let mut screen = page(
            "Hall of fame",
            Colour::Blue,
            [("Menu", Some(MENU)), ("Invaders", Some(super::INVADERS)), ("Snake", Some(super::SNAKE)), ("Index", Some(INDEX))],
        );
        screen.coloured(HEADING_ROW, TABLE_COLUMN, Colour::Cyan, "No  Player       Game          Score");
        if self.entries.is_empty() {
            screen.centred(EMPTY_ROW, Colour::White, "No scores yet.");
            screen.centred(EMPTY_ROW.saturating_add(2), Colour::Green, "Play a game: yours could be first.");
        }
        let mut row = FIRST_ROW;
        for (place, entry) in self.entries.iter().enumerate() {
            // Gold, then the others in turn, so the rows read across.
            let colour = match place {
                0 => Colour::Yellow,
                _ if place.checked_rem(2) == Some(1) => Colour::White,
                _ => Colour::Cyan,
            };
            let line = format!("{:>2}  {:<12} {:<12}  {}", place.saturating_add(1), player_name(entry.player), entry.game.name(), digits(entry.score));
            screen.coloured(row, TABLE_COLUMN, colour, &line);
            row = row.saturating_add(1);
        }
        screen.centred(FOOT_ROW, Colour::Green, "One shared page: every viewer sees");
        screen.centred(FOOT_ROW.saturating_add(1), Colour::Green, "each new best the moment a game ends");
        screen
    }
}
