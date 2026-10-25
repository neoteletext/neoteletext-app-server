// Station Zero's world: fourteen rooms on two decks, the things in them
// and the ways between. Descriptions are marked up for ../text.ts: a span
// in braces is highlighted (crates/teletext-showcase/src/adventure/station/
// world.rs).

/** The rooms. */
export const Room = {
  /** Where the player wakes. */
  CryoBay: "CryoBay",
  /** The radiation suit. */
  MedicalBay: "MedicalBay",
  /** Between the cryo bay and the hub. */
  CrewCorridor: "CrewCorridor",
  /** The locker with the keycard. */
  CrewQuarters: "CrewQuarters",
  /** The lift, the laboratory door and the ways to every section. */
  CentralHub: "CentralHub",
  /** The vending machine with the fuse. */
  Galley: "Galley",
  /** Behind the sealed door: the datapad. */
  Laboratory: "Laboratory",
  /** The crowbar. */
  Greenhouse: "Greenhouse",
  /** Between the hub, the reactor and the airlock. */
  Engineering: "Engineering",
  /** Where the fuse goes. */
  ReactorRoom: "ReactorRoom",
  /** The console that releases the shuttle. */
  Bridge: "Bridge",
  /** The distress beacon. */
  CommsRoom: "CommsRoom",
  /** Between engineering and the docking bay. */
  Airlock: "Airlock",
  /** The shuttle: the way home. */
  DockingBay: "DockingBay",
} as const;
/** A room. */
export type Room = (typeof Room)[keyof typeof Room];

/** Every room, for tests. */
export const ROOMS: readonly Room[] = Object.values(Room);

/** The station's sections, each with its picture. */
export const Section = {
  /** Cryo and medical. */
  Habitat: "Habitat",
  /** The quarters and the galley. */
  Living: "Living",
  /** The corridor and the hub. */
  Core: "Core",
  /** The laboratory and the greenhouse. */
  Science: "Science",
  /** Engineering and the reactor. */
  Engineering: "Engineering",
  /** The bridge and comms. */
  Command: "Command",
  /** The airlock and the docking bay. */
  Docking: "Docking",
} as const;
/** A section. */
export type Section = (typeof Section)[keyof typeof Section];

/** The ways out of a room, each its name. */
export const Direction = {
  North: "north",
  South: "south",
  East: "east",
  West: "west",
  Up: "up",
  Down: "down",
} as const;
/** A way out of a room. */
export type Direction = (typeof Direction)[keyof typeof Direction];

/** The directions, in the order the exits are listed. */
export const DIRECTIONS: readonly Direction[] = [Direction.North, Direction.South, Direction.East, Direction.West, Direction.Up, Direction.Down];

/** The name of `direction`. */
export function directionName(direction: Direction): string {
  return direction;
}

/** The direction a word names: the word or its first letter. */
export function directionOf(word: string): Direction | undefined {
  const letters = [...word];
  const initial = letters.length === 1 ? letters[0] : undefined;
  return DIRECTIONS.find((direction) => word === direction || (initial !== undefined && initial === direction[0]));
}

/** Things that can be carried. */
export const Item = {
  /** Opens the laboratory. */
  Keycard: "Keycard",
  /** Opens the vending machine. */
  Crowbar: "Crowbar",
  /** Restores the power. */
  Fuse: "Fuse",
  /** Lets the player into the reactor room. */
  Suit: "Suit",
  /** A hint. */
  Datapad: "Datapad",
  /** For fun. */
  Mug: "Mug",
} as const;
/** Something that can be carried. */
export type Item = (typeof Item)[keyof typeof Item];

/** Every item, in the order of {@link itemIndex}. */
export const ITEMS: readonly Item[] = [Item.Keycard, Item.Crowbar, Item.Fuse, Item.Suit, Item.Datapad, Item.Mug];

/** Its place in {@link ITEMS}. */
export function itemIndex(item: Item): number {
  return ITEMS.indexOf(item);
}

/** Its name, as listed. */
export function itemName(item: Item): string {
  switch (item) {
    case Item.Keycard:
      return "keycard";
    case Item.Crowbar:
      return "crowbar";
    case Item.Fuse:
      return "fuse";
    case Item.Suit:
      return "radiation suit";
    case Item.Datapad:
      return "datapad";
    case Item.Mug:
      return "mug";
  }
}

/** What examining it says. */
export function itemDescription(item: Item): string {
  switch (item) {
    case Item.Keycard:
      return "A crew keycard: {LEVEL 3 - SCIENCE}. The photo is of someone who looks very tired.";
    case Item.Crowbar:
      return "A heavy crowbar, mud on the end. Good for opening things that do not want to open.";
    case Item.Fuse:
      return "A {reactor fuse}, palm-sized and glowing faintly blue. Handle with care.";
    case Item.Suit:
      return "A lead-lined {radiation suit}. You pull it on over your jumpsuit. It smells of old rubber.";
    case Item.Datapad:
      return "Dr Okafor's log: {spare fuse stuck in the galley vending machine}. Fit it in the reactor, then free the clamps from the bridge.";
    case Item.Mug:
      return "A mug that reads {WORLD'S OKAYEST ENGINEER}. The coffee is cold and slightly furry.";
  }
}

