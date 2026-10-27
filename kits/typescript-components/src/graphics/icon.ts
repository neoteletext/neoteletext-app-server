// Icons in mosaic sixels, three cells across and two rows down (six by six
// sixels), as a teletext weather map's suns and clouds were: the weather,
// marks for notices, and a few everyday things. Each is drawn in its own
// colour, or the one given; its colour's cell is the gap before it.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { Colour, SIXELS_ACROSS, SIXELS_DOWN } from "../kit.ts";
import { type Bitmap, sixelsOf } from "./mosaic.ts";

export type IconName = "sun" | "cloud" | "rain" | "snow" | "storm" | "tick" | "cross" | "warning" | "info" | "heart" | "star" | "home" | "mail" | "music";

/** Each icon's sixels, "#" set, and its colour. */
export const ICONS: Readonly<Record<IconName, { readonly bitmap: Bitmap; readonly foreground: Colour }>> = {
  sun: { bitmap: ["#.##.#", ".####.", "######", "######", ".####.", "#.##.#"], foreground: Colour.Yellow },
  cloud: { bitmap: ["......", "..##..", ".####.", "######", "######", "......"], foreground: Colour.White },
  rain: { bitmap: [".####.", "######", "......", "#.#.#.", ".#.#.#", "#.#.#."], foreground: Colour.Cyan },
  snow: { bitmap: ["#..#..", ".##..#", "#..#..", "..#..#", "#..##.", "..#..#"], foreground: Colour.White },
  storm: { bitmap: [".####.", "######", "...#..", "..##..", ".####.", "..#..."], foreground: Colour.Yellow },
  tick: { bitmap: [".....#", "....##", "#..##.", "####..", ".##...", "......"], foreground: Colour.Green },
  cross: { bitmap: ["#....#", ".#..#.", "..##..", "..##..", ".#..#.", "#....#"], foreground: Colour.Red },
  warning: { bitmap: ["..##..", "..##..", ".#..#.", ".#..#.", "#.##.#", "######"], foreground: Colour.Yellow },
  info: { bitmap: ["..##..", "......", ".###..", "..##..", "..##..", ".####."], foreground: Colour.Cyan },
  heart: { bitmap: [".#..#.", "######", "######", ".####.", "..##..", "......"], foreground: Colour.Red },
  star: { bitmap: ["..##..", "######", ".####.", ".####.", "##..##", "#....#"], foreground: Colour.Yellow },
  home: { bitmap: ["..##..", ".####.", "######", ".#..#.", ".#..#.", ".####."], foreground: Colour.White },
  mail: { bitmap: ["######", "##..##", "#.##.#", "#....#", "######", "......"], foreground: Colour.White },
  music: { bitmap: ["..####", "..#..#", "..#..#", "..#..#", "###.##", "##.###"], foreground: Colour.Magenta },
};

export interface IconProps {
  readonly name: IconName;
  readonly foreground?: Colour;
}

/** The icon's size: six sixels each way, after its colour's cell. */
const ICON_SIXELS = 6;

export class Icon extends Component<IconProps> {
  override measure(space: Size): Size {
    return { width: Math.min(space.width, ICON_SIXELS / SIXELS_ACROSS + 1), height: Math.min(space.height, ICON_SIXELS / SIXELS_DOWN) };
  }

  override paint(canvas: Canvas): void {
    const icon = ICONS[this.props.name];
    canvas.sixels(0, 1, this.props.foreground ?? icon.foreground, sixelsOf(icon.bitmap));
  }
}
