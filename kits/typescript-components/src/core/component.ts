// The class component: React's class-component model, for teletext.
//
// A component has props from its parent and state of its own; setState
// merges a change and asks for a render, batched until the event or the
// timer that caused it has been handled. render answers elements; a
// component that draws itself answers nothing and paints instead. The
// lifecycle is React's: componentDidMount after its first render is on the
// page, shouldComponentUpdate before a render, componentDidUpdate after,
// componentWillUnmount when it leaves the page. A page's mount is made when
// a viewer opens it (or at the channel's start, for the copy every viewer
// shares) and unmounted when they leave it or go (mount.ts).
//
// Input is the server's, in cells: a key, typed text or a paste go to the
// focused component, a pointer press, a drag's move or a release and the
// wheel to the component drawn in that cell, a move with no button held to
// the component whose pointer area it is in (canvas.ts), a hover to the
// component that declared the control; each bubbles to the parents until a
// handler answers true.

import type { Canvas } from "./canvas.ts";
import type { ComponentEvent, ComponentEventInit } from "./component-events.ts";
import type { Child, LayoutProps, WithChildren } from "./element.ts";
import type { HoverEvent, KeyEvent, PointerEvent, TextEvent, WheelEvent } from "./events.ts";
import type { PageNeeds } from "./needs.ts";
import type { Services } from "./services.ts";

/** A size in cells. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/** A box of cells on the page: its top-left cell and its size. */
export interface Box extends Size {
  readonly row: number;
  readonly column: number;
}

/** A child as its parent's layout sees it: measured in some space, then placed in a box. */
export interface LayoutChild {
  readonly props: LayoutProps;
  measure(space: Size): Size;
  place(box: Box): void;
}

/** What a mounted component reaches of its mount. */
export interface ComponentHost {
  readonly services: Services;
  invalidate(component: Component<object, object>): void;
  focus(component: Component<object, object>): void;
  isFocused(component: Component<object, object>): boolean;
  /** Whether the pointer is on a control the component declared, for any viewer of this copy. */
  isHovered(component: Component<object, object>): boolean;
  setTimer(component: Component<object, object>, run: () => void, delay: number, every: boolean, whileViewed: boolean): number;
  clearTimer(id: number): void;
  /** The box the component was last laid out in. */
  boxOf(component: Component<object, object>): Box;
  /** Sends `event` up the tree from the component. */
  emit(component: Component<object, object>, event: ComponentEventInit): void;
  /** What the component under the viewer's pointer, or else the focused one, says its keys do, or null. */
  focusedHint(): string | null;
}

/** How a timer runs. */
export interface TimerOptions {
  /** It runs only while someone has the page open: a clock or an animation on the copy every viewer shares, which costs nothing while nobody watches. Missed ticks run once when someone comes. */
  readonly whileViewed?: boolean;
}

/** The default stack: children top to bottom, each as tall as it measures, sharing what is left by `grow`. */
export function stack(box: Box, children: readonly LayoutChild[]): void {
  const heights = children.map((child) => child.props.height ?? child.measure({ width: box.width, height: box.height }).height);
  const used = heights.reduce((sum, height) => sum + height, 0);
  const shares = children.map((child) => child.props.grow ?? 0);
  const total = shares.reduce((sum, share) => sum + share, 0);
  let left = Math.max(0, box.height - used);
  let row = box.row;
  children.forEach((child, index) => {
    const share = shares[index] ?? 0;
    const extra = total > 0 ? Math.floor((left * share) / total) : 0;
    const height = Math.min((heights[index] ?? 0) + extra, box.row + box.height - row);
    child.place({ row, column: box.column, width: box.width, height: Math.max(0, height) });
    row += Math.max(0, height);
    if (share > 0) {
      left -= extra;
    }
  });
}

/** The default stack's size: as wide as the space, as tall as its children together. */
export function stackSize(space: Size, children: readonly LayoutChild[]): Size {
  let height = 0;
  for (const child of children) {
    height += child.props.height ?? child.measure({ width: space.width, height: Math.max(0, space.height - height) }).height;
  }
  return { width: space.width, height: Math.min(height, space.height) };
}

/**
 * A class component. `P` is its props, `S` its state. Subclasses override
 * `render`, or `measure` and `paint` to draw themselves, and any of the
 * lifecycle methods and input handlers.
 */
export abstract class Component<P extends object = object, S extends object = object> {
  /** The props, replaced when the parent renders it again. */
  props: Readonly<P & WithChildren>;
  /** The state: set it in the class body, change it with setState. */
  state: Readonly<S> = {} as S;
  #host: ComponentHost | undefined;
  #pending: Partial<S>[] = [];

  constructor(props: P) {
    // The mount makes components with the element's props, children included.
    this.props = props as Readonly<P & WithChildren>;
  }

  /** The mount this component is on, from its mount until it is unmounted. */
  get mounted(): boolean {
    return this.#host !== undefined;
  }

  /** The channel's services: the session, the clock, going to pages, sounds, the event bus. */
  protected get services(): Services {
    if (this.#host === undefined) {
      throw new Error(`${this.constructor.name}: services are reached once mounted`);
    }
    return this.#host.services;
  }

