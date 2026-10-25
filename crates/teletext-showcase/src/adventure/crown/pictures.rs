//! The Lost Crown's illustrations, 40 sixels across and 18 down: twenty
//! cells by six rows, centred on a band of background colour.

use super::super::art::Art;

/// The rows of cells each picture takes.
pub const ROWS: usize = 6;

/// The crossroads at dawn: the signpost, the sun, the road.
pub const SIGNPOST: Art = Art {
    sixels: &[
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
    backgrounds: "bbbbgg",
};

/// The forest of Wyr and its path.
pub const FOREST: Art = Art {
    sixels: &[
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
    backgrounds: "....gg",
};

/// The river Kell and its rope bridge.
pub const RIVER: Art = Art {
    sixels: &[
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
    backgrounds: "bbbbbb",
};

/// The Black Goat inn by moonlight.
pub const INN: Art = Art {
    sixels: &[
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
    backgrounds: "bbbbbb",
};

/// The wise woman's fire in the dark.
pub const FIRE: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// The crown of Kell on its cushion.
pub const CROWN: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// Wolves' eyes in the dark wood, under the moon.
pub const WOLF: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// The castle of Kell on its hill, under the stars.
pub const CASTLE: Art = Art {
    sixels: &[
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
    backgrounds: "bbbbbg",
};

/// The ferryman's boat in the mist.
pub const BOAT: Art = Art {
    sixels: &[
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
    backgrounds: "....bb",
};

/// The stranger's map: ink on parchment.
pub const MAP: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// Two dice on the green baize: a five and a six.
pub const DICE: Art = Art {
    sixels: &[
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
    backgrounds: "gggggg",
};

/// The hidden cave, lit by mushrooms.
pub const CAVE: Art = Art {
    sixels: &[
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
    backgrounds: ".....b",
};

/// The castle gate and its portcullis.
pub const GATE: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// The last king's ghost above his tomb.
pub const GHOST: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// Morvath the dragon, breathing fire.
pub const DRAGON: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// The dragon's eye, opening.
pub const EYE: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// A skull, for the endings that are not happy.
pub const SKULL: Art = Art {
    sixels: &[
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
    backgrounds: "rrrrrr",
};

/// The dragon's hoard: gold in drifts, the crown on its pillar.
pub const HOARD: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// The silver sword on the cellar floor.
pub const SWORD: Art = Art {
    sixels: &[
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
    backgrounds: "......",
};

/// Two dice showing one each: snake eyes.
pub const SNAKE_EYES: Art = Art {
    sixels: &[
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
    backgrounds: "gggggg",
};

/// Every picture, for tests.
#[cfg(test)]
pub const ALL: [&Art; 20] = [
    &SIGNPOST,
    &FOREST,
    &RIVER,
    &INN,
    &FIRE,
    &CROWN,
    &WOLF,
    &CASTLE,
    &BOAT,
    &MAP,
    &DICE,
    &SNAKE_EYES,
    &CAVE,
    &SWORD,
    &GATE,
    &GHOST,
    &HOARD,
    &DRAGON,
    &EYE,
    &SKULL,
];
