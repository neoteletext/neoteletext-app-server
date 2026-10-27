// A tree: items that open to show the items under them. "+" marks one that
// is shut, "-" one that is open; with guides, mosaic lines join each item
// to its parent, as a tree view's do. Up and Down move (core/list-cursor.ts),
// Right opens, Left shuts or goes to the parent, Enter or Space opens or
// shuts one with children and chooses one without, a click does as Enter
// does. More than fit scroll beside a scroll bar.

import type { Canvas } from "../core/canvas.ts";
import { Component, type Size } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { isPress, type KeyEvent } from "../core/events.ts";
import { choosing } from "../core/keys.ts";
import { keepInView, listStep, moveChoice } from "../core/list-cursor.ts";
import { LEFT_HALF, MOSAIC_BASE, SIXEL } from "../core/mosaics.ts";
import { Colour, Key } from "../kit.ts";
import { Control, type ControlProps } from "../input/control.ts";
import { Columns } from "../layout/layouts.ts";
import { ListRows } from "../lists/list-row.ts";
import { BAR_CELLS, ScrollBar } from "../lists/scroll.ts";

export interface TreeNode {
  readonly label: string;
  readonly children?: readonly TreeNode[];
  /** A page it goes to when chosen. */
  readonly page?: number;
}

export interface TreeProps extends ControlProps {
  readonly nodes: readonly TreeNode[];
  /** Mosaic lines join the items. */
  readonly guides?: boolean;
  /** The rows it shows. */
  readonly shown: number;
  /** Items opened to begin with, by label. */
  readonly open?: readonly string[];
  readonly empty?: string;
  readonly onChoose?: (node: TreeNode, session: number) => void;
}

interface TreeState {
  /** Open items, by their place: indices from the top joined with "/". */
  readonly open: ReadonlySet<string>;
  readonly chosen: number;
  readonly top: number;
}

/** One visible row: the node, where it is, its depth, and for each level above it whether a line goes on down. */
interface Visible {
  readonly node: TreeNode;
  readonly id: string;
  readonly depth: number;
  readonly last: boolean;
  readonly through: readonly boolean[];
  readonly parent: number | null;
}

/** The guides' cells (EN 300 706 Table 47): a line down, a branch to an item, the last branch. */
const DOWN = LEFT_HALF;
const BRANCH = LEFT_HALF | SIXEL.middleRight;
const LAST_BRANCH = MOSAIC_BASE | SIXEL.topLeft | SIXEL.middleLeft | SIXEL.middleRight;
/** Indentation without guides, a level. */
const INDENT = 2;

function opened(nodes: readonly TreeNode[], labels: readonly string[], prefix = ""): string[] {
  return nodes.flatMap((node, index) => {
    const id = `${prefix}${index}`;
    return [...(labels.includes(node.label) ? [id] : []), ...opened(node.children ?? [], labels, `${id}/`)];
  });
}

function visibleRows(nodes: readonly TreeNode[], open: ReadonlySet<string>): Visible[] {
  const out: Visible[] = [];
  const visit = (level: readonly TreeNode[], depth: number, through: readonly boolean[], prefix: string, parent: number | null): void => {
    level.forEach((node, index) => {
      const id = `${prefix}${index}`;
      const last = index === level.length - 1;
      const place = out.length;
      out.push({ node, id, depth, last, through, parent });
      if (node.children !== undefined && open.has(id)) {
        visit(node.children, depth + 1, [...through, !last], `${id}/`, place);
      }
    });
  };
  visit(nodes, 0, [], "", null);
  return out;
}

/** The mosaic lines that join an item to its parent, in blue after their colour's cell. */
class TreeGuides extends Component<{ readonly codes: readonly number[] }> {
  override paint(canvas: Canvas): void {
    canvas.mosaics(0, 0, Colour.Blue, [...this.props.codes]);
  }
}

