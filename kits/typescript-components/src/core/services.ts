// What a mounted component reaches of the channel, through its mount: the
// viewer it is drawn for, the clock, the protocol's goto and title, sounds
// and the event bus. Calls made outside a server call (none should be) are
// held until the next one.

import type { Bus } from "./bus.ts";
import type { LibraryEvents } from "./library-events.ts";
import type { Sound } from "./sound.ts";
import type { Listening, StreamSource } from "./stream.ts";

/** What a page is opened with: what it shows of many (an article, an episode). */
export type PageParams = Readonly<Record<string, string | number>>;

export interface Services {
  /** The viewer this copy of the page is drawn for, or null for the copy every viewer shares. */
  readonly session: number | null;
  /** The page it is on. */
  readonly page: number;
  /** Milliseconds since the program started. */
  now(): number;
  /** The real time, in milliseconds since the Unix epoch, UTC. */
  epochMs(): number;
  /** Sends a viewer to a page (docs/applications.md section 3.3), opened with `params` when given: a page whose root takes them shows that one of many. */
  goto(session: number, page: number, params?: PageParams): void;
  /** Gives this page a new title in the channel's contents (section 3.5). */
  title(text: string): void;
  /** Plays a sound to one viewer, or to everyone with this page open. */
  play(sound: Sound, session?: number): void;
  /** Plays a long sound to one viewer as a live stream (stream.ts), bound to this page: it stops when they leave. A new one replaces theirs. */
  stream(source: StreamSource, session: number): void;
  /** Stops the viewer's stream. */
  stopStream(session: number): void;
  /** Pauses the viewer's stream where they have heard to; resumeStream goes on from there. */
  pauseStream(session: number): void;
  resumeStream(session: number): void;
  /** The viewer's stream, or null. */
  listening(session: number): Listening | null;
  /** A pseudo-random number below `bound`: for games, never for secrets. */
  below(bound: number): number;
  /** Reports a line in the server's log. */
  log(text: string): void;
  /** The channel's event bus: what is heard is what is for this page and this viewer. */
  readonly bus: Bus<LibraryEvents>;
}
