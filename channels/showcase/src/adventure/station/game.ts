// One viewer's game of Station Zero: where they are, what they carry,
// which puzzles are solved, and what each typed command does
// (crates/teletext-showcase/src/adventure/station/game.rs).

import {
  DIRECTIONS,
  Direction,
  Fixture,
  ITEMS,
  Item,
  type Noun,
  Room,
  about,
  directionName,
  directionOf,
  exit,
  fixtureNoun,
  fixtureRoom,
  itemDescription,
  itemIndex,
  itemName,
  itemNoun,
  itemStart,
  nounOf,
} from "./world.ts";

/** The locker in the crew quarters is open. */
const LOCKER_OPEN = 1;
/** The laboratory door is open. */
const LAB_OPEN = 1 << 1;
/** The vending machine is prised open. */
const MACHINE_OPEN = 1 << 2;
/** The reactor has its fuse: the station has power. */
const POWER = 1 << 3;
/** The shuttle's docking clamps are released. */
const CLAMPS_RELEASED = 1 << 4;
/** The distress beacon is calling. */
const BEACON = 1 << 5;
/** The shuttle has launched. */
const WON = 1 << 6;

/** The longest command kept, in characters: what the command line shows. */
export const MAX_COMMAND = 34;
/** The most moves counted: the Rust game's u16 count, which saturates. */
const MOST_MOVES = 0xffff;

/** Words a command may carry that name nothing. */
const FILLERS: readonly string[] = ["the", "a", "an", "at", "to", "on", "with", "into", "in", "up", "around", "my"];

/** An item not in the game yet, or used up. */
export const NOWHERE = "nowhere";
/** An item in the player's hands. */
export const CARRIED = "carried";
/** Where an item is: nowhere, lying in a room, or carried. */
export type Place = typeof NOWHERE | typeof CARRIED | Room;

/** How a reply reads. */
export const Tone = {
  /** Information. */
  Plain: "plain",
  /** Something went right. */
  Good: "good",
  /** Something was refused. */
  Bad: "bad",
} as const;
export type Tone = (typeof Tone)[keyof typeof Tone];

/** What a command did, for the sound it makes. */
export const Event = {
  /** Nothing worth a sound. */
  Quiet: "quiet",
  /** The player moved. */
  Moved: "moved",
  /** Something was taken. */
  Took: "took",
  /** Something was dropped. */
  Dropped: "dropped",
  /** A puzzle was solved. */
  Solved: "solved",
  /** The command was refused. */
  Refused: "refused",
  /** The player was driven back by radiation. */
  Hurt: "hurt",
  /** The shuttle launched: the game is won. */
  Won: "won",
  /** A new game began. */
  Restarted: "restarted",
} as const;
export type Event = (typeof Event)[keyof typeof Event];

/** A command, parsed. `named` says whether any word but a filler was given. */
export type Command =
  /** Nothing typed. */
  | { readonly kind: "empty" }
  /** Look around. */
  | { readonly kind: "look" }
  /** Go somewhere, if the command said where. */
  | { readonly kind: "go"; readonly direction: Direction | undefined }
  /** Take a thing. */
  | { readonly kind: "take"; readonly noun: Noun | undefined; readonly named: boolean }
  /** Take everything here. */
  | { readonly kind: "takeAll" }
  /** Drop a thing. */
  | { readonly kind: "drop"; readonly noun: Noun | undefined; readonly named: boolean }
  /** Use things, in the order named. */
  | { readonly kind: "use"; readonly nouns: readonly Noun[]; readonly named: boolean }
  /** Open a thing. */
  | { readonly kind: "open"; readonly noun: Noun | undefined; readonly named: boolean }
  /** Examine a thing. */
  | { readonly kind: "examine"; readonly noun: Noun | undefined; readonly named: boolean }
  /** List what is carried. */
  | { readonly kind: "inventory" }
  /** List the commands. */
  | { readonly kind: "help" }
  /** A clue for the next step. */
  | { readonly kind: "hint" }
  /** Let time pass. */
  | { readonly kind: "wait" }
  /** Begin again. */
  | { readonly kind: "restart" }
  /** The magic word of another game. */
  | { readonly kind: "xyzzy" }
  /** Not a command. */
  | { readonly kind: "unknown" };

