//! Station Zero's world: fourteen rooms on two decks, the things in them
//! and the ways between. Descriptions are marked up for
//! [`super::super::text`]: a span in braces is highlighted.

/// A room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    /// Where the player wakes.
    CryoBay,
    /// The radiation suit.
    MedicalBay,
    /// Between the cryo bay and the hub.
    CrewCorridor,
    /// The locker with the keycard.
    CrewQuarters,
    /// The lift, the laboratory door and the ways to every section.
    CentralHub,
    /// The vending machine with the fuse.
    Galley,
    /// Behind the sealed door: the datapad.
    Laboratory,
    /// The crowbar.
    Greenhouse,
    /// Between the hub, the reactor and the airlock.
    Engineering,
    /// Where the fuse goes.
    ReactorRoom,
    /// The console that releases the shuttle.
    Bridge,
    /// The distress beacon.
    CommsRoom,
    /// Between engineering and the docking bay.
    Airlock,
    /// The shuttle: the way home.
    DockingBay,
}

/// Every room, for tests.
pub const ROOMS: [Room; 14] = [
    Room::CryoBay,
    Room::MedicalBay,
    Room::CrewCorridor,
    Room::CrewQuarters,
    Room::CentralHub,
    Room::Galley,
    Room::Laboratory,
    Room::Greenhouse,
    Room::Engineering,
    Room::ReactorRoom,
    Room::Bridge,
    Room::CommsRoom,
    Room::Airlock,
    Room::DockingBay,
];

/// The station's sections, each with its picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    /// Cryo and medical.
    Habitat,
    /// The quarters and the galley.
    Living,
    /// The corridor and the hub.
    Core,
    /// The laboratory and the greenhouse.
    Science,
    /// Engineering and the reactor.
    Engineering,
    /// The bridge and comms.
    Command,
    /// The airlock and the docking bay.
    Docking,
}

/// A way out of a room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// North.
    North,
    /// South.
    South,
    /// East.
    East,
    /// West.
    West,
    /// Up.
    Up,
    /// Down.
    Down,
}

/// The directions, in the order the exits are listed.
pub const DIRECTIONS: [Direction; 6] = [Direction::North, Direction::South, Direction::East, Direction::West, Direction::Up, Direction::Down];

impl Direction {
    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    /// The direction a word names: the word or its first letter.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        let mut letters = word.chars();
        let initial = letters.next().filter(|_| letters.next().is_none());
        DIRECTIONS.into_iter().find(|direction| word == direction.name() || (initial.is_some() && initial == direction.name().chars().next()))
    }
}

/// Something that can be carried.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    /// Opens the laboratory.
    Keycard,
    /// Opens the vending machine.
    Crowbar,
    /// Restores the power.
    Fuse,
    /// Lets the player into the reactor room.
    Suit,
    /// A hint.
    Datapad,
    /// For fun.
    Mug,
}

/// Every item, in the order of [`Item::index`].
pub const ITEMS: [Item; 6] = [Item::Keycard, Item::Crowbar, Item::Fuse, Item::Suit, Item::Datapad, Item::Mug];

impl Item {
    /// Its place in [`ITEMS`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Its name, as listed.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Keycard => "keycard",
            Self::Crowbar => "crowbar",
            Self::Fuse => "fuse",
            Self::Suit => "radiation suit",
            Self::Datapad => "datapad",
            Self::Mug => "mug",
        }
    }

    /// What examining it says.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Keycard => "A crew keycard: {LEVEL 3 - SCIENCE}. The photo is of someone who looks very tired.",
            Self::Crowbar => "A heavy crowbar, mud on the end. Good for opening things that do not want to open.",
            Self::Fuse => "A {reactor fuse}, palm-sized and glowing faintly blue. Handle with care.",
            Self::Suit => "A lead-lined {radiation suit}. You pull it on over your jumpsuit. It smells of old rubber.",
            Self::Datapad => "Dr Okafor's log: {spare fuse stuck in the galley vending machine}. Fit it in the reactor, then free the clamps from the bridge.",
            Self::Mug => "A mug that reads {WORLD'S OKAYEST ENGINEER}. The coffee is cold and slightly furry.",
        }
    }

    /// Where it starts: in a room, or nowhere until a puzzle shows it.
    #[must_use]
    pub const fn start(self) -> Option<Room> {
        match self {
            Self::Keycard | Self::Fuse => None,
            Self::Crowbar => Some(Room::Greenhouse),
            Self::Suit => Some(Room::MedicalBay),
            Self::Datapad => Some(Room::Laboratory),
            Self::Mug => Some(Room::CrewQuarters),
        }
    }
}

