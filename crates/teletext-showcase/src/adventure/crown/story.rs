//! The Lost Crown's story: every node a page, every choice a Fastext
//! link to another node. The story keeps no state of its own; what the
//! reader carries is where the path has been, so a node reached only
//! through the wise woman's charm or the cellar's sword knows the reader
//! holds silver.

use super::super::MENU;
use super::super::art::Art;
use super::pictures::{
    BOAT, CASTLE, CAVE, CROWN, DICE, DRAGON, EYE, FIRE, FOREST, GATE, GHOST, HOARD, INN, MAP, RIVER, SIGNPOST, SKULL, SNAKE_EYES, SWORD, WOLF,
};

/// How a node ends the story, if it does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ending {
    /// The story goes on.
    Not,
    /// The crown is won.
    Won,
    /// The quest failed.
    Lost,
    /// Neither: a life without the crown.
    Other,
}

/// A choice: its Fastext label, what it says on the page, and the node it
/// leads to.
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    /// The label on row 24: at most nine characters.
    pub label: &'static str,
    /// The choice in words.
    pub text: &'static str,
    /// The page it leads to.
    pub to: u16,
}

/// A node of the story: one page.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// Its page.
    pub page: u16,
    /// Its title.
    pub title: &'static str,
    /// What happens, marked up.
    pub text: &'static str,
    /// Its illustration.
    pub picture: &'static Art,
    /// The choices, at most four: red, green, yellow, cyan.
    pub choices: &'static [Choice],
    /// Whether it ends the story.
    pub ending: Ending,
}

/// The story's first page.
pub const CROSSROADS: u16 = 0x410;
const FOREST_OF_WYR: u16 = 0x411;
const RIVER_KELL: u16 = 0x412;
const BLACK_GOAT: u16 = 0x413;
const WISE_WOMAN: u16 = 0x414;
const CROWN_LORE: u16 = 0x415;
const WOLVES: u16 = 0x416;
const TALL_PINE: u16 = 0x417;
const LOST_IN_WYR: u16 = 0x418;
const ROPE_BRIDGE: u16 = 0x419;
const FERRYMAN: u16 = 0x420;
const STRANGERS_MAP: u16 = 0x421;
const DICE_GAME: u16 = 0x422;
const ONE_ROLL_TOO_MANY: u16 = 0x423;
const INNKEEPER: u16 = 0x424;
const HIDDEN_CAVE: u16 = 0x425;
const CASTLE_GATE: u16 = 0x426;
const CASTLE_WALLS: u16 = 0x427;
const SECRET_DOOR: u16 = 0x428;
const CELLARS: u16 = 0x429;
const KINGS_TOMB: u16 = 0x430;
const GREAT_HALL: u16 = 0x431;
const TIPTOE: u16 = 0x432;
const DRAGONS_LAIR: u16 = 0x433;
const DUNGEON: u16 = 0x434;
const DRAGON_FIRE: u16 = 0x435;
const NARROW_ESCAPE: u16 = 0x436;
const CROWNED: u16 = 0x437;
const DRAGON_GOLD: u16 = 0x438;

/// The story's last page.
pub const LAST: u16 = DRAGON_GOLD;

/// The two choices at every ending.
const AFTER_THE_END: &[Choice] =
    &[Choice { label: "Again", text: "Begin a new quest", to: CROSSROADS }, Choice { label: "Menu", text: "Back to the adventures", to: MENU }];