/** What Rust's `char::is_alphanumeric` takes: the Alphabetic property or a number. */
const ALPHANUMERIC = /^[\p{Alphabetic}\p{N}]$/u;
/** ASCII capitals, which `to_ascii_lowercase` lowers, and nothing else. */
const ASCII_CAPITAL = /[A-Z]/gu;

/** The words of a typed line: lower case, split at anything that is not a letter, a digit or a question mark. */
function wordsOf(line: string): string[] {
  const cleaned = [...line]
    .map((character) => (ALPHANUMERIC.test(character) || character === "?" ? character.replace(ASCII_CAPITAL, (capital) => capital.toLowerCase()) : " "))
    .join("");
  return cleaned.split(" ").filter((word) => word.length > 0);
}

/** Parses a typed line. */
export function parse(line: string): Command {
  const [verb, ...rest] = wordsOf(line);
  if (verb === undefined) {
    return { kind: "empty" };
  }
  const nouns = rest.map(nounOf).filter((noun) => noun !== undefined);
  const noun = nouns[0];
  const named = rest.some((word) => !FILLERS.includes(word));
  const every = rest.some((word) => word === "all" || word === "everything");
  switch (verb) {
    case "look":
    case "l":
      return named ? { kind: "examine", noun, named } : { kind: "look" };
    case "examine":
    case "x":
    case "inspect":
    case "read":
    case "check":
    case "search":
      return { kind: "examine", noun, named };
    case "go":
    case "walk":
    case "move":
    case "head":
    case "run":
    case "climb":
    case "crawl":
      return { kind: "go", direction: rest.map(directionOf).find((direction) => direction !== undefined) };
    case "take":
    case "get":
    case "grab":
    case "pick":
    case "collect":
      return every ? { kind: "takeAll" } : { kind: "take", noun, named };
    case "drop":
    case "put":
    case "leave":
    case "discard":
    case "throw":
      return { kind: "drop", noun, named };
    case "use":
    case "push":
    case "press":
    case "activate":
    case "insert":
    case "fit":
    case "pry":
    case "prise":
    case "prize":
    case "lever":
    case "swipe":
    case "switch":
    case "plug":
    case "install":
    case "drink":
    case "release":
      return { kind: "use", nouns, named };
    case "wear":
      return { kind: "use", nouns: [itemNoun(Item.Suit)], named: true };
    case "board":
    case "enter":
    case "launch":
    case "fly":
    case "escape":
      return { kind: "use", nouns: [fixtureNoun(Fixture.Shuttle)], named: true };
    case "open":
    case "unlock":
    case "force":
      return { kind: "open", noun, named };
    case "inventory":
    case "inv":
    case "i":
      return { kind: "inventory" };
    case "help":
    case "?":
    case "commands":
      return { kind: "help" };
    case "hint":
    case "clue":
      return { kind: "hint" };
    case "wait":
    case "z":
      return { kind: "wait" };
    case "restart":
    case "reset":
    case "again":
      return { kind: "restart" };
    case "xyzzy":
    case "plugh":
      return { kind: "xyzzy" };
    default: {
      const direction = directionOf(verb);
      return direction === undefined ? { kind: "unknown" } : { kind: "go", direction };
    }
  }
}

/** What a command answers: its reply, marked up, its tone, and its event. */
interface Outcome {
  readonly reply: string;
  readonly tone: Tone;
  readonly event: Event;
}

function plain(reply: string): Outcome {
  return { reply, tone: Tone.Plain, event: Event.Quiet };
}

function good(reply: string, event: Event): Outcome {
  return { reply, tone: Tone.Good, event };
}

function refused(reply: string): Outcome {
  return { reply, tone: Tone.Bad, event: Event.Refused };
}

