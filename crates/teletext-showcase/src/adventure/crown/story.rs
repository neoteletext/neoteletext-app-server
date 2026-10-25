//! The Lost Crown's story: every node one view of the story's page, every
//! choice a coloured key leading to another node. The story keeps no state
//! of its own; what the
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

/// A node of the story, by its place in the telling.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Id(pub u8);

/// Where a choice leads: on in the story, or out of it to another page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Leads {
    /// Another node, drawn on the same page.
    Node(Id),
    /// Another page of the channel.
    Page(u16),
}

/// A choice: its Fastext label, what it says on the page, and where it
/// leads.
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    /// The label on row 24: at most nine characters.
    pub label: &'static str,
    /// The choice in words.
    pub text: &'static str,
    /// Where it leads.
    pub to: Leads,
}

/// A node of the story: what the page shows at one point of it.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// Which node it is.
    pub id: Id,
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

/// The story's first node.
pub const CROSSROADS: Id = Id(0);
const FOREST_OF_WYR: Id = Id(1);
const RIVER_KELL: Id = Id(2);
const BLACK_GOAT: Id = Id(3);
const WISE_WOMAN: Id = Id(4);
const CROWN_LORE: Id = Id(5);
const WOLVES: Id = Id(6);
const TALL_PINE: Id = Id(7);
const LOST_IN_WYR: Id = Id(8);
const ROPE_BRIDGE: Id = Id(9);
const FERRYMAN: Id = Id(10);
const STRANGERS_MAP: Id = Id(11);
const DICE_GAME: Id = Id(12);
const ONE_ROLL_TOO_MANY: Id = Id(13);
const INNKEEPER: Id = Id(14);
const HIDDEN_CAVE: Id = Id(15);
const CASTLE_GATE: Id = Id(16);
const CASTLE_WALLS: Id = Id(17);
const SECRET_DOOR: Id = Id(18);
const CELLARS: Id = Id(19);
const KINGS_TOMB: Id = Id(20);
const GREAT_HALL: Id = Id(21);
const TIPTOE: Id = Id(22);
const DRAGONS_LAIR: Id = Id(23);
const DUNGEON: Id = Id(24);
const DRAGON_FIRE: Id = Id(25);
const NARROW_ESCAPE: Id = Id(26);
const CROWNED: Id = Id(27);
const DRAGON_GOLD: Id = Id(28);

/// The two choices at every ending.
const AFTER_THE_END: &[Choice] = &[
    Choice { label: "Again", text: "Begin a new quest", to: Leads::Node(CROSSROADS) },
    Choice { label: "Menu", text: "Back to the adventures", to: Leads::Page(MENU) },
];