/// Every node, in page order.
pub const NODES: &[Node] = &[
    Node {
        page: CROSSROADS,
        title: "The Crossroads",
        text: "Dawn breaks over the moor. Three roads meet beneath a leaning signpost. Beyond them lies the ruined castle of Kell, and in it the {lost crown} that no thief has ever come back with.",
        picture: &SIGNPOST,
        choices: &[
            Choice { label: "Forest", text: "Take the north road, to the forest", to: FOREST_OF_WYR },
            Choice { label: "River", text: "Take the east road, to the river", to: RIVER_KELL },
            Choice { label: "Inn", text: "Take the west road, to the inn", to: BLACK_GOAT },
        ],
        ending: Ending::Not,
    },
    Node {
        page: FOREST_OF_WYR,
        title: "Forest of Wyr",
        text: "The trees close over you until the day is a green dusk. Smoke curls from a fire where an {old woman} sits, beckoning. Beside the path, fresh {wolf tracks} lead deeper in.",
        picture: &FOREST,
        choices: &[
            Choice { label: "Woman", text: "Sit by the old woman's fire", to: WISE_WOMAN },
            Choice { label: "Tracks", text: "Follow the wolf tracks", to: WOLVES },
            Choice { label: "Back", text: "Return to the crossroads", to: CROSSROADS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: RIVER_KELL,
        title: "The River Kell",
        text: "The road ends at a roaring river. A rotten {rope bridge} sways across it. Downstream a {ferryman} leans on his pole, a lantern at his feet, watching you without blinking.",
        picture: &RIVER,
        choices: &[
            Choice { label: "Bridge", text: "Cross the rope bridge", to: ROPE_BRIDGE },
            Choice { label: "Ferry", text: "Pay the ferryman your coin", to: FERRYMAN },
            Choice { label: "Back", text: "Return to the crossroads", to: CROSSROADS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: BLACK_GOAT,
        title: "The Black Goat",
        text: "The inn is warm and loud. A hooded stranger slides a {map} across the table: 'Your last coin, and it is yours.' In the corner, sailors play {dice} for silver.",
        picture: &INN,
        choices: &[
            Choice { label: "Map", text: "Buy the stranger's map", to: STRANGERS_MAP },
            Choice { label: "Dice", text: "Join the game of dice", to: DICE_GAME },
            Choice { label: "Leave", text: "Go back to the crossroads", to: CROSSROADS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: WISE_WOMAN,
        title: "The Wise Woman",
        text: "'You seek the crown,' she says, before you speak. She presses a {silver charm} into your hand. 'Dragon fire fears silver. And there is a cave that runs beneath the castle.'",
        picture: &FIRE,
        choices: &[
            Choice { label: "Cave", text: "Go to the hidden cave", to: HIDDEN_CAVE },
            Choice { label: "Ask", text: "Ask her about the crown", to: CROWN_LORE },
        ],
        ending: Ending::Not,
    },
    Node {
        page: CROWN_LORE,
        title: "The Crown of Kell",
        text: "'The last king hid it from the dragon {Morvath},' she says, 'and the dragon sleeps on it still, above the king's own tomb. Go by the cave, and keep the charm close.'",
        picture: &CROWN,
        choices: &[Choice { label: "Cave", text: "Go to the hidden cave", to: HIDDEN_CAVE }],
        ending: Ending::Not,
    },
    Node {
        page: WOLVES,
        title: "The Wolves",
        text: "The tracks lead into a clearing, and the clearing is full of {wolves}. Yellow eyes turn towards you, one pair after another. The nearest wolf lowers its head and growls.",
        picture: &WOLF,
        choices: &[Choice { label: "Climb", text: "Climb the nearest tree", to: TALL_PINE }, Choice { label: "Run", text: "Run for it", to: LOST_IN_WYR }],
        ending: Ending::Not,
    },
    Node {
        page: TALL_PINE,
        title: "The Tall Pine",
        text: "You scramble up as jaws snap at your boots. From the top you see the river, and beyond it the {castle of Kell}, black against the sky. By noon the wolves give up and slink away.",
        picture: &CASTLE,
        choices: &[Choice { label: "Castle", text: "Climb down and make for the castle", to: CASTLE_WALLS }],
        ending: Ending::Not,
    },
    Node {
        page: LOST_IN_WYR,
        title: "Lost in Wyr",
        text: "You run until you cannot run, and then you are lost. The forest of Wyr keeps its travellers. They say a ragged figure wanders there still, asking the way to {Kell}.",
        picture: &SKULL,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        page: ROPE_BRIDGE,
        title: "The Rope Bridge",
        text: "Halfway across, a rope parts with a crack like a whip. The river takes you, roaring, and throws you out miles away on a muddy bank, without your boots and without your {courage}.",
        picture: &RIVER,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        page: FERRYMAN,
        title: "The Ferryman",
        text: "He takes your coin without a word and poles you into the mist. 'Few come back from Kell,' he says at last, and sets you ashore beneath the walls. Above you, the {gate} is shut.",
        picture: &BOAT,
        choices: &[
            Choice { label: "Gate", text: "Go up to the castle gate", to: CASTLE_GATE },
            Choice { label: "Walls", text: "Climb the ivy on the walls", to: CASTLE_WALLS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: STRANGERS_MAP,
        title: "The Stranger's Map",
        text: "The map is old and good. It shows a {secret door} at the foot of the east tower, hidden by brambles, and the ferry that crosses to the castle. When you look up, the stranger is gone.",
        picture: &MAP,
        choices: &[
            Choice { label: "Door", text: "Find the secret door", to: SECRET_DOOR },
            Choice { label: "River", text: "Make for the river and the ferry", to: RIVER_KELL },
        ],
        ending: Ending::Not,
    },
    Node {
        page: DICE_GAME,
        title: "The Dice Game",
        text: "Luck is with you. Roll after roll, the sailors' silver piles up in front of you, until they glare at you across the table. You have won a {small fortune}.",
        picture: &DICE,
        choices: &[
            Choice { label: "Again", text: "Roll once more, for everything", to: ONE_ROLL_TOO_MANY },
            Choice { label: "Stop", text: "Quit while you are ahead", to: INNKEEPER },
        ],
        ending: Ending::Not,
    },
    Node {
        page: ONE_ROLL_TOO_MANY,
        title: "One Roll Too Many",
        text: "{Snake eyes.} You lose everything, down to your boots, and the sailors throw you out into the road with a cheer. Barefoot, you limp back the way you came.",
        picture: &SNAKE_EYES,
        choices: &[Choice { label: "Onward", text: "Start again from the crossroads", to: CROSSROADS }],
        ending: Ending::Not,
    },
    Node {
        page: INNKEEPER,
        title: "The Innkeeper",
        text: "With your winnings you buy the Black Goat itself. You never find the crown, but every evening you tell the story of the night you {nearly went looking for it}.",
        picture: &INN,
        choices: AFTER_THE_END,
        ending: Ending::Other,
    },
    Node {
        page: HIDDEN_CAVE,
        title: "The Hidden Cave",
        text: "Behind a waterfall, just where she said, a cave mouth opens. Glowing {mushrooms} light a tunnel that runs down and down, under the river, towards the castle.",
        picture: &CAVE,
        choices: &[Choice { label: "Tunnel", text: "Follow the tunnel", to: KINGS_TOMB }],
        ending: Ending::Not,
    },
    Node {
        page: CASTLE_GATE,
        title: "The Castle Gate",
        text: "The portcullis is down. Carved above it: {What eats all things, and is never full?} Below are two levers, one marked with an hourglass and one with a flame.",
        picture: &GATE,
        choices: &[
            Choice { label: "Hourglass", text: "Pull the lever of the hourglass", to: GREAT_HALL },
            Choice { label: "Flame", text: "Pull the lever of the flame", to: DUNGEON },
            Choice { label: "Walls", text: "Climb the walls instead", to: CASTLE_WALLS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: CASTLE_WALLS,
        title: "The Castle Walls",
        text: "Ivy as thick as rope climbs the wall. You climb with it, stone by stone, and drop over the battlements into a {courtyard} of dead roses. A great door stands ajar.",
        picture: &CASTLE,
        choices: &[Choice { label: "Door", text: "Go through the great door", to: GREAT_HALL }],
        ending: Ending::Not,
    },
    Node {
        page: SECRET_DOOR,
        title: "The Secret Door",
        text: "Behind the brambles, as the map promised, is a small iron door. Inside, a stair winds up towards the halls, and down into the castle's {cellars}.",
        picture: &GATE,
        choices: &[
            Choice { label: "Up", text: "Climb the stair to the halls", to: GREAT_HALL },
            Choice { label: "Down", text: "Search the cellars", to: CELLARS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: CELLARS,
        title: "The Cellars",
        text: "Among the old barrels you find a {silver sword}, bright as the day it was forged, and behind them a passage that leads to a tomb.",
        picture: &SWORD,
        choices: &[Choice { label: "Passage", text: "Take the sword, follow the passage", to: KINGS_TOMB }],
        ending: Ending::Not,
    },
    Node {
        page: KINGS_TOMB,
        title: "The King's Tomb",
        text: "A pale {ghost} rises from the tomb of the last king of Kell. 'You carry silver,' it whispers. 'Good. My crown lies above, beneath the dragon. Be brave.' A stair leads up.",
        picture: &GHOST,
        choices: &[Choice { label: "Up", text: "Climb to the dragon's lair", to: DRAGONS_LAIR }],
        ending: Ending::Not,
    },
    Node {
        page: GREAT_HALL,
        title: "The Great Hall",
        text: "Gold lies in drifts across the hall, and on the gold sleeps {Morvath} the dragon, smoke curling from its nostrils. On a pillar above its head, the lost crown glitters.",
        picture: &HOARD,
        choices: &[
            Choice { label: "Sneak", text: "Creep past and take the crown", to: TIPTOE },
            Choice { label: "Fight", text: "Draw your knife and attack", to: DRAGON_FIRE },
            Choice { label: "Flee", text: "Run back the way you came", to: CROSSROADS },
        ],
        ending: Ending::Not,
    },
    Node {
        page: TIPTOE,
        title: "Tiptoe",
        text: "Coin by coin you creep across the gold. You climb the pillar. Your fingers close on the {crown}... and one great yellow eye opens, and looks at you.",
        picture: &EYE,
        choices: &[
            Choice { label: "Run", text: "Run for the door", to: NARROW_ESCAPE },
            Choice { label: "Freeze", text: "Stay perfectly still", to: DRAGON_FIRE },
        ],
        ending: Ending::Not,
    },
    Node {
        page: DRAGONS_LAIR,
        title: "The Dragon's Lair",
        text: "Morvath wakes and roars, and a river of fire pours over you... and parts around you, turned aside by your {silver}. The dragon cowers. The crown is yours to take.",
        picture: &DRAGON,
        choices: &[
            Choice { label: "Crown", text: "Take the crown", to: CROWNED },
            Choice { label: "Gold", text: "Fill your pockets with gold", to: DRAGON_GOLD },
        ],
        ending: Ending::Not,
    },
    Node {
        page: DUNGEON,
        title: "The Dungeon",
        text: "The floor drops away, and you land in a dark dungeon. Far above, the trapdoor thuds shut. The answer was {time}, and now you have plenty of it.",
        picture: &GATE,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        page: DRAGON_FIRE,
        title: "Dragon Fire",
        text: "Morvath's breath fills the hall with fire. There is a flash, a roar, and then nothing at all. Somewhere a bard begins a song about a brave and {very foolish} thief.",
        picture: &SKULL,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        page: NARROW_ESCAPE,
        title: "A Narrow Escape",
        text: "You run, the crown in your fist, fire roaring at your heels, and dive through the door as the hall burns. Singed and smoking, you carry the {crown of Kell} home.",
        picture: &CROWN,
        choices: AFTER_THE_END,
        ending: Ending::Won,
    },
    Node {
        page: CROWNED,
        title: "Long Live the Crown",
        text: "You lift the crown, and the dragon bows its head and flies north, never to return. The people of Kell crown you their ruler, and a little {silver} hangs above your throne.",
        picture: &CROWN,
        choices: AFTER_THE_END,
        ending: Ending::Won,
    },
    Node {
        page: DRAGON_GOLD,
        title: "Dragon Gold",
        text: "You fill your pockets, then your boots, then your hat. The more gold you hold, the heavier you feel, and the {scalier}. By morning a new dragon sleeps on the hoard of Kell.",
        picture: &DRAGON,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
];
