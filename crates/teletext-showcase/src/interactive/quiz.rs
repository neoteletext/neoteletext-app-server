//! Page 204: a quiz of teletext and television history, each viewer's
//! own: ten questions answered with 1 to 4, a score, and a result.

use alloc::collections::BTreeMap;
use alloc::format;

use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Context, Input, Named, Owner, SessionId};

use super::field::choice;
use super::{CALCULATOR, MENU, POLL, QUIZ};
use crate::common::page;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// One question: its text on up to two rows, four answers, and the
/// right one's place, 0 to 3.
pub(super) struct Question {
    text: [&'static str; 2],
    answers: [&'static str; ANSWERS],
    /// The right answer, 0 to 3.
    pub(super) right: usize,
}

/// The questions: facts of the record, well known.
pub(super) const QUESTIONS: [Question; 10] = [
    Question { text: ["In which year did the BBC's Ceefax", "service begin?"], answers: ["1966", "1974", "1982", "1990"], right: 1 },
    Question { text: ["Before 1993, ITV's teletext service", "was called what?"], answers: ["Prestel", "Antiope", "Oracle", "Viewdata"], right: 2 },
    Question { text: ["Which page carried subtitles on UK", "teletext?"], answers: ["100", "302", "500", "888"], right: 3 },
    Question { text: ["How many characters fit across one", "teletext row?"], answers: ["40", "32", "64", "80"], right: 0 },
    Question { text: ["How many colours has Level 1", "teletext, black included?"], answers: ["4", "16", "8", "256"], right: 2 },
    Question { text: ["A mosaic graphics character divides", "its cell into how many blocks?"], answers: ["4", "6", "8", "9"], right: 1 },
    Question {
        text: ["Teletext travels in which part of", "the television signal?"],
        answers: ["The sound carrier", "The colour burst", "The vertical blanking interval", "The line sync pulses"],
        right: 2,
    },
    Question {
        text: ["Who first showed television to the", "public, in London in 1926?"],
        answers: ["John Logie Baird", "Guglielmo Marconi", "Thomas Edison", "Alexander Graham Bell"],
        right: 0,
    },
    Question { text: ["Antiope was the teletext system of", "which country?"], answers: ["Germany", "Italy", "Sweden", "France"], right: 3 },
    Question { text: ["In which year did Ceefax close?", ""], answers: ["2006", "2009", "2012", "2015"], right: 2 },
];
/// The answers to each question.
const ANSWERS: usize = 4;
/// The score that wins.
const PASS: usize = 7;
/// The answers' first row, and rows between them.
const ANSWER_ROW: usize = 9;
/// Rows between answers.
const ANSWER_STEP: usize = 2;

/// What the last answer was.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Last {
    None,
    Right,
    /// Wrong: the question it was for.
    Wrong(usize),
}

/// One viewer's game.
#[derive(Clone, Copy)]
struct Game {
    /// The questions answered so far.
    asked: usize,
    score: usize,
    last: Last,
}

impl Game {
    const fn new() -> Self {
        Self { asked: 0, score: 0, last: Last::None }
    }

    fn finished(&self) -> bool {
        self.asked >= QUESTIONS.len()
    }

    /// Answers the question asked with `answer`, 0 to 3: the sound.
    fn answer(&mut self, answer: usize) -> Effect {
        let Some(question) = QUESTIONS.get(self.asked) else { return Effect::Hit };
        let right = answer == question.right;
        if right {
            self.score = self.score.saturating_add(1);
            self.last = Last::Right;
        } else {
            self.last = Last::Wrong(self.asked);
        }
        self.asked = self.asked.saturating_add(1);
        match (self.finished(), right) {
            (true, _) if self.score >= PASS => Effect::Win,
            (true, _) => Effect::Lose,
            (false, true) => Effect::Coin,
            (false, false) => Effect::Hit,
        }
    }

    fn screen(&self) -> Screen {
        let mut screen = page("Teletext quiz", Colour::Cyan, [("Input", Some(MENU)), ("Poll", Some(POLL)), ("Sums", Some(CALCULATOR)), ("Index", Some(INDEX))]);
        let total = QUESTIONS.len();
        match self.last {
            Last::None => {}
            Last::Right => screen.coloured(4, 1, Colour::Green, "Right!"),
            Last::Wrong(asked) => {
                if let Some(question) = QUESTIONS.get(asked) {
                    let answer = question.answers.get(question.right).copied().unwrap_or_default();
                    screen.coloured(4, 1, Colour::Red, &format!("No: it was {}, {answer}", question.right.saturating_add(1)));
                }
            }
        }
        if let Some(question) = QUESTIONS.get(self.asked) {
            screen.coloured(3, 1, Colour::Cyan, &format!("Question {} of {total}", self.asked.saturating_add(1)));
            screen.coloured(3, 27, Colour::Cyan, &format!("Score {}", self.score));
            for (row, line) in (6_usize..).zip(question.text) {
                screen.coloured(row, 1, Colour::White, line);
            }
            let mut row = ANSWER_ROW;
            for (number, answer) in (1_u8..).zip(question.answers) {
                screen.coloured(row, 1, Colour::Yellow, &format!("{number}"));
                screen.coloured(row, 3, Colour::White, answer);
                row = row.saturating_add(ANSWER_STEP);
            }
            screen.coloured(21, 1, Colour::Magenta, "Press 1, 2, 3 or 4 to answer");
        } else {
            screen.double_height(8, 1, Colour::Yellow, &format!("You scored {} out of {total}", self.score));
            let verdict = match self.score {
                score if score == total => "Perfect: a teletext legend!",
                score if score >= PASS => "Well done: a true viewer.",
                score if score >= total / 2 => "Not bad: keep reading.",
                _ => "Time to read more teletext!",
            };
            screen.coloured(11, 1, Colour::White, verdict);
            screen.coloured(21, 1, Colour::Magenta, "Press Enter to play again");
        }
        screen
    }
}

/// Every viewer's game.
#[derive(Default)]
pub(super) struct Quiz {
    games: BTreeMap<SessionId, Game>,
}

impl Quiz {
    /// The page as a viewer first sees it, which is also the channel's
    /// copy, for a receiver that shows the page before saying it opened it.
    pub(super) fn first_screen() -> Screen {
        Game::new().screen()
    }

    fn publish(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(game) = self.games.get(&session) {
            context.publish(Owner::Session(session), QUIZ, &game.screen());
        }
    }

    pub(super) fn opened(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.games.entry(session).or_insert_with(Game::new);
        self.publish(session, context);
    }

    pub(super) fn input(&mut self, session: SessionId, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let game = self.games.entry(session).or_insert_with(Game::new);
        let effect = if game.finished() {
            if !input.pressed(Named::Enter) {
                return;
            }
            *game = Game::new();
            Effect::Click
        } else {
            let Some(answer) = choice(input, ANSWERS) else { return };
            game.answer(answer)
        };
        self.publish(session, context);
        sounds.play(session, effect, context);
    }

    pub(super) fn ended(&mut self, session: SessionId) {
        self.games.remove(&session);
    }
}