/// Every node, in the order of their ids.
pub const NODES: &[Node] = &[
    Node {
        id: CROSSROADS,
        title: "The Crossroads",
        text: "Dawn breaks over the moor. Three roads meet beneath a leaning signpost. Beyond them lies the ruined castle of Kell, and in it the {lost crown} that no thief has ever come back with.",
        picture: &SIGNPOST,
        choices: &[
            Choice { label: "Forest", text: "Take the north road, to the forest", to: Leads::Node(FOREST_OF_WYR) },
            Choice { label: "River", text: "Take the east road, to the river", to: Leads::Node(RIVER_KELL) },
            Choice { label: "Inn", text: "Take the west road, to the inn", to: Leads::Node(BLACK_GOAT) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: FOREST_OF_WYR,
        title: "Forest of Wyr",
        text: "The trees close over you until the day is a green dusk. Smoke curls from a fire where an {old woman} sits, beckoning. Beside the path, fresh {wolf tracks} lead deeper in.",
        picture: &FOREST,
        choices: &[
            Choice { label: "Woman", text: "Sit by the old woman's fire", to: Leads::Node(WISE_WOMAN) },
            Choice { label: "Tracks", text: "Follow the wolf tracks", to: Leads::Node(WOLVES) },
            Choice { label: "Back", text: "Return to the crossroads", to: Leads::Node(CROSSROADS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: RIVER_KELL,
        title: "The River Kell",
        text: "The road ends at a roaring river. A rotten {rope bridge} sways across it. Downstream a {ferryman} leans on his pole, a lantern at his feet, watching you without blinking.",
        picture: &RIVER,
        choices: &[
            Choice { label: "Bridge", text: "Cross the rope bridge", to: Leads::Node(ROPE_BRIDGE) },
            Choice { label: "Ferry", text: "Pay the ferryman your coin", to: Leads::Node(FERRYMAN) },
            Choice { label: "Back", text: "Return to the crossroads", to: Leads::Node(CROSSROADS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: BLACK_GOAT,
        title: "The Black Goat",
        text: "The inn is warm and loud. A hooded stranger slides a {map} across the table: 'Your last coin, and it is yours.' In the corner, sailors play {dice} for silver.",
        picture: &INN,
        choices: &[
            Choice { label: "Map", text: "Buy the stranger's map", to: Leads::Node(STRANGERS_MAP) },
            Choice { label: "Dice", text: "Join the game of dice", to: Leads::Node(DICE_GAME) },
            Choice { label: "Leave", text: "Go back to the crossroads", to: Leads::Node(CROSSROADS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: WISE_WOMAN,
        title: "The Wise Woman",
        text: "'You seek the crown,' she says, before you speak. She presses a {silver charm} into your hand. 'Dragon fire fears silver. And there is a cave that runs beneath the castle.'",
        picture: &FIRE,
        choices: &[
            Choice { label: "Cave", text: "Go to the hidden cave", to: Leads::Node(HIDDEN_CAVE) },
            Choice { label: "Ask", text: "Ask her about the crown", to: Leads::Node(CROWN_LORE) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: CROWN_LORE,
        title: "The Crown of Kell",
        text: "'The last king hid it from the dragon {Morvath},' she says, 'and the dragon sleeps on it still, above the king's own tomb. Go by the cave, and keep the charm close.'",
        picture: &CROWN,
        choices: &[Choice { label: "Cave", text: "Go to the hidden cave", to: Leads::Node(HIDDEN_CAVE) }],
        ending: Ending::Not,
    },
    Node {
        id: WOLVES,
        title: "The Wolves",
        text: "The tracks lead into a clearing, and the clearing is full of {wolves}. Yellow eyes turn towards you, one pair after another. The nearest wolf lowers its head and growls.",
        picture: &WOLF,
        choices: &[
            Choice { label: "Climb", text: "Climb the nearest tree", to: Leads::Node(TALL_PINE) },
            Choice { label: "Run", text: "Run for it", to: Leads::Node(LOST_IN_WYR) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: TALL_PINE,
        title: "The Tall Pine",
        text: "You scramble up as jaws snap at your boots. From the top you see the river, and beyond it the {castle of Kell}, black against the sky. By noon the wolves give up and slink away.",
        picture: &CASTLE,
        choices: &[Choice { label: "Castle", text: "Climb down and make for the castle", to: Leads::Node(CASTLE_WALLS) }],
        ending: Ending::Not,
    },
    Node {
        id: LOST_IN_WYR,
        title: "Lost in Wyr",
        text: "You run until you cannot run, and then you are lost. The forest of Wyr keeps its travellers. They say a ragged figure wanders there still, asking the way to {Kell}.",
        picture: &SKULL,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        id: ROPE_BRIDGE,
        title: "The Rope Bridge",
        text: "Halfway across, a rope parts with a crack like a whip. The river takes you, roaring, and throws you out miles away on a muddy bank, without your boots and without your {courage}.",
        picture: &RIVER,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        id: FERRYMAN,
        title: "The Ferryman",
        text: "He takes your coin without a word and poles you into the mist. 'Few come back from Kell,' he says at last, and sets you ashore beneath the walls. Above you, the {gate} is shut.",
        picture: &BOAT,
        choices: &[
            Choice { label: "Gate", text: "Go up to the castle gate", to: Leads::Node(CASTLE_GATE) },
            Choice { label: "Walls", text: "Climb the ivy on the walls", to: Leads::Node(CASTLE_WALLS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: STRANGERS_MAP,
        title: "The Stranger's Map",
        text: "The map is old and good. It shows a {secret door} at the foot of the east tower, hidden by brambles, and the ferry that crosses to the castle. When you look up, the stranger is gone.",
        picture: &MAP,
        choices: &[
            Choice { label: "Door", text: "Find the secret door", to: Leads::Node(SECRET_DOOR) },
            Choice { label: "River", text: "Make for the river and the ferry", to: Leads::Node(RIVER_KELL) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: DICE_GAME,
        title: "The Dice Game",
        text: "Luck is with you. Roll after roll, the sailors' silver piles up in front of you, until they glare at you across the table. You have won a {small fortune}.",
        picture: &DICE,
        choices: &[
            Choice { label: "Again", text: "Roll once more, for everything", to: Leads::Node(ONE_ROLL_TOO_MANY) },
            Choice { label: "Stop", text: "Quit while you are ahead", to: Leads::Node(INNKEEPER) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: ONE_ROLL_TOO_MANY,
        title: "One Roll Too Many",
        text: "{Snake eyes.} You lose everything, down to your boots, and the sailors throw you out into the road with a cheer. Barefoot, you limp back the way you came.",
        picture: &SNAKE_EYES,
        choices: &[Choice { label: "Onward", text: "Start again from the crossroads", to: Leads::Node(CROSSROADS) }],
        ending: Ending::Not,
    },
    Node {
        id: INNKEEPER,
        title: "The Innkeeper",
        text: "With your winnings you buy the Black Goat itself. You never find the crown, but every evening you tell the story of the night you {nearly went looking for it}.",
        picture: &INN,
        choices: AFTER_THE_END,
        ending: Ending::Other,
    },
    Node {
        id: HIDDEN_CAVE,
        title: "The Hidden Cave",
        text: "Behind a waterfall, just where she said, a cave mouth opens. Glowing {mushrooms} light a tunnel that runs down and down, under the river, towards the castle.",
        picture: &CAVE,
        choices: &[Choice { label: "Tunnel", text: "Follow the tunnel", to: Leads::Node(KINGS_TOMB) }],
        ending: Ending::Not,
    },
    Node {
        id: CASTLE_GATE,
        title: "The Castle Gate",
        text: "The portcullis is down. Carved above it: {What eats all things, and is never full?} Below are two levers, one marked with an hourglass and one with a flame.",
        picture: &GATE,
        choices: &[
            Choice { label: "Hourglass", text: "Pull the lever of the hourglass", to: Leads::Node(GREAT_HALL) },
            Choice { label: "Flame", text: "Pull the lever of the flame", to: Leads::Node(DUNGEON) },
            Choice { label: "Walls", text: "Climb the walls instead", to: Leads::Node(CASTLE_WALLS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: CASTLE_WALLS,
        title: "The Castle Walls",
        text: "Ivy as thick as rope climbs the wall. You climb with it, stone by stone, and drop over the battlements into a {courtyard} of dead roses. A great door stands ajar.",
        picture: &CASTLE,
        choices: &[Choice { label: "Door", text: "Go through the great door", to: Leads::Node(GREAT_HALL) }],
        ending: Ending::Not,
    },
    Node {
        id: SECRET_DOOR,
        title: "The Secret Door",
        text: "Behind the brambles, as the map promised, is a small iron door. Inside, a stair winds up towards the halls, and down into the castle's {cellars}.",
        picture: &GATE,
        choices: &[
            Choice { label: "Up", text: "Climb the stair to the halls", to: Leads::Node(GREAT_HALL) },
            Choice { label: "Down", text: "Search the cellars", to: Leads::Node(CELLARS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: CELLARS,
        title: "The Cellars",
        text: "Among the old barrels you find a {silver sword}, bright as the day it was forged, and behind them a passage that leads to a tomb.",
        picture: &SWORD,
        choices: &[Choice { label: "Passage", text: "Take the sword, follow the passage", to: Leads::Node(KINGS_TOMB) }],
        ending: Ending::Not,
    },
    Node {
        id: KINGS_TOMB,
        title: "The King's Tomb",
        text: "A pale {ghost} rises from the tomb of the last king of Kell. 'You carry silver,' it whispers. 'Good. My crown lies above, beneath the dragon. Be brave.' A stair leads up.",
        picture: &GHOST,
        choices: &[Choice { label: "Up", text: "Climb to the dragon's lair", to: Leads::Node(DRAGONS_LAIR) }],
        ending: Ending::Not,
    },
    Node {
        id: GREAT_HALL,
        title: "The Great Hall",
        text: "Gold lies in drifts across the hall, and on the gold sleeps {Morvath} the dragon, smoke curling from its nostrils. On a pillar above its head, the lost crown glitters.",
        picture: &HOARD,
        choices: &[
            Choice { label: "Sneak", text: "Creep past and take the crown", to: Leads::Node(TIPTOE) },
            Choice { label: "Fight", text: "Draw your knife and attack", to: Leads::Node(DRAGON_FIRE) },
            Choice { label: "Flee", text: "Run back the way you came", to: Leads::Node(CROSSROADS) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: TIPTOE,
        title: "Tiptoe",
        text: "Coin by coin you creep across the gold. You climb the pillar. Your fingers close on the {crown}... and one great yellow eye opens, and looks at you.",
        picture: &EYE,
        choices: &[
            Choice { label: "Run", text: "Run for the door", to: Leads::Node(NARROW_ESCAPE) },
            Choice { label: "Freeze", text: "Stay perfectly still", to: Leads::Node(DRAGON_FIRE) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: DRAGONS_LAIR,
        title: "The Dragon's Lair",
        text: "Morvath wakes and roars, and a river of fire pours over you... and parts around you, turned aside by your {silver}. The dragon cowers. The crown is yours to take.",
        picture: &DRAGON,
        choices: &[
            Choice { label: "Crown", text: "Take the crown", to: Leads::Node(CROWNED) },
            Choice { label: "Gold", text: "Fill your pockets with gold", to: Leads::Node(DRAGON_GOLD) },
        ],
        ending: Ending::Not,
    },
    Node {
        id: DUNGEON,
        title: "The Dungeon",
        text: "The floor drops away, and you land in a dark dungeon. Far above, the trapdoor thuds shut. The answer was {time}, and now you have plenty of it.",
        picture: &GATE,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        id: DRAGON_FIRE,
        title: "Dragon Fire",
        text: "Morvath's breath fills the hall with fire. There is a flash, a roar, and then nothing at all. Somewhere a bard begins a song about a brave and {very foolish} thief.",
        picture: &SKULL,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
    Node {
        id: NARROW_ESCAPE,
        title: "A Narrow Escape",
        text: "You run, the crown in your fist, fire roaring at your heels, and dive through the door as the hall burns. Singed and smoking, you carry the {crown of Kell} home.",
        picture: &CROWN,
        choices: AFTER_THE_END,
        ending: Ending::Won,
    },
    Node {
        id: CROWNED,
        title: "Long Live the Crown",
        text: "You lift the crown, and the dragon bows its head and flies north, never to return. The people of Kell crown you their ruler, and a little {silver} hangs above your throne.",
        picture: &CROWN,
        choices: AFTER_THE_END,
        ending: Ending::Won,
    },
    Node {
        id: DRAGON_GOLD,
        title: "Dragon Gold",
        text: "You fill your pockets, then your boots, then your hat. The more gold you hold, the heavier you feel, and the {scalier}. By morning a new dragon sleeps on the hoard of Kell.",
        picture: &DRAGON,
        choices: AFTER_THE_END,
        ending: Ending::Lost,
    },
];
