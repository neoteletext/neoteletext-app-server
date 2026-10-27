// Large digits drawn in mosaic graphics, five sixels by seven, for counters.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { Colour, Sixels, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";

/** Large characters, five sixels wide and seven high, for counters. */
const DIGITS: Readonly<Record<string, readonly string[]>> = {
  "0": ["#####", "#...#", "#..##", "#.#.#", "##..#", "#...#", "#####"],
  "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
  "2": ["#####", "....#", "....#", "#####", "#....", "#....", "#####"],
  "3": ["#####", "....#", "....#", ".####", "....#", "....#", "#####"],
  "4": ["#...#", "#...#", "#...#", "#####", "....#", "....#", "....#"],
  "5": ["#####", "#....", "#....", "#####", "....#", "....#", "#####"],
  "6": ["#####", "#....", "#....", "#####", "#...#", "#...#", "#####"],
  "7": ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
  "8": ["#####", "#...#", "#...#", "#####", "#...#", "#...#", "#####"],
  "9": ["#####", "#...#", "#...#", "#####", "....#", "....#", "#####"],
  ":": [".....", "..#..", "..#..", ".....", "..#..", "..#..", "....."],
  "-": [".....", ".....", ".....", "#####", ".....", ".....", "....."],
};

const DIGIT_WIDTH = 5;

const DIGIT_HEIGHT = 7;

const DIGIT_GAP = 1;

/** The rows large digits take: seven sixels over three to a cell. */
export const BIG_ROWS = Math.ceil(DIGIT_HEIGHT / SIXELS_DOWN);

export interface BigDigitsProps {
  readonly text: string;
  readonly foreground?: Colour;
}

/** `text` in large mosaic digits. */
export class BigDigits extends Component<BigDigitsProps> {
  #canvas(): Sixels {
    const width = this.props.text.length * (DIGIT_WIDTH + DIGIT_GAP);
    const sixels = new Sixels(width, DIGIT_HEIGHT);
    [...this.props.text].forEach((character, place) => {
      (DIGITS[character] ?? DIGITS["-"] ?? []).forEach((pattern, y) => {
        [...pattern].forEach((sixel, x) => sixels.set(place * (DIGIT_WIDTH + DIGIT_GAP) + x, y, sixel === "#"));
      });
    });
    return sixels;
  }

  override measure(space: Size): Size {
    return { width: Math.min(space.width, Math.ceil(this.#canvas().width / SIXELS_ACROSS) + 1), height: Math.min(BIG_ROWS, space.height) };
  }

  override paint(canvas: Canvas): void {
    canvas.sixels(0, 1, this.props.foreground ?? Colour.Green, this.#canvas());
  }
}
