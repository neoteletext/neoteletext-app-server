//! A picture for each of Station Zero's sections, 28 sixels across and
//! 18 down: fourteen cells by six rows, beside the room's description.

use super::super::art::Art;
use super::world::Section;

/// The rows of cells each picture takes.
pub const ROWS: usize = 6;

/// No background on any row: the pictures float in space.
const SPACE_BLACK: &str = "......";

/// The cryo pod the player wakes in.
const POD: Art = Art {
    sixels: &[
        "........cccccccccccc........",
        "......cccccccccccccccc......",
        "....cccc............cccc....",
        "....cc..wwwwwwwwwwww..cc....",
        "....cc..ww........ww..cc....",
        "....cc..ww........ww..cc....",
        "....cc..ww...yy...ww..cc....",
        "....cc..ww..yyyy..ww..cc....",
        "....cc..ww...yy...ww..cc....",
        "....cc..ww..yyyy..ww..cc....",
        "....cc..ww..yyyy..ww..cc....",
        "....cc..ww..yyyy..ww..cc....",
        "....cc..wwwwwwwwwwww..cc....",
        "....cc................cc....",
        "....cc................cc....",
        "....cccc............cccc....",
        "......cccccccccccccccc......",
        "........cccccccccccc........",
    ],
    backgrounds: SPACE_BLACK,
};

/// A mug of coffee, steaming, as the crew left it.
const MUG: Art = Art {
    sixels: &[
        "..........w...w...w.........",
        ".........w...w...w..........",
        "..........w...w...w.........",
        "............................",
        "......cccccccccccccc........",
        "......cccccccccccccccccc....",
        "......cccccccccccccc..cc....",
        "......cccccccccccccc..cc....",
        "......cccccccccccccc..cc....",
        "......cccccccccccccccccc....",
        "......cccccccccccccc........",
        "......cccccccccccccc........",
        ".......cccccccccccc.........",
        "........cccccccccc..........",
        "............................",
        "..wwwwwwwwwwwwwwwwwwwwww....",
        "............................",
        "............................",
    ],
    backgrounds: SPACE_BLACK,
};

/// The station's ring, turning among the stars.
const RING: Art = Art {
    sixels: &[
        "y...........wwww...........y",
        ".........wwwwwwwwww.........",
        ".......www...ww...www.......",
        "......ww.....ww.....ww......",
        ".....ww......ww......ww.....",
        "....ww.......ww.......ww....",
        "....ww......wwww......ww....",
        "...ww......wwwwww......ww...",
        "...wwwwwwwwwwwwwwwwwwwwww...",
        "...wwwwwwwwwwwwwwwwwwwwww...",
        "...ww......wwwwww......ww...",
        "....ww......wwww......ww....",
        "....ww.......ww.......ww....",
        ".....ww......ww......ww.....",
        "......ww.....ww.....ww......",
        ".......www...ww...www.......",
        ".........wwwwwwwwww.........",
        "y...........wwww..........y.",
    ],
    backgrounds: SPACE_BLACK,
};

/// The greenhouse dome, overgrown.
const DOME: Art = Art {
    sixels: &[
        "..........cccccccc..........",
        "........cccccccccccc........",
        "......ccc..........ccc......",
        ".....cc..............cc.....",
        "....cc....gg....gg....cc....",
        "....cc...gggg..gggg...cc....",
        "...cc....gggggggggg....cc...",
        "...cc.....gggggggg.....cc...",
        "...cc...rr...gg...rr...cc...",
        "..cc...rrrr..gg..rrrr...cc..",
        "..cc....rr...gg...rr....cc..",
        "..cc...gg....gg....gg...cc..",
        "..cc....gg...gg...gg....cc..",
        "..cc.....gg..gg..gg.....cc..",
        "..cc......gggggggg......cc..",
        "wwwwwwwwwwwwwwwwwwwwwwwwwwww",
        "wwwwwwwwwwwwwwwwwwwwwwwwwwww",
        "............................",
    ],
    backgrounds: SPACE_BLACK,
};

/// The reactor's warning: the radiation trefoil.
const REACTOR: Art = Art {
    sixels: &[
        "............................",
        "..........yyyyyyyy..........",
        "..........yyyyyyyy..........",
        "..........yyyyyyyy..........",
        "...........yyyyyy...........",
        "............yyyy............",
        "............y..y............",
        ".............yy.............",
        "............yyyy............",
        ".....yyyyyy.yyyy.yyyyyy.....",
        ".....yyyyyyy.yy.yyyyyyy.....",
        ".....yyyyyyy....yyyyyyy.....",
        ".....yyyyyyy....yyyyyyy.....",
        "......yyyyy......yyyyy......",
        ".......yyy........yyy.......",
        "........yy........yy........",
        "............................",
        "............................",
    ],
    backgrounds: SPACE_BLACK,
};

/// The bridge's window on the planet below.
const WINDOW: Art = Art {
    sixels: &[
        "wwwwwwwwwwwwwwwwwwwwwwwwwwww",
        "ww........................ww",
        "ww..y.........y.......y...ww",
        "ww........................ww",
        "ww......y...........y.....ww",
        "ww........................ww",
        "ww..y.........y...........ww",
        "ww........................ww",
        "ww.......cccccccccc.......ww",
        "ww....cccccccccccccccc....ww",
        "ww..cccccccccccccccccccc..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "ww..bbbbbbbbbbbbbbbbbbbb..ww",
        "wwwwwwwwwwwwwwwwwwwwwwwwwwww",
    ],
    backgrounds: SPACE_BLACK,
};

/// The escape shuttle, nose up.
const SHUTTLE: Art = Art {
    sixels: &[
        "............................",
        ".............ww.............",
        "............wwww............",
        "........wwwwwwwwwwww........",
        "........wwwwccccwwww........",
        "........wwwwccccwwww........",
        "........wwwwwwwwwwww........",
        "........wwwwwwwwwwww........",
        "......wwwwwwwwwwwwwwww......",
        "....wwwwwwwwwwwwwwwwwwww....",
        "..wwwwwwwwwwwwwwwwwwwwwwww..",
        "..wwww....wwwwwwww....wwww..",
        "..........wwwwwwww..........",
        "..........wwwwwwww..........",
        "...........yyyyyy...........",
        "............yyyy............",
        "............rrrr............",
        ".............rr.............",
    ],
    backgrounds: SPACE_BLACK,
};

/// The picture of `section`.
#[must_use]
pub const fn of(section: Section) -> &'static Art {
    match section {
        Section::Habitat => &POD,
        Section::Living => &MUG,
        Section::Core => &RING,
        Section::Science => &DOME,
        Section::Engineering => &REACTOR,
        Section::Command => &WINDOW,
        Section::Docking => &SHUTTLE,
    }
}

/// Every picture, for tests.
#[cfg(test)]
pub const ALL: [&Art; 7] = [&POD, &MUG, &RING, &DOME, &REACTOR, &WINDOW, &SHUTTLE];
