// A live poll: every viewer's copy hears each vote through the bus.

import { Bar } from "../charts/bar-chart.ts";
import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent } from "../core/events.ts";
import { pad } from "../core/text.ts";
import { Colour, Key } from "../kit.ts";
import { Rows } from "../layout/layouts.ts";
import { Paragraph } from "../text/styles.ts";
import { Control, type ControlProps } from "../input/control.ts";

/** A poll's votes, kept by the page that makes it and told to every copy through the bus: no state is global. */
export class Tally {
  readonly name: string;
  readonly options: readonly string[];
  readonly #votes: number[];

  constructor(name: string, options: readonly string[]) {
    this.name = name;
    this.options = options;
    this.#votes = options.map(() => 0);
  }

  get votes(): readonly number[] {
    return this.#votes;
  }

  /** Moves a vote from `from` (or none) to `to`. */
  move(from: number | null, to: number): void {
    if (from !== null && (this.#votes[from] ?? 0) > 0) {
      this.#votes[from] = (this.#votes[from] ?? 0) - 1;
    }
    this.#votes[to] = (this.#votes[to] ?? 0) + 1;
  }
}

export interface PollProps extends ControlProps {
  readonly tally: Tally;
}

interface PollState {
  readonly chosen: number;
  /** This viewer's vote. */
  readonly mine: number | null;
  readonly votes: readonly number[];
}

/** A live poll: Up and Down choose, Enter votes; every viewer's copy hears each vote through the bus. */
export class Poll extends Control<PollProps, PollState> {
  override state: PollState = { chosen: 0, mine: null, votes: this.props.tally.votes.slice() };

  protected override keysHint(): string {
    return "Up Down choose, Enter votes";
  }
  #unsubscribe: (() => void) | null = null;

  override componentDidMount(): void {
    this.#unsubscribe = this.services.bus.on("poll", ({ name, votes }) => {
      if (name === this.props.tally.name) {
        this.setState({ votes });
      }
    });
  }

  override componentWillUnmount(): void {
    this.#unsubscribe?.();
  }


  override render(): Child {
    const { options } = this.props.tally;
    const most = Math.max(1, ...this.state.votes);
    const total = this.state.votes.reduce((sum, count) => sum + count, 0);
    return h(
      Rows,
      {},
      ...options.flatMap((option, index) => [
        h(Paragraph, {
          key: `option-${index}`,
          text: `${index === this.state.chosen ? ">" : " "}${this.state.mine === index ? "*" : " "}${pad(option, 26)}${String(this.state.votes[index] ?? 0).padStart(4)}`,
          foreground: index === this.state.chosen ? Colour.Yellow : Colour.White,
        }),
        h(Bar, { key: `bar-${index}`, value: this.state.votes[index] ?? 0, max: most, foreground: Colour.Magenta }),
      ]),
      h(Paragraph, { text: `${total} votes, live${this.state.mine === null ? "" : ": * is yours"}`, foreground: Colour.Cyan }),
    );
  }

  override onKey(event: KeyEvent): boolean {
    const count = this.props.tally.options.length;
    if (isPress(event, Key.Enter)) {
      this.props.tally.move(this.state.mine, this.state.chosen);
      this.setState({ mine: this.state.chosen });
      this.services.bus.emit("poll", { name: this.props.tally.name, votes: this.props.tally.votes.slice() });
      return true;
    }
    const step = isPress(event, Key.Down) ? 1 : isPress(event, Key.Up) ? -1 : 0;
    if (step === 0) {
      return false;
    }
    this.setState({ chosen: (this.state.chosen + step + count) % count });
    return true;
  }
}
