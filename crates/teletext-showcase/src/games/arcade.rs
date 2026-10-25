//! The application behind pages 300 to 305: the menu, the four games and
//! the hall of fame.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use teletext_runtime::screen::{Colour, Screen};
use teletext_runtime::{Application, Context, Input, Named, Owner, PageEntry, SessionId};

use super::hall::Hall;
use super::invaders::{self, POINTS};
use super::look;
use super::sprites::{self, Sprite};
use super::{Game, HALL, Keys, MENU, Running, breakout, cave, control_of, snake};
use crate::channels::Picture;
use crate::sound::{Effect, Sounds};

/// The games' tick, in milliseconds: ten frames a second. Each frame is
/// the viewer's whole page, about a kilobyte, so a game costs its viewer
/// about ten kilobytes a second, and the server one page for each player
/// a tick (decision 7).
pub const TICK_MS: u64 = 100;
/// After a game ends, keys are ignored this long, so the fire key still
/// held from the game does not start the next at once.
const RESTART_PAUSE_MS: u64 = 1_000;
/// The enemies' portraits on the invaders' title page, in sixels.
const PORTRAIT_WIDTH: usize = 12;
const PORTRAIT_HEIGHT: usize = 9;
/// The enemies' size in a wave, in sixels.
const ENEMY_WIDTH: usize = invaders::ENEMY_WIDTH;
const ENEMY_HEIGHT: usize = invaders::ENEMY_HEIGHT;

/// A viewer's game.
struct Player {
    running: Running,
    keys: Keys,
    /// When the game ended; `None` while it is in play.
    ended_at: Option<u64>,
}

/// The menu, the games and the hall of fame.
pub struct Arcade {
    /// The enemies at wave size and at portrait size, one each per
    /// sci-fi sprite.
    enemies: Vec<Sprite>,
    portraits: Vec<Sprite>,
    /// Each viewer's game, while they are on its page.
    players: BTreeMap<SessionId, Player>,
    /// The game pages each session has had a page of its own on: going
    /// back to one, it is given the title page again, since its own page
    /// still overrides the shared one.
    owned: BTreeMap<SessionId, u8>,
    hall: Hall,
    sounds: Sounds,
    /// Whether a wake is asked for.
    ticking: bool,
}

impl Arcade {
    /// The arcade, its enemies taken from the sci-fi sprites among
    /// `pictures`.
    #[must_use]
    pub fn new(pictures: &[Picture]) -> Self {
        Self {
            enemies: sprites::enemies(pictures, ENEMY_WIDTH, ENEMY_HEIGHT),
            portraits: sprites::enemies(pictures, PORTRAIT_WIDTH, PORTRAIT_HEIGHT),
            players: BTreeMap::new(),
            owned: BTreeMap::new(),
            hall: Hall::default(),
            sounds: Sounds::new(),
            ticking: false,
        }
    }

    /// The title page of `game`.
    fn title(&self, game: Game) -> Screen {
        let best = self.hall.best(game);
        match game {
            Game::Invaders => {
                let first_wave: Vec<Sprite> = (0..invaders::ENEMY_ROWS).filter_map(|row| invaders::enemy(&self.portraits, 0, row).cloned()).collect();
                look::invaders_title(&first_wave, &POINTS, best)
            }
            Game::Snake => look::title(
                game,
                &snake::art(),
                &[
                    (Colour::White, "Steer with the arrows. Eat the red"),
                    (Colour::White, "food to grow; the snake speeds up."),
                    (Colour::Cyan, "Walls and your own tail are fatal."),
                ],
                best,
            ),
            Game::Breakout => look::title(
                game,
                &breakout::art(),
                &[
                    (Colour::White, "Hold \u{2190} \u{2192} to move the paddle."),
                    (Colour::White, "Space launches the ball. Clear the"),
                    (Colour::Cyan, "wall; each wall comes back faster."),
                ],
                best,
            ),
            Game::Cave => look::title(
                game,
                &cave::art(),
                &[
                    (Colour::White, "Hold \u{2191} and down to climb and dive."),
                    (Colour::White, "The cave narrows as you go deeper."),
                    (Colour::Cyan, "Gold diamonds are worth 25 points."),
                ],
                best,
            ),
        }
    }

