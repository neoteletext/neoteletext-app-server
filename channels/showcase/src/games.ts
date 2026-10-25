// Action games (pages 300 to 399; crates/teletext-showcase/src/games.rs):
// invaders with the sci-fi sprites, snake, breakout and a cave flyer,
// each viewer's game its own, and a hall of fame every viewer shares.
//
// One application serves them all (decision 7, scale): it keeps a small
// state for each viewer who is playing, and one timer ticks every game in
// play at once; with nobody playing it asks for no time at all. The menu,
// the hall of fame and each game's title page are the channel's, one copy
// for every viewer; a game in play is its viewer's own page, which
// overrides the title page for that viewer alone (SERVER.md section 2).

import type { Area, Section } from "./area.ts";
import { Arcade } from "./games/arcade.ts";
import { MENU } from "./games/pages.ts";

/** Action games. */
export const games: Area = {
  applications: (pictures) => [new Arcade(pictures)],
  sections: (): readonly Section[] => [{ page: MENU, name: "Action games", about: "invaders, snake, breakout, cave" }],
};
