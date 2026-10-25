// The Lost Crown's illustrations, 40 sixels across and 18 down: twenty
// cells by six rows, centred on a band of background colour
// (crates/teletext-showcase/src/adventure/crown/pictures.rs).

import { Art } from "../art.ts";

/** The rows of cells each picture takes. */
export const ROWS = 6;

/** The crossroads at dawn: the signpost, the sun, the road. */
export const SIGNPOST = new Art(
  [
    "................................yyyy....",
    "..............................yyyyyyyy..",
    "..............................yyyyyyyy..",
    "..........wwwwwwwwwwwwwwww......yyyy....",
    "..........ww.w.w.w.w.w.wwwww............",
    "..........wwwwwwwwwwwwwwww..............",
    "....wwwwwwwwwwwwwwww....................",
    "..wwww.w.w.w.w.w.www....................",
    "....wwwwwwwwwwwwwwww....................",
    "..............ww........................",
    "..............ww........................",
    "..............ww........................",
    "..............ww......yyyyyy............",
    "..............ww....yyyyyyyyyy..........",
    "............wwwwwwyyyyyyyyyyyyyy........",
    "................yyyyyyyyyyyyyyyyyy......",
    "..............yyyyyyyyyyyyyyyyyyyyyy....",
    "............yyyyyyyyyyyyyyyyyyyyyyyyyy..",
  ],
  "bbbbgg",
);

/** The forest of Wyr and its path. */
export const FOREST = new Art(
  [
    "....gg..............gg..............gg..",
    "...gggg............gggg............gggg.",
    "..gggggg..........gggggg..........gggggg",
    "...gggg............gggg............gggg.",
    "..gggggg..........gggggg..........gggggg",
    ".gggggggg........gggggggg........ggggggg",
    "..gggggg..........gggggg..........gggggg",
    ".gggggggg........gggggggg........ggggggg",
    "gggggggggg......gggggggggg......gggggggg",
    "....rr..............rr..............rr..",
    "....rr..............rr..............rr..",
    "....rr..............rr..............rr..",
    "..............yyyyyy....................",
    ".............yyyyyyyy...................",
    "............yyyyyyyyyy..................",
    "...........yyyyyyyyyyyy.................",
    "..........yyyyyyyyyyyyyy................",
    ".........yyyyyyyyyyyyyyyy...............",
  ],
  "....gg",
);

/** The river Kell and its rope bridge. */
export const RIVER = new Art(
  [
    "gggggg..................................",
    "gggggg..............................gggg",
    "gggggg.............................ggggg",
    "gggggggg...........................ggggg",
    "gggggggg..........................gggggg",
    "gggggggg..........................gggggg",
    "gggggggg..yyyy..............yyyy..gggggg",
    "gggggggg....yyyyyy......yyyyyy....gggggg",
    "gggggggg........yyyyyyyyyy........gggggg",
    "gggggggg..........................gggggg",
    "gggggggg..cc....cc....cc....cc....gggggg",
    "gggggggg...cccc..cccc..cccc..cc...gggggg",
    "gggggggg..........................gggggg",
    "gggggggg....cc....cc....cc....cc..gggggg",
    "gggggggg.....cccc..cccc..cccc.....gggggg",
    "gggggggg..........................gggggg",
    "gggggggg..cc....cc....cc....cc....gggggg",
    "gggggggg...cccc..cccc..cccc..cc...gggggg",
  ],
  "bbbbbb",
);

