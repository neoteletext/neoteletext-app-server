// A wizard: steps taken in order. The steps' names run along the top, the
// current one on a band and those done ticked; a progress bar shows how far
// along; the current step's content is under it; Back and Next are the red
// and green keys (F1 and F2 on a keyboard), so the page takes the coloured
// keys, and the last step's Next is Finish.

import { type Child, h } from "../core/element.ts";
import type { KeyEvent } from "../core/events.ts";
import { colourKeyOf } from "../core/keys.ts";
import type { PageNeeds } from "../core/needs.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { Colour } from "../kit.ts";
import { ProgressBar } from "../feedback/progress.ts";
import { Gap, Rows } from "../layout/layouts.ts";
import { Paragraph } from "../text/styles.ts";
import { Badges } from "../text/badge.ts";
import { CHOSEN } from "../core/look.ts";

export interface WizardProps extends ControlProps {
  /** The steps' names; the children are their contents, in order. */
  readonly steps: readonly string[];
  /** Whether the step may be left forward: a step with something missing says what. */
  readonly ready?: (step: number) => string | null;
  readonly onFinish?: (session: number) => void;
  readonly onStep?: (step: number, session: number) => void;
}

interface WizardState {
  readonly step: number;
  readonly finished: boolean;
  readonly complaint: string | null;
}

/** Back and Next: the red and green keys. */
const BACK = 0;
const NEXT = 1;

export class Wizard extends Control<WizardProps, WizardState> {
  override state: WizardState = { step: 0, finished: false, complaint: null };

  protected override keyNeeds(): PageNeeds {
    return { colourKeys: true };
  }

  protected override keysHint(): string {
    return "red Back, green Next; Tab to a step's fields";
  }

  override render(): Child {
    const { steps } = this.props;
    const { step, finished } = this.state;
    const last = step === steps.length - 1;
    const badges = steps.map((name, index) => ({
      text: `${index < step || finished ? "x" : String(index + 1)} ${name}`,
      background: index === step && !finished ? CHOSEN.background : index < step || finished ? Colour.Green : Colour.Blue,
      foreground: index === step && !finished ? CHOSEN.foreground : index < step || finished ? Colour.Black : Colour.White,
    }));
    return h(
      Rows,
      {},
      h(Badges, { badges }),
      h(ProgressBar, { label: finished ? "Done" : `Step ${step + 1} of ${steps.length}`, value: finished ? steps.length : step, max: steps.length, foreground: Colour.Cyan }),
      h(Gap, {}),
      finished ? h(Paragraph, { text: "All done. Thank you.", foreground: Colour.Green }) : ((this.props.children ?? [])[step] ?? null),
      h(Gap, { rows: 0, grow: 1 }),
      this.state.complaint === null ? null : h(Paragraph, { text: `! ${this.state.complaint}`, foreground: Colour.Red }),
      finished ? null : h(Paragraph, { text: `${step === 0 ? "" : "red Back   "}green ${last ? "Finish" : "Next"}`, foreground: Colour.Green }),
    );
  }

  #go(step: number, session: number): void {
    this.setState({ step, complaint: null });
    this.props.onStep?.(step, session);
  }

  override onKey(event: KeyEvent): boolean {
    if (this.state.finished) {
      return false;
    }
    const { step } = this.state;
    const key = colourKeyOf(event);
    if (key === BACK) {
      if (step > 0) {
        this.#go(step - 1, event.session);
      }
      return true;
    }
    if (key === NEXT) {
      const complaint = this.props.ready?.(step) ?? null;
      if (complaint !== null) {
        this.setState({ complaint });
      } else if (step === this.props.steps.length - 1) {
        this.setState({ finished: true, complaint: null });
        this.emit({ type: "submit", value: step, session: event.session, origin: "viewer" });
        this.props.onFinish?.(event.session);
      } else {
        this.#go(step + 1, event.session);
      }
      return true;
    }
    return false;
  }
}
