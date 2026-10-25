//! Page 203: a live poll. Viewers vote on the results page itself with
//! the keys 1 to 4, and the results, the channel's one page, change for
//! every viewer at once (decision 7), coalesced as the guestbook is.
//!
//! The poll counts the sessions connected now: a session has one vote,
//! which it may change at most once each [`CHANGE_INTERVAL_MS`], and its
//! vote goes when it ends. So no viewer counts twice by reconnecting, and
//! the state is bounded by the sessions.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;

use teletext_runtime::screen::{Colour, Screen, Sixels};
use teletext_runtime::{Context, Input, Owner, SessionId};

use super::field::choice;
use super::{GUESTBOOK, MENU, POLL, QUIZ, Republish};
use crate::common::page;
use crate::sound::{Effect, Sounds};

/// The question, on two rows.
const QUESTION: [&str; 2] = ["Which teletext page do you", "turn to first?"];
/// The answers, with each one's colour.
pub(super) const ANSWERS: [(&str, Colour); 4] = [("News", Colour::Red), ("Weather", Colour::Green), ("Sport", Colour::Yellow), ("TV listings", Colour::Cyan)];
/// The shortest time between two votes of one session.
pub(super) const CHANGE_INTERVAL_MS: u64 = 1_000;
/// The first answer's row; each answer takes a label row, a bar row and a
/// gap.
const FIRST_ROW: usize = 6;
/// Rows each answer takes.
const ROWS_EACH: usize = 3;
/// The widest bar, in cells: from column 2 to 37.
pub(super) const BAR_CELLS: usize = 36;
/// Where a bar starts, after its colour attribute.
const BAR_COLUMN: usize = 2;
/// The sixel rows a bar fills of the cell's three: two, so bars stand
/// apart from the label below.
const BAR_SIXEL_ROWS: usize = 2;
/// Where the count and percentage stand, so they end inside the bar's
/// width.
const PERCENT_COLUMN: usize = 28;
/// A whole, in percent.
const WHOLE: u64 = 100;

/// One session's vote.
struct Vote {
    answer: usize,
    at: u64,
}

/// The poll.
#[derive(Default)]
pub(super) struct Poll {
    votes: BTreeMap<SessionId, Vote>,
    counts: [u64; 4],
    republish: Republish,
}

/// `part` of `whole` as `scale` would show it, rounded down; 0 of nothing.
fn share(part: u64, whole: u64, scale: u64) -> u64 {
    part.saturating_mul(scale).checked_div(whole).unwrap_or(0)
}

impl Poll {
    pub(super) fn start(&self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, POLL, &self.screen());
    }

    fn screen(&self) -> Screen {
        let mut screen = page(
            "Live poll",
            Colour::Magenta,
            [("Input", Some(MENU)), ("Guestbook", Some(GUESTBOOK)), ("Quiz", Some(QUIZ)), ("Index", Some(crate::index::INDEX))],
        );
        for (row, line) in (3_usize..).zip(QUESTION) {
            screen.coloured(row, 1, Colour::White, line);
        }
        let total: u64 = self.counts.iter().sum();
        let mut row = FIRST_ROW;
        for (number, ((answer, colour), &count)) in (1_u8..).zip(ANSWERS.iter().zip(self.counts.iter())) {
            screen.coloured(row, 0, *colour, &format!("{number} {answer}"));
            let percent = share(count, total, WHOLE);
            screen.coloured(row, PERCENT_COLUMN, Colour::White, &format!("{count:>3}  {percent:>3}%"));
            let bar_cells = usize::try_from(share(count, total, u64::try_from(BAR_CELLS).unwrap_or(0))).unwrap_or(0);
            let mut bar = Sixels::new(BAR_CELLS.saturating_mul(2), BAR_SIXEL_ROWS);
            for x in 0..bar_cells.saturating_mul(2) {
                for y in 0..BAR_SIXEL_ROWS {
                    bar.set(x, y, true);
                }
            }
            bar.draw(&mut screen, row.saturating_add(1), BAR_COLUMN, *colour);
            row = row.saturating_add(ROWS_EACH);
        }
        let votes = if total == 1 { String::from("1 vote") } else { format!("{total} votes") };
        screen.coloured(19, 1, Colour::Cyan, &format!("{votes} from viewers here now"));
        screen.coloured(21, 1, Colour::Yellow, "Press 1, 2, 3 or 4 to vote; press");
        screen.coloured(22, 1, Colour::Yellow, "another to change your mind.");
        screen
    }

    pub(super) fn input(&mut self, session: SessionId, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let Some(answer) = choice(input, ANSWERS.len()) else { return };
        let now = context.now();
        if let Some(vote) = self.votes.get(&session)
            && (vote.answer == answer || now.saturating_sub(vote.at) < CHANGE_INTERVAL_MS)
        {
            sounds.play(session, Effect::Hit, context);
            return;
        }
        let before = self.votes.insert(session, Vote { answer, at: now }).map(|vote| vote.answer);
        self.count(before, Some(answer));
        sounds.play(session, Effect::Click, context);
        self.changed(context);
    }

    /// Moves one vote from `from` to `to`.
    fn count(&mut self, from: Option<usize>, to: Option<usize>) {
        if let Some(count) = from.and_then(|answer| self.counts.get_mut(answer)) {
            *count = count.saturating_sub(1);
        }
        if let Some(count) = to.and_then(|answer| self.counts.get_mut(answer)) {
            *count = count.saturating_add(1);
        }
    }

    fn changed(&mut self, context: &mut Context<'_>) {
        if self.republish.changed(context) {
            context.publish(Owner::Channel, POLL, &self.screen());
        }
    }

    pub(super) fn wake(&mut self, context: &mut Context<'_>) {
        if self.republish.woken(context) {
            context.publish(Owner::Channel, POLL, &self.screen());
        }
    }

    pub(super) fn ended(&mut self, session: SessionId, context: &mut Context<'_>) {
        if let Some(vote) = self.votes.remove(&session) {
            self.count(Some(vote.answer), None);
            self.changed(context);
        }
    }
}
