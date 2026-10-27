// The events the library's components send one another on the channel's
// bus (bus.ts): what one page's components tell others that share nothing
// else with them.

import type { ServerStats } from "../kit.ts";

/** A notice for a toast (feedback/notice.ts): what it says and how it is meant. Sent to one viewer or page with the bus's audience. */
export interface ToastEvent {
  readonly text: string;
  readonly kind: "info" | "success" | "warning" | "error";
}

export interface LibraryEvents {
  /** The server's figures, every 10 seconds. */
  readonly stats: ServerStats;
  /** A poll's votes changed: its name and every option's count. */
  readonly poll: { readonly name: string; readonly votes: readonly number[] };
  /** A toast to show on every page with a toast shelf. */
  readonly toast: ToastEvent;
}
