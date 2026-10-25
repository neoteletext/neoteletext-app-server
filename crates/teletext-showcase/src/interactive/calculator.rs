//! Page 205: a calculator, each viewer's own. A sum of whole numbers with
//! `+`, `-`, `x` or `*`, `/` and parentheses is typed, and Enter works it
//! out in 64-bit integers, every step checked: overflow and division by
//! zero are answers, not failures.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_input::cursor::ECHO;
use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Context, Input, Owner, SessionId};

use super::field::{BAND_LEAD, Edit, Field, band};
use super::{CALCULATOR, MENU, PAINT, QUIZ};
use crate::common::page;
use crate::index::INDEX;
use crate::sound::{Effect, Sounds};

/// The longest sum.
const SUM_LIMIT: usize = 30;
/// The deepest nesting of parentheses and signs: more than a sum of
/// [`SUM_LIMIT`] characters needs, so the parser's recursion is bounded
/// whatever is typed.
const DEEPEST: usize = SUM_LIMIT;
/// Sums kept on the page.
const HISTORY: usize = 5;
/// The field's row.
const FIELD_ROW: usize = 5;
/// Where the field's band starts.
const BAND_COLUMN: usize = 1;
/// Divide in the English set (EN 300 706 Table 36: 0x7E).
const DIVIDE_CODE: u8 = 0x7E;
/// Base ten.
const RADIX: i64 = 10;

/// Why a sum has no answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Problem {
    /// A division by zero.
    DivisionByZero,
    /// A number or a step beyond 64 bits.
    Overflow,
    /// Not a sum.
    Unreadable,
}

impl Problem {
    const fn words(self) -> &'static str {
        match self {
            Self::DivisionByZero => "Cannot divide by zero",
            Self::Overflow => "Too big for 64 bits",
            Self::Unreadable => "Not a sum I can read",
        }
    }
}

/// A sum's symbol as the calculator keeps it, from what a viewer types:
/// `x`, `X` and `×` multiply, `÷` divides, `−` subtracts.
pub(super) fn symbol(character: char) -> Option<char> {
    match character {
        '0'..='9' | '+' | '-' | '*' | '/' | '(' | ')' | ' ' => Some(character),
        'x' | 'X' | '\u{d7}' => Some('*'),
        '\u{f7}' => Some('/'),
        '\u{2212}' => Some('-'),
        _ => None,
    }
}

/// A recursive-descent parser of `sum := term (('+' | '-') term)*`,
/// `term := factor (('*' | '/') factor)*`, `factor := '-' factor | '('
/// sum ')' | digits`.
struct Parser<'a> {
    symbols: &'a [u8],
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    /// The next symbol, past spaces.
    fn peek(&mut self) -> Option<u8> {
        while self.symbols.get(self.at) == Some(&b' ') {
            self.at = self.at.saturating_add(1);
        }
        self.symbols.get(self.at).copied()
    }

    fn advance(&mut self) {
        self.at = self.at.saturating_add(1);
    }

    fn sum(&mut self) -> Result<i64, Problem> {
        let mut value = self.term()?;
        while let Some(operator @ (b'+' | b'-')) = self.peek() {
            self.advance();
            let right = self.term()?;
            value = if operator == b'+' { value.checked_add(right) } else { value.checked_sub(right) }.ok_or(Problem::Overflow)?;
        }
        Ok(value)
    }

    fn term(&mut self) -> Result<i64, Problem> {
        let mut value = self.factor()?;
        while let Some(operator @ (b'*' | b'/')) = self.peek() {
            self.advance();
            let right = self.factor()?;
            value = if operator == b'*' {
                value.checked_mul(right).ok_or(Problem::Overflow)?
            } else if right == 0 {
                return Err(Problem::DivisionByZero);
            } else {
                // The one overflowing division is the least value by -1.
                value.checked_div(right).ok_or(Problem::Overflow)?
            };
        }
        Ok(value)
    }

    fn factor(&mut self) -> Result<i64, Problem> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > DEEPEST {
            return Err(Problem::Unreadable);
        }
        let value = match self.peek() {
            Some(b'-') => {
                self.advance();
                self.factor()?.checked_neg().ok_or(Problem::Overflow)?
            }
            Some(b'(') => {
                self.advance();
                let value = self.sum()?;
                if self.peek() != Some(b')') {
                    return Err(Problem::Unreadable);
                }
                self.advance();
                value
            }
            Some(b'0'..=b'9') => self.number()?,
            _ => return Err(Problem::Unreadable),
        };
        self.depth = self.depth.saturating_sub(1);
        Ok(value)
    }

    fn number(&mut self) -> Result<i64, Problem> {
        let mut value = 0_i64;
        while let Some(digit @ b'0'..=b'9') = self.symbols.get(self.at).copied() {
            self.advance();
            value = value.checked_mul(RADIX).and_then(|value| value.checked_add(i64::from(digit.saturating_sub(b'0')))).ok_or(Problem::Overflow)?;
        }
        Ok(value)
    }
}

