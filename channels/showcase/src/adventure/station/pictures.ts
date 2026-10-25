// A picture for each of Station Zero's sections, 28 sixels across and 18
// down: fourteen cells by six rows, beside the room's description
// (crates/teletext-showcase/src/adventure/station/pictures.rs).

import { Art } from "../art.ts";
import { Section } from "./world.ts";

/** The rows of cells each picture takes. */
export const ROWS = 6;

/** No background on any row: the pictures float in space. */
const SPACE_BLACK = "......";

/** The cryo pod the player wakes in. */
const POD = new Art(
  [
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
  SPACE_BLACK,
);

/** A mug of coffee, steaming, as the crew left it. */
const MUG = new Art(
  [
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
  SPACE_BLACK,
);

/** The station's ring, turning among the stars. */
const RING = new Art(
  [
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
  SPACE_BLACK,
);

/** The greenhouse dome, overgrown. */
const DOME = new Art(
  [
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
  SPACE_BLACK,
);

/** The reactor's warning: the radiation trefoil. */
const REACTOR = new Art(
  [
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
  SPACE_BLACK,
);

/** The bridge's window on the planet below. */
const WINDOW = new Art(
  [
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
  SPACE_BLACK,
);

/** The escape shuttle, nose up. */
const SHUTTLE = new Art(
  [
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
  SPACE_BLACK,
);

/** The picture of `section`. */
export function of(section: Section): Art {
  switch (section) {
    case Section.Habitat:
      return POD;
    case Section.Living:
      return MUG;
    case Section.Core:
      return RING;
    case Section.Science:
      return DOME;
    case Section.Engineering:
      return REACTOR;
    case Section.Command:
      return WINDOW;
    case Section.Docking:
      return SHUTTLE;
  }
}

/** Every picture, for tests. */
export const ALL: readonly Art[] = [POD, MUG, RING, DOME, REACTOR, WINDOW, SHUTTLE];