/** The reply when a thing is not here. */
const NOT_HERE = "You see no such thing here.";
/** The help text. */
export const HELP = "{LOOK}, {N S E W U D}, {TAKE} x, {DROP} x, {USE} x, {OPEN} x, {EXAMINE} x, {INVENTORY}, {HINT}, {RESTART}. Type, then press Enter.";
/** The first reply of a new game. */
const WELCOME = "You wake, cold and alone. The station is failing. Find a way home. Type {HELP} for the commands.";

/** What Rust's `str::trim` trims: the Unicode White_Space property, which JavaScript's `trim` differs from in U+0085 and U+FEFF. */
const WHITE_SPACE = /^[\t-\r \u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]+|[\t-\r \u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]+$/gu;

/** One viewer's game. */
export class Game {
  #room: Room = Room.CryoBay;
  #places: Place[] = ITEMS.map((item) => itemStart(item) ?? NOWHERE);
  #flags = 0;
  #moves = 0;
  #said = "";
  #reply = WELCOME;
  #tone: Tone = Tone.Plain;

  /** A copy, played on apart from this one. */
  clone(): Game {
    const copy = new Game();
    copy.#room = this.#room;
    copy.#places = [...this.#places];
    copy.#flags = this.#flags;
    copy.#moves = this.#moves;
    copy.#said = this.#said;
    copy.#reply = this.#reply;
    copy.#tone = this.#tone;
    return copy;
  }

  /** The room the player is in. */
  room(): Room {
    return this.#room;
  }

  /** The commands acted on. */
  moves(): number {
    return this.#moves;
  }

  /** The last command, as typed. */
  said(): string {
    return this.#said;
  }

  /** The reply to it, marked up. */
  reply(): string {
    return this.#reply;
  }

  /** How the reply reads. */
  tone(): Tone {
    return this.#tone;
  }

  /** Whether the shuttle has launched. */
  won(): boolean {
    return this.#flag(WON);
  }

  /** Whether the beacon was sent. */
  beacon(): boolean {
    return this.#flag(BEACON);
  }

  #flag(flag: number): boolean {
    return (this.#flags & flag) !== 0;
  }

