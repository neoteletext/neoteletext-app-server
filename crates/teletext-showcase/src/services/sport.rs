//! Page 106: sport, simulated. A made-up league of eight teams plays its
//! rounds live: four matches at once, scores changing as the simulated
//! minutes pass, a goal flashing until the next update, and the table as
//! it stands with the live scores in it. The page says it is
//! demonstration data.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen, attribute};
use teletext_runtime::{Application, Context, PageEntry};

use super::calendar::SECOND_MS;
use super::draw::{band, demonstration, publish, right};
use super::{SPORT, navigation};
use crate::common::page;

/// The league's teams.
const TEAMS: [&str; 8] = ["Sixel Rovers", "Mosaic United", "Fastext Town", "Parity City", "Header Albion", "Carousel FC", "Raster Rangers", "Pixel Athletic"];
/// The number of teams, which the schedule pairs off.
const TEAM_COUNT: usize = TEAMS.len();
/// The matches in a round: every team plays.
const MATCHES: usize = TEAM_COUNT / 2;
/// Rounds in which every team meets every other once (the circle method).
const ROUNDS_ONCE: usize = TEAM_COUNT - 1;
/// A season: each pair meets home and away.
const SEASON_ROUNDS: usize = ROUNDS_ONCE * 2;

/// The page changes every fifteen seconds.
const INTERVAL_MS: u64 = 15 * SECOND_MS;
/// Simulated minutes in each update: a match lasts 30 updates, seven and
/// a half minutes.
const MINUTES_PER_UPDATE: u8 = 3;
/// A match's length in minutes.
const FULL_TIME: u8 = 90;
/// Half time, where play pauses for one update.
const HALF_TIME: u8 = 45;
/// Updates at full time before the next round kicks off: one minute.
const FULL_TIME_UPDATES: u8 = 4;
/// The chance of a goal for the home team in an update, in thousandths:
/// about 1.5 goals a match.
const HOME_GOAL_CHANCE: u32 = 50;
/// For the away team: about 1.2.
const AWAY_GOAL_CHANCE: u32 = 40;
/// Chances are reckoned in thousandths.
const CHANCE_SCALE: u32 = 1_000;
/// Points for a win.
const WIN_POINTS: u32 = 3;

/// The row of the round and the time.
const STATUS_ROW: usize = 3;
/// The first match's row.
const MATCHES_ROW: usize = 5;
/// The width of a team's name.
const NAME_WIDTH: usize = 14;
/// The score's colour attribute's column, after the home team.
const SCORE_COLUMN: usize = 15;
/// The away team's colour attribute's column.
const AWAY_COLUMN: usize = 21;
/// The table's heading row.
const TABLE_ROW: usize = 10;
/// The note's row, under the table.
const NOTE_ROW: usize = 20;

/// Where the matches are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Playing, at this minute.
    Playing(u8),
    /// Half time, for one update.
    HalfTime,
    /// Full time, with updates left before the next round.
    FullTime(u8),
}

/// One match of the round.
#[derive(Clone, Copy, Debug, Default)]
pub struct Match {
    /// The home team, an index into the teams.
    pub home: usize,
    /// The away team.
    pub away: usize,
    /// Home goals.
    pub home_goals: u32,
    /// Away goals.
    pub away_goals: u32,
    /// Whether a goal came in the last update.
    pub scored: bool,
}

/// A team's line of the table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Standing {
    /// Played.
    pub played: u32,
    /// Won.
    pub won: u32,
    /// Drawn.
    pub drawn: u32,
    /// Lost.
    pub lost: u32,
    /// Goals for.
    pub scored: u32,
    /// Goals against.
    pub conceded: u32,
}

impl Standing {
    /// Points: three for a win, one for a draw.
    #[must_use]
    pub const fn points(&self) -> u32 {
        self.won.saturating_mul(WIN_POINTS).saturating_add(self.drawn)
    }

    /// Goal difference.
    #[must_use]
    pub fn difference(&self) -> i64 {
        i64::from(self.scored).saturating_sub(i64::from(self.conceded))
    }