/** An item's guide lines: one cell a level above it, a line down where that level goes on, then its branch. */
function guideCodes(item: Visible): number[] {
  return [...item.through.slice(1).map((down) => (down ? DOWN : MOSAIC_BASE)), item.last ? LAST_BRANCH : BRANCH];
}

export class Tree extends Control<TreeProps, TreeState> {
  override state: TreeState = { open: new Set(opened(this.props.nodes, this.props.open ?? [])), chosen: 0, top: 0 };

  protected override keysHint(): string {
    return "Up Down; Right opens, Left shuts; Enter";
  }

  override measure(space: Size): Size {
    return { width: space.width, height: Math.min(this.props.shown, space.height) };
  }

  #rows(): Visible[] {
    return visibleRows(this.props.nodes, this.state.open);
  }

  override render(): Child {
    const rows = this.#rows();
    const guides = this.props.guides === true;
    // Shutting an item leaves fewer rows: the view comes up so none of it is empty.
    const top = Math.min(this.state.top, Math.max(0, rows.length - this.props.shown));
    return h(
      Columns,
      { widths: [null, BAR_CELLS] },
      h(ListRows<Visible>, {
        items: rows,
        top,
        shown: this.props.shown,
        chosen: this.state.chosen,
        focused: this.focused,
        empty: this.props.empty ?? "nothing here",
        // With guides, the lines and their colour's cell; without, two cells a level.
        indent: (item: Visible) => (guides ? (item.depth > 0 ? guideCodes(item).length + 1 : 0) : item.depth * INDENT),
        before: (item: Visible) => (guides && item.depth > 0 ? h(TreeGuides, { codes: guideCodes(item) }) : undefined),
        text: (item: Visible) => `${item.node.children === undefined ? " " : this.state.open.has(item.id) ? "-" : "+"} ${item.node.label}`,
        foreground: (item: Visible) => (item.node.children === undefined ? Colour.White : Colour.Cyan),
        disabled: () => this.disabled,
        onPick: (index: number, session: number) => this.#activate(index, session),
      }),
      h(ScrollBar, { top, total: rows.length, shown: this.props.shown, disabled: this.disabled, onScroll: (to) => this.setState({ top: to }) }),
    );
  }

  #toggle(item: Visible, open: boolean): void {
    const next = new Set(this.state.open);
    if (open) {
      next.add(item.id);
    } else {
      next.delete(item.id);
    }
    this.setState({ open: next });
  }

  #activate(index: number, session: number): void {
    const item = this.#rows()[index];
    if (item === undefined || this.disabled) {
      return;
    }
    this.setState({ chosen: index, top: keepInView(this.state.top, index, this.props.shown, this.#rows().length) });
    if (item.node.children !== undefined) {
      this.#toggle(item, !this.state.open.has(item.id));
      return;
    }
    this.activate(item.node.label, session);
    this.props.onChoose?.(item.node, session);
    if (item.node.page !== undefined) {
      this.services.goto(session, item.node.page);
    }
  }

  override onKey(event: KeyEvent): boolean {
    const rows = this.#rows();
    const item = rows[this.state.chosen];
    if (item === undefined || this.disabled) {
      return false;
    }
    if (choosing(event)) {
      this.#activate(this.state.chosen, event.session);
      return true;
    }
    if (isPress(event, Key.Right) && item.node.children !== undefined) {
      this.#toggle(item, true);
      return true;
    }
    if (isPress(event, Key.Left)) {
      if (item.node.children !== undefined && this.state.open.has(item.id)) {
        this.#toggle(item, false);
      } else if (item.parent !== null) {
        this.setState({ chosen: item.parent, top: keepInView(this.state.top, item.parent, this.props.shown, rows.length) });
      }
      return true;
    }
    const step = listStep(event, this.props.shown);
    if (step === null) {
      return false;
    }
    const chosen = moveChoice(this.state.chosen, step, rows.length);
    this.setState({ chosen, top: keepInView(this.state.top, chosen, this.props.shown, rows.length) });
    this.play("move", event.session);
    return true;
  }
}
