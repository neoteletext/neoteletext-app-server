// A mount: one tree of component instances behind one copy of a page, the
// channel's or one viewer's. It reconciles each render with the last, as
// React does (an instance is kept where the same class renders in the same
// place, by key or by position), runs the lifecycle, lays out and paints
// every sub-page, keeps which component drew each cell so the pointer and
// the wheel reach it, routes the keys through focus, runs its components'
// timers, and answers the page as TTI, with the controls and pointer areas
// its components declared (controls.ts) where a click reaches them, and
// gives a hover of a control to the component that declared it. Clicks
// are found on the first sub-page's cells, since a channel is not told
// which sub-page a viewer has up (INPUT.md section 5.4, PAGE), so only the
// first sub-page declares any. A move with no button held comes only from
// a pointer area, and goes to the component that declared it.

import { Colour, COLUMNS, type FastextLinks, Key, MOST_SUBPAGES, ROWS, Screen, tti as subpagesTti } from "../kit.ts";
import { Text } from "../text/styles.ts";
import { Canvas, type Declared, nothingDeclared, type PageRequests, type Region } from "./canvas.ts";
import { controlLines, keptDeclarations } from "./controls.ts";
import type { ComponentEvent, ComponentEventInit } from "./component-events.ts";
import type { Box, Component, ComponentHost, LayoutChild, Size } from "./component.ts";
import { type Element, flatten, isElement } from "./element.ts";
import { type HoverEvent, isPress, type KeyEvent, typedText, type PointerEvent, type TextEvent, type WheelEvent } from "./events.ts";
import { RawSubPage, SCREEN, SubPage } from "./page.ts";
import type { Services } from "./services.ts";
import { fit } from "./text.ts";

type AnyComponent = Component<object, object>;

const EMPTY: Box = { row: 0, column: 0, width: 0, height: 0 };

/** One instance in the tree, with the element it was last rendered from. */
class Node {
  element: Element;
  readonly component: AnyComponent;
  readonly parent: Node | null;
  children: Node[] = [];
  box: Box = EMPTY;
  /** The regions it marked as it last painted the first sub-page (canvas.ts). */
  regions: Region[] = [];

  constructor(element: Element, component: AnyComponent, parent: Node | null) {
    this.element = element;
    this.component = component;
    this.parent = parent;
  }
}

/** A timer of a component. */
interface Timer {
  at: number;
  readonly period: number | null;
  readonly run: () => void;
  readonly owner: AnyComponent;
  readonly whileViewed: boolean;
}

/** Renders a mount may do in one flush, as commit-phase updates cause more: past it, a loop is reported. */
const MOST_PASSES = 8;

/** Text in a render is a line in white. */
function textElement(text: string, key: string | number | undefined): Element {
  return { type: Text as never, props: { text } as never, key };
}

/** What a mount needs of the runtime. */
export interface MountHost {
  readonly services: Services;
  readonly nextTimerId: () => number;
  readonly report: (text: string) => void;
  /** Whether someone has the page open, for the timers that run only then. */
  readonly viewed: () => boolean;
}

export class Mount implements ComponentHost {
  readonly page: number;
  readonly services: Services;
  readonly #host: MountHost;
  #rootElement: Element;
  #root: Node | null = null;
  #dirty = true;
  readonly #forced = new Set<AnyComponent>();
  readonly #nodes = new Map<AnyComponent, Node>();
  #focus: Node | null = null;
  /** For each cell of the first sub-page, the deepest component drawn there. */
  #grid: Grid = [];
  /** The pointer areas the first sub-page declares where they reach their component, and the component: null for the whole page. */
  #pointerAreas: { readonly node: Node; readonly area: Box | null }[] = [];
  /** The controls the first sub-page keeps, each with the component that declared it. */
  #controls: Control[] = [];
  /** The control under each session's pointer, from its HOVER enter until the leave. */
  readonly #hovered = new Map<number, Control>();
  /** The component holding the pointer, by session, from a press it took until the release. */
  readonly #captured = new Map<number, Node>();
  readonly #timers = new Map<number, Timer>();
  #mounted: Node[] = [];
  #updated: { node: Node; props: object; state: object }[] = [];
  /** The page as last answered. */
  lastTti: string | null = null;