    /// Publishes every shared page: the menu, the titles and the hall.
    fn publish_shared(&self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, MENU, &look::menu(&self.enemies));
        for game in Game::ALL {
            context.publish(Owner::Channel, game.page(), &self.title(game));
        }
        context.publish(Owner::Channel, HALL, &self.hall.screen());
    }

    /// Publishes `session`'s game.
    fn publish_game(&self, session: SessionId, player: &Player, context: &mut Context<'_>) {
        context.publish(Owner::Session(session), player.running.game().page(), &self.screen_of(player));
    }

    fn screen_of(&self, player: &Player) -> Screen {
        let game = player.running.game();
        let mut screen = look::play_page(game);
        player.running.draw(&mut screen, &self.enemies);
        if player.ended_at.is_some() {
            let best = self.hall.best(game).is_some_and(|(score, _)| score == player.running.score());
            let line = if best { "A new best! Space plays again" } else { "Space plays again, Esc for menu" };
            look::banner(&mut screen, player.running.banner_row(), Colour::Red, "GAME OVER", line);
        }
        screen
    }

    /// Starts `game` for `session`.
    fn start_game(&mut self, session: SessionId, game: Game, context: &mut Context<'_>) {
        let player = Player { running: Running::new(game, context), keys: Keys::default(), ended_at: None };
        self.publish_game(session, &player, context);
        self.players.insert(session, player);
        *self.owned.entry(session).or_insert(0) |= game.bit();
        self.sounds.play(session, Effect::Coin, context);
        if !self.ticking {
            self.ticking = true;
            context.wake_in(TICK_MS);
        }
    }

    /// Ends `session`'s game, if it has one: a game left in play still
    /// counts for the hall of fame.
    fn finish(&mut self, session: SessionId, context: &mut Context<'_>) {
        if let Some(player) = self.players.remove(&session)
            && player.ended_at.is_none()
            && self.hall.record(player.running.game(), session, player.running.score())
        {
            self.publish_shared_scores(context);
        }
    }

    /// The hall changed: its page, and the titles' best lines.
    fn publish_shared_scores(&self, context: &mut Context<'_>) {
        context.publish(Owner::Channel, HALL, &self.hall.screen());
        for game in Game::ALL {
            context.publish(Owner::Channel, game.page(), &self.title(game));
        }
    }
}

impl Application for Arcade {
    fn pages(&self) -> Vec<PageEntry> {
        let mut pages = alloc::vec![PageEntry::new(MENU, "Action games")];
        pages.extend(Game::ALL.iter().map(|game| PageEntry::new(game.page(), game.name())));
        pages.push(PageEntry::new(HALL, "Hall of fame"));
        pages
    }

    fn start(&mut self, context: &mut Context<'_>) {
        self.publish_shared(context);
    }

    fn opened(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        let Some(game) = Game::of_page(page) else { return };
        if self.owned.get(&session).is_some_and(|owned| owned & game.bit() != 0) {
            context.publish(Owner::Session(session), page, &self.title(game));
        }
    }

    fn left_page(&mut self, session: SessionId, page: u16, context: &mut Context<'_>) {
        if self.players.get(&session).is_some_and(|player| player.running.game().page() == page) {
            self.finish(session, context);
        }
    }

    fn input(&mut self, session: SessionId, page: u16, input: &Input<'_>, context: &mut Context<'_>) {
        let Some(game) = Game::of_page(page) else { return };
        if input.pressed(Named::Escape) {
            self.finish(session, context);
            context.goto(session, MENU);
            return;
        }
        if *input == Input::FocusOut {
            if let Some(player) = self.players.get_mut(&session) {
                player.keys.release_all();
            }
            return;
        }
        let now = context.now();
        let playing = self.players.get(&session).filter(|player| player.running.game() == game).map(|player| player.ended_at);
        match playing {
            // In play: the controls.
            Some(None) => {
                let Some(player) = self.players.get_mut(&session) else { return };
                if let Input::Key { key, .. } = *input
                    && let Some(control) = control_of(key)
                {
                    if input.pressed(key) {
                        player.keys.press(control);
                        player.running.press(control);
                    } else if input.released(key) {
                        player.keys.release(control);
                    }
                } else if input.character() == Some(' ') {
                    // Text, from a receiver that does not send every key:
                    // no release will follow, so a tap.
                    player.keys.tap(super::control::FIRE);
                }
            }
            // On the title page, or after a game: Space or Enter plays.
            ended => {
                let starts = input.character() == Some(' ') || input.pressed(Named::Enter);
                let paused = ended.flatten().is_some_and(|at| now < at.saturating_add(RESTART_PAUSE_MS));
                if starts && !paused {
                    self.start_game(session, game, context);
                }
            }
        }
    }

    fn wake(&mut self, context: &mut Context<'_>) {
        let now = context.now();
        let mut scores_changed = false;
        let mut finished: Vec<SessionId> = Vec::new();
        let mut changed: Vec<SessionId> = Vec::new();
        let mut sounds: Vec<(SessionId, Effect)> = Vec::new();
        for (&session, player) in &mut self.players {
            if player.ended_at.is_some() {
                continue;
            }
            let mut effects = Vec::new();
            if player.running.tick(player.keys, context, &mut effects) {
                changed.push(session);
            }
            player.keys.end_tick();
            if player.running.over() {
                player.ended_at = Some(now);
                finished.push(session);
                if !changed.contains(&session) {
                    changed.push(session);
                }
            }
            sounds.extend(effects.into_iter().map(|effect| (session, effect)));
        }
        for &session in &finished {
            if let Some(player) = self.players.get(&session) {
                scores_changed |= self.hall.record(player.running.game(), session, player.running.score());
            }
        }
        for (session, effect) in sounds {
            self.sounds.play(session, effect, context);
        }
        for session in changed {
            if let Some(player) = self.players.get(&session) {
                self.publish_game(session, player, context);
            }
        }
        if scores_changed {
            self.publish_shared_scores(context);
        }
        self.ticking = self.players.values().any(|player| player.ended_at.is_none());
        if self.ticking {
            context.wake_in(TICK_MS);
        }
    }

    fn ended(&mut self, session: SessionId, context: &mut Context<'_>) {
        self.finish(session, context);
        self.owned.remove(&session);
        self.sounds.forget(session);
    }
}
