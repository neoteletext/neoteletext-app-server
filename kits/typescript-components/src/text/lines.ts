// Lines of a document: an article's blocks laid out as coloured lines, and
// lines drawn as they are.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { paintEmpty } from "../core/look.ts";
import { fit, wrap } from "../core/text.ts";
import { Colour } from "../kit.ts";

/** One block of an article: a paragraph, a heading, a quotation or code, or an item of a list. */
export interface Block {
  readonly kind: "paragraph" | "heading" | "quote" | "item";
  readonly text: string;
}

/** A line of a document: its colour and text. */
export interface Line {
  readonly foreground: Colour;
  readonly text: string;
}

/** An article's blocks as lines of `width`: headings yellow, quotations cyan, items marked, a blank line after each block. */
export function articleLines(blocks: readonly Block[], width: number): Line[] {
  const lines: Line[] = [];
  for (const block of blocks) {
    if (block.kind === "item") {
      wrap(block.text, width - 2).forEach((line, index) => lines.push({ foreground: Colour.White, text: `${index === 0 ? "* " : "  "}${line}` }));
    } else {
      const foreground = block.kind === "heading" ? Colour.Yellow : block.kind === "quote" ? Colour.Cyan : Colour.White;
      lines.push(...wrap(block.text, width).map((text) => ({ foreground, text })));
    }
    lines.push({ foreground: Colour.White, text: "" });
  }
  while (lines.length > 0 && lines[lines.length - 1]?.text === "") {
    lines.pop();
  }
  return lines;
}

export interface LinesProps {
  readonly lines: readonly Line[];
  /** What it says when there are no lines; nothing when absent. */
  readonly empty?: string;
}

/** Lines as they are, a row each. */
export class LinesBlock extends Component<LinesProps> {
  override measure(space: Size): Size {
    const rows = this.props.lines.length === 0 && this.props.empty !== undefined ? 1 : this.props.lines.length;
    return { width: space.width, height: Math.min(rows, space.height) };
  }

  override paint(canvas: Canvas): void {
    if (this.props.lines.length === 0 && this.props.empty !== undefined) {
      paintEmpty(canvas, this.props.empty);
      return;
    }
    this.props.lines.forEach((line, row) => {
      if (line.text !== "") {
        canvas.coloured(row, 0, line.foreground, fit(line.text, canvas.width - 1));
      }
    });
  }
}