  #set(flag: number): void {
    this.#flags |= flag;
  }

  /** Where `item` is. */
  place(item: Item): Place {
    return this.#places[itemIndex(item)] ?? NOWHERE;
  }

  #put(item: Item, place: Place): void {
    const index = itemIndex(item);
    if (index >= 0 && index < this.#places.length) {
      this.#places[index] = place;
    }
  }

  /** Whether the player carries `item`. */
  carrying(item: Item): boolean {
    return this.place(item) === CARRIED;
  }

  /** The items carried. */
  carried(): Item[] {
    return ITEMS.filter((item) => this.carrying(item));
  }

  /** The items lying in the player's room. */
  here(): Item[] {
    return ITEMS.filter((item) => this.place(item) === this.#room);
  }

  /** The directions out of the player's room. */
  exits(): Direction[] {
    return DIRECTIONS.filter((direction) => exit(this.#room, direction) !== undefined);
  }

  /** The player's room as it is now: its description and what has changed in it, marked up. */
  description(): string {
    const text = about(this.#room).description;
    const extra = this.#extra();
    return extra.length === 0 ? text : `${text} ${extra}`;
  }

  /** What has changed in the player's room, marked up, or nothing. */
  #extra(): string {
    switch (this.#room) {
      case Room.CryoBay:
        return this.#flag(POWER) ? "The lights burn steady and white now." : "Red emergency lights pulse slowly.";
      case Room.CrewQuarters:
        return this.#flag(LOCKER_OPEN) ? "Its door hangs open." : "Its door is held shut by a bent latch.";
      case Room.CentralHub: {
        const door = this.#flag(LAB_OPEN) ? "The laboratory door stands open." : "The laboratory door is sealed; its reader blinks red.";
        const lift = this.#flag(POWER) ? "The lift glows green." : "The lift's light is dark.";
        return `${door} ${lift}`;
      }
      case Room.Galley:
        return this.#flag(MACHINE_OPEN) ? "Its door has been prised off." : "Behind its cracked glass, something glows blue.";
      case Room.ReactorRoom:
        return this.#flag(POWER) ? "A new fuse glows in its socket." : "One socket is empty, scorched black.";
      case Room.Bridge:
        if (this.#flag(CLAMPS_RELEASED)) {
          return "The console reads {CLAMPS RELEASED}.";
        }
        return this.#flag(POWER) ? "The console's screen flickers awake." : "";
      case Room.CommsRoom:
        return this.#flag(BEACON) ? "The beacon blinks, calling for help." : "";
      case Room.DockingBay:
        return this.#flag(CLAMPS_RELEASED) ? "The docking clamps stand open." : "Four massive docking clamps hold it fast.";
      default:
        return "";
    }
  }

  /** The ending, marked up. */
  ending(): string {
    return this.#flag(BEACON)
      ? "The shuttle falls away from Station Zero, and the stars turn around you. Ahead, a rescue ship answers your {beacon}. You are going home."
      : "The shuttle falls away from Station Zero, and the stars turn around you. No one knows you are coming, but the shuttle knows the way {home}.";
  }

  /** Acts on a typed line and remembers it and the reply. */
  command(line: string): Event {
    const command = parse(line);
    if (command.kind === "empty") {
      return Event.Quiet;
    }
    const said = [...line.replace(WHITE_SPACE, "")].slice(0, MAX_COMMAND).join("");
    if (command.kind === "restart") {
      const fresh = new Game();
      this.#room = fresh.#room;
      this.#places = fresh.#places;
      this.#flags = fresh.#flags;
      this.#moves = fresh.#moves;
      this.#tone = fresh.#tone;
      this.#said = said;
      this.#reply = "A new game. You wake again in the cryo bay, cold and alone.";
      return Event.Restarted;
    }
    if (this.won()) {
      // The moves stay as they were at the launch.
      this.#said = said;
      this.#reply = command.kind === "help" ? HELP : "The shuttle is on its way home. Type {RESTART} to play again.";
      this.#tone = Tone.Plain;
      return Event.Quiet;
    }
    const outcome = this.#act(command);
    this.#moves = Math.min(MOST_MOVES, this.#moves + 1);
    this.#said = said;
    this.#reply = outcome.reply;
    this.#tone = outcome.tone;
    return outcome.event;
  }

  #act(command: Command): Outcome {
    switch (command.kind) {
      case "empty":
      case "restart":
      case "look":
        return plain("You take a careful look around.");
      case "go":
        return this.#go(command.direction);
      case "take":
        return this.#take(command.noun, command.named);
      case "takeAll":
        return this.#takeAll();
      case "drop":
        return this.#drop(command.noun, command.named);
      case "use":
        return this.#useThings(command.nouns, command.named);
      case "open":
        return this.#open(command.noun, command.named);
      case "examine":
        return this.#examine(command.noun, command.named);
      case "inventory":
        return this.#inventory();
      case "help":
        return plain(HELP);
      case "hint":
        return plain(this.#hint());
      case "wait":
        return plain("Time passes. The stars wheel by the windows.");
      case "xyzzy":
        return plain("A hollow voice says: {wrong game}.");
      case "unknown":
        return refused("That is not a command here. Type {HELP} for the commands.");
    }
  }

  /** Why `direction` from the player's room is closed, if it is. */
  #blocked(direction: Direction): Outcome | undefined {
    const bad = (reply: string, event: Event): Outcome => ({ reply, tone: Tone.Bad, event });
    if (this.#room === Room.CentralHub && direction === Direction.East && !this.#flag(LAB_OPEN)) {
      return bad("The laboratory door is sealed. Its card reader blinks red.", Event.Refused);
    }
    if (this.#room === Room.CentralHub && direction === Direction.Up && !this.#flag(POWER)) {
      return bad("You press the lift button. Nothing. The lift has no power.", Event.Refused);
    }
    if (this.#room === Room.Engineering && direction === Direction.East && !this.carrying(Item.Suit)) {
      return bad("Radiation alarms scream as the hatch opens. You back out fast: you need protection.", Event.Hurt);
    }
    if (this.#room === Room.Airlock && direction === Direction.South && !this.#flag(POWER)) {
      return bad("The outer door will not cycle without power.", Event.Refused);
    }
    return undefined;
  }

  #go(direction: Direction | undefined): Outcome {
    if (direction === undefined) {
      return refused("Go where? North, south, east, west, up or down.");
    }
    const to = exit(this.#room, direction);
    if (to === undefined) {
      return refused("You cannot go that way.");
    }
    const blocked = this.#blocked(direction);
    if (blocked !== undefined) {
      return blocked;
    }
    this.#room = to;
    let reply = `You go {${directionName(direction)}}.`;
    if (to === Room.ReactorRoom) {
      reply += " The suit's counter clicks madly.";
    }
    return { reply, tone: Tone.Plain, event: Event.Moved };
  }

  /** Whether `item` is carried or here. */
  #reachable(item: Item): boolean {
    return this.carrying(item) || this.place(item) === this.#room;
  }

  /** Whether `fixture` is in the player's room. */
  #present(fixture: Fixture): boolean {
    return fixtureRoom(fixture) === this.#room;
  }

  #take(noun: Noun | undefined, named: boolean): Outcome {
    if (noun === undefined) {
      return refused(named ? NOT_HERE : "Take what?");
    }
    if (noun.kind === "item" && this.carrying(noun.item)) {
      return plain("You already have it.");
    }
    if (noun.kind === "item" && this.place(noun.item) === this.#room) {
      this.#put(noun.item, CARRIED);
      return good(`Taken: {${itemName(noun.item)}}.`, Event.Took);
    }
    if (noun.kind === "fixture" && this.#present(noun.fixture)) {
      return refused("It is fixed in place.");
    }
    return refused(NOT_HERE);
  }

  #takeAll(): Outcome {
    const here = this.here();
    if (here.length === 0) {
      return refused("There is nothing here to take.");
    }
    const names = here.map(itemName);
    for (const item of here) {
      this.#put(item, CARRIED);
    }
    return good(`Taken: {${names.join(", ")}}.`, Event.Took);
  }

  #drop(noun: Noun | undefined, named: boolean): Outcome {
    if (noun === undefined && !named) {
      return refused("Drop what?");
    }
    if (noun?.kind === "item" && this.carrying(noun.item)) {
      this.#put(noun.item, this.#room);
      return { reply: `Dropped: {${itemName(noun.item)}}.`, tone: Tone.Plain, event: Event.Dropped };
    }
    return refused("You are not carrying that.");
  }

  #inventory(): Outcome {
    const carried = this.carried();
    if (carried.length === 0) {
      return plain("You are carrying nothing but a hospital gown.");
    }
    return plain(`You carry: {${carried.map(itemName).join(", ")}}.`);
  }

  /** A puzzle `noun` solves in the player's room, if it is one. */
  #puzzle(noun: Noun): Outcome | undefined {
    const names = (item: Item, fixture: Fixture): boolean =>
      (noun.kind === "item" && noun.item === item) || (noun.kind === "fixture" && noun.fixture === fixture);
    const isFixture = (fixture: Fixture): boolean => noun.kind === "fixture" && noun.fixture === fixture;
    switch (this.#room) {
      case Room.CentralHub:
        if (!names(Item.Keycard, Fixture.Door)) {
          return undefined;
        }
        if (this.#flag(LAB_OPEN)) {
          return plain("The laboratory door is already open.");
        }
        if (this.carrying(Item.Keycard)) {
          this.#set(LAB_OPEN);
          return good("You swipe the keycard. The reader blinks {green}, and the laboratory door slides open.", Event.Solved);
        }
        return refused("The card reader wants a keycard.");
      case Room.Galley:
        if (!names(Item.Crowbar, Fixture.Machine)) {
          return undefined;
        }
        if (this.#flag(MACHINE_OPEN)) {
          return plain("The machine is already open, and empty.");
        }
        if (this.carrying(Item.Crowbar)) {
          this.#set(MACHINE_OPEN);
          this.#put(Item.Fuse, Room.Galley);
          return good("You lever the crowbar in and heave. Glass rains down, and a {reactor fuse} drops out.", Event.Solved);
        }
        return refused("The machine's door is jammed. You need something to lever it open.");
      case Room.ReactorRoom:
        if (!names(Item.Fuse, Fixture.Panel)) {
          return undefined;
        }
        if (this.#flag(POWER)) {
          return plain("The panel hums happily. The power is on.");
        }
        if (this.carrying(Item.Fuse)) {
          this.#set(POWER);
          this.#put(Item.Fuse, NOWHERE);
          return good("You push the fuse home. With a deep {WHUMMM} the lights come up across the station.", Event.Solved);
        }
        return refused("The empty socket needs a fuse.");
      case Room.Bridge:
        if (!isFixture(Fixture.Console)) {
          return undefined;
        }
        if (this.#flag(CLAMPS_RELEASED)) {
          return plain("The clamps are already released.");
        }
        this.#set(CLAMPS_RELEASED);
        return good("You wipe off the dust and key the release. Far below, the {docking clamps} let go with a clang.", Event.Solved);
      case Room.CommsRoom:
        if (!isFixture(Fixture.Radio)) {
          return undefined;
        }
        if (this.#flag(BEACON)) {
          return plain("The beacon is already calling.");
        }
        this.#set(BEACON);
        return good("You press the big red button. The beacon starts to sing into the dark: {someone will hear}.", Event.Solved);
      case Room.DockingBay:
        if (!isFixture(Fixture.Shuttle)) {
          return undefined;
        }
        if (this.#flag(CLAMPS_RELEASED)) {
          this.#set(WON);
          return good("You strap in and punch {LAUNCH}. The shuttle leaps out of the bay. You made it!", Event.Won);
        }
        return refused("You climb in, but the {docking clamps} hold the shuttle fast. They are worked from the bridge.");
      default:
        return undefined;
    }
  }

  #useThings(nouns: readonly Noun[], named: boolean): Outcome {
    for (const noun of nouns) {
      const outcome = this.#puzzle(noun);
      if (outcome !== undefined) {
        return outcome;
      }
    }
    const noun = nouns[0];
    if (noun === undefined) {
      return refused(named ? NOT_HERE : "Use what?");
    }
    if (noun.kind === "item") {
      return this.#reachable(noun.item) ? plain(this.#used(noun.item)) : refused("You do not have that.");
    }
    if (!this.#present(noun.fixture)) {
      return refused(NOT_HERE);
    }
    switch (noun.fixture) {
      case Fixture.Pod:
        return plain("You are not getting back in there.");
      case Fixture.Locker:
        return this.#open(fixtureNoun(Fixture.Locker), true);
      default:
        return plain("Nothing happens.");
    }
  }

  /** What using `item`, at hand, says where no puzzle takes it. */
  #used(item: Item): string {
    switch (item) {
      case Item.Keycard:
        return "Nothing here reads a keycard.";
      case Item.Crowbar:
        return "Nothing here needs prising open.";
      case Item.Fuse:
        return "Nothing here takes a fuse.";
      case Item.Suit:
        return "You check the seals. The suit will keep the radiation out.";
      case Item.Datapad:
        return itemDescription(item);
      case Item.Mug:
        return "You sip the cold coffee. It is terrible. You feel braver anyway.";
    }
  }

  #open(noun: Noun | undefined, named: boolean): Outcome {
    if (noun === undefined) {
      return refused(named ? NOT_HERE : "Open what?");
    }
    if (noun.kind === "item") {
      return refused(this.#reachable(noun.item) ? "It does not open." : NOT_HERE);
    }
    if (!this.#present(noun.fixture)) {
      return refused(NOT_HERE);
    }
    switch (noun.fixture) {
      case Fixture.Locker:
        if (this.#flag(LOCKER_OPEN)) {
          return plain("The locker is already open.");
        }
        this.#set(LOCKER_OPEN);
        this.#put(Item.Keycard, Room.CrewQuarters);
        return good("You force the bent latch. The locker swings open, and a {keycard} falls out.", Event.Solved);
      case Fixture.Pod:
        return plain("Your pod is already open. You are not getting back in.");
      case Fixture.Machine:
      case Fixture.Door:
      case Fixture.Shuttle:
        return this.#puzzle(noun) ?? refused("It will not open.");
      default:
        return refused("It does not open.");
    }
  }

  #examine(noun: Noun | undefined, named: boolean): Outcome {
    if (noun === undefined) {
      return refused(named ? NOT_HERE : "Examine what?");
    }
    if (noun.kind === "item" && this.#reachable(noun.item)) {
      return plain(itemDescription(noun.item));
    }
    if (noun.kind === "fixture" && this.#present(noun.fixture)) {
      return plain(this.#fixtureDescription(noun.fixture));
    }
    return refused(NOT_HERE);
  }

  #fixtureDescription(fixture: Fixture): string {
    switch (fixture) {
      case Fixture.Pod:
        return "Your pod, still cold. Its display reads {SLEEPER 12 REVIVED. CREW EVACUATED.}";
      case Fixture.Locker:
        return this.#flag(LOCKER_OPEN)
          ? "The locker hangs open. Only a sock remains."
          : "A dented crew locker. The latch is bent, but you could force it {open}.";
      case Fixture.Machine:
        return this.#flag(MACHINE_OPEN)
          ? "Smashed open and empty, apart from a very old sandwich."
          : "Wedged behind the cracked glass, between snack bars, a {fuse} glows blue. The door is jammed.";
      case Fixture.Door:
        return this.#flag(LAB_OPEN) ? "The laboratory door stands open." : "Sealed. The card reader beside it wants a {keycard}.";
      case Fixture.Panel:
        return this.#flag(POWER) ? "The new fuse glows steadily in its socket." : "One socket in the panel is empty and scorched. It needs a {fuse}.";
      case Fixture.Console:
        return this.#flag(CLAMPS_RELEASED) ? "The console reads {CLAMPS RELEASED}." : "A dusty console. One big key reads {RELEASE CLAMPS}.";
      case Fixture.Radio:
        return this.#flag(BEACON) ? "The beacon is calling. Someone will hear." : "The distress beacon. A big red button waits to be {pressed}.";
      case Fixture.Shuttle:
        return this.#flag(CLAMPS_RELEASED)
          ? "Free of its clamps and ready to fly. {BOARD} it!"
          : "A two-seat escape shuttle, fuelled, held fast by its docking clamps.";
    }
  }

  /** A clue for the next step, by how far the player has got. */
  #hint(): string {
    if (!this.#flag(LOCKER_OPEN)) {
      return "The locker in the crew quarters looks worth opening.";
    }
    if (!this.#flag(LAB_OPEN)) {
      return "That keycard must open something. The card reader in the hub?";
    }
    if (!this.#flag(MACHINE_OPEN) && !this.carrying(Item.Crowbar)) {
      return "Something in the greenhouse could prise things open.";
    }
    if (!this.#flag(MACHINE_OPEN)) {
      return "The galley's vending machine is hiding something.";
    }
    if (!this.#flag(POWER) && !this.carrying(Item.Suit)) {
      return "The reactor room is deadly without protection. Try the medical bay.";
    }
    if (!this.#flag(POWER)) {
      return "Fit the fuse into the panel in the reactor room.";
    }
    if (!this.#flag(CLAMPS_RELEASED)) {
      return "The lift works now. Release the clamps from the bridge.";
    }
    return "Board the shuttle in the docking bay. The beacon in comms is up to you.";
  }
}