/// Works out `sum`.
pub(super) fn evaluate(sum: &str) -> Result<i64, Problem> {
    let mut parser = Parser { symbols: sum.as_bytes(), at: 0, depth: 0 };
    let value = parser.sum()?;
    if parser.peek().is_some() {
        return Err(Problem::Unreadable);
    }
    Ok(value)
}

/// A sum as the page shows it: `x` for multiply and the English set's
/// divide sign.
fn codes_of(sum: &str) -> Vec<u8> {
    sum.bytes()
        .map(|byte| match byte {
            b'*' => b'x',
            b'/' => DIVIDE_CODE,
            other => other,
        })
        .collect()
}

/// One viewer's calculator.
struct State {
    field: Field,
    answer: Option<Result<i64, Problem>>,
    history: VecDeque<(String, Result<i64, Problem>)>,
}

impl State {
    const fn new() -> Self {
        Self { field: Field::new(SUM_LIMIT), answer: None, history: VecDeque::new() }
    }

    /// Acts on an edit: the sound, if it changed anything.
    fn edit(&mut self, edit: Edit<'_>) -> Option<Effect> {
        match edit {
            Edit::Text(text) => {
                self.field.type_text(text, symbol);
                Some(Effect::Click)
            }
            Edit::Character(character) => {
                let mut buffer = [0_u8; 4];
                self.field.type_text(character.encode_utf8(&mut buffer), symbol);
                Some(Effect::Click)
            }
            Edit::Backspace => self.field.backspace().then_some(Effect::Click),
            Edit::Escape => {
                self.field.clear();
                self.answer = None;
                Some(Effect::Click)
            }
            Edit::Enter if self.field.is_blank() => None,
            Edit::Enter => {
                let answer = evaluate(self.field.text());
                self.history.push_front((String::from(self.field.text().trim()), answer));
                self.history.truncate(HISTORY);
                self.answer = Some(answer);
                self.field.clear();
                Some(if answer.is_ok() { Effect::Bell } else { Effect::Hit })
            }
            Edit::Tab | Edit::Other => None,
        }
    }

    fn screen(&self) -> Screen {
        let mut screen = page("Calculator", Colour::Yellow, [("Input", Some(MENU)), ("Quiz", Some(QUIZ)), ("Paint", Some(PAINT)), ("Index", Some(INDEX))]);
        screen.coloured(3, 1, Colour::Cyan, "Type a sum, then press Enter:");
        band(&mut screen, FIELD_ROW, BAND_COLUMN, SUM_LIMIT, Colour::Blue, Colour::White, &codes_of(self.field.text()));
        self.field.cursor(&mut screen, FIELD_ROW, BAND_COLUMN.saturating_add(BAND_LEAD), ECHO);
        match self.answer {
            None => {}
            Some(Ok(value)) => screen.double_height(8, 1, Colour::Yellow, &format!("= {value}")),
            Some(Err(problem)) => screen.double_height(8, 1, Colour::Red, problem.words()),
        }
        if !self.history.is_empty() {
            screen.coloured(11, 1, Colour::Cyan, "Your last sums");
        }
        for (row, (sum, answer)) in (12_usize..).zip(self.history.iter()) {
            let mut codes = codes_of(sum);
            codes.extend_from_slice(b" = ");
            screen.code(row, 1, Colour::White.text());
            screen.codes(row, 2, &codes);
            let column = codes.len().saturating_add(2);
            match answer {
                Ok(value) => screen.coloured(row, column, Colour::Yellow, &format!("{value}")),
                Err(problem) => screen.coloured(row, column, Colour::Red, problem.words()),
            }
        }
        screen.coloured(19, 1, Colour::Green, "Whole numbers, 64 bits, each step");
        screen.coloured(20, 1, Colour::Green, "checked; division rounds to zero.");
        screen.code(22, 1, Colour::Magenta.text());
        screen.codes(22, 2, &codes_of("+ - * / ( )   Backspace   Esc clears"));
        screen
    }
}

/// Every viewer's calculator.
#[derive(Default)]
pub(super) struct Calculator {
    sessions: BTreeMap<SessionId, State>,
}

impl Calculator {
    /// The page as a viewer first sees it, which is also the channel's
    /// copy, for a receiver that shows the page before saying it opened it.
    pub(super) fn first_screen() -> Screen {
        State::new().screen()
    }

    fn publish(&self, session: SessionId, context: &mut Context<'_>) {
        if let Some(state) = self.sessions.get(&session) {
            context.publish(Owner::Session(session), CALCULATOR, &state.screen());
        }
    }

    pub(super) fn opened(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.sessions.entry(session).or_insert_with(State::new);
        self.publish(session, context);
    }

    pub(super) fn input(&mut self, session: SessionId, input: &Input<'_>, sounds: &mut Sounds, context: &mut Context<'_>) {
        let state = self.sessions.entry(session).or_insert_with(State::new);
        if let Some(effect) = state.edit(Edit::of(input)) {
            self.publish(session, context);
            sounds.play(session, effect, context);
        }
    }

    pub(super) fn ended(&mut self, session: SessionId) {
        self.sessions.remove(&session);
    }
}