  constructor(page: number, root: Element, host: MountHost) {
    this.page = page;
    this.#rootElement = root;
    this.#host = host;
    this.services = host.services;
  }

  /** Whether something waits to be drawn. */
  get dirty(): boolean {
    return this.#dirty;
  }

  /** Gives the root new props: data from outside changed. */
  setRoot(root: Element): void {
    this.#rootElement = root;
    this.#dirty = true;
  }

  // ComponentHost.

  invalidate(component: AnyComponent): void {
    if (this.#nodes.has(component)) {
      this.#dirty = true;
      this.#forced.add(component);
    }
  }

  focus(component: AnyComponent): void {
    const node = this.#nodes.get(component);
    if (node !== undefined) {
      this.#focusOn(node);
    }
  }

  /** Gives the keys to `node`, telling the one that had them and the one that has them now. */
  #focusOn(node: Node | null): void {
    if (node === this.#focus) {
      return;
    }
    const before = this.#focus;
    this.#focus = node;
    this.#dirty = true;
    // Both render again: what they render may say whether they have the keys.
    for (const each of [before, node]) {
      if (each !== null) {
        this.#forced.add(each.component);
      }
    }
    if (before !== null && before.component.mounted) {
      before.component.componentDidBlur?.();
    }
    node?.component.componentDidFocus?.();
  }

  isFocused(component: AnyComponent): boolean {
    return this.#focus?.component === component;
  }

  setTimer(component: AnyComponent, run: () => void, delay: number, every: boolean, whileViewed: boolean): number {
    const id = this.#host.nextTimerId();
    const wait = Math.max(1, Math.round(delay));
    this.#timers.set(id, { at: this.services.now() + wait, period: every ? wait : null, run, owner: component, whileViewed });
    return id;
  }

  boxOf(component: AnyComponent): Box {
    return this.#nodes.get(component)?.box ?? EMPTY;
  }

  emit(component: AnyComponent, event: ComponentEventInit): void {
    const sent = { ...event, source: component } as ComponentEvent;
    this.#bubble(this.#nodes.get(component)?.parent ?? null, (each) => each.onComponentEvent?.(sent));
  }

  isHovered(component: AnyComponent): boolean {
    return [...this.#hovered.values()].some(({ node }) => node.component === component);
  }

  focusedHint(): string | null {
    // On a viewer's own copy the control under their pointer says what it is, nearest first; the copy every viewer shares follows no one's pointer.
    const { session } = this.services;
    for (let node = session === null ? null : (this.#hovered.get(session)?.node ?? null); node !== null; node = node.parent) {
      const hint = node.component.hint();
      if (hint !== null) {
        return hint;
      }
    }
    return this.#focus?.component.hint() ?? null;
  }

  clearTimer(id: number): void {
    this.#timers.delete(id);
  }

  // Rendering.

  /** Renders what changed and answers the page as TTI. */
  render(): string {
    for (let pass = 0; pass < MOST_PASSES && this.#dirty; pass += 1) {
      this.#dirty = false;
      this.#reconcileRoot();
      const { tti } = this.#draw();
      this.lastTti = tti;
      this.#commit();
    }
    if (this.#dirty) {
      this.#host.report(`page ${this.page.toString(16)}: still changing after ${MOST_PASSES} renders; a component's update sets state each time`);
      this.#dirty = false;
    }
    return this.lastTti ?? "";
  }

  #reconcileRoot(): void {
    if (this.#root === null || this.#root.element.type !== this.#rootElement.type || this.#root.element.key !== this.#rootElement.key) {
      if (this.#root !== null) {
        this.#unmount(this.#root);
      }
      this.#root = this.#create(this.#rootElement, null);
    } else {
      this.#update(this.#root, this.#rootElement);
    }
  }

  #create(element: Element, parent: Node | null): Node {
    const Type = element.type as unknown as new (props: object) => AnyComponent;
    const component = new Type(element.props);
    const node = new Node(element, component, parent);
    this.#nodes.set(component, node);
    component.attach(this);
    this.#renderChildren(node);
    this.#mounted.push(node);
    return node;
  }

  #update(node: Node, element: Element): void {
    const { component } = node;
    const changed = element !== node.element || this.#forced.has(component) || component.hasPending();
    this.#forced.delete(component);
    if (!changed) {
      // Nothing of its own changed; a descendant may have.
      for (const child of node.children) {
        this.#update(child, child.element);
      }
      return;
    }
    const previousProps = component.props;
    const previousState = component.state;
    const nextState = component.nextState();
    const wanted = component.shouldComponentUpdate?.(element.props as never, nextState) ?? true;
    node.element = element;
    component.props = element.props as never;
    component.commitState();
    if (wanted) {
      this.#renderChildren(node);
      this.#updated.push({ node, props: previousProps, state: previousState });
    } else {
      for (const child of node.children) {
        this.#update(child, child.element);
      }
    }
  }

  #renderChildren(node: Node): void {
    const rendered = flatten(node.component.render()).map((child, index) => (isElement(child) ? child : textElement(child, `text-${index}`)));
    const byKey = new Map<string, Node>();
    node.children.forEach((child, index) => byKey.set(this.#slot(child.element, index), child));
    const kept = new Set<Node>();
    node.children = rendered.map((element, index) => {
      const existing = byKey.get(this.#slot(element, index));
      if (existing !== undefined && existing.element.type === element.type && !kept.has(existing)) {
        kept.add(existing);
        this.#update(existing, element);
        return existing;
      }
      return this.#create(element, node);
    });
    for (const old of byKey.values()) {
      if (!kept.has(old)) {
        this.#unmount(old);
      }
    }
  }

  /** Where an element sits among its siblings: its key, or its place and class. */
  #slot(element: Element, index: number): string {
    return element.key === undefined ? `#${index}` : `k${String(element.key)}`;
  }

  #unmount(node: Node): void {
    node.component.componentWillUnmount?.();
    for (const child of node.children) {
      this.#unmount(child);
    }
    for (const [id, timer] of this.#timers) {
      if (timer.owner === node.component) {
        this.#timers.delete(id);
      }
    }
    for (const [session, held] of this.#captured) {
      if (held === node) {
        this.#captured.delete(session);
      }
    }
    for (const [session, hovered] of this.#hovered) {
      if (hovered.node === node) {
        this.#hovered.delete(session);
      }
    }
    if (this.#focus === node) {
      this.#focus = null;
    }
    this.#nodes.delete(node.component);
    node.component.attach(undefined);
  }

  /** Unmounts every component: the viewer left the page or went. */
  unmount(): void {
    if (this.#root !== null) {
      this.#unmount(this.#root);
      this.#root = null;
    }
    this.#timers.clear();
  }

  #commit(): void {
    const mounted = this.#mounted;
    const updated = this.#updated;
    this.#mounted = [];
    this.#updated = [];
    for (const node of mounted) {
      if (node.component.mounted) {
        node.component.componentDidMount?.();
      }
    }
    for (const { node, props, state } of updated) {
      if (node.component.mounted) {
        node.component.componentDidUpdate?.(props as never, state as never);
      }
    }
  }

  // Layout and painting.

  #subPages(): Node[] {
    const found: Node[] = [];
    const visit = (node: Node): void => {
      if (node.component instanceof SubPage) {
        found.push(node);
        return;
      }
      node.children.forEach(visit);
    };
    if (this.#root !== null) {
      visit(this.#root);
    }
    return found;
  }

  #layoutChild(node: Node): LayoutChild {
    return {
      props: node.element.props,
      measure: (space: Size): Size => node.component.measure(space, node.children.map((child) => this.#layoutChild(child))),
      place: (box: Box): void => this.#layout(node, box),
    };
  }

  #layout(node: Node, box: Box): void {
    node.box = box;
    node.component.arrange(
      box,
      node.children.map((child) => this.#layoutChild(child)),
    );
  }

  /** Paints `node` and what is under it on `screen`, marking `grid` with who drew each cell and collecting what each declares; on the first sub-page, keeps each one's regions for the pointer. */
  #paint(node: Node, screen: Screen, requests: PageRequests, grid: Grid, owned: Owned[], first: boolean): void {
    const { component, box } = node;
    const regions: Region[] = [];
    const declared = nothingDeclared();
    component.paint(new Canvas(screen, box, requests, regions, declared));
    owned.push({ node, declared });
    if (first) {
      node.regions = regions;
    }
    const own = component.pointerCells(box);
    if (own !== null) {
      for (let row = own.row; row < own.row + own.height && row < ROWS; row += 1) {
        const line = grid[row];
        for (let column = own.column; column < own.column + own.width && column < COLUMNS; column += 1) {
          if (line !== undefined) {
            line[column] = node;
          }
        }
      }
    }
    for (const child of node.children) {
      this.#paint(child, screen, requests, grid, owned, first);
    }
  }

  #draw(): { tti: string } {
    const subPages = this.#subPages();
    if (subPages.length > MOST_SUBPAGES) {
      this.#host.report(`page ${this.page.toString(16)}: ${subPages.length} sub-pages, the most is ${MOST_SUBPAGES}; the rest are left out`);
    }
    const shown = subPages.slice(0, MOST_SUBPAGES);
    // The keys go to the first component that takes them until another is given them; chosen before painting, so a focused component draws itself so.
    if (this.#focus === null || !this.#focus.component.mounted) {
      this.#focusOn(this.#firstFocusable());
    }
    const numbered = (index: number): number => (shown.length === 1 ? 0 : index + 1);
    this.#grid = emptyGrid();
    this.#pointerAreas = [];
    this.#controls = [];
    const parts = shown.map((node, index) => {
      const component = node.component as SubPage;
      const links = component.links();
      if (component instanceof RawSubPage) {
        const { tti: text, title, background, foreground } = component.props;
        return rawTti(text, this.page, numbered(index), links.fastext, title === undefined ? null : { title, background: background ?? Colour.Blue, foreground: foreground ?? Colour.White });
      }
      const screen = new Screen();
      const requests: PageRequests = { cursor: null };
      const grid = emptyGrid();
      const owned: Owned[] = [];
      this.#layout(node, SCREEN);
      this.#paint(node, screen, requests, grid, owned, index === 0);
      if (index === 0) {
        this.#grid = grid;
      }
      screen.fastext(links.fastext);
      if (links.index !== null) {
        screen.indexLink(links.index);
      }
      if (requests.cursor !== null) {
        screen.cursor(requests.cursor.row, requests.cursor.column, requests.cursor.width, requests.cursor.flags);
      }
      // Only the first sub-page's cells take clicks, so only it declares: elsewhere the hand would point at another sub-page's component.
      const declared = index === 0 ? reachedDeclarations(grid, owned) : { controls: [], areas: [] };
      const controls = declared.controls.map(({ control }) => control);
      if (index === 0) {
        this.#pointerAreas = declared.areas;
        // The controls the page keeps, as the browser reads them, each with its owner: a hover names one by its first cell.
        const kept = keptDeclarations(controls);
        this.#controls = kept.controls.flatMap((control, at) => {
          const owner = declared.controls[kept.declaredAt[at] ?? -1];
          return owner === undefined ? [] : [{ node: owner.node, control }];
        });
      }
      const wholePage = declared.areas.some(({ area }) => area === null);
      const areas = declared.areas.flatMap(({ area }) => (area === null ? [] : [area]));
      return subpagesTti(this.page, [{ screen, subpage: numbered(index) }]) + controlLines(controls, { areas, wholePage }).map((line) => `${line}\n`).join("");
    });
    return { tti: parts.join("") };
  }

  #firstFocusable(): Node | null {
    let found: Node | null = null;
    const visit = (node: Node): void => {
      if (found === null && node.component.focusable()) {
        found = node;
      }
      node.children.forEach(visit);
    };
    if (this.#root !== null) {
      visit(this.#root);
    }
    return found;
  }

  // Input.

  #bubble(from: Node | null, handle: (component: AnyComponent, node: Node) => boolean | void): boolean {
    for (let node = from; node !== null; node = node.parent) {
      if (handle(node.component, node) === true) {
        return true;
      }
    }
    return false;
  }

  key(event: KeyEvent): boolean {
    if (this.#bubble(this.#focus ?? this.#root, (component) => component.onKey?.(event))) {
      return true;
    }
    // Tab no component took moves the keys to the next component that takes them, in the order they are drawn.
    if (isPress(event, Key.Tab)) {
      const order = this.#focusables();
      const next = order[(order.indexOf(this.#focus as Node) + 1) % Math.max(1, order.length)];
      if (next !== undefined && next !== this.#focus) {
        this.#focusOn(next);
        return true;
      }
    }
    // Typing comes as each key's press and repeat with its text (INPUT.md section 6.3): what no component took as a key is typed text.
    const typed = typedText(event);
    return typed !== undefined && this.text({ session: event.session, text: typed });
  }

  /** Every component that takes the keys, in the order they are drawn. */
  #focusables(): Node[] {
    const found: Node[] = [];
    const visit = (node: Node): void => {
      if (node.component.focusable()) {
        found.push(node);
      }
      node.children.forEach(visit);
    };
    if (this.#root !== null) {
      visit(this.#root);
    }
    return found;
  }

  text(event: TextEvent): boolean {
    return this.#bubble(this.#focus ?? this.#root, (component) => component.onText?.(event));
  }

  paste(event: TextEvent): boolean {
    return this.#bubble(this.#focus ?? this.#root, (component) => component.onPaste?.(event) ?? component.onText?.(event));
  }

  /** The region `node` marked round a cell, as an event's field. */
  #regionAt(node: Node, row: number, column: number): { region?: string } {
    const found = node.regions.find((region) => inside(region, row, column));
    return found === undefined ? {} : { region: found.id };
  }

  #local(node: Node, row: number, column: number): { row: number; column: number } {
    return { row: row - node.box.row, column: column - node.box.column };
  }

  pointer(session: number, action: "press" | "release" | "move", row: number, column: number, button: number, buttons: number): boolean {
    const held = this.#captured.get(session);
    if (held !== undefined && action !== "press") {
      if (action === "release") {
        this.#captured.delete(session);
      }
      const event: PointerEvent = { session, action, row, column, button, buttons, local: this.#local(held, row, column), ...this.#regionAt(held, row, column) };
      held.component.onPointer?.(event);
      return true;
    }
    // A move with no button held is sent only from a pointer area: it goes to each component whose area it is in, and no further.
    if (action === "move" && buttons === 0) {
      let taken = false;
      for (const owner of new Set(this.#pointerAreas.filter(({ area }) => area === null || inside(area, row, column)).map(({ node }) => node))) {
        const event: PointerEvent = { session, action, row, column, button, buttons, local: this.#local(owner, row, column), ...this.#regionAt(owner, row, column) };
        taken = owner.component.onPointer?.(event) === true || taken;
      }
      return taken;
    }
    const target = this.#grid[row]?.[column] ?? null;
    // A press gives the keys to the component pressed, or the nearest one round it that takes them.
    if (action === "press") {
      for (let node = target; node !== null; node = node.parent) {
        if (node.component.focusable()) {
          this.#focusOn(node);
          break;
        }
      }
    }
    return this.#bubble(target, (component, node) => {
      const event: PointerEvent = { session, action, row, column, button, buttons, local: this.#local(node, row, column), ...this.#regionAt(node, row, column) };
      const taken = component.onPointer?.(event) === true;
      if (taken && action === "press") {
        this.#captured.set(session, node);
        return true;
      }
      return taken || (action === "press" && button === 0 && component.onClick?.(event) === true);
    });
  }

  wheel(session: number, row: number, column: number, dx: number, dy: number): boolean {
    const target = this.#grid[row]?.[column] ?? null;
    return this.#bubble(target, (component, node) => {
      const event: WheelEvent = { session, row, column, dx, dy, local: this.#local(node, row, column), ...this.#regionAt(node, row, column) };
      return component.onWheel?.(event);
    });
  }

  /**
   * The pointer entered or left the control whose first cell is `row`,
   * `column` (INPUT.md section 5.4, HOVER): it goes to the component that
   * declared the control, then up. A leave goes where the enter went,
   * whatever the page has become since; an enter while another control is
   * still entered leaves that one first. An enter where no control of the
   * page begins now, as the page changed under the pointer, and a leave
   * with no enter before it, are dropped. The hint row is drawn again when
   * what it says changes.
   */
  hover(session: number, entered: boolean, row: number, column: number): boolean {
    const hintBefore = this.focusedHint();
    const before = this.#hovered.get(session);
    // An enter of the control already entered says nothing new.
    const again = entered && before !== undefined && before.control.row === row && before.control.column === column;
    let taken = false;
    if (before !== undefined && !again) {
      this.#hovered.delete(session);
      taken = this.#hoverOn(before.node, session, false, before.control.row, before.control.column);
    }
    if (entered && !again) {
      const found = this.#controls.find(({ control }) => control.row === row && control.column === column);
      if (found !== undefined) {
        this.#hovered.set(session, found);
        taken = this.#hoverOn(found.node, session, true, row, column) || taken;
      }
    }
    if (this.focusedHint() !== hintBefore) {
      this.#dirty = true;
    }
    return taken;
  }

  #hoverOn(from: Node, session: number, entered: boolean, row: number, column: number): boolean {
    return this.#bubble(from, (component, node) => {
      const event: HoverEvent = { session, entered, row, column, local: this.#local(node, row, column), ...this.#regionAt(node, row, column) };
      return component.onHover?.(event);
    });
  }

  /** The keys came to the page, or left it: every component hears it. */
  focusChanged(arrived: boolean): void {
    for (const node of this.#nodes.values()) {
      if (arrived) {
        node.component.onPageFocus?.();
      } else {
        node.component.onPageBlur?.();
      }
    }
  }

  // Timers.

  /** The earliest timer that may run, or null: one that runs only while viewed waits, without a wake, until someone comes. */
  nextWake(): number | null {
    const viewed = this.#host.viewed();
    let earliest: number | null = null;
    for (const timer of this.#timers.values()) {
      if (timer.whileViewed && !viewed) {
        continue;
      }
      earliest = earliest === null ? timer.at : Math.min(earliest, timer.at);
    }
    return earliest;
  }

  /** Runs the timers due by `now`; a periodic one next falls a period after now, so a late wake does not run it in a burst. */
  runTimers(now: number): void {
    const viewed = this.#host.viewed();
    for (const [id, timer] of [...this.#timers]) {
      if (timer.at > now || !this.#timers.has(id) || (timer.whileViewed && !viewed)) {
        continue;
      }
      if (timer.period === null) {
        this.#timers.delete(id);
      } else {
        timer.at = now + timer.period;
      }
      timer.run();
    }
  }
}

