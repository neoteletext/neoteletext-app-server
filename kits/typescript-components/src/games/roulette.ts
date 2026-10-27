// A roulette wheel that spins down to a random article, a sound at each
// step.

import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent } from "../core/events.ts";
import type { Sound } from "../core/sound.ts";
import type { Article } from "../documents/document.ts";
import { Wheel } from "../graphics/wheel.ts";
import { Colour, Key } from "../kit.ts";
import { At, Gap, Layers, Rows } from "../layout/layouts.ts";
import { Headline } from "../text/news.ts";
import { Paragraph } from "../text/styles.ts";
import { Control, type ControlProps } from "../input/control.ts";

/** The roulette's steps, in milliseconds between them: fast, then slowing to a stop. */
export const SPIN_STEPS_MS = [...new Array<number>(10).fill(60), 80, 100, 130, 170, 220, 280, 350, 450] as const;

export interface RouletteProps extends ControlProps {
  readonly articles: readonly Article[];
  readonly tick?: Sound;
  readonly done?: Sound;
}

interface RouletteState {
  /** The step of the spin, or null when it is still. */
  readonly step: number | null;
  readonly lit: number;
  /** The article it stopped on, or flicks past while it spins. */
  readonly article: number | null;
}

/** Enter spins the wheel; each step lights the next segment and shows a title flicking past; it stops on one, and Enter again reads it. */
export class Roulette extends Control<RouletteProps, RouletteState> {
  override state: RouletteState = { step: null, lit: 0, article: null };

  protected override keysHint(): string {
    return "Enter spins; Enter again reads it";
  }
  #session = 0;


  #spin(step: number): void {
    const wait = SPIN_STEPS_MS[step];
    const article = this.services.below(this.props.articles.length);
    if (wait === undefined) {
      this.setState({ step: null, article });
      if (this.props.done !== undefined) {
        this.services.play(this.props.done, this.#session);
      }
      return;
    }
    this.setState({ step, lit: this.state.lit + 1, article });
    if (this.props.tick !== undefined) {
      this.services.play(this.props.tick, this.#session);
    }
    this.setTimeout(() => this.#spin(step + 1), wait);
  }

  override render(): Child {
    const article = this.state.article === null ? undefined : this.props.articles[this.state.article];
    const spinning = this.state.step !== null;
    return h(
      Layers,
      {},
      h(At, { row: 1, column: 10, rows: 11 }, h(Wheel, { lit: this.state.lit })),
      h(
        At,
        { row: 13, column: 0 },
        h(
          Rows,
          {},
          article === undefined
            ? h(Paragraph, { text: "Press Enter to spin the wheel", foreground: Colour.Cyan })
            : h(Headline, { title: article.title, page: article.page, foreground: spinning ? Colour.White : Colour.Yellow }),
          h(Gap, {}),
          h(Paragraph, { text: spinning ? "Spinning..." : article === undefined ? "" : "Enter reads it, or spin again with Space", foreground: Colour.Green }),
        ),
      ),
    );
  }

  override onKey(event: KeyEvent): boolean {
    if (this.state.step !== null) {
      return isPress(event, Key.Enter) || isPress(event, " ");
    }
    const article = this.state.article === null ? undefined : this.props.articles[this.state.article];
    if (isPress(event, Key.Enter) && article !== undefined) {
      this.services.goto(event.session, article.page);
      return true;
    }
    if (isPress(event, Key.Enter) || isPress(event, " ")) {
      this.#session = event.session;
      this.#spin(0);
      return true;
    }
    return false;
  }
}
