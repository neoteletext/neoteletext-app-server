//! One viewer's game of Station Zero: where they are, what they carry,
//! which puzzles are solved, and what each typed command does.

use alloc::string::String;
use alloc::vec::Vec;

use super::world::{self, DIRECTIONS, Direction, Fixture, ITEMS, Item, Noun, Room};

/// The locker in the crew quarters is open.
const LOCKER_OPEN: u8 = 1;
/// The laboratory door is open.
const LAB_OPEN: u8 = 1 << 1;
/// The vending machine is prised open.
const MACHINE_OPEN: u8 = 1 << 2;
/// The reactor has its fuse: the station has power.
const POWER: u8 = 1 << 3;
/// The shuttle's docking clamps are released.
const CLAMPS_RELEASED: u8 = 1 << 4;
/// The distress beacon is calling.
const BEACON: u8 = 1 << 5;
/// The shuttle has launched.
const WON: u8 = 1 << 6;

/// The longest command kept, in characters: what the command line
/// shows.
pub const MAX_COMMAND: usize = 34;

/// Words a command may carry that name nothing.
const FILLERS: [&str; 12] = ["the", "a", "an", "at", "to", "on", "with", "into", "in", "up", "around", "my"];

/// Where an item is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// Not in the game yet, or used up.
    Nowhere,
    /// Lying in a room.
    In(Room),
    /// In the player's hands.
    Carried,
}

/// How a reply reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// Information.
    Plain,
    /// Something went right.
    Good,
    /// Something was refused.
    Bad,
}

/// What a command did, for the sound it makes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Nothing worth a sound.
    Quiet,
    /// The player moved.
    Moved,
    /// Something was taken.
    Took,
    /// Something was dropped.
    Dropped,
    /// A puzzle was solved.
    Solved,
    /// The command was refused.
    Refused,
    /// The player was driven back by radiation.
    Hurt,
    /// The shuttle launched: the game is won.
    Won,
    /// A new game began.
    Restarted,
}

/// A command, parsed.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Command {
    /// Nothing typed.
    Empty,
    /// Look around.
    Look,
    /// Go somewhere, if the command said where.
    Go(Option<Direction>),
    /// Take a thing; `named` says whether any word was given.
    Take { noun: Option<Noun>, named: bool },
    /// Take everything here.
    TakeAll,
    /// Drop a thing.
    Drop { noun: Option<Noun>, named: bool },
    /// Use things, in the order named.
    Use { nouns: Vec<Noun>, named: bool },
    /// Open a thing.
    Open { noun: Option<Noun>, named: bool },
    /// Examine a thing.
    Examine { noun: Option<Noun>, named: bool },
    /// List what is carried.
    Inventory,
    /// List the commands.
    Help,
    /// A clue for the next step.
    Hint,
    /// Let time pass.
    Wait,
    /// Begin again.
    Restart,
    /// The magic word of another game.
    Xyzzy,
    /// Not a command.
    Unknown,
}

/// The words of a typed line: lower case, split at anything that is not
/// a letter, a digit or a question mark.
fn words_of(line: &str) -> Vec<String> {
    let cleaned: String =
        line.chars().map(|character| if character.is_alphanumeric() || character == '?' { character.to_ascii_lowercase() } else { ' ' }).collect();
    cleaned.split_whitespace().map(String::from).collect()
}

