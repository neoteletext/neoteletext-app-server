// What every control has in common: a label in a column of its own, a
// state that cannot be used (disabled), one look while it has the keys
// (core/look.ts), optional sounds, a hint of its keys for the frame's row
// 23, and what it needs of its page: a copy for each viewer unless it is
// shared, so one viewer's ticks are never everyone's. A control with a
// value takes it from its parent (`value`) or keeps its own (`initial`),
// checks it, and sends change, invalid and activate events up the tree
// (core/component-events.ts) as well as calling its own callbacks.

import { Component } from "../core/component.ts";
import type { Origin } from "../core/component-events.ts";
import { LABEL_CELLS, paintLabel } from "../core/look.ts";
import type { Canvas } from "../core/canvas.ts";
import type { PageNeeds } from "../core/needs.ts";
import type { Sound } from "../core/sound.ts";

/** Sounds a control plays to the viewer: as the choice moves, as one is made, as a value changes, as one is refused. */
export interface ControlSounds {
  readonly move?: Sound;
  readonly choose?: Sound;
  readonly change?: Sound;
  readonly error?: Sound;
}

export interface ControlProps {
  readonly label?: string;
  /** The label's column; LABEL_CELLS when absent. */
  readonly labelCells?: number;
  readonly disabled?: boolean;
  readonly sounds?: ControlSounds;
  /** One copy for every viewer: what one does, all see. */
  readonly shared?: boolean;
  /** What its keys do, for the frame's row 23; the control's own when absent. */
  readonly hint?: string;
  /** It takes the keys when it arrives, as a dialog's field or a newly chosen tab's list does. */
  readonly autoFocus?: boolean;
}

export abstract class Control<P extends ControlProps = ControlProps, S extends object = object> extends Component<P, S> {
  get disabled(): boolean {
    return this.props.disabled === true;
  }

  override focusable(): boolean {
    return !this.disabled;
  }

  override componentDidMount(): void {
    if (this.props.autoFocus === true) {
      this.focus();
    }
  }

  override needs(): PageNeeds {
    return { ownCopy: this.props.shared !== true, ...this.keyNeeds() };
  }

  /** The keys it takes from page selection: digits, the coloured keys. */
  protected keyNeeds(): PageNeeds {
    return {};
  }

  override hint(): string | null {
    return this.props.hint ?? this.keysHint();
  }

  /** What its keys do, when its user does not say. */
  protected keysHint(): string | null {
    return null;
  }

  /** The label's column: none without a label. */
  protected get labelCells(): number {
    return this.props.label === undefined ? 0 : (this.props.labelCells ?? LABEL_CELLS);
  }

  /** Paints the label on `row`; answers the column the control starts in. */
  protected paintLabel(canvas: Canvas, row = 0): number {
    if (this.props.label !== undefined) {
      paintLabel(canvas, row, this.props.label, this.labelCells, { focused: this.focused, disabled: this.disabled });
    }
    return this.labelCells;
  }

  protected play(kind: keyof ControlSounds, session: number | null): void {
    const sound = this.props.sounds?.[kind];
    if (sound !== undefined && session !== null) {
      this.services.play(sound, session);
    }
  }

  /** Something was chosen or pressed: the activate event, and the choose sound. */
  protected activate(value: unknown, session: number, origin: Origin = "viewer"): void {
    this.emit({ type: "activate", value, session, origin });
    this.play("choose", session);
  }

  /** Escape: the cancel event. */
  protected cancel(session: number): void {
    this.emit({ type: "cancel", session, origin: "viewer" });
  }
}

export interface ValueProps<T> extends ControlProps {
  /** The value, kept by the parent: the control shows it and reports changes, and the parent sets it again. */
  readonly value?: T;
  /** The value to begin with, when the control keeps its own. */
  readonly initial?: T;
  readonly onChange?: (value: T, session: number) => void;
  /** What is wrong with a value, or null. */
  readonly validate?: (value: T) => string | null;
}

export interface ValueState<T> {
  readonly own: T;
}

/** Values compare by what they hold: a list of ticks is the same list if it ticks the same. */
function same(a: unknown, b: unknown): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}

export abstract class ValueControl<T, P extends ValueProps<T> = ValueProps<T>, S extends ValueState<T> = ValueState<T>> extends Control<P, S> {
  override state = { own: this.props.initial ?? this.empty() } as S;

  /** Its value when it is given none. */
  protected abstract empty(): T;

  /** The value: the parent's when it keeps it, its own otherwise. */
  get value(): T {
    return this.props.value !== undefined ? this.props.value : this.state.own;
  }

  /** What is wrong with the value, or null. */
  get complaint(): string | null {
    return this.props.validate?.(this.value) ?? null;
  }

  /** The viewer changed the value: kept (unless the parent keeps it), checked, reported. */
  protected change(value: T, session: number): void {
    const previous = this.value;
    if (this.disabled || same(previous, value)) {
      return;
    }
    if (this.props.value === undefined) {
      this.setState({ own: value } as Partial<S>);
    }
    const reason = this.props.validate?.(value) ?? null;
    if (reason !== null) {
      this.emit({ type: "invalid", value, reason, session, origin: "viewer" });
      this.play("error", session);
    } else {
      this.play("change", session);
    }
    this.emit({ type: "change", value, previous, session, origin: "viewer" });
    this.props.onChange?.(value, session);
  }

  /** A parent that keeps the value set it: the change is reported as the program's. */
  override componentDidUpdate(previousProps: Readonly<P>, _previousState: Readonly<S>): void {
    if (previousProps.value !== undefined && this.props.value !== undefined && !same(previousProps.value, this.props.value)) {
      this.emit({ type: "change", value: this.props.value, previous: previousProps.value, session: this.services.session, origin: "program" });
    }
  }
}