/** The Black Goat inn by moonlight. */
export const INN = new Art(
  [
    "..ww......................y.....rr......",
    ".wwww.....y.....................rr......",
    "..ww............................rr...y..",
    "................rrrrrrrr........rr......",
    "..............rrrrrrrrrrrr......rr......",
    "............rrrrrrrrrrrrrrrr....rr......",
    "..........rrrrrrrrrrrrrrrrrrrr..........",
    "........rrrrrrrrrrrrrrrrrrrrrrrr........",
    "......rrrrrrrrrrrrrrrrrrrrrrrrrrrr......",
    "......wwwwwwwwwwwwwwwwwwwwwwwwwwww......",
    "......wwwwwwwwwwwwwwwwwwwwwwwwwwww......",
    "......wwwwwwwwwwwwwwwwwwwwwwwwwwww......",
    "......wwwwyyyywwwwyyyywwwwyyyywwww......",
    "......wwwwyyyywwwwyyyywwwwyyyywwww......",
    "......wwwwyyyywwwwyyyywwwwyyyywwww......",
    "......wwwwwwwwwwwwyyyywwwwwwwwwwww......",
    "......wwwwwwwwwwwwyyyywwwwwwwwwwww......",
    "......wwwwwwwwwwwwyyyywwwwwwwwwwww......",
  ],
  "bbbbbb",
);

/** The wise woman's fire in the dark. */
export const FIRE = new Art(
  [
    "...................rr...................",
    "..................rrrr..................",
    ".................rrrrrr...........y.....",
    "................rrryyrrr................",
    "......y.........rryyyyrr.....y..........",
    "...............rrryyyyrrr...............",
    "..............rrryyyyyyrrr..............",
    ".............rrrryyyyyyrrrr.............",
    ".............rrryyyyyyyyrrr.............",
    "............rrrryyyyyyyyrrrr............",
    "............rrryyyyyyyyyyrrr............",
    ".............rrryyyyyyyyrrr.............",
    ".........wwwwwwwwwwwwwwwwwwwwww.........",
    "...........wwwwwwwwwwwwwwwwww...........",
    ".........wwwwwwwwwwwwwwwwwwwwww.........",
    "........................................",
    "........................................",
    "........................................",
  ],
  "......",
);

/** The crown of Kell on its cushion. */
export const CROWN = new Art(
  [
    "..........w...........w...........w.....",
    "........yyyy........yyyy........yyyy....",
    "........yyyy........yyyy........yyyy....",
    "........yyyyy......yyyyyy......yyyyy....",
    "........yyyyyy....yyyyyyyy....yyyyyy....",
    "........yyyyyyy..yyyyyyyyyy..yyyyyyy....",
    "........yyyyyyyyyyyyyyyyyyyyyyyyyyyy....",
    "........yyyyyyyyyyyyyyyyyyyyyyyyyyyy....",
    "........yyyyyyyyyyyyyyyyyyyyyyyyyyyy....",
    "........yyyyrrrryyyyccccyyyyrrrryyyy....",
    "........yyyyrrrryyyyccccyyyyrrrryyyy....",
    "........yyyyrrrryyyyccccyyyyrrrryyyy....",
    "........yyyyyyyyyyyyyyyyyyyyyyyyyyyy....",
    "........yyyyyyyyyyyyyyyyyyyyyyyyyyyy....",
    "........................................",
    "....mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm..",
    "..mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm",
    "..mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm",
  ],
  "......",
);

/** Wolves' eyes in the dark wood, under the moon. */
export const WOLF = new Art(
  [
    "...gg..............ww..............gg...",
    "..gggg............wwww............gggg..",
    ".gggggg............ww............gggggg.",
    "gggggggg........................gggggggg",
    ".gggggg.......y..y...............gggggg.",
    "gggggggg........................gggggggg",
    "ggggggggg......................ggggggggg",
    "gggggggg..............y..y......gggggggg",
    "ggggggggg......................ggggggggg",
    "gggggggggg....................gggggggggg",
    "...rr.......y..y...................rr...",
    "...rr..............................rr...",
    "...rr...................y..y.......rr...",
    "...rr..............................rr...",
    "...rr.............y..y.............rr...",
    "........................................",
    "........................................",
    "........................................",
  ],
  "......",
);

