// The Lost Crown's story: every node one view of the story's page, every
// choice a coloured key leading to another node. The story keeps no state
// of its own; what the reader carries is where the path has been, so a
// node reached only through the wise woman's charm or the cellar's sword
// knows the reader holds silver
// (crates/teletext-showcase/src/adventure/crown/story.rs).

import type { Art } from "../art.ts";
import { MENU } from "../pages.ts";
import {
  BOAT,
  CASTLE,
  CAVE,
  CROWN,
  DICE,
  DRAGON,
  EYE,
  FIRE,
  FOREST,
  GATE,
  GHOST,
  HOARD,
  INN,
  MAP,
  RIVER,
  SIGNPOST,
  SKULL,
  SNAKE_EYES,
  SWORD,
  WOLF,
} from "./pictures.ts";

/** How a node ends the story, if it does. */
export const Ending = {
  /** The story goes on. */
  Not: "not",
  /** The crown is won. */
  Won: "won",
  /** The quest failed. */
  Lost: "lost",
  /** Neither: a life without the crown. */
  Other: "other",
} as const;
export type Ending = (typeof Ending)[keyof typeof Ending];

/** A node of the story, by its place in the telling. */
export type Id = number;

/** Where a choice leads: on in the story, to another node drawn on the same page, or out of it to another page of the channel. */
export type Leads = { readonly node: Id } | { readonly page: number };

/** A choice: its Fastext label, what it says on the page, and where it leads. */
export interface Choice {
  /** The label on row 24: at most nine characters. */
  readonly label: string;
  /** The choice in words. */
  readonly text: string;
  /** Where it leads. */
  readonly to: Leads;
}

/** A node of the story: what the page shows at one point of it. */
export interface Node {
  /** Which node it is. */
  readonly id: Id;
  /** Its title. */
  readonly title: string;
  /** What happens, marked up. */
  readonly text: string;
  /** Its illustration. */
  readonly picture: Art;
  /** The choices, at most four: red, green, yellow, cyan. */
  readonly choices: readonly Choice[];
  /** Whether it ends the story. */
  readonly ending: Ending;
}

/** The story's first node. */
export const CROSSROADS: Id = 0;
const FOREST_OF_WYR: Id = 1;
const RIVER_KELL: Id = 2;
const BLACK_GOAT: Id = 3;
const WISE_WOMAN: Id = 4;
const CROWN_LORE: Id = 5;
const WOLVES: Id = 6;
const TALL_PINE: Id = 7;
const LOST_IN_WYR: Id = 8;
const ROPE_BRIDGE: Id = 9;
const FERRYMAN: Id = 10;
const STRANGERS_MAP: Id = 11;
const DICE_GAME: Id = 12;
const ONE_ROLL_TOO_MANY: Id = 13;
const INNKEEPER: Id = 14;
const HIDDEN_CAVE: Id = 15;
const CASTLE_GATE: Id = 16;
const CASTLE_WALLS: Id = 17;
const SECRET_DOOR: Id = 18;
const CELLARS: Id = 19;
const KINGS_TOMB: Id = 20;
const GREAT_HALL: Id = 21;
const TIPTOE: Id = 22;
const DRAGONS_LAIR: Id = 23;
const DUNGEON: Id = 24;
const DRAGON_FIRE: Id = 25;
const NARROW_ESCAPE: Id = 26;
const CROWNED: Id = 27;
const DRAGON_GOLD: Id = 28;

/** The two choices at every ending. */
const AFTER_THE_END: readonly Choice[] = [
  { label: "Again", text: "Begin a new quest", to: { node: CROSSROADS } },
  { label: "Menu", text: "Back to the adventures", to: { page: MENU } },
];

