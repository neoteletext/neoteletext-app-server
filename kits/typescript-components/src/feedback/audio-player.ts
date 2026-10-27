// A player for the viewer's stream (core/stream.ts): what plays and how far
// (now-playing.ts), and buttons to pause it, resume it and stop it, one
// width, under it. It looks again twice a second while someone watches, so
// its buttons say what pressing them does now.

import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { Listening } from "../core/stream.ts";
import type { Colour } from "../kit.ts";
import { Button } from "../input/button.ts";
import { Columns, Gap, Rows } from "../layout/layouts.ts";
import { ListeningView, LOOK_MS } from "./now-playing.ts";

const PAUSE = "pause";
const RESUME = "resume";
const STOP = "stop";
/** Every button one width: the widest label's. */
const BUTTON_WIDTH = Math.max(...[PAUSE, RESUME, STOP].map((label) => Button.widthOf(label)));

export interface AudioPlayerProps {
  readonly foreground?: Colour;
}

export class AudioPlayer extends Component<AudioPlayerProps, { readonly listening: Listening | null }> {
  override state = { listening: null as Listening | null };

  #look(): void {
    const session = this.services.session;
    if (session !== null) {
      this.setState({ listening: this.services.listening(session) });
    }
  }

  override componentDidMount(): void {
    if (this.services.session === null) {
      return;
    }
    this.#look();
    this.setInterval(() => this.#look(), LOOK_MS, { whileViewed: true });
  }

  override render(): Child {
    const { listening } = this.state;
    const state = listening?.state ?? null;
    const paused = state === "paused";
    const going = state === "playing" || state === "waiting";
    return h(
      Rows,
      {},
      h(ListeningView, { listening, ...(this.props.foreground === undefined ? {} : { foreground: this.props.foreground }) }),
      h(
        Columns,
        { widths: [BUTTON_WIDTH, 1, BUTTON_WIDTH], height: 1 },
        h(Button, {
          label: paused ? RESUME : PAUSE,
          width: BUTTON_WIDTH,
          disabled: !paused && !going,
          onPress: (session: number) => {
            if (paused) {
              this.services.resumeStream(session);
            } else {
              this.services.pauseStream(session);
            }
            this.#look();
          },
        }),
        h(Gap, {}),
        h(Button, {
          label: STOP,
          width: BUTTON_WIDTH,
          disabled: listening === null,
          onPress: (session: number) => {
            this.services.stopStream(session);
            this.#look();
          },
        }),
      ),
    );
  }
}