/// Something fixed in a room.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fixture {
    /// The cryo pod the player woke in.
    Pod,
    /// The crew locker, holding the keycard.
    Locker,
    /// The vending machine, holding the fuse.
    Machine,
    /// The laboratory door and its card reader.
    Door,
    /// The reactor's fuse panel.
    Panel,
    /// The bridge console.
    Console,
    /// The comms radio.
    Radio,
    /// The escape shuttle.
    Shuttle,
}

impl Fixture {
    /// The room it is in.
    #[must_use]
    pub const fn room(self) -> Room {
        match self {
            Self::Pod => Room::CryoBay,
            Self::Locker => Room::CrewQuarters,
            Self::Machine => Room::Galley,
            Self::Door => Room::CentralHub,
            Self::Panel => Room::ReactorRoom,
            Self::Console => Room::Bridge,
            Self::Radio => Room::CommsRoom,
            Self::Shuttle => Room::DockingBay,
        }
    }
}

/// A thing a command names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Noun {
    /// An item.
    Item(Item),
    /// A fixture.
    Fixture(Fixture),
}

impl Noun {
    /// The thing a word names, with the words a player might use.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        let noun = match word {
            "keycard" | "card" | "key" => Self::Item(Item::Keycard),
            "crowbar" | "bar" | "lever" => Self::Item(Item::Crowbar),
            "fuse" => Self::Item(Item::Fuse),
            "suit" | "radiation" => Self::Item(Item::Suit),
            "datapad" | "pad" | "tablet" | "log" => Self::Item(Item::Datapad),
            "mug" | "coffee" | "cup" => Self::Item(Item::Mug),
            "pod" | "pods" | "cryopod" => Self::Fixture(Fixture::Pod),
            "locker" | "lockers" => Self::Fixture(Fixture::Locker),
            "machine" | "vending" | "vendor" => Self::Fixture(Fixture::Machine),
            "door" | "reader" | "lock" => Self::Fixture(Fixture::Door),
            "panel" | "socket" | "reactor" | "slot" => Self::Fixture(Fixture::Panel),
            "console" | "controls" | "computer" | "clamps" => Self::Fixture(Fixture::Console),
            "radio" | "beacon" | "transmitter" => Self::Fixture(Fixture::Radio),
            "shuttle" | "ship" => Self::Fixture(Fixture::Shuttle),
            _ => return None,
        };
        Some(noun)
    }
}

/// A room's name, section and description.
pub struct About {
    /// Its name, for the title band: at most 21 characters.
    pub name: &'static str,
    /// Its section.
    pub section: Section,
    /// What it looks like, marked up.
    pub description: &'static str,
}

