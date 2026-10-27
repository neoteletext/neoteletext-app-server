// What's That Sound? (128), the podcast's game: every round
// whatsthatsound found, each a mystery sound played on one episode and
// revealed on the next, newest first. A round chosen from the list plays
// its challenge, as the episode played it; the buttons under the list play
// the challenge, the reveal from the next episode, or the sound alone, and
// the player under them pauses and stops it. Each viewer has the page to themselves: their own round chosen,
// and what they are hearing under it.

import { Colour } from "../kit.ts";
import { AudioPlayer, Button, Centred, type Child, ChoiceList, Columns, Component, DATE_FORMATS, DateText, Gap, h, type PageSpec, type RowLook, Text } from "../../../../kits/typescript-components/src/index.ts";
import type { Round } from "../library.ts";
import { frame, prompts, type Site } from "../parts.ts";
import { GAME_LOOK, PAGES } from "../site.ts";

type Clip = keyof Round["clips"];

/** The list's rows: as many as leave room for the three buttons, a blank row above each, and the player. */
const SHOWN = 11;
/** What each button plays, what it says, and its colour. */
const BUTTONS: readonly (readonly [Clip, string, Colour])[] = [
  ["play", "play the challenge", Colour.Green],
  ["reveal", "play the reveal", Colour.Yellow],
  ["sound", "play just the sound", Colour.Magenta],
];
/** Every button the widest's width, with a cell more each side than it needs. */
const BUTTON_WIDTH = Math.max(...BUTTONS.map(([, label]) => Button.widthOf(label, 2, 2)));

/** "Podcast episode 127": without a "#" the browser may take the number for a page (the owner's choice, 27 October 2026). */
function episodeText(number: number): string {
  return `Podcast episode ${number}`;
}

/** The episode column: "Podcast episode 377", its colour's cell and a space. */
const EPISODE_CELLS = 21;

/** A round's row: its episode and the day it was played, in the row's look. */
function roundRow(round: Round, look: RowLook): Child {
  return h(Columns, { widths: [EPISODE_CELLS, null] }, h(Text, { text: episodeText(round.played), foreground: look.foreground }), h(DateText, { value: round.playedOn, pattern: DATE_FORMATS.long, foreground: look.foreground }));
}

function clipSource(site: Site, round: Round, clip: Clip): ReturnType<Site["clips"]["source"]> {
  // What it is first: "Now playing" has room for about 28 characters.
  const names: Record<Clip, string> = {
    play: `The challenge, episode ${round.played}`,
    reveal: `The reveal, episode ${round.revealed}`,
    sound: `The sound, episode ${round.played}`,
  };
  return site.clips.source(names[clip], `${site.roundClips}/${round.clips[clip]}`, clip === "sound" ? round.soundSeconds : null);
}

interface RoundsProps {
  readonly site: Site;
  /** Newest first. */
  readonly rounds: readonly Round[];
}

class Rounds extends Component<RoundsProps, { readonly chosen: number }> {
  override state = { chosen: 0 };

  #play(clip: Clip, session: number, index = this.state.chosen): void {
    const round = this.props.rounds[index];
    if (round !== undefined) {
      this.services.stream(clipSource(this.props.site, round, clip), session);
    }
  }

  override render(): Child {
    const { rounds } = this.props;
    return [
      h(ChoiceList<Round>, {
        items: rounds,
        render: roundRow,
        shown: SHOWN,
        onMove: (chosen: number) => this.setState({ chosen }),
        onChoose: (chosen: number, session: number) => this.#play("play", session, chosen),
      }),
      rounds[this.state.chosen] === undefined
        ? null
        : h(
            Centred,
            {},
            ...BUTTONS.flatMap(([clip, label, background]) => [h(Gap, { key: `gap-${clip}` }), h(Button, { key: clip, label, background, width: BUTTON_WIDTH, onPress: (session: number) => this.#play(clip, session) })]),
          ),
      h(AudioPlayer, {}),
    ];
  }
}

export function soundGamePages(site: Site): PageSpec[] {
  const rounds = [...site.library.rounds].sort((a, b) => b.playedOn.localeCompare(a.playedOn) || b.round - a.round);
  return [
    {
      page: PAGES.rounds,
      title: "What's that sound?",
      perViewer: true,
      root: () =>
        frame(GAME_LOOK, "What's that sound?", prompts(["Podcast", PAGES.podcast], ["Episodes", PAGES.episodes], ["Playing", PAGES.player], ["Front", PAGES.front]), "Enter or a click plays the challenge", h(Rounds, { site, rounds })),
    },
  ];
}
