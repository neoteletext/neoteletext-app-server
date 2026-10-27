// A text field that suggests: the items that match what is typed show
// under it, the first chosen; Up and Down choose another, Tab completes the
// field to it, Enter takes it. With every item shown while nothing is
// typed, it is a combo box. Its value is the item taken; its field keeps
// the keys, and what it does not take comes here.

import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent } from "../core/events.ts";
import { listStep, moveChoice } from "../core/list-cursor.ts";
import { Key } from "../kit.ts";
import { Rows } from "../layout/layouts.ts";
import { ListRows } from "../lists/list-row.ts";
import { type ValueProps, ValueControl, type ValueState } from "./control.ts";
import { TextField } from "./text-field.ts";

export interface AutocompleteProps extends ValueProps<string> {
  readonly items: readonly string[];
  readonly placeholder?: string;
  /** The most suggestions shown. */
  readonly shown: number;
  /** Matches anywhere in an item, not only at its start. */
  readonly anywhere?: boolean;
  /** Every item shows while nothing is typed: a combo box. */
  readonly showAll?: boolean;
  /** What it says when nothing matches. */
  readonly empty?: string;
  /** The most characters typed. */
  readonly longest?: number;
}

interface AutocompleteState extends ValueState<string> {
  readonly typed: string;
  readonly chosen: number;
}

const LONGEST = 30;

export class Autocomplete extends ValueControl<string, AutocompleteProps, AutocompleteState> {
  override state: AutocompleteState = { own: this.props.initial ?? "", typed: this.props.initial ?? "", chosen: 0 };

  protected empty(): string {
    return "";
  }

  /** Its field has the keys. */
  override focusable(): boolean {
    return false;
  }

  #matches(): string[] {
    const wanted = this.state.typed.toLowerCase();
    if (wanted === "") {
      return this.props.showAll === true ? [...this.props.items] : [];
    }
    return this.props.items.filter((item) => (this.props.anywhere === true ? item.toLowerCase().includes(wanted) : item.toLowerCase().startsWith(wanted)));
  }

  #take(item: string, session: number): void {
    this.setState({ typed: item, chosen: 0 });
    this.change(item, session);
    this.activate(item, session);
  }

  override render(): Child {
    const matches = this.#matches();
    return h(
      Rows,
      {},
      h(TextField, {
        longest: this.props.longest ?? LONGEST,
        value: this.state.typed,
        disabled: this.disabled,
        hint: "Type; Up Down choose, Tab completes, Enter takes",
        ...(this.props.label === undefined ? {} : { label: this.props.label, labelCells: this.labelCells }),
        ...(this.props.placeholder === undefined ? {} : { placeholder: this.props.placeholder }),
        onChange: (typed: string) => this.setState({ typed, chosen: 0 }),
      }),
      this.state.typed === "" && this.props.showAll !== true
        ? null
        : h(ListRows<string>, {
            items: matches,
            shown: this.props.shown,
            chosen: this.state.chosen,
            focused: true,
            indent: () => this.labelCells,
            disabled: () => this.disabled,
            empty: this.props.empty ?? "no suggestions",
            onPick: (index: number, session: number) => {
              const item = matches[index];
              if (item !== undefined) {
                this.#take(item, session);
              }
            },
          }),
    );
  }

  override onKey(event: KeyEvent): boolean {
    const matches = this.#matches().slice(0, this.props.shown);
    const chosen = matches[this.state.chosen];
    if (isPress(event, Key.Enter) && chosen !== undefined) {
      this.#take(chosen, event.session);
      return true;
    }
    if (isPress(event, Key.Tab) && chosen !== undefined) {
      this.setState({ typed: chosen, chosen: 0 });
      return true;
    }
    const step = listStep(event, this.props.shown);
    if (step === null || matches.length === 0 || isPress(event, Key.Home) || isPress(event, Key.End)) {
      return false;
    }
    this.setState({ chosen: moveChoice(this.state.chosen, step, matches.length) });
    this.play("move", event.session);
    return true;
  }
}
