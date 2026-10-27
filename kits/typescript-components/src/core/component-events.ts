// The events a component sends up the tree to the components round it, as
// a change in a web form goes up to the form: a control changed, something
// was chosen or pressed, a form was sent, something was cancelled, or what
// was given is wrong. Each says which component sent it, for which viewer,
// and whether the viewer did it or the program did. A component round it
// hears it in onComponentEvent and answers true to stop it going further.

import type { Component } from "./component.ts";

/** Whether the viewer did it, or the program set it. */
export type Origin = "viewer" | "program";

interface Sent {
  readonly source: Component<object, object>;
  /** The viewer it came from; null when the program did it on the copy every viewer shares. */
  readonly session: number | null;
  readonly origin: Origin;
}

export type ComponentEvent =
  | (Sent & { readonly type: "change"; readonly value: unknown; readonly previous: unknown })
  | (Sent & { readonly type: "activate"; readonly value: unknown })
  | (Sent & { readonly type: "submit"; readonly value: unknown })
  | (Sent & { readonly type: "cancel" })
  | (Sent & { readonly type: "invalid"; readonly value: unknown; readonly reason: string });

/** An event as a component sends it: the mount adds its source. */
export type ComponentEventInit = ComponentEvent extends infer Event ? (Event extends ComponentEvent ? Omit<Event, "source"> : never) : never;
