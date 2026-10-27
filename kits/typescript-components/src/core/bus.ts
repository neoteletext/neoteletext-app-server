// The channel's event bus: how components that share nothing else hear one
// another (a vote reaching every poll, new figures reaching every page that
// shows them), so no state is global. A component subscribes when it mounts
// and unsubscribes when it unmounts.

/** A function that ends a subscription. */
export type Unsubscribe = () => void;

/** Who an event is for: one page, one viewer, or both; everyone when empty. */
export interface Audience {
  readonly page?: number;
  readonly session?: number;
}

/** Where a listener is: its page, and its viewer or null for the copy every viewer shares. */
export interface Place {
  readonly page: number;
  readonly session: number | null;
}

/** Whether an event for `audience` reaches a listener at `place`. */
export function reaches(audience: Audience, place: Place): boolean {
  return (audience.page === undefined || audience.page === place.page) && (audience.session === undefined || audience.session === place.session);
}

/** The bus as a component reaches it: what it hears is what is for its page and its viewer. */
export interface Bus<Events extends object> {
  on<Name extends keyof Events>(name: Name, listener: (value: Events[Name]) => void): Unsubscribe;
  emit<Name extends keyof Events>(name: Name, value: Events[Name], audience?: Audience): void;
}

/** Events by name, each with what it carries. */
export class EventBus<Events extends object> implements Bus<Events> {
  readonly #listeners = new Map<keyof Events, Set<{ readonly listener: (value: never) => void; readonly place: Place | null }>>();

  /** Calls `listener` with every `name` event for `place` (every event, when null) from now until the answer is called. */
  on<Name extends keyof Events>(name: Name, listener: (value: Events[Name]) => void, place: Place | null = null): Unsubscribe {
    const listeners = this.#listeners.get(name) ?? new Set();
    const entry = { listener: listener as (value: never) => void, place };
    listeners.add(entry);
    this.#listeners.set(name, listeners);
    return () => {
      listeners.delete(entry);
    };
  }

  /** Tells every listener of `name` that `audience` reaches. */
  emit<Name extends keyof Events>(name: Name, value: Events[Name], audience: Audience = {}): void {
    for (const { listener, place } of [...(this.#listeners.get(name) ?? [])]) {
      if (place === null || reaches(audience, place)) {
        (listener as (value: Events[Name]) => void)(value);
      }
    }
  }

  /** The bus as a component at `place` reaches it. */
  at(place: Place): Bus<Events> {
    return {
      on: (name, listener) => this.on(name, listener, place),
      emit: (name, value, audience) => this.emit(name, value, audience),
    };
  }

  /** How many listen to `name`. */
  listening(name: keyof Events): number {
    return this.#listeners.get(name)?.size ?? 0;
  }
}