/** The castle of Kell on its hill, under the stars. */
export const CASTLE = new Art(
  [
    "....ww...ww...ww........ww...ww...ww....",
    ".y..ww...ww...ww........ww...ww...ww....",
    "....wwwwwwwwwwww........wwwwwwwwwwww....",
    "....wwwwwwwwwwww.y......wwwwwwwwwwww..y.",
    "....wwwwwwwwwwww........wwwwwwwwwwww....",
    "....wwwwwwwwwwwwww..ww..wwwwwwwwwwww....",
    "....wwwwyyyywwwwwwwwwwwwwwwwyyyywwww....",
    "y...wwwwyyyywwwwwwwwwwwwwwwwyyyywwww....",
    "....wwwwyyyywwwwwwwwwwwwwwwwyyyywwww....",
    "....wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww...y",
    "....wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwwwyyyywwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwwwyyyywwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwwwyyyywwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwwwyyyywwwwwwwwwwwwww....",
    "........................................",
    "........................................",
  ],
  "bbbbbg",
);

/** The ferryman's boat in the mist. */
export const BOAT = new Art(
  [
    "..wwww..............................y...",
    ".wwwwww.........y.......................",
    "..wwww..................................",
    "...................ww...................",
    "..................wwww........yy........",
    "...................ww.........yy........",
    "..................wwww........ww........",
    ".................wwwwww.......ww........",
    ".................wwwwww.......ww........",
    "......rrrrrrrrrrrrrrrrrrrrrrrrrrrrrr....",
    "........rrrrrrrrrrrrrrrrrrrrrrrrrr......",
    "..........rrrrrrrrrrrrrrrrrrrrrr........",
    "........................................",
    "..cc....cc....cc....cc....cc....cc......",
    "...cccc..cccc..cccc..cccc..cccc..cccc...",
    "........................................",
    "......cc....cc....cc....cc....cc....cc..",
    ".......cccc..cccc..cccc..cccc..cccc.....",
  ],
  "....bb",
);

/** The stranger's map: ink on parchment. */
export const MAP = new Art(
  [
    "........................................",
    "....ww..yyyyyyyyyyyyyyyyyyyyyyyyyy..ww..",
    "....ww..yy..yyyyyyyyyyyyyyyyyyyyyy..ww..",
    "....ww..yyy..yyyyyyyyyyyyyyyy.yy.y..ww..",
    "....ww..yyyy..yyyyyyyyyyyyyyyy.yyy..ww..",
    "....ww..yyyyy..yyyyyyyyyyyyyy.y.yy..ww..",
    "....ww..yyyyyy.....yyyyyyyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyy..yyyyyyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyyy..yyyyyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyyyy..yyyyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyyyyy...yyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyyyyyyy...yyyyyyyy..ww..",
    "....ww..yyyyyyyyyyyyyyyyy....yyyyy..ww..",
    "....ww..yyyyyyyyyyyyyyyyyyyy..yyyy..ww..",
    "....ww..yyyyyyyyyyyyyyyyyyyyyyyyyy..ww..",
    "....ww..yyyyyyyyyyyyyyyyyyyyyyyyyy..ww..",
    "........................................",
    "........................................",
  ],
  "......",
);

/** Two dice on the green baize: a five and a six. */
export const DICE = new Art(
  [
    "........................................",
    "......................wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....ww..wwwwww..ww....wwwwwwwwwwwwww....",
    "....ww..wwwwww..ww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....wwwwww..wwwwww....wwwwwwwwwwwwww....",
    "....wwwwww..wwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....wwwwwwwwwwwwww....ww..wwwwww..ww....",
    "....ww..wwwwww..ww....wwwwwwwwwwwwww....",
    "....ww..wwwwww..ww......................",
    "....wwwwwwwwwwwwww......................",
    "....wwwwwwwwwwwwww......................",
    "........................................",
    "........................................",
  ],
  "gggggg",
);

