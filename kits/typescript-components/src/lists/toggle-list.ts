// A list whose items are switched on and off, each marked with a star:
// Up and Down choose (core/list-cursor.ts), Enter, Space or a click
// switches one. Its value is the items on. Its rows are ListRows
// (list-row.ts), controls the browser lights as the pointer passes.

import type { Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import type { KeyEvent } from "../core/events.ts";
import { choosing } from "../core/keys.ts";
import { listStep, moveChoice } from "../core/list-cursor.ts";
import { type ValueProps, ValueControl, type ValueState } from "../input/control.ts";
import { ListRows } from "./list-row.ts";

export interface ToggleListProps extends ValueProps<readonly number[]> {
  readonly items: readonly string[];
  readonly empty?: string;
}

interface ToggleListState extends ValueState<readonly number[]> {
  readonly chosen: number;
}

export class ToggleList extends ValueControl<readonly number[], ToggleListProps, ToggleListState> {
  override state: ToggleListState = { own: this.props.initial ?? [], chosen: 0 };

  protected empty(): readonly number[] {
    return [];
  }

  protected override keysHint(): string {
    return "Up Down, then Enter or a click to star";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(Math.max(1, this.props.items.length), space.height) };
  }

  #toggle(index: number, session: number): void {
    this.setState({ chosen: index });
    this.change(this.value.includes(index) ? this.value.filter((item) => item !== index) : [...this.value, index].sort((a, b) => a - b), session);
  }

  override render(): Child {
    return h(ListRows<string>, {
      items: this.props.items,
      shown: this.props.items.length,
      chosen: this.state.chosen,
      focused: this.focused,
      empty: this.props.empty ?? "nothing to star",
      text: (item: string, index: number) => `${this.value.includes(index) ? "*" : " "} ${item}`,
      disabled: () => this.disabled,
      onPick: (index: number, session: number) => this.#toggle(index, session),
    });
  }

  override onKey(event: KeyEvent): boolean {
    if (this.disabled) {
      return false;
    }
    if (choosing(event)) {
      this.#toggle(this.state.chosen, event.session);
      return true;
    }
    const step = listStep(event, this.props.items.length);
    if (step === null) {
      return false;
    }
    this.setState({ chosen: moveChoice(this.state.chosen, step, this.props.items.length) });
    this.play("move", event.session);
    return true;
  }
}
