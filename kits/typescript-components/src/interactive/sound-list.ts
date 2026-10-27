// A list of sounds: choosing one plays it to the viewer.

import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { RATE, type Sound, sound } from "../core/sound.ts";
import { Colour } from "../kit.ts";
import { Rows } from "../layout/layouts.ts";
import { ChoiceList } from "../lists/choice-list.ts";
import { Flashing } from "../text/styles.ts";

export interface SoundListProps {
  readonly sounds: readonly Sound[];
  readonly shown: number;
}

interface SoundListState {
  readonly playing: string | null;
}

/** Sounds to choose from: Enter or a click plays one to the viewer, and says so flashing. */
export class SoundList extends Component<SoundListProps, SoundListState> {
  override state: SoundListState = { playing: null };

  override render(): Child {
    return h(
      Rows,
      {},
      h(ChoiceList, {
        items: this.props.sounds.map((sound) => `${sound.name} (${(sound.samples.length / RATE).toFixed(1)}s)`),
        shown: this.props.shown,
        onChoose: (index: number, session: number) => {
          const sound = this.props.sounds[index];
          if (sound !== undefined) {
            this.services.play(sound, session);
            this.setState({ playing: sound.name });
          }
        },
      }),
      this.state.playing === null ? null : h(Flashing, { text: `Playing: ${this.state.playing}`, foreground: Colour.Green }),
    );
  }
}