/** The hidden cave, lit by mushrooms. */
export const CAVE = new Art(
  [
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwww............wwwwwwwwwwwwww",
    "wwwwwwwwwww..................wwwwwwwwwww",
    "wwwwwwwww......................wwwwwwwww",
    "wwwwwwww........................wwwwwwww",
    "wwwwwww..........................wwwwwww",
    "wwwwww............................wwwwww",
    "wwwwww............................wwwwww",
    "wwwww..............................wwwww",
    "wwwww....cccc................cccc..wwwww",
    "wwwww...cccccc..............cccccc.wwwww",
    "wwwww.....cc.......cccc.......cc...wwwww",
    "wwwww.....cc......cccccc......cc...wwwww",
    "wwwww...............cc.............wwwww",
    "wwwww...............cc.............wwwww",
    "........................................",
    "........................................",
  ],
  ".....b",
);

/** The castle gate and its portcullis. */
export const GATE = new Art(
  [
    "..wwww..wwww..wwww..wwww..wwww..wwww....",
    "..wwww..wwww..wwww..wwww..wwww..wwww....",
    "..wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww....",
    "..wwwwwwwwww..............wwwwwwwwww....",
    "..wwwwwwww..................wwwwwwww....",
    "..wwwwwwww..................wwwwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww....yyyyyyyyyyyyyy....wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww....yyyyyyyyyyyyyy....wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww....yyyyyyyyyyyyyy....wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww......y..y..y..y......wwwwww....",
    "..wwwwww......................wwwwww....",
  ],
  "......",
);

/** The last king's ghost above his tomb. */
export const GHOST = new Art(
  [
    "...............wwwwwwwwww...............",
    ".............wwwwwwwwwwwwww.............",
    "............wwwwwwwwwwwwwwww............",
    "............www..wwwwww..www............",
    "............www..wwwwww..www............",
    "............wwwwwwwwwwwwwwww............",
    "............wwwwwww..wwwwwww............",
    "............wwwwww....wwwwww............",
    "............wwwwwwwwwwwwwwww............",
    "............wwwwwwwwwwwwwwww............",
    "............ww..wwww..wwww..............",
    "............w....ww....ww...............",
    "........................................",
    "......mmmmmmmmmmmmmmmmmmmmmmmmmmmm......",
    "......mmmmmmmmmmmmmmmmmmmmmmmmmmmm......",
    "......mm..........................mm....",
    "......mm..........................mm....",
    "......mmmmmmmmmmmmmmmmmmmmmmmmmmmmmm....",
  ],
  "......",
);

/** Morvath the dragon, breathing fire. */
export const DRAGON = new Art(
  [
    "....gg.......gg.........................",
    ".....gg.......gg........................",
    "......gggggggggggg......................",
    "....gggggggggggggggg....................",
    "...gggggg..ggggggggggg..................",
    "..ggggggg..ggggggggggggg................",
    "..gggggggggggggggggggggggggg........rr..",
    "..gggggggggggggggggggggggggggg....rrrr..",
    "..ggggggggggggggggggggggggg.gg..rrrr....",
    "..gggggggggggg..yyyyyyyyyyyyyyyyyyyyyyyy",
    "..gggggggggg..yyyyyyyyyyyyyyyyyyyyyyyyyy",
    "..gggggggggggg..yyyyyyyyyyyyyyyyyyyyyyyy",
    "..gggggggggggggggggggggg..rrrr..rrrr....",
    "..ggggggggggggggggggggg.....rrrr...rrr..",
    "...gggggggggggggggggg.....rr.....rr.....",
    "....gggggggg............................",
    "....gggggggg............................",
    "....gggggggg............................",
  ],
  "......",
);