/** Where it starts: in a room, or nowhere until a puzzle shows it. */
export function itemStart(item: Item): Room | undefined {
  switch (item) {
    case Item.Keycard:
    case Item.Fuse:
      return undefined;
    case Item.Crowbar:
      return Room.Greenhouse;
    case Item.Suit:
      return Room.MedicalBay;
    case Item.Datapad:
      return Room.Laboratory;
    case Item.Mug:
      return Room.CrewQuarters;
  }
}

/** Things fixed in rooms. */
export const Fixture = {
  /** The cryo pod the player woke in. */
  Pod: "Pod",
  /** The crew locker, holding the keycard. */
  Locker: "Locker",
  /** The vending machine, holding the fuse. */
  Machine: "Machine",
  /** The laboratory door and its card reader. */
  Door: "Door",
  /** The reactor's fuse panel. */
  Panel: "Panel",
  /** The bridge console. */
  Console: "Console",
  /** The comms radio. */
  Radio: "Radio",
  /** The escape shuttle. */
  Shuttle: "Shuttle",
} as const;
/** Something fixed in a room. */
export type Fixture = (typeof Fixture)[keyof typeof Fixture];

/** The room `fixture` is in. */
export function fixtureRoom(fixture: Fixture): Room {
  switch (fixture) {
    case Fixture.Pod:
      return Room.CryoBay;
    case Fixture.Locker:
      return Room.CrewQuarters;
    case Fixture.Machine:
      return Room.Galley;
    case Fixture.Door:
      return Room.CentralHub;
    case Fixture.Panel:
      return Room.ReactorRoom;
    case Fixture.Console:
      return Room.Bridge;
    case Fixture.Radio:
      return Room.CommsRoom;
    case Fixture.Shuttle:
      return Room.DockingBay;
  }
}

/** A thing a command names: an item or a fixture. */
export type Noun = { readonly kind: "item"; readonly item: Item } | { readonly kind: "fixture"; readonly fixture: Fixture };

/** The noun of an item. */
export function itemNoun(item: Item): Noun {
  return { kind: "item", item };
}

/** The noun of a fixture. */
export function fixtureNoun(fixture: Fixture): Noun {
  return { kind: "fixture", fixture };
}

/** The words a player might use for each thing. */
const NOUN_WORDS: readonly (readonly [Noun, readonly string[]])[] = [
  [itemNoun(Item.Keycard), ["keycard", "card", "key"]],
  [itemNoun(Item.Crowbar), ["crowbar", "bar", "lever"]],
  [itemNoun(Item.Fuse), ["fuse"]],
  [itemNoun(Item.Suit), ["suit", "radiation"]],
  [itemNoun(Item.Datapad), ["datapad", "pad", "tablet", "log"]],
  [itemNoun(Item.Mug), ["mug", "coffee", "cup"]],
  [fixtureNoun(Fixture.Pod), ["pod", "pods", "cryopod"]],
  [fixtureNoun(Fixture.Locker), ["locker", "lockers"]],
  [fixtureNoun(Fixture.Machine), ["machine", "vending", "vendor"]],
  [fixtureNoun(Fixture.Door), ["door", "reader", "lock"]],
  [fixtureNoun(Fixture.Panel), ["panel", "socket", "reactor", "slot"]],
  [fixtureNoun(Fixture.Console), ["console", "controls", "computer", "clamps"]],
  [fixtureNoun(Fixture.Radio), ["radio", "beacon", "transmitter"]],
  [fixtureNoun(Fixture.Shuttle), ["shuttle", "ship"]],
];

/** The thing a word names, with the words a player might use. */
export function nounOf(word: string): Noun | undefined {
  return NOUN_WORDS.find(([, names]) => names.includes(word))?.[0];
}

/** A room's name, section and description. */
export interface About {
  /** Its name, for the title band: at most 21 characters. */
  readonly name: string;
  /** Its section. */
  readonly section: Section;
  /** What it looks like, marked up. */
  readonly description: string;
}