    /// The standing with one more result.
    #[must_use]
    pub const fn with(self, scored: u32, conceded: u32) -> Self {
        let (won, drawn, lost) = if scored > conceded {
            (1, 0, 0)
        } else if scored == conceded {
            (0, 1, 0)
        } else {
            (0, 0, 1)
        };
        Self {
            played: self.played.saturating_add(1),
            won: self.won.saturating_add(won),
            drawn: self.drawn.saturating_add(drawn),
            lost: self.lost.saturating_add(lost),
            scored: self.scored.saturating_add(scored),
            conceded: self.conceded.saturating_add(conceded),
        }
    }
}

/// The league: the round being played and the table before it.
#[derive(Debug)]
pub struct Sport {
    /// The round in the season, from 0.
    pub round: usize,
    /// The season, from 1.
    pub season: u32,
    /// Where the round's matches are.
    pub phase: Phase,
    /// The round's matches.
    pub matches: [Match; MATCHES],
    /// The table before this round.
    pub table: [Standing; TEAM_COUNT],
}

impl Default for Sport {
    fn default() -> Self {
        Self { round: 0, season: 1, phase: Phase::Playing(0), matches: fixtures(0), table: [Standing::default(); TEAM_COUNT] }
    }
}

/// Round `round`'s matches by the circle method: team 0 stays, the others
/// turn one place each round, and the second half of the season swaps
/// home and away.
#[must_use]
pub fn fixtures(round: usize) -> [Match; MATCHES] {
    let turn = round % ROUNDS_ONCE;
    let second_half = round >= ROUNDS_ONCE;
    let others = ROUNDS_ONCE;
    // The circle's places: place 0 is team 0, place p > 0 holds the
    // team the turn has moved there.
    let team_at = |place: usize| -> usize { if place == 0 { 0 } else { (place.saturating_sub(1).saturating_add(turn) % others).saturating_add(1) } };
    let mut matches = [Match::default(); MATCHES];
    for (index, slot) in matches.iter_mut().enumerate() {
        let (first, second) = (team_at(index), team_at(TEAM_COUNT.saturating_sub(1).saturating_sub(index)));
        // Alternating who is at home spreads home matches across teams.
        let first_at_home = turn.saturating_add(index) % 2 == 0;
        let (home, away) = if first_at_home ^ second_half { (first, second) } else { (second, first) };
        *slot = Match { home, away, ..Match::default() };
    }
    matches
}

impl Sport {
    /// The table with the matches' scores as they stand counted in.
    #[must_use]
    pub fn live_table(&self) -> [Standing; TEAM_COUNT] {
        let mut table = self.table;
        let started = !matches!(self.phase, Phase::Playing(0));
        if started {
            for game in &self.matches {
                if let Some(home) = table.get_mut(game.home) {
                    *home = home.with(game.home_goals, game.away_goals);
                }
                if let Some(away) = table.get_mut(game.away) {
                    *away = away.with(game.away_goals, game.home_goals);
                }
            }
        }
        table
    }

    /// One update: the clock runs on, goals may come, and after full time
    /// the next round begins.
    pub fn update(&mut self, context: &mut Context<'_>) {
        for game in &mut self.matches {
            game.scored = false;
        }
        self.phase = match self.phase {
            Phase::Playing(minute) => {
                let minute = minute.saturating_add(MINUTES_PER_UPDATE).min(FULL_TIME);
                for game in &mut self.matches {
                    if context.below(CHANCE_SCALE) < HOME_GOAL_CHANCE {
                        game.home_goals = game.home_goals.saturating_add(1);
                        game.scored = true;
                    }
                    if context.below(CHANCE_SCALE) < AWAY_GOAL_CHANCE {
                        game.away_goals = game.away_goals.saturating_add(1);
                        game.scored = true;
                    }
                }
                match minute {
                    HALF_TIME => Phase::HalfTime,
                    FULL_TIME => Phase::FullTime(FULL_TIME_UPDATES),
                    _ => Phase::Playing(minute),
                }
            }
            Phase::HalfTime => Phase::Playing(HALF_TIME),
            Phase::FullTime(0) => {
                self.next_round();
                Phase::Playing(0)
            }
            Phase::FullTime(left) => Phase::FullTime(left.saturating_sub(1)),
        };
    }