/** For each cell of a sub-page, the deepest component drawn there. */
type Grid = (Node | null)[][];

/** A control a sub-page keeps, and the component that declared it. */
interface Control {
  readonly node: Node;
  readonly control: Box;
}

/** What a component declared as it painted. */
interface Owned {
  readonly node: Node;
  readonly declared: Declared;
}

function inside(box: Box, row: number, column: number): boolean {
  return row >= box.row && row < box.row + box.height && column >= box.column && column < box.column + box.width;
}

function emptyGrid(): Grid {
  return Array.from({ length: ROWS }, () => new Array<Node | null>(COLUMNS).fill(null));
}

/**
 * What a sub-page declares of what its components did: a control or a
 * pointer area only where a click reaches what declared it, not under a
 * dialog or a layer drawn over it; the whole page only for a component
 * nothing covers.
 */
function reachedDeclarations(grid: Grid, owned: readonly Owned[]): { controls: Control[]; areas: { node: Node; area: Box | null }[] } {
  const controls: Control[] = [];
  const areas: { node: Node; area: Box | null }[] = [];
  for (const { node, declared } of owned) {
    controls.push(...declared.controls.filter((control) => reaches(grid, node, control)).map((control) => ({ node, control })));
    areas.push(...declared.pointerAreas.filter((area) => reaches(grid, node, area)).map((area) => ({ node, area })));
    if (declared.wholePage && reaches(grid, node, clipped(node.box))) {
      areas.push({ node, area: null });
    }
  }
  return { controls, areas };
}