/// Parses a typed line.
fn parse(line: &str) -> Command {
    let words = words_of(line);
    let Some((verb, rest)) = words.split_first() else { return Command::Empty };
    let nouns: Vec<Noun> = rest.iter().filter_map(|word| Noun::of(word)).collect();
    let noun = nouns.first().copied();
    let named = rest.iter().any(|word| !FILLERS.contains(&word.as_str()));
    let every = rest.iter().any(|word| word == "all" || word == "everything");
    match verb.as_str() {
        "look" | "l" if named => Command::Examine { noun, named },
        "look" | "l" => Command::Look,
        "examine" | "x" | "inspect" | "read" | "check" | "search" => Command::Examine { noun, named },
        "go" | "walk" | "move" | "head" | "run" | "climb" | "crawl" => Command::Go(rest.iter().find_map(|word| Direction::of(word))),
        "take" | "get" | "grab" | "pick" | "collect" if every => Command::TakeAll,
        "take" | "get" | "grab" | "pick" | "collect" => Command::Take { noun, named },
        "drop" | "put" | "leave" | "discard" | "throw" => Command::Drop { noun, named },
        "use" | "push" | "press" | "activate" | "insert" | "fit" | "pry" | "prise" | "prize" | "lever" | "swipe" | "switch" | "plug" | "install" | "drink"
        | "release" => Command::Use { nouns, named },
        "wear" => Command::Use { nouns: alloc::vec![Noun::Item(Item::Suit)], named: true },
        "board" | "enter" | "launch" | "fly" | "escape" => Command::Use { nouns: alloc::vec![Noun::Fixture(Fixture::Shuttle)], named: true },
        "open" | "unlock" | "force" => Command::Open { noun, named },
        "inventory" | "inv" | "i" => Command::Inventory,
        "help" | "?" | "commands" => Command::Help,
        "hint" | "clue" => Command::Hint,
        "wait" | "z" => Command::Wait,
        "restart" | "reset" | "again" => Command::Restart,
        "xyzzy" | "plugh" => Command::Xyzzy,
        word => Direction::of(word).map_or(Command::Unknown, |direction| Command::Go(Some(direction))),
    }
}

/// What a command answers: its reply, marked up, its tone, and its event.
struct Outcome {
    reply: String,
    tone: Tone,
    event: Event,
}

impl Outcome {
    fn plain(reply: &str) -> Self {
        Self { reply: String::from(reply), tone: Tone::Plain, event: Event::Quiet }
    }

    fn good(reply: &str, event: Event) -> Self {
        Self { reply: String::from(reply), tone: Tone::Good, event }
    }

    fn refused(reply: &str) -> Self {
        Self { reply: String::from(reply), tone: Tone::Bad, event: Event::Refused }
    }
}

/// The reply when a thing is not here.
const NOT_HERE: &str = "You see no such thing here.";
/// The help text.
pub const HELP: &str = "{LOOK}, {N S E W U D}, {TAKE} x, {DROP} x, {USE} x, {OPEN} x, {EXAMINE} x, {INVENTORY}, {HINT}, {RESTART}. Type, then press Enter.";