/** The dragon's eye, opening. */
export const EYE = new Art(
  [
    "gggggggggggggggggggggggggggggggggggggggg",
    "gggggggggggggggggggggggggggggggggggggggg",
    "gggggggggggggggg........gggggggggggggggg",
    "gggggggggg....................gggggggggg",
    "gggggg......yyyyyyyyyyyyyyyy......gggggg",
    "gggg....yyyyyyyyyyy..yyyyyyyyyyy....gggg",
    "gg....yyyyyyyyyyyy....yyyyyyyyyyyy....gg",
    "gg....yyyyyyyyyyyy....yyyyyyyyyyyy....gg",
    "gg....yyyyyyyyyyyy....yyyyyyyyyyyy....gg",
    "gg....yyyyyyyyyyyy....yyyyyyyyyyyy....gg",
    "gg....yyyyyyyyyyyy....yyyyyyyyyyyy....gg",
    "gggg....yyyyyyyyyyy..yyyyyyyyyyy....gggg",
    "gggggg......yyyyyyyyyyyyyyyy......gggggg",
    "gggggggggg....................gggggggggg",
    "gggggggggggggggg........gggggggggggggggg",
    "gggggggggggggggggggggggggggggggggggggggg",
    "gggggggggggggggggggggggggggggggggggggggg",
    "gggggggggggggggggggggggggggggggggggggggg",
  ],
  "......",
);

/** A skull, for the endings that are not happy. */
export const SKULL = new Art(
  [
    "..............wwwwwwwwwwww..............",
    "............wwwwwwwwwwwwwwww............",
    "...........wwwwwwwwwwwwwwwwww...........",
    "..........wwwwwwwwwwwwwwwwwwww..........",
    "..........wwww....wwww....wwww..........",
    "..........wwww....wwww....wwww..........",
    "..........wwwwwwwwww..wwwwwwww..........",
    "............wwwwww......wwwwww..........",
    "..............wwwwwwwwwwwwww............",
    "..............ww..ww..ww..ww............",
    "..............wwwwwwwwwwwwww............",
    "........................................",
    "....wwww........................wwww....",
    "......wwwwwwww............wwwwwwww......",
    "..............wwwwwwwwwwww..............",
    "......wwwwwwww............wwwwwwww......",
    "....wwww........................wwww....",
    "........................................",
  ],
  "rrrrrr",
);

/** The dragon's hoard: gold in drifts, the crown on its pillar. */
export const HOARD = new Art(
  [
    "................yy..yy..yy..............",
    "................yyyyyyyyyy..............",
    "................yyyyyyyyyy..............",
    "..................wwwwww................",
    "..................wwwwww................",
    "..................wwwwww................",
    "..................wwwwww................",
    "..................wwwwww................",
    "..................wwwwww................",
    ".......y........wwwwwwwwww........y.....",
    "...........y....wwwwwwwwww...y..........",
    "....yyyy..........................yyyy..",
    "..yyyyyyyy....yyyyyyyyyyyyyy....yyyyyyyy",
    "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
    "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
    "yyyy.yyyyyyy.yyyyyyy.yyyyyyy.yyyyyyy.yyy",
    "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
    "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
  ],
  "......",
);

/** The silver sword on the cellar floor. */
export const SWORD = new Art(
  [
    "........................................",
    "........................................",
    "........................................",
    "....ww......y...............y...........",
    "....ww..............y...................",
    "....ww..................................",
    "....ww..................................",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww......",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww....",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww......",
    "....ww..................................",
    "....ww..................................",
    "....ww...................y..............",
    "....ww..........y.......................",
    "........................................",
    "rr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.r",
    "rr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.r",
    "rr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.rrrrr.r",
  ],
  "......",
);

/** Two dice showing one each: snake eyes. */
export const SNAKE_EYES = new Art(
  [
    "........................................",
    "........................................",
    "......................wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwww..wwwwww....",
    "....wwwwww..wwwwww....wwwwww..wwwwww....",
    "....wwwwww..wwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww....wwwwwwwwwwwwww....",
    "....wwwwwwwwwwwwww......................",
    "........................................",
    "........................................",
    "........................................",
  ],
  "gggggg",
);

/** Every picture, for tests. */
export const ALL: readonly Art[] = [
  SIGNPOST,
  FOREST,
  RIVER,
  INN,
  FIRE,
  CROWN,
  WOLF,
  CASTLE,
  BOAT,
  MAP,
  DICE,
  SNAKE_EYES,
  CAVE,
  SWORD,
  GATE,
  GHOST,
  HOARD,
  DRAGON,
  EYE,
  SKULL,
];
