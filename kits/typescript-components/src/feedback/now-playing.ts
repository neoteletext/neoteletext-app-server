// What the viewer is listening to: the stream the runtime plays them
// (core/stream.ts), its name, how far it has played of how long, and
// whether it is waiting, playing, paused, done or cannot play. It looks
// again twice a second while someone watches; the copy every viewer shares
// has nobody to look for, and says nothing plays. ListeningView is the
// display alone, for a player that looks for itself (audio-player.ts).

import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { Listening } from "../core/stream.ts";
import { Colour } from "../kit.ts";
import { Rows } from "../layout/layouts.ts";
import { Paragraph } from "../text/styles.ts";
import { ProgressBar } from "./progress.ts";

/** How often it looks again at what is playing. */
export const LOOK_MS = 500;
/** The time's column before the bar: "1:02:03/1:02:03". */
const TIME_CELLS = 11;

/** "1:02:03" or "2:03". */
export function clockTime(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = String(whole % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

export interface ListeningViewProps {
  /** What plays, or null for nothing. */
  readonly listening: Listening | null;
  readonly foreground?: Colour;
}

/** A stream as the viewer sees it: what it is and whether it plays, and how far it has played of how long. */
export class ListeningView extends Component<ListeningViewProps> {
  override render(): Child {
    const { listening } = this.props;
    if (listening === null) {
      return h(Paragraph, { text: "Nothing playing", foreground: Colour.Blue, lines: 1 });
    }
    const states: Record<Listening["state"], string> = {
      failed: `Cannot play: ${listening.failure ?? ""}`,
      waiting: "Getting it ready...",
      playing: "Playing",
      paused: "Paused",
      ended: "Played to the end",
    };
    const heard = Math.floor(listening.heardMs / 1000);
    const length = listening.lengthMs === null ? null : Math.floor(listening.lengthMs / 1000);
    return h(
      Rows,
      {},
      h(Paragraph, { text: `${states[listening.state]}: ${listening.name}`, foreground: listening.state === "failed" ? Colour.Red : listening.state === "paused" ? Colour.Yellow : Colour.Green, lines: 1 }),
      h(ProgressBar, { label: `${clockTime(heard)}${length === null ? "" : `/${clockTime(length)}`}`, value: heard, max: length ?? Math.max(1, heard), foreground: this.props.foreground ?? Colour.Magenta, labelCells: TIME_CELLS }),
    );
  }
}

export interface NowPlayingProps {
  readonly foreground?: Colour;
}

/** The viewer's stream, looked at again twice a second. */
export class NowPlaying extends Component<NowPlayingProps, { readonly listening: Listening | null }> {
  override state = { listening: null as Listening | null };

  override componentDidMount(): void {
    const session = this.services.session;
    if (session === null) {
      return;
    }
    const look = (): void => this.setState({ listening: this.services.listening(session) });
    look();
    this.setInterval(look, LOOK_MS, { whileViewed: true });
  }

  override render(): Child {
    return h(ListeningView, { listening: this.state.listening, ...(this.props.foreground === undefined ? {} : { foreground: this.props.foreground }) });
  }
}