/// One viewer's game.
#[derive(Clone, Debug)]
pub struct Game {
    room: Room,
    places: [Place; ITEMS.len()],
    flags: u8,
    moves: u16,
    said: String,
    reply: String,
    tone: Tone,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// A new game: awake in the cryo bay, carrying nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            room: Room::CryoBay,
            places: ITEMS.map(|item| item.start().map_or(Place::Nowhere, Place::In)),
            flags: 0,
            moves: 0,
            said: String::new(),
            reply: String::from("You wake, cold and alone. The station is failing. Find a way home. Type {HELP} for the commands."),
            tone: Tone::Plain,
        }
    }

    /// The room the player is in.
    #[must_use]
    pub const fn room(&self) -> Room {
        self.room
    }

    /// The commands acted on.
    #[must_use]
    pub const fn moves(&self) -> u16 {
        self.moves
    }

    /// The last command, as typed.
    #[must_use]
    pub fn said(&self) -> &str {
        &self.said
    }

    /// The reply to it, marked up.
    #[must_use]
    pub fn reply(&self) -> &str {
        &self.reply
    }

    /// How the reply reads.
    #[must_use]
    pub const fn tone(&self) -> Tone {
        self.tone
    }

    /// Whether the shuttle has launched.
    #[must_use]
    pub const fn won(&self) -> bool {
        self.flag(WON)
    }

    /// Whether the beacon was sent.
    #[must_use]
    pub const fn beacon(&self) -> bool {
        self.flag(BEACON)
    }

    const fn flag(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    const fn set(&mut self, flag: u8) {
        self.flags |= flag;
    }

    /// Where `item` is.
    #[must_use]
    pub fn place(&self, item: Item) -> Place {
        self.places.get(item.index()).copied().unwrap_or(Place::Nowhere)
    }

    fn put(&mut self, item: Item, place: Place) {
        if let Some(slot) = self.places.get_mut(item.index()) {
            *slot = place;
        }
    }

    /// Whether the player carries `item`.
    #[must_use]
    pub fn carrying(&self, item: Item) -> bool {
        self.place(item) == Place::Carried
    }

    /// The items carried.
    #[must_use]
    pub fn carried(&self) -> Vec<Item> {
        ITEMS.into_iter().filter(|&item| self.carrying(item)).collect()
    }

    /// The items lying in the player's room.
    #[must_use]
    pub fn here(&self) -> Vec<Item> {
        ITEMS.into_iter().filter(|&item| self.place(item) == Place::In(self.room)).collect()
    }

    /// The directions out of the player's room.
    #[must_use]
    pub fn exits(&self) -> Vec<Direction> {
        DIRECTIONS.into_iter().filter(|&direction| world::exit(self.room, direction).is_some()).collect()
    }

    /// The player's room as it is now: its description and what has
    /// changed in it, marked up.
    #[must_use]
    pub fn description(&self) -> String {
        let mut text = String::from(world::about(self.room).description);
        let extra = match self.room {
            Room::CryoBay if self.flag(POWER) => "The lights burn steady and white now.",
            Room::CryoBay => "Red emergency lights pulse slowly.",
            Room::CrewQuarters if self.flag(LOCKER_OPEN) => "Its door hangs open.",
            Room::CrewQuarters => "Its door is held shut by a bent latch.",
            Room::CentralHub => match (self.flag(LAB_OPEN), self.flag(POWER)) {
                (false, false) => "The laboratory door is sealed; its reader blinks red. The lift's light is dark.",
                (true, false) => "The laboratory door stands open. The lift's light is dark.",
                (false, true) => "The laboratory door is sealed; its reader blinks red. The lift glows green.",
                (true, true) => "The laboratory door stands open. The lift glows green.",
            },
            Room::Galley if self.flag(MACHINE_OPEN) => "Its door has been prised off.",
            Room::Galley => "Behind its cracked glass, something glows blue.",
            Room::ReactorRoom if self.flag(POWER) => "A new fuse glows in its socket.",
            Room::ReactorRoom => "One socket is empty, scorched black.",
            Room::Bridge if self.flag(CLAMPS_RELEASED) => "The console reads {CLAMPS RELEASED}.",
            Room::Bridge if self.flag(POWER) => "The console's screen flickers awake.",
            Room::CommsRoom if self.flag(BEACON) => "The beacon blinks, calling for help.",
            Room::DockingBay if self.flag(CLAMPS_RELEASED) => "The docking clamps stand open.",
            Room::DockingBay => "Four massive docking clamps hold it fast.",
            _ => "",
        };
        if !extra.is_empty() {
            text.push(' ');
            text.push_str(extra);
        }
        text
    }

    /// The ending, marked up.
    #[must_use]
    pub const fn ending(&self) -> &'static str {
        if self.flag(BEACON) {
            "The shuttle falls away from Station Zero, and the stars turn around you. Ahead, a rescue ship answers your {beacon}. You are going home."
        } else {
            "The shuttle falls away from Station Zero, and the stars turn around you. No one knows you are coming, but the shuttle knows the way {home}."
        }
    }

    /// Acts on a typed line and remembers it and the reply.
    pub fn command(&mut self, line: &str) -> Event {
        let command = parse(line);
        if command == Command::Empty {
            return Event::Quiet;
        }
        let said: String = line.trim().chars().take(MAX_COMMAND).collect();
        if command == Command::Restart {
            *self = Self::new();
            self.said = said;
            self.reply = String::from("A new game. You wake again in the cryo bay, cold and alone.");
            return Event::Restarted;
        }
        if self.won() {
            // The moves stay as they were at the launch.
            self.said = said;
            self.reply = String::from(if command == Command::Help { HELP } else { "The shuttle is on its way home. Type {RESTART} to play again." });
            self.tone = Tone::Plain;
            return Event::Quiet;
        }
        let outcome = self.act(command);
        self.moves = self.moves.saturating_add(1);
        self.said = said;
        self.reply = outcome.reply;
        self.tone = outcome.tone;
        outcome.event
    }

    fn act(&mut self, command: Command) -> Outcome {
        match command {
            Command::Empty | Command::Restart | Command::Look => Outcome::plain("You take a careful look around."),
            Command::Go(direction) => self.go(direction),
            Command::Take { noun, named } => self.take(noun, named),
            Command::TakeAll => self.take_all(),
            Command::Drop { noun, named } => self.drop(noun, named),
            Command::Use { nouns, named } => self.use_things(&nouns, named),
            Command::Open { noun, named } => self.open(noun, named),
            Command::Examine { noun, named } => self.examine(noun, named),
            Command::Inventory => self.inventory(),
            Command::Help => Outcome::plain(HELP),
            Command::Hint => Outcome::plain(self.hint()),
            Command::Wait => Outcome::plain("Time passes. The stars wheel by the windows."),
            Command::Xyzzy => Outcome::plain("A hollow voice says: {wrong game}."),
            Command::Unknown => Outcome::refused("That is not a command here. Type {HELP} for the commands."),
        }
    }

    /// Why `direction` from the player's room is closed, if it is.
    fn blocked(&self, direction: Direction) -> Option<(&'static str, Event)> {
        match (self.room, direction) {
            (Room::CentralHub, Direction::East) if !self.flag(LAB_OPEN) => Some(("The laboratory door is sealed. Its card reader blinks red.", Event::Refused)),
            (Room::CentralHub, Direction::Up) if !self.flag(POWER) => Some(("You press the lift button. Nothing. The lift has no power.", Event::Refused)),
            (Room::Engineering, Direction::East) if !self.carrying(Item::Suit) => {
                Some(("Radiation alarms scream as the hatch opens. You back out fast: you need protection.", Event::Hurt))
            }
            (Room::Airlock, Direction::South) if !self.flag(POWER) => Some(("The outer door will not cycle without power.", Event::Refused)),
            _ => None,
        }
    }

    fn go(&mut self, direction: Option<Direction>) -> Outcome {
        let Some(direction) = direction else { return Outcome::refused("Go where? North, south, east, west, up or down.") };
        let Some(to) = world::exit(self.room, direction) else { return Outcome::refused("You cannot go that way.") };
        if let Some((reason, event)) = self.blocked(direction) {
            return Outcome { reply: String::from(reason), tone: Tone::Bad, event };
        }
        self.room = to;
        let mut reply = alloc::format!("You go {{{}}}.", direction.name());
        if to == Room::ReactorRoom {
            reply.push_str(" The suit's counter clicks madly.");
        }
        Outcome { reply, tone: Tone::Plain, event: Event::Moved }
    }

    /// Whether `item` is carried or here.
    fn reachable(&self, item: Item) -> bool {
        self.carrying(item) || self.place(item) == Place::In(self.room)
    }

    /// Whether `fixture` is in the player's room.
    fn present(&self, fixture: Fixture) -> bool {
        fixture.room() == self.room
    }

    fn take(&mut self, noun: Option<Noun>, named: bool) -> Outcome {
        match noun {
            None if named => Outcome::refused(NOT_HERE),
            None => Outcome::refused("Take what?"),
            Some(Noun::Item(item)) if self.carrying(item) => Outcome::plain("You already have it."),
            Some(Noun::Item(item)) if self.place(item) == Place::In(self.room) => {
                self.put(item, Place::Carried);
                Outcome::good(&alloc::format!("Taken: {{{}}}.", item.name()), Event::Took)
            }
            Some(Noun::Fixture(fixture)) if self.present(fixture) => Outcome::refused("It is fixed in place."),
            Some(_) => Outcome::refused(NOT_HERE),
        }
    }

    fn take_all(&mut self) -> Outcome {
        let here = self.here();
        if here.is_empty() {
            return Outcome::refused("There is nothing here to take.");
        }
        let names: Vec<&str> = here.iter().map(|item| item.name()).collect();
        for item in here {
            self.put(item, Place::Carried);
        }
        Outcome::good(&alloc::format!("Taken: {{{}}}.", names.join(", ")), Event::Took)
    }

    fn drop(&mut self, noun: Option<Noun>, named: bool) -> Outcome {
        match noun {
            None if !named => Outcome::refused("Drop what?"),
            Some(Noun::Item(item)) if self.carrying(item) => {
                self.put(item, Place::In(self.room));
                Outcome { reply: alloc::format!("Dropped: {{{}}}.", item.name()), tone: Tone::Plain, event: Event::Dropped }
            }
            _ => Outcome::refused("You are not carrying that."),
        }
    }

    fn inventory(&self) -> Outcome {
        let carried = self.carried();
        if carried.is_empty() {
            return Outcome::plain("You are carrying nothing but a hospital gown.");
        }
        let names: Vec<&str> = carried.iter().map(|item| item.name()).collect();
        Outcome::plain(&alloc::format!("You carry: {{{}}}.", names.join(", ")))
    }

    /// A puzzle `noun` solves in the player's room, if it is one.
    fn puzzle(&mut self, noun: Noun) -> Option<Outcome> {
        let outcome = match (self.room, noun) {
            (Room::CentralHub, Noun::Item(Item::Keycard) | Noun::Fixture(Fixture::Door)) => {
                if self.flag(LAB_OPEN) {
                    Outcome::plain("The laboratory door is already open.")
                } else if self.carrying(Item::Keycard) {
                    self.set(LAB_OPEN);
                    Outcome::good("You swipe the keycard. The reader blinks {green}, and the laboratory door slides open.", Event::Solved)
                } else {
                    Outcome::refused("The card reader wants a keycard.")
                }
            }
            (Room::Galley, Noun::Item(Item::Crowbar) | Noun::Fixture(Fixture::Machine)) => {
                if self.flag(MACHINE_OPEN) {
                    Outcome::plain("The machine is already open, and empty.")
                } else if self.carrying(Item::Crowbar) {
                    self.set(MACHINE_OPEN);
                    self.put(Item::Fuse, Place::In(Room::Galley));
                    Outcome::good("You lever the crowbar in and heave. Glass rains down, and a {reactor fuse} drops out.", Event::Solved)
                } else {
                    Outcome::refused("The machine's door is jammed. You need something to lever it open.")
                }
            }
            (Room::ReactorRoom, Noun::Item(Item::Fuse) | Noun::Fixture(Fixture::Panel)) => {
                if self.flag(POWER) {
                    Outcome::plain("The panel hums happily. The power is on.")
                } else if self.carrying(Item::Fuse) {
                    self.set(POWER);
                    self.put(Item::Fuse, Place::Nowhere);
                    Outcome::good("You push the fuse home. With a deep {WHUMMM} the lights come up across the station.", Event::Solved)
                } else {
                    Outcome::refused("The empty socket needs a fuse.")
                }
            }
            (Room::Bridge, Noun::Fixture(Fixture::Console)) => {
                if self.flag(CLAMPS_RELEASED) {
                    Outcome::plain("The clamps are already released.")
                } else {
                    self.set(CLAMPS_RELEASED);
                    Outcome::good("You wipe off the dust and key the release. Far below, the {docking clamps} let go with a clang.", Event::Solved)
                }
            }
            (Room::CommsRoom, Noun::Fixture(Fixture::Radio)) => {
                if self.flag(BEACON) {
                    Outcome::plain("The beacon is already calling.")
                } else {
                    self.set(BEACON);
                    Outcome::good("You press the big red button. The beacon starts to sing into the dark: {someone will hear}.", Event::Solved)
                }
            }
            (Room::DockingBay, Noun::Fixture(Fixture::Shuttle)) => {
                if self.flag(CLAMPS_RELEASED) {
                    self.set(WON);
                    Outcome::good("You strap in and punch {LAUNCH}. The shuttle leaps out of the bay. You made it!", Event::Won)
                } else {
                    Outcome::refused("You climb in, but the {docking clamps} hold the shuttle fast. They are worked from the bridge.")
                }
            }
            _ => return None,
        };
        Some(outcome)
    }

    fn use_things(&mut self, nouns: &[Noun], named: bool) -> Outcome {
        for &noun in nouns {
            if let Some(outcome) = self.puzzle(noun) {
                return outcome;
            }
        }
        match nouns.first().copied() {
            None if named => Outcome::refused(NOT_HERE),
            None => Outcome::refused("Use what?"),
            Some(Noun::Item(item)) if !self.reachable(item) => Outcome::refused("You do not have that."),
            Some(Noun::Item(item)) => Outcome::plain(match item {
                Item::Keycard => "Nothing here reads a keycard.",
                Item::Crowbar => "Nothing here needs prising open.",
                Item::Fuse => "Nothing here takes a fuse.",
                Item::Suit => "You check the seals. The suit will keep the radiation out.",
                Item::Datapad => item.description(),
                Item::Mug => "You sip the cold coffee. It is terrible. You feel braver anyway.",
            }),
            Some(Noun::Fixture(fixture)) if !self.present(fixture) => Outcome::refused(NOT_HERE),
            Some(Noun::Fixture(Fixture::Pod)) => Outcome::plain("You are not getting back in there."),
            Some(Noun::Fixture(Fixture::Locker)) => self.open(Some(Noun::Fixture(Fixture::Locker)), true),
            Some(Noun::Fixture(_)) => Outcome::plain("Nothing happens."),
        }
    }

    fn open(&mut self, noun: Option<Noun>, named: bool) -> Outcome {
        match noun {
            None if named => Outcome::refused(NOT_HERE),
            None => Outcome::refused("Open what?"),
            Some(Noun::Fixture(fixture)) if !self.present(fixture) => Outcome::refused(NOT_HERE),
            Some(Noun::Fixture(Fixture::Locker)) => {
                if self.flag(LOCKER_OPEN) {
                    Outcome::plain("The locker is already open.")
                } else {
                    self.set(LOCKER_OPEN);
                    self.put(Item::Keycard, Place::In(Room::CrewQuarters));
                    Outcome::good("You force the bent latch. The locker swings open, and a {keycard} falls out.", Event::Solved)
                }
            }
            Some(Noun::Fixture(Fixture::Pod)) => Outcome::plain("Your pod is already open. You are not getting back in."),
            Some(noun @ Noun::Fixture(Fixture::Machine | Fixture::Door | Fixture::Shuttle)) => {
                self.puzzle(noun).unwrap_or_else(|| Outcome::refused("It will not open."))
            }
            Some(Noun::Item(item)) if !self.reachable(item) => Outcome::refused(NOT_HERE),
            Some(_) => Outcome::refused("It does not open."),
        }
    }

    fn examine(&self, noun: Option<Noun>, named: bool) -> Outcome {
        let text = match noun {
            None if named => return Outcome::refused(NOT_HERE),
            None => return Outcome::refused("Examine what?"),
            Some(Noun::Item(item)) if self.reachable(item) => item.description(),
            Some(Noun::Fixture(fixture)) if self.present(fixture) => self.fixture_description(fixture),
            Some(_) => return Outcome::refused(NOT_HERE),
        };
        Outcome::plain(text)
    }

    const fn fixture_description(&self, fixture: Fixture) -> &'static str {
        match fixture {
            Fixture::Pod => "Your pod, still cold. Its display reads {SLEEPER 12 REVIVED. CREW EVACUATED.}",
            Fixture::Locker if self.flag(LOCKER_OPEN) => "The locker hangs open. Only a sock remains.",
            Fixture::Locker => "A dented crew locker. The latch is bent, but you could force it {open}.",
            Fixture::Machine if self.flag(MACHINE_OPEN) => "Smashed open and empty, apart from a very old sandwich.",
            Fixture::Machine => "Wedged behind the cracked glass, between snack bars, a {fuse} glows blue. The door is jammed.",
            Fixture::Door if self.flag(LAB_OPEN) => "The laboratory door stands open.",
            Fixture::Door => "Sealed. The card reader beside it wants a {keycard}.",
            Fixture::Panel if self.flag(POWER) => "The new fuse glows steadily in its socket.",
            Fixture::Panel => "One socket in the panel is empty and scorched. It needs a {fuse}.",
            Fixture::Console if self.flag(CLAMPS_RELEASED) => "The console reads {CLAMPS RELEASED}.",
            Fixture::Console => "A dusty console. One big key reads {RELEASE CLAMPS}.",
            Fixture::Radio if self.flag(BEACON) => "The beacon is calling. Someone will hear.",
            Fixture::Radio => "The distress beacon. A big red button waits to be {pressed}.",
            Fixture::Shuttle if self.flag(CLAMPS_RELEASED) => "Free of its clamps and ready to fly. {BOARD} it!",
            Fixture::Shuttle => "A two-seat escape shuttle, fuelled, held fast by its docking clamps.",
        }
    }

    /// A clue for the next step, by how far the player has got.
    fn hint(&self) -> &'static str {
        if !self.flag(LOCKER_OPEN) {
            "The locker in the crew quarters looks worth opening."
        } else if !self.flag(LAB_OPEN) {
            "That keycard must open something. The card reader in the hub?"
        } else if !self.flag(MACHINE_OPEN) && !self.carrying(Item::Crowbar) {
            "Something in the greenhouse could prise things open."
        } else if !self.flag(MACHINE_OPEN) {
            "The galley's vending machine is hiding something."
        } else if !self.flag(POWER) && !self.carrying(Item::Suit) {
            "The reactor room is deadly without protection. Try the medical bay."
        } else if !self.flag(POWER) {
            "Fit the fuse into the panel in the reactor room."
        } else if !self.flag(CLAMPS_RELEASED) {
            "The lift works now. Release the clamps from the bridge."
        } else {
            "Board the shuttle in the docking bay. The beacon in comms is up to you."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_with_their_synonyms() {
        assert_eq!(parse("  "), Command::Empty);
        assert_eq!(parse("n"), Command::Go(Some(Direction::North)));
        assert_eq!(parse("Go UP."), Command::Go(Some(Direction::Up)));
        assert_eq!(parse("pick up the keycard"), Command::Take { noun: Some(Noun::Item(Item::Keycard)), named: true });
        assert_eq!(parse("take all"), Command::TakeAll);
        assert_eq!(
            parse("use crowbar on machine"),
            Command::Use { nouns: alloc::vec![Noun::Item(Item::Crowbar), Noun::Fixture(Fixture::Machine)], named: true }
        );
        assert_eq!(parse("look at the pod"), Command::Examine { noun: Some(Noun::Fixture(Fixture::Pod)), named: true });
        assert_eq!(parse("take"), Command::Take { noun: None, named: false });
        assert_eq!(parse("take banana"), Command::Take { noun: None, named: true });
        assert_eq!(parse("dance"), Command::Unknown);
    }
}