/** Every node, in the order of their ids. */
export const NODES: readonly Node[] = [
  {
    id: CROSSROADS,
    title: "The Crossroads",
    text: "Dawn breaks over the moor. Three roads meet beneath a leaning signpost. Beyond them lies the ruined castle of Kell, and in it the {lost crown} that no thief has ever come back with.",
    picture: SIGNPOST,
    choices: [
      { label: "Forest", text: "Take the north road, to the forest", to: { node: FOREST_OF_WYR } },
      { label: "River", text: "Take the east road, to the river", to: { node: RIVER_KELL } },
      { label: "Inn", text: "Take the west road, to the inn", to: { node: BLACK_GOAT } },
    ],
    ending: Ending.Not,
  },
  {
    id: FOREST_OF_WYR,
    title: "Forest of Wyr",
    text: "The trees close over you until the day is a green dusk. Smoke curls from a fire where an {old woman} sits, beckoning. Beside the path, fresh {wolf tracks} lead deeper in.",
    picture: FOREST,
    choices: [
      { label: "Woman", text: "Sit by the old woman's fire", to: { node: WISE_WOMAN } },
      { label: "Tracks", text: "Follow the wolf tracks", to: { node: WOLVES } },
      { label: "Back", text: "Return to the crossroads", to: { node: CROSSROADS } },
    ],
    ending: Ending.Not,
  },
  {
    id: RIVER_KELL,
    title: "The River Kell",
    text: "The road ends at a roaring river. A rotten {rope bridge} sways across it. Downstream a {ferryman} leans on his pole, a lantern at his feet, watching you without blinking.",
    picture: RIVER,
    choices: [
      { label: "Bridge", text: "Cross the rope bridge", to: { node: ROPE_BRIDGE } },
      { label: "Ferry", text: "Pay the ferryman your coin", to: { node: FERRYMAN } },
      { label: "Back", text: "Return to the crossroads", to: { node: CROSSROADS } },
    ],
    ending: Ending.Not,
  },
  {
    id: BLACK_GOAT,
    title: "The Black Goat",
    text: "The inn is warm and loud. A hooded stranger slides a {map} across the table: 'Your last coin, and it is yours.' In the corner, sailors play {dice} for silver.",
    picture: INN,
    choices: [
      { label: "Map", text: "Buy the stranger's map", to: { node: STRANGERS_MAP } },
      { label: "Dice", text: "Join the game of dice", to: { node: DICE_GAME } },
      { label: "Leave", text: "Go back to the crossroads", to: { node: CROSSROADS } },
    ],
    ending: Ending.Not,
  },
  {
    id: WISE_WOMAN,
    title: "The Wise Woman",
    text: "'You seek the crown,' she says, before you speak. She presses a {silver charm} into your hand. 'Dragon fire fears silver. And there is a cave that runs beneath the castle.'",
    picture: FIRE,
    choices: [
      { label: "Cave", text: "Go to the hidden cave", to: { node: HIDDEN_CAVE } },
      { label: "Ask", text: "Ask her about the crown", to: { node: CROWN_LORE } },
    ],
    ending: Ending.Not,
  },
  {
    id: CROWN_LORE,
    title: "The Crown of Kell",
    text: "'The last king hid it from the dragon {Morvath},' she says, 'and the dragon sleeps on it still, above the king's own tomb. Go by the cave, and keep the charm close.'",
    picture: CROWN,
    choices: [{ label: "Cave", text: "Go to the hidden cave", to: { node: HIDDEN_CAVE } }],
    ending: Ending.Not,
  },
  {
    id: WOLVES,
    title: "The Wolves",
    text: "The tracks lead into a clearing, and the clearing is full of {wolves}. Yellow eyes turn towards you, one pair after another. The nearest wolf lowers its head and growls.",
    picture: WOLF,
    choices: [
      { label: "Climb", text: "Climb the nearest tree", to: { node: TALL_PINE } },
      { label: "Run", text: "Run for it", to: { node: LOST_IN_WYR } },
    ],
    ending: Ending.Not,
  },
  {
    id: TALL_PINE,
    title: "The Tall Pine",
    text: "You scramble up as jaws snap at your boots. From the top you see the river, and beyond it the {castle of Kell}, black against the sky. By noon the wolves give up and slink away.",
    picture: CASTLE,
    choices: [{ label: "Castle", text: "Climb down and make for the castle", to: { node: CASTLE_WALLS } }],
    ending: Ending.Not,
  },
  {
    id: LOST_IN_WYR,
    title: "Lost in Wyr",
    text: "You run until you cannot run, and then you are lost. The forest of Wyr keeps its travellers. They say a ragged figure wanders there still, asking the way to {Kell}.",
    picture: SKULL,
    choices: AFTER_THE_END,
    ending: Ending.Lost,
  },
  {
    id: ROPE_BRIDGE,
    title: "The Rope Bridge",
    text: "Halfway across, a rope parts with a crack like a whip. The river takes you, roaring, and throws you out miles away on a muddy bank, without your boots and without your {courage}.",
    picture: RIVER,
    choices: AFTER_THE_END,
    ending: Ending.Lost,
  },
  {
    id: FERRYMAN,
    title: "The Ferryman",
    text: "He takes your coin without a word and poles you into the mist. 'Few come back from Kell,' he says at last, and sets you ashore beneath the walls. Above you, the {gate} is shut.",
    picture: BOAT,
    choices: [
      { label: "Gate", text: "Go up to the castle gate", to: { node: CASTLE_GATE } },
      { label: "Walls", text: "Climb the ivy on the walls", to: { node: CASTLE_WALLS } },
    ],
    ending: Ending.Not,
  },
  {
    id: STRANGERS_MAP,
    title: "The Stranger's Map",
    text: "The map is old and good. It shows a {secret door} at the foot of the east tower, hidden by brambles, and the ferry that crosses to the castle. When you look up, the stranger is gone.",
    picture: MAP,
    choices: [
      { label: "Door", text: "Find the secret door", to: { node: SECRET_DOOR } },
      { label: "River", text: "Make for the river and the ferry", to: { node: RIVER_KELL } },
    ],
    ending: Ending.Not,
  },
  {
    id: DICE_GAME,
    title: "The Dice Game",
    text: "Luck is with you. Roll after roll, the sailors' silver piles up in front of you, until they glare at you across the table. You have won a {small fortune}.",
    picture: DICE,
    choices: [
      { label: "Again", text: "Roll once more, for everything", to: { node: ONE_ROLL_TOO_MANY } },
      { label: "Stop", text: "Quit while you are ahead", to: { node: INNKEEPER } },
    ],
    ending: Ending.Not,
  },
  {
    id: ONE_ROLL_TOO_MANY,
    title: "One Roll Too Many",
    text: "{Snake eyes.} You lose everything, down to your boots, and the sailors throw you out into the road with a cheer. Barefoot, you limp back the way you came.",
    picture: SNAKE_EYES,
    choices: [{ label: "Onward", text: "Start again from the crossroads", to: { node: CROSSROADS } }],
    ending: Ending.Not,
  },
  {
    id: INNKEEPER,
    title: "The Innkeeper",
    text: "With your winnings you buy the Black Goat itself. You never find the crown, but every evening you tell the story of the night you {nearly went looking for it}.",
    picture: INN,
    choices: AFTER_THE_END,
    ending: Ending.Other,
  },
  {
    id: HIDDEN_CAVE,
    title: "The Hidden Cave",
    text: "Behind a waterfall, just where she said, a cave mouth opens. Glowing {mushrooms} light a tunnel that runs down and down, under the river, towards the castle.",
    picture: CAVE,
    choices: [{ label: "Tunnel", text: "Follow the tunnel", to: { node: KINGS_TOMB } }],
    ending: Ending.Not,
  },
  {
    id: CASTLE_GATE,
    title: "The Castle Gate",
    text: "The portcullis is down. Carved above it: {What eats all things, and is never full?} Below are two levers, one marked with an hourglass and one with a flame.",
    picture: GATE,
    choices: [
      { label: "Hourglass", text: "Pull the lever of the hourglass", to: { node: GREAT_HALL } },
      { label: "Flame", text: "Pull the lever of the flame", to: { node: DUNGEON } },
      { label: "Walls", text: "Climb the walls instead", to: { node: CASTLE_WALLS } },
    ],
    ending: Ending.Not,
  },
  {
    id: CASTLE_WALLS,
    title: "The Castle Walls",
    text: "Ivy as thick as rope climbs the wall. You climb with it, stone by stone, and drop over the battlements into a {courtyard} of dead roses. A great door stands ajar.",
    picture: CASTLE,
    choices: [{ label: "Door", text: "Go through the great door", to: { node: GREAT_HALL } }],
    ending: Ending.Not,
  },
  {
    id: SECRET_DOOR,
    title: "The Secret Door",
    text: "Behind the brambles, as the map promised, is a small iron door. Inside, a stair winds up towards the halls, and down into the castle's {cellars}.",
    picture: GATE,
    choices: [
      { label: "Up", text: "Climb the stair to the halls", to: { node: GREAT_HALL } },
      { label: "Down", text: "Search the cellars", to: { node: CELLARS } },
    ],
    ending: Ending.Not,
  },
  {
    id: CELLARS,
    title: "The Cellars",
    text: "Among the old barrels you find a {silver sword}, bright as the day it was forged, and behind them a passage that leads to a tomb.",
    picture: SWORD,
    choices: [{ label: "Passage", text: "Take the sword, follow the passage", to: { node: KINGS_TOMB } }],
    ending: Ending.Not,
  },
  {
    id: KINGS_TOMB,
    title: "The King's Tomb",
    text: "A pale {ghost} rises from the tomb of the last king of Kell. 'You carry silver,' it whispers. 'Good. My crown lies above, beneath the dragon. Be brave.' A stair leads up.",
    picture: GHOST,
    choices: [{ label: "Up", text: "Climb to the dragon's lair", to: { node: DRAGONS_LAIR } }],
    ending: Ending.Not,
  },
  {
    id: GREAT_HALL,
    title: "The Great Hall",
    text: "Gold lies in drifts across the hall, and on the gold sleeps {Morvath} the dragon, smoke curling from its nostrils. On a pillar above its head, the lost crown glitters.",
    picture: HOARD,
    choices: [
      { label: "Sneak", text: "Creep past and take the crown", to: { node: TIPTOE } },
      { label: "Fight", text: "Draw your knife and attack", to: { node: DRAGON_FIRE } },
      { label: "Flee", text: "Run back the way you came", to: { node: CROSSROADS } },
    ],
    ending: Ending.Not,
  },
  {
    id: TIPTOE,
    title: "Tiptoe",
    text: "Coin by coin you creep across the gold. You climb the pillar. Your fingers close on the {crown}... and one great yellow eye opens, and looks at you.",
    picture: EYE,
    choices: [
      { label: "Run", text: "Run for the door", to: { node: NARROW_ESCAPE } },
      { label: "Freeze", text: "Stay perfectly still", to: { node: DRAGON_FIRE } },
    ],
    ending: Ending.Not,
  },
  {
    id: DRAGONS_LAIR,
    title: "The Dragon's Lair",
    text: "Morvath wakes and roars, and a river of fire pours over you... and parts around you, turned aside by your {silver}. The dragon cowers. The crown is yours to take.",
    picture: DRAGON,
    choices: [
      { label: "Crown", text: "Take the crown", to: { node: CROWNED } },
      { label: "Gold", text: "Fill your pockets with gold", to: { node: DRAGON_GOLD } },
    ],
    ending: Ending.Not,
  },
  {
    id: DUNGEON,
    title: "The Dungeon",
    text: "The floor drops away, and you land in a dark dungeon. Far above, the trapdoor thuds shut. The answer was {time}, and now you have plenty of it.",
    picture: GATE,
    choices: AFTER_THE_END,
    ending: Ending.Lost,
  },
  {
    id: DRAGON_FIRE,
    title: "Dragon Fire",
    text: "Morvath's breath fills the hall with fire. There is a flash, a roar, and then nothing at all. Somewhere a bard begins a song about a brave and {very foolish} thief.",
    picture: SKULL,
    choices: AFTER_THE_END,
    ending: Ending.Lost,
  },
  {
    id: NARROW_ESCAPE,
    title: "A Narrow Escape",
    text: "You run, the crown in your fist, fire roaring at your heels, and dive through the door as the hall burns. Singed and smoking, you carry the {crown of Kell} home.",
    picture: CROWN,
    choices: AFTER_THE_END,
    ending: Ending.Won,
  },
  {
    id: CROWNED,
    title: "Long Live the Crown",
    text: "You lift the crown, and the dragon bows its head and flies north, never to return. The people of Kell crown you their ruler, and a little {silver} hangs above your throne.",
    picture: CROWN,
    choices: AFTER_THE_END,
    ending: Ending.Won,
  },
  {
    id: DRAGON_GOLD,
    title: "Dragon Gold",
    text: "You fill your pockets, then your boots, then your hat. The more gold you hold, the heavier you feel, and the {scalier}. By morning a new dragon sleeps on the hoard of Kell.",
    picture: DRAGON,
    choices: AFTER_THE_END,
    ending: Ending.Lost,
  },
];
