// Time that ticks: a countdown and a clock, each keeping time with a timer
// of its own.

import { Component } from "../core/component.ts";
import { type Child, h } from "../core/element.ts";
import { Colour } from "../kit.ts";
import { BigDigits } from "./big-digits.ts";
import { DATE_FORMATS, formatDate } from "./date-text.ts";

const SECOND_MS = 1_000;

const MINUTE_S = 60;

const HOUR_S = 3_600;

const DAY_S = 86_400;

export interface CountdownProps {
  /** When it reaches nought, in milliseconds since the Unix epoch. */
  readonly until: number;
  readonly foreground?: Colour;
}

interface CountdownState {
  readonly now: number;
}

/** Days, hours, minutes and seconds to a time, in large digits, ticking each second. */
export class Countdown extends Component<CountdownProps, CountdownState> {
  override state: CountdownState = { now: 0 };

  override componentDidMount(): void {
    this.setState({ now: this.services.epochMs() });
    this.setInterval(() => this.setState({ now: this.services.epochMs() }), SECOND_MS, { whileViewed: true });
  }

  override render(): Child {
    const left = Math.max(0, Math.floor((this.props.until - this.state.now) / SECOND_MS));
    const two = (value: number): string => value.toString().padStart(2, "0");
    const text = `${Math.floor(left / DAY_S)}:${two(Math.floor((left % DAY_S) / HOUR_S))}:${two(Math.floor((left % HOUR_S) / MINUTE_S))}:${two(left % MINUTE_S)}`;
    return h(BigDigits, { text, foreground: this.props.foreground ?? Colour.Green });
  }
}

export interface ClockProps {
  readonly foreground?: Colour;
  /** Seconds shown. */
  readonly seconds?: boolean;
}

interface ClockState {
  readonly now: number;
}

/** The time, UTC, in large digits, ticking each second. */
export class Clock extends Component<ClockProps, ClockState> {
  override state: ClockState = { now: 0 };

  override componentDidMount(): void {
    const tick = (): void => this.setState({ now: this.services.epochMs() });
    tick();
    this.setInterval(tick, SECOND_MS, { whileViewed: true });
  }

  override render(): Child {
    const text = formatDate(this.state.now, this.props.seconds === true ? DATE_FORMATS.seconds : DATE_FORMATS.time);
    return h(BigDigits, { text, foreground: this.props.foreground ?? Colour.Yellow });
  }
}