  /** Merges `change` into the state and asks for a render, as React's setState: batched until the event is handled. */
  setState(change: Partial<S> | ((state: Readonly<S>, props: Readonly<P>) => Partial<S>)): void {
    const next = typeof change === "function" ? change({ ...this.state, ...Object.assign({}, ...this.#pending) }, this.props) : change;
    this.#pending.push(next);
    this.#host?.invalidate(this as unknown as Component<object, object>);
  }

  /** Asks for a render although nothing in the state changed: what the component reads from elsewhere did. */
  forceUpdate(): void {
    this.#host?.invalidate(this as unknown as Component<object, object>);
  }

  /** Takes the keys: typed text and keys come here first. */
  protected focus(): void {
    this.#host?.focus(this as unknown as Component<object, object>);
  }

  /** Whether the keys come here first. */
  protected get focused(): boolean {
    return this.#host?.isFocused(this as unknown as Component<object, object>) ?? false;
  }

  /** Whether the pointer is on one of its controls (onHover). */
  protected get hovered(): boolean {
    return this.#host?.isHovered(this as unknown as Component<object, object>) ?? false;
  }

  /** Runs `run` once after `delay` milliseconds; cleared if the component is unmounted first. */
  protected setTimeout(run: () => void, delay: number, options: TimerOptions = {}): number {
    return this.#host?.setTimer(this as unknown as Component<object, object>, run, delay, false, options.whileViewed === true) ?? -1;
  }

  /** Runs `run` every `period` milliseconds until cleared or unmounted. */
  protected setInterval(run: () => void, period: number, options: TimerOptions = {}): number {
    return this.#host?.setTimer(this as unknown as Component<object, object>, run, period, true, options.whileViewed === true) ?? -1;
  }

  /** The box it was last laid out in, in cells of the page: for finding where in it a click fell. */
  protected get box(): Box {
    return this.#host?.boxOf(this as unknown as Component<object, object>) ?? { row: 0, column: 0, width: 0, height: 0 };
  }

  /** Sends an event up the tree (component-events.ts). */
  protected emit(event: ComponentEventInit): void {
    this.#host?.emit(this as unknown as Component<object, object>, event);
  }

  /** What the component on the page under the viewer's pointer says its keys do, or else the focused one: for the frame's hint row. */
  protected get focusedHint(): string | null {
    return this.#host?.focusedHint() ?? null;
  }

  /** Clears a timer. */
  protected clearTimer(id: number): void {
    this.#host?.clearTimer(id);
  }

  // What the component draws.

  /** The elements this component is made of; by default its children. */
  render(): Child {
    return this.props.children ?? null;
  }

  /** Its size in `space`: by default its children's stacked. */
  measure(space: Size, children: readonly LayoutChild[]): Size {
    return stackSize(space, children);
  }

  /** Places its children in its `box`: by default stacked. */
  arrange(box: Box, children: readonly LayoutChild[]): void {
    stack(box, children);
  }

  /** Draws the component itself, before its children, on a canvas of its own box. */
  paint(_canvas: Canvas): void {}

  /**
   * The cells of its `box` that are its own for the pointer: a click there
   * on nothing drawn deeper reaches it, and covers what is under it in a
   * layer. The whole box by default; a layout that only places its
   * children answers null, so what lies under it stays in reach.
   */
  pointerCells(box: Box): Box | null {
    return box;
  }

  /** Whether the keys may come here: a component that takes typing says so. */
  focusable(): boolean {
    return false;
  }

  /** What it needs of its page (needs.ts); asked of an unmounted instance, so it reads only props. */
  needs(): PageNeeds {
    return {};
  }

  /** What its keys do, for the frame's hint row while it has the keys or the pointer is on one of its controls; null for nothing. */
  hint(): string | null {
    return null;
  }

  // The lifecycle, as React's.

  componentDidMount?(): void;
  shouldComponentUpdate?(nextProps: Readonly<P & WithChildren>, nextState: Readonly<S>): boolean;
  componentDidUpdate?(previousProps: Readonly<P & WithChildren>, previousState: Readonly<S>): void;
  componentWillUnmount?(): void;
  /** The keys came to this component, or left it for another. */
  componentDidFocus?(): void;
  componentDidBlur?(): void;

  // Input. Answer true when it is taken; otherwise it goes to the parent.

  onKey?(event: KeyEvent): boolean | void;
  onText?(event: TextEvent): boolean | void;
  onPaste?(event: TextEvent): boolean | void;
  /** A left press on a cell: the common case of onPointer. */
  onClick?(event: PointerEvent): boolean | void;
  /** A press, move or release; a press answered true holds the pointer here until it is released. A move with no button held comes only from a pointer area the component declared. */
  onPointer?(event: PointerEvent): boolean | void;
  onWheel?(event: WheelEvent): boolean | void;
  /** The pointer came onto a control it declared, or left it (`entered` false); answered true, it goes no further up. A leave always goes where its enter went. */
  onHover?(event: HoverEvent): boolean | void;
  /** The keys came to the page, or left it: the window was focused, or the viewer went to another. */
  onPageFocus?(): void;
  onPageBlur?(): void;
  /** An event from a component under it (component-events.ts); true stops it going further up. */
  onComponentEvent?(event: ComponentEvent): boolean | void;

  // For the mount only.

  /** @internal The mount gives the component its host, or takes it away. */
  attach(host: ComponentHost | undefined): void {
    this.#host = host;
  }

  /** @internal The state the pending changes make, without applying them. */
  nextState(): Readonly<S> {
    return this.#pending.length === 0 ? this.state : ({ ...this.state, ...Object.assign({}, ...this.#pending) } as S);
  }

  /** @internal Applies the pending changes. */
  commitState(): void {
    if (this.#pending.length > 0) {
      this.state = this.nextState();
      this.#pending = [];
    }
  }

  /** @internal Whether a change waits. */
  hasPending(): boolean {
    return this.#pending.length > 0;
  }
}