    /// Counts the round into the table and draws up the next; a new
    /// season starts with an empty table.
    fn next_round(&mut self) {
        self.table = self.live_table();
        self.round = self.round.saturating_add(1);
        if self.round >= SEASON_ROUNDS {
            self.round = 0;
            self.season = self.season.saturating_add(1);
            self.table = [Standing::default(); TEAM_COUNT];
        }
        self.matches = fixtures(self.round);
    }

    /// The page.
    #[must_use]
    pub fn screen(&self) -> Screen {
        let mut screen = page("Sport", Colour::Red, navigation(SPORT));
        screen.coloured(STATUS_ROW, 0, Colour::Cyan, &format!("Mosaic League, round {}", self.round.saturating_add(1)));
        let time = match self.phase {
            Phase::Playing(0) => String::from("Kick-off"),
            Phase::Playing(minute) => format!("{minute} min"),
            Phase::HalfTime => String::from("Half time"),
            Phase::FullTime(_) => String::from("Full time"),
        };
        right(&mut screen, STATUS_ROW, Colour::Yellow, &time);
        for (game, row) in self.matches.iter().zip(MATCHES_ROW..) {
            draw_match(&mut screen, row, game);
        }
        // The heading's text starts two cells right of the rows', where
        // their positions are, so it leaves that field out.
        let heading = format!(" {:<14} {:>2} {:>2} {:>2} {:>2} {:>3} {:>3}", "TEAM", "P", "W", "D", "L", "GD", "PTS");
        band(&mut screen, TABLE_ROW, Colour::Blue, Colour::White, &heading);
        let table = self.live_table();
        let mut order: Vec<usize> = (0..TEAM_COUNT).collect();
        order.sort_by(|&a, &b| {
            let (first, second) = (table.get(a).copied().unwrap_or_default(), table.get(b).copied().unwrap_or_default());
            second.points().cmp(&first.points()).then(second.difference().cmp(&first.difference())).then(second.scored.cmp(&first.scored)).then(a.cmp(&b))
        });
        for ((position, team), row) in order.iter().enumerate().zip(TABLE_ROW.saturating_add(1)..) {
            let standing = table.get(*team).copied().unwrap_or_default();
            let name = TEAMS.get(*team).copied().unwrap_or("");
            let colour = if position == 0 { Colour::Yellow } else { Colour::White };
            let text = format!(
                "{:>2} {name:<14} {:>2} {:>2} {:>2} {:>2} {:>3} {:>3}",
                position.saturating_add(1),
                standing.played,
                standing.won,
                standing.drawn,
                standing.lost,
                standing.difference(),
                standing.points()
            );
            screen.coloured(row, 0, colour, &text);
        }
        let note = match self.phase {
            Phase::FullTime(_) => "next round kicks off in a minute",
            _ => "scores update every 15 s",
        };
        screen.coloured(NOTE_ROW, 0, Colour::Green, &format!("Season {}, {note}", self.season));
        demonstration(&mut screen);
        screen
    }
}

/// A match's line: the home team right-aligned against the score, a new
/// goal's score flashing, the away team after it.
fn draw_match(screen: &mut Screen, row: usize, game: &Match) {
    let home = TEAMS.get(game.home).copied().unwrap_or("");
    let away = TEAMS.get(game.away).copied().unwrap_or("");
    screen.coloured(row, 0, Colour::White, &format!("{home:>NAME_WIDTH$}"));
    screen.code(row, SCORE_COLUMN, Colour::Yellow.text());
    if game.scored {
        screen.code(row, SCORE_COLUMN.saturating_add(1), attribute::FLASH);
    }
    screen.text(row, SCORE_COLUMN.saturating_add(2), &format!("{}-{}", game.home_goals, game.away_goals));
    // Steady is set-at, so the cell after the score is already steady.
    screen.code(row, AWAY_COLUMN.saturating_sub(1), attribute::STEADY);
    screen.coloured(row, AWAY_COLUMN, Colour::White, away);
}

impl Application for Sport {
    fn pages(&self) -> Vec<PageEntry> {
        alloc::vec![PageEntry::new(SPORT, "Sport")]
    }

    fn start(&mut self, context: &mut Context<'_>) {
        publish(context, SPORT, &self.screen());
        context.wake_in(INTERVAL_MS);
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        self.update(context);
        publish(context, SPORT, &self.screen());
        context.wake_in(INTERVAL_MS);
    }
}