/** `box` cut to the page. */
function clipped(box: Box): Box {
  const row = Math.max(0, box.row);
  const column = Math.max(0, box.column);
  return { row, column, width: Math.max(0, Math.min(COLUMNS, box.column + box.width) - column), height: Math.max(0, Math.min(ROWS, box.row + box.height) - row) };
}

/** Whether a click on every cell of `control` reaches `owner`: the deepest component drawn there is it or under it. */
function reaches(grid: Grid, owner: Node, control: Box): boolean {
  for (let row = control.row; row < control.row + control.height; row += 1) {
    for (let column = control.column; column < control.column + control.width; column += 1) {
      let at = grid[row]?.[column] ?? null;
      while (at !== null && at !== owner) {
        at = at.parent;
      }
      if (at === null) {
        return false;
      }
    }
  }
  return true;
}

/** A raw sub-page's TTI made this page's: its PN and SC lines replaced, its FL line too when the sub-page gives links, and rows 1 and 2 a named band when it has a title. */
function rawTti(text: string, page: number, subpage: number, fastext: FastextLinks, background: { title: string; background: Colour; foreground: Colour } | null): string {
  const linked = fastext.some((link) => link !== null);
  const banded = (line: string): boolean => background !== null && /^OL,(1|2),/u.test(line);
  const body = text
    .replace(/\r/gu, "")
    .split("\n")
    .filter((line) => line !== "" && !/^(PN|SC|DE|CT),/u.test(line) && !(linked && line.startsWith("FL,")) && !banded(line));
  if (background !== null) {
    const screen = new Screen();
    screen.background(1, 0, background.background);
    screen.doubleHeight(1, 2, background.foreground, fit(background.title, COLUMNS - 5));
    screen.background(2, 0, background.background);
    const rows = subpagesTti(page, [{ screen, subpage }]).split("\n").filter((line) => /^OL,(1|2),/u.test(line));
    body.unshift(...rows);
  }
  const screen = new Screen();
  if (linked) {
    screen.fastext(fastext);
  }
  const ours = subpagesTti(page, [{ screen, subpage }]).split("\n");
  const head = ours.filter((line) => line.startsWith("PN,") || line.startsWith("SC,"));
  const links = ours.filter((line) => line.startsWith("FL,"));
  return `${[...head, ...body, ...links].join("\n")}\n`;
}