/// What a room is.
#[must_use]
pub const fn about(room: Room) -> About {
    let (name, section, description) = match room {
        Room::CryoBay => (
            "Cryo Bay",
            Section::Habitat,
            "Frost creeps across a ring of twelve {cryo pods}. Eleven are dark and empty; yours hisses open. Somewhere far off, an alarm complains to no one.",
        ),
        Room::MedicalBay => (
            "Medical Bay",
            Section::Habitat,
            "White cabinets hang open, raided long ago. A scanner bed hums to itself. A poster on the wall says {RADIATION KILLS - SUIT UP} beneath a cartoon of a smiling skeleton.",
        ),
        Room::CrewCorridor => (
            "Crew Corridor",
            Section::Core,
            "A long curved corridor follows the station's ring. Handprints smear the windows, and beyond them the {stars} wheel past as Station Zero turns.",
        ),
        Room::CrewQuarters => ("Crew Quarters", Section::Living, "Bunks, photographs and a forgotten sock. A dented crew {locker} stands against the wall."),
        Room::CentralHub => {
            ("Central Hub", Section::Core, "The hub, where every corridor meets. A {lift} rises to the bridge, and the {laboratory door} leads east.")
        }
        Room::Galley => ("Galley", Section::Living, "Trays float in a slow drift of spilled soup. A {vending machine} leans in the corner."),
        Room::Laboratory => (
            "Laboratory",
            Section::Science,
            "Benches of instruments, a whiteboard of equations ending in a large question mark. A green glow spills through the doorway to the east.",
        ),
        Room::Greenhouse => (
            "Greenhouse",
            Section::Science,
            "Under a dome of glass, tomato vines have taken over everything. The air is warm and damp, and it smells of soil and summer.",
        ),
        Room::Engineering => (
            "Engineering",
            Section::Engineering,
            "Pipes, gauges and warning stripes. East, behind a yellow hatch marked {DANGER - RADIATION}, the reactor ticks. South is the airlock.",
        ),
        Room::ReactorRoom => ("Reactor Room", Section::Engineering, "The reactor squats in its cage, humming to itself. A {fuse panel} hangs on the wall."),
        Room::Bridge => (
            "Bridge",
            Section::Command,
            "A wide window shows the blue curve of the planet below. The command {console} waits under a layer of dust. A doorway leads east to comms.",
        ),
        Room::CommsRoom => (
            "Comms Room",
            Section::Command,
            "Racks of radio gear and a single chair. The {distress beacon} has a big red button, and a sign that says {DO NOT PRESS (unless)}.",
        ),
        Room::Airlock => (
            "Airlock",
            Section::Docking,
            "A narrow chamber between two heavy doors. Spacesuits hang on hooks like sleeping bats. The outer door leads south to the docking bay.",
        ),
        Room::DockingBay => ("Docking Bay", Section::Docking, "A small escape {shuttle} sits in the bay, nose towards the stars. It is the only way home."),
    };
    About { name, section, description }
}

/// Where `direction` leads from `room`, if anywhere.
#[must_use]
pub const fn exit(room: Room, direction: Direction) -> Option<Room> {
    use Direction::{Down, East, North, South, Up, West};
    let to = match (room, direction) {
        (Room::CryoBay, East) => Room::MedicalBay,
        (Room::CryoBay, South) | (Room::CrewQuarters, West) | (Room::CentralHub, North) => Room::CrewCorridor,
        (Room::MedicalBay, West) | (Room::CrewCorridor, North) => Room::CryoBay,
        (Room::CrewCorridor, East) => Room::CrewQuarters,
        (Room::CrewCorridor, South) | (Room::Galley, East) | (Room::Laboratory, West) | (Room::Engineering, North) | (Room::Bridge, Down) => Room::CentralHub,
        (Room::CentralHub, West) => Room::Galley,
        (Room::CentralHub, East) | (Room::Greenhouse, West) => Room::Laboratory,
        (Room::CentralHub, South) | (Room::ReactorRoom, West) | (Room::Airlock, North) => Room::Engineering,
        (Room::CentralHub, Up) | (Room::CommsRoom, West) => Room::Bridge,
        (Room::Laboratory, East) => Room::Greenhouse,
        (Room::Engineering, East) => Room::ReactorRoom,
        (Room::Engineering, South) | (Room::DockingBay, North) => Room::Airlock,
        (Room::Bridge, East) => Room::CommsRoom,
        (Room::Airlock, South) => Room::DockingBay,
        _ => return None,
    };
    Some(to)
}
