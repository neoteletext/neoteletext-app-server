// What the keys do: each key in yellow and its action in white, in one
// column or two.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { fit } from "../core/text.ts";
import { Colour } from "../kit.ts";

/** A key and what it does. */
export type KeyHelpEntry = readonly [key: string, action: string];

export interface KeyHelpProps {
  readonly keys: readonly KeyHelpEntry[];
  /** One column or two side by side. */
  readonly columns?: 1 | 2;
  /** The cells the keys' column takes. */
  readonly keyCells?: number;
}

/** The keys' column, after its colour cell, when none is given. */
const KEY_CELLS = 10;

export class KeyHelp extends Component<KeyHelpProps> {
  get #columns(): number {
    return this.props.columns ?? 1;
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(Math.ceil(this.props.keys.length / this.#columns), space.height) };
  }

  override paint(canvas: Canvas): void {
    const across = Math.floor(canvas.width / this.#columns);
    const keyCells = this.props.keyCells ?? (this.#columns === 1 ? KEY_CELLS : Math.floor(across / 2));
    const down = Math.ceil(this.props.keys.length / this.#columns);
    this.props.keys.forEach(([key, action], index) => {
      const row = index % down;
      const column = Math.floor(index / down) * across;
      canvas.coloured(row, column, Colour.Yellow, fit(key, keyCells - 1));
      canvas.coloured(row, column + keyCells, Colour.White, fit(action, across - keyCells - 1));
    });
  }
}