/** What each room is. */
const ABOUT: Readonly<Record<Room, About>> = {
  CryoBay: {
    name: "Cryo Bay",
    section: Section.Habitat,
    description:
      "Frost creeps across a ring of twelve {cryo pods}. Eleven are dark and empty; yours hisses open. Somewhere far off, an alarm complains to no one.",
  },
  MedicalBay: {
    name: "Medical Bay",
    section: Section.Habitat,
    description:
      "White cabinets hang open, raided long ago. A scanner bed hums to itself. A poster on the wall says {RADIATION KILLS - SUIT UP} beneath a cartoon of a smiling skeleton.",
  },
  CrewCorridor: {
    name: "Crew Corridor",
    section: Section.Core,
    description:
      "A long curved corridor follows the station's ring. Handprints smear the windows, and beyond them the {stars} wheel past as Station Zero turns.",
  },
  CrewQuarters: {
    name: "Crew Quarters",
    section: Section.Living,
    description: "Bunks, photographs and a forgotten sock. A dented crew {locker} stands against the wall.",
  },
  CentralHub: {
    name: "Central Hub",
    section: Section.Core,
    description: "The hub, where every corridor meets. A {lift} rises to the bridge, and the {laboratory door} leads east.",
  },
  Galley: {
    name: "Galley",
    section: Section.Living,
    description: "Trays float in a slow drift of spilled soup. A {vending machine} leans in the corner.",
  },
  Laboratory: {
    name: "Laboratory",
    section: Section.Science,
    description:
      "Benches of instruments, a whiteboard of equations ending in a large question mark. A green glow spills through the doorway to the east.",
  },
  Greenhouse: {
    name: "Greenhouse",
    section: Section.Science,
    description:
      "Under a dome of glass, tomato vines have taken over everything. The air is warm and damp, and it smells of soil and summer.",
  },
  Engineering: {
    name: "Engineering",
    section: Section.Engineering,
    description:
      "Pipes, gauges and warning stripes. East, behind a yellow hatch marked {DANGER - RADIATION}, the reactor ticks. South is the airlock.",
  },
  ReactorRoom: {
    name: "Reactor Room",
    section: Section.Engineering,
    description: "The reactor squats in its cage, humming to itself. A {fuse panel} hangs on the wall.",
  },
  Bridge: {
    name: "Bridge",
    section: Section.Command,
    description:
      "A wide window shows the blue curve of the planet below. The command {console} waits under a layer of dust. A doorway leads east to comms.",
  },
  CommsRoom: {
    name: "Comms Room",
    section: Section.Command,
    description:
      "Racks of radio gear and a single chair. The {distress beacon} has a big red button, and a sign that says {DO NOT PRESS (unless)}.",
  },
  Airlock: {
    name: "Airlock",
    section: Section.Docking,
    description:
      "A narrow chamber between two heavy doors. Spacesuits hang on hooks like sleeping bats. The outer door leads south to the docking bay.",
  },
  DockingBay: {
    name: "Docking Bay",
    section: Section.Docking,
    description: "A small escape {shuttle} sits in the bay, nose towards the stars. It is the only way home.",
  },
};

/** What a room is. */
export function about(room: Room): About {
  return ABOUT[room];
}

/** The ways between rooms: from a room, which way, to which room. */
const EXITS: readonly (readonly [from: Room, direction: Direction, to: Room])[] = [
  [Room.CryoBay, Direction.East, Room.MedicalBay],
  [Room.CryoBay, Direction.South, Room.CrewCorridor],
  [Room.CrewQuarters, Direction.West, Room.CrewCorridor],
  [Room.CentralHub, Direction.North, Room.CrewCorridor],
  [Room.MedicalBay, Direction.West, Room.CryoBay],
  [Room.CrewCorridor, Direction.North, Room.CryoBay],
  [Room.CrewCorridor, Direction.East, Room.CrewQuarters],
  [Room.CrewCorridor, Direction.South, Room.CentralHub],
  [Room.Galley, Direction.East, Room.CentralHub],
  [Room.Laboratory, Direction.West, Room.CentralHub],
  [Room.Engineering, Direction.North, Room.CentralHub],
  [Room.Bridge, Direction.Down, Room.CentralHub],
  [Room.CentralHub, Direction.West, Room.Galley],
  [Room.CentralHub, Direction.East, Room.Laboratory],
  [Room.Greenhouse, Direction.West, Room.Laboratory],
  [Room.CentralHub, Direction.South, Room.Engineering],
  [Room.ReactorRoom, Direction.West, Room.Engineering],
  [Room.Airlock, Direction.North, Room.Engineering],
  [Room.CentralHub, Direction.Up, Room.Bridge],
  [Room.CommsRoom, Direction.West, Room.Bridge],
  [Room.Laboratory, Direction.East, Room.Greenhouse],
  [Room.Engineering, Direction.East, Room.ReactorRoom],
  [Room.Engineering, Direction.South, Room.Airlock],
  [Room.DockingBay, Direction.North, Room.Airlock],
  [Room.Bridge, Direction.East, Room.CommsRoom],
  [Room.Airlock, Direction.South, Room.DockingBay],
];

/** Where `direction` leads from `room`, if anywhere. */
export function exit(room: Room, direction: Direction): Room | undefined {
  return EXITS.find(([from, way]) => from === room && way === direction)?.[2];
}
