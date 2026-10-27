// Pages of components as one of the kit's applications: the protocol's
// events become mounts, renders and publications (docs/applications.md).
//
// - `start`: every page is mounted once as the channel's copy, which every
//   viewer is sent, and published.
// - `open`: a page that keeps state for each viewer is mounted again for
//   that viewer (or found, if it was kept while they were away).
// - `input`: to the viewer's mount, or the channel's for a page every viewer
//   shares, whose state is then everyone's.
// - `leave` and `close`: the viewer's mounts are unmounted.
// - `wake`: the timers that are due run.
// After each call every mount with something new renders, and a page is
// published only if its TTI changed. A viewer's copy is published only
// while it differs from the channel's; when it is the same again it is
// released, so a viewer who only reads costs the server nothing.

import { type Application, type Context, type Input, LONGEST_TITLE, type PageEntry, type ServerStats } from "../kit.ts";
import { EventBus } from "./bus.ts";
import type { LibraryEvents } from "./library-events.ts";
import { type PageNeeds, probeNeeds } from "./needs.ts";
import type { PageParams } from "./services.ts";
import { BLOCK_SAMPLES, LEAD_MS, type Listening, MOST_BLOCKS, SECOND_MS, STREAM_CHANNEL, STREAM_HANDLE, type StreamSource, TICK_MS, WRAP } from "./stream.ts";
import type { Element } from "./element.ts";
import { Mount } from "./mount.ts";
import type { Services } from "./services.ts";
import { RATE, type Sound } from "./sound.ts";
import { fit } from "./text.ts";

/** One page of the channel. */
export interface PageSpec {
  readonly page: number;
  readonly title: string;
  /** The section of the contents it starts: its magazine's name. */
  readonly section?: string;
  /** The page takes digits itself (typing, answers), rather than page selection; when absent, whether a component on it needs them (needs.ts). */
  readonly ownsDigits?: boolean;
  /** The page takes the coloured keys itself; when absent, whether a component on it needs them. */
  readonly ownsColourKeys?: boolean;
  /** Each viewer gets a mount of their own, with state of their own; when absent, whether a component on it needs its own copy. */
  readonly perViewer?: boolean;
  /** A viewer's mount is kept while they are on other pages, until they go. */
  readonly keepWhileAway?: boolean;
  /** The page's root element; on a page that keeps a copy for each viewer, made with what the viewer was sent there with (Services.goto), if anything. */
  root(params?: PageParams): Element;
}

/** A viewer's live stream (stream.ts). */
interface Stream {
  readonly source: StreamSource;
  readonly page: number;
  readonly generation: number;
  /** When its first block was sent, by the channel's clock, or null while it waits for samples. */
  startedAt: number | null;
  /** Samples sent. */
  sent: number;
  nextBlock: number;
  state: Listening["state"];
  /** The sample this generation starts from: 0, or where it was paused. */
  readonly from: number;
}

/** The sound channels the program plays on (the engine's audio profile has four). */
const SOUND_CHANNEL = 0;


/** Every sound here is made once, so its generation never changes. */
const GENERATION = 1;

export class ComponentChannel implements Application {
  readonly bus = new EventBus<LibraryEvents>();
  readonly #specs: ReadonlyMap<number, PageSpec>;
  readonly #shared = new Map<number, Mount>();
  /** Each viewer's mounts, by session, then page. */
  readonly #viewers = new Map<number, Map<number, Mount>>();
  /** Whether a viewer's own copy of a page is what they are sent now. */
  readonly #own = new Map<number, Set<number>>();
  #context: Context | null = null;
  #timerId = 0;
  readonly #handles = new Map<string, number>();
  /** The sounds each session has been sent. */
  readonly #sent = new Map<number, Set<string>>();
  /** Who has each page open. */
  readonly #watching = new Map<number, Set<number>>();
  /** What each page's components need, found once (needs.ts). */
  readonly #needs = new Map<number, Required<PageNeeds>>();
  /** What each viewer was last sent to each page with. */
  readonly #params = new Map<number, Map<number, PageParams>>();
  /** Each viewer's live stream. */
  readonly #streams = new Map<number, Stream>();
  /** Each viewer's streams so far, for the next one's generation. */
  readonly #generations = new Map<number, number>();

  constructor(specs: readonly PageSpec[]) {
    this.#specs = new Map(specs.map((spec) => [spec.page, spec]));
  }

  #needsOf(spec: PageSpec): Required<PageNeeds> {
    let needs = this.#needs.get(spec.page);
    if (needs === undefined) {
      needs = probeNeeds(spec.root());
      this.#needs.set(spec.page, needs);
    }
    return needs;
  }

  #ownsDigits(spec: PageSpec): boolean {
    return spec.ownsDigits ?? this.#needsOf(spec).digits;
  }

  #ownsColourKeys(spec: PageSpec): boolean {
    return spec.ownsColourKeys ?? this.#needsOf(spec).colourKeys;
  }

  #perViewer(spec: PageSpec): boolean {
    return spec.perViewer ?? this.#needsOf(spec).ownCopy;
  }

  /** Whether the page's spec, or what is on it, keeps a copy for each viewer: for tests. */
  perViewer(page: number): boolean {
    const spec = this.#specs.get(page);
    return spec !== undefined && this.#perViewer(spec);
  }

  pages(): readonly PageEntry[] {
    return [...this.#specs.values()].map((spec) => ({
      page: spec.page,
      // The kit's bound on a title in the contents.
      title: fit(spec.title, LONGEST_TITLE),
      ...(spec.section === undefined ? {} : { section: spec.section }),
      ...(this.#ownsDigits(spec) ? { ownsDigits: true } : {}),
      ...(this.#ownsColourKeys(spec) ? { ownsColourKeys: true } : {}),
    }));
  }

  start(context: Context): void {
    this.#run(context, () => {
      // A program the server started again is sent start again: whatever was mounted before goes.
      for (const mount of this.#mounts()) {
        mount.unmount();
      }
      this.#shared.clear();
      this.#viewers.clear();
      this.#own.clear();
      this.#sent.clear();
      this.#watching.clear();
      for (const spec of this.#specs.values()) {
        this.#shared.set(spec.page, this.#mount(spec, null));
      }
    });
  }

  opened(session: number, page: number, context: Context): void {
    this.#run(context, () => {
      const watching = this.#watching.get(page) ?? new Set<number>();
      watching.add(session);
      this.#watching.set(page, watching);
      const spec = this.#specs.get(page);
      if (spec !== undefined && this.#perViewer(spec)) {
        this.#viewerMount(spec, session);
      }
      // A clock that waited while nobody watched catches up before the page is sent.
      this.#shared.get(page)?.runTimers(context.now());
    });
  }

  leftPage(session: number, page: number, context: Context): void {
    this.#run(context, () => {
      this.#watching.get(page)?.delete(session);
      if (this.#streams.get(session)?.page === page) {
        this.#stopStream(context, session);
      }
      const spec = this.#specs.get(page);
      const mount = this.#viewers.get(session)?.get(page);
      if (spec === undefined || mount === undefined || spec.keepWhileAway === true) {
        return;
      }
      mount.unmount();
      this.#viewers.get(session)?.delete(page);
      if (this.#own.get(session)?.delete(page) === true) {
        context.release(session, page);
      }
    });
  }

  input(session: number, page: number, input: Input, context: Context): boolean {
    let taken = false;
    this.#run(context, () => {
      const spec = this.#specs.get(page);
      if (spec === undefined) {
        return;
      }
      const mount = this.#perViewer(spec) ? this.#viewerMount(spec, session) : this.#shared.get(page);
      if (mount === undefined) {
        return;
      }
      taken = dispatch(mount, session, input);
    });
    return taken;
  }

  wake(context: Context): void {
    this.#run(context, () => {
      const now = context.now();
      this.#pumpStreams(context, now);
      for (const mount of this.#mounts()) {
        mount.runTimers(now);
      }
    });
  }

  ended(session: number, context: Context): void {
    this.#run(context, () => {
      for (const mount of this.#viewers.get(session)?.values() ?? []) {
        mount.unmount();
      }
      this.#viewers.delete(session);
      this.#own.delete(session);
      this.#sent.delete(session);
      this.#streams.delete(session);
      this.#params.delete(session);
      for (const watching of this.#watching.values()) {
        watching.delete(session);
      }
    });
  }

  stats(stats: ServerStats, context: Context): void {
    this.#run(context, () => this.bus.emit("stats", stats));
  }

  // The mounts.

  *#mounts(): Generator<Mount> {
    yield* this.#shared.values();
    for (const pages of this.#viewers.values()) {
      yield* pages.values();
    }
  }

  #viewerMount(spec: PageSpec, session: number): Mount {
    const pages = this.#viewers.get(session) ?? new Map<number, Mount>();
    this.#viewers.set(session, pages);
    const found = pages.get(spec.page);
    if (found !== undefined) {
      return found;
    }
    const mount = this.#mount(spec, session, this.#params.get(session)?.get(spec.page));
    pages.set(spec.page, mount);
    return mount;
  }

  #mount(spec: PageSpec, session: number | null, params?: PageParams): Mount {
    return new Mount(spec.page, spec.root(params), {
      services: this.#services(spec.page, session),
      nextTimerId: () => {
        this.#timerId += 1;
        return this.#timerId;
      },
      report: (text) => this.#context?.log(text),
      viewed: () => {
        const watching = this.#watching.get(spec.page);
        return session === null ? (watching?.size ?? 0) > 0 : watching?.has(session) === true;
      },
    });
  }

  #services(page: number, session: number | null): Services {
    const context = (): Context => {
      if (this.#context === null) {
        throw new Error("a component reached the channel outside a server call");
      }
      return this.#context;
    };
    return {
      session,
      page,
      bus: this.bus.at({ page, session }),
      now: () => context().now(),
      epochMs: () => context().epochMs(),
      goto: (to, target, params) => this.#goto(context(), to, target, params),
      stream: (source, to) => this.#startStream(context(), to, page, source),
      stopStream: (to) => this.#stopStream(context(), to),
      pauseStream: (to) => this.#pauseStream(context(), to),
      resumeStream: (to) => this.#resumeStream(context(), to),
      listening: (to) => this.#listening(to),
      title: (text) => context().title(page, fit(text, LONGEST_TITLE)),
      below: (bound) => context().below(bound),
      log: (text) => context().log(text),
      play: (sound, to) => this.#play(context(), sound, to ?? session, page),
    };
  }

  /** Sends a viewer to a page, with what it is to show: a page they already have open shows it at once. */
  #goto(context: Context, session: number, page: number, params: PageParams | undefined): void {
    if (params !== undefined) {
      const pages = this.#params.get(session) ?? new Map<number, PageParams>();
      pages.set(page, params);
      this.#params.set(session, pages);
      const spec = this.#specs.get(page);
      const mount = this.#viewers.get(session)?.get(page);
      if (spec !== undefined && mount !== undefined) {
        mount.setRoot(spec.root(params));
      }
    }
    context.goto(session, page);
  }

  // Streams (stream.ts).

  #startStream(context: Context, session: number, page: number, source: StreamSource, from = 0): void {
    const generation = ((this.#generations.get(session) ?? 0) + 1) % WRAP;
    this.#generations.set(session, generation);
    const sound = { handle: STREAM_HANDLE, generation, rate: source.rate };
    context.sound({ session }, { live: sound });
    // Bound to the page: the receiver fades it out when the viewer leaves, before the server hears (the engine's audio.md 7.3).
    context.sound({ session }, { play: { channel: STREAM_CHANNEL, ...sound, page } });
    this.#streams.set(session, { source, page, generation, startedAt: null, sent: 0, nextBlock: 0, state: "waiting", from });
    this.#pumpStreams(context, context.now());
  }

  #stopStream(context: Context, session: number): void {
    if (this.#streams.delete(session)) {
      context.sound({ session }, { stop: { channel: STREAM_CHANNEL } });
    }
  }

  /** Samples of `stream` heard by `now`: what was sent and has had time to play. */
  #heard(stream: Stream, now: number): number {
    return stream.startedAt === null ? 0 : Math.min(stream.sent, Math.floor(((now - stream.startedAt) * stream.source.rate) / SECOND_MS));
  }

  /** The receiver stops the sound; the stream is kept, paused at what was heard, sending nothing. */
  #pauseStream(context: Context, session: number): void {
    const stream = this.#streams.get(session);
    if (stream === undefined || (stream.state !== "playing" && stream.state !== "waiting")) {
      return;
    }
    const at = stream.from + this.#heard(stream, context.now());
    context.sound({ session }, { stop: { channel: STREAM_CHANNEL } });
    this.#streams.set(session, { ...stream, startedAt: null, sent: 0, nextBlock: 0, state: "paused", from: at });
  }

  /** A new stream, of a new generation, from where the paused one was. */
  #resumeStream(context: Context, session: number): void {
    const stream = this.#streams.get(session);
    if (stream?.state === "paused") {
      this.#startStream(context, session, stream.page, stream.source, stream.from);
    }
  }

  #listening(session: number): Listening | null {
    const stream = this.#streams.get(session);
    if (stream === undefined) {
      return null;
    }
    const { source } = stream;
    const now = this.#context?.now() ?? 0;
    const heard = stream.from + this.#heard(stream, now);
    const length = source.complete ? source.samples().length : source.expected;
    return {
      name: source.name,
      heardMs: Math.floor((heard * SECOND_MS) / source.rate),
      lengthMs: length === null ? null : Math.floor((length * SECOND_MS) / source.rate),
      state: source.failure !== null ? "failed" : stream.state,
      failure: source.failure,
    };
  }

  /** Sends each stream's blocks due by `now` and the lead, as far as its samples are known. */
  #pumpStreams(context: Context, now: number): void {
    for (const [session, stream] of this.#streams) {
      const { source } = stream;
      if (source.failure !== null) {
        stream.state = "failed";
        continue;
      }
      if (stream.state === "paused") {
        continue;
      }
      const samples = source.samples().subarray(stream.from);
      if (stream.startedAt === null) {
        if (samples.length < BLOCK_SAMPLES && !source.complete) {
          continue;
        }
        stream.startedAt = now;
        stream.state = "playing";
      }
      const due = Math.floor(((now - stream.startedAt + LEAD_MS) * source.rate) / SECOND_MS);
      const known = source.complete ? Math.ceil(samples.length / BLOCK_SAMPLES) * BLOCK_SAMPLES : samples.length;
      const count = Math.min(MOST_BLOCKS, Math.floor((Math.min(due, known) - stream.sent) / BLOCK_SAMPLES));
      if (count > 0) {
        // The last block is filled out with silence.
        const blocks = new Int16Array(count * BLOCK_SAMPLES);
        blocks.set(samples.subarray(stream.sent, Math.min(samples.length, stream.sent + blocks.length)));
        context.sound({ session }, { blocks: { handle: STREAM_HANDLE, generation: stream.generation, rate: source.rate, first: stream.nextBlock, samples: blocks } });
        stream.sent += blocks.length;
        stream.nextBlock = (stream.nextBlock + count) % WRAP;
      }
      const heard = Math.floor(((now - stream.startedAt) * source.rate) / SECOND_MS);
      if (source.complete && heard >= samples.length) {
        stream.state = "ended";
      }
    }
  }

  #play(context: Context, sound: Sound, session: number | null, page: number): void {
    let handle = this.#handles.get(sound.name);
    if (handle === undefined) {
      // Every handle but the streams'.
      handle = this.#handles.size % STREAM_HANDLE;
      this.#handles.set(sound.name, handle);
    }
    const object = { object: { handle, generation: GENERATION, rate: RATE, samples: sound.samples } };
    const play = { play: { channel: SOUND_CHANNEL, handle, generation: GENERATION, page } };
    if (session === null) {
      // Everyone on the page: whoever arrived since may not have it, so it goes with each play.
      context.sound({ page }, object);
      context.sound({ page }, play);
      return;
    }
    const sent = this.#sent.get(session) ?? new Set<string>();
    if (!sent.has(sound.name)) {
      context.sound({ session }, object);
      sent.add(sound.name);
      this.#sent.set(session, sent);
    }
    context.sound({ session }, play);
  }

  /** Runs `work` in a server call, then publishes what changed and asks for the next wake. */
  #run(context: Context, work: () => void): void {
    this.#context = context;
    try {
      work();
      this.#publish(context);
      let earliest: number | null = null;
      for (const mount of this.#mounts()) {
        const at = mount.nextWake();
        if (at !== null) {
          earliest = earliest === null ? at : Math.min(earliest, at);
        }
      }
      // A stream still sending, or still to be heard to its end, wants its tick.
      if ([...this.#streams.values()].some((stream) => stream.state === "waiting" || stream.state === "playing")) {
        const tick = context.now() + TICK_MS;
        earliest = earliest === null ? tick : Math.min(earliest, tick);
      }
      if (earliest !== null) {
        context.wakeAt(earliest);
      }
    } finally {
      this.#context = null;
    }
  }

  #publish(context: Context): void {
    // The channel's copies first: a viewer's is compared with its page's.
    for (const mount of this.#shared.values()) {
      if (mount.dirty) {
        const before = mount.lastTti;
        const text = mount.render();
        if (text !== before) {
          context.publishTti("channel", mount.page, text);
        }
      }
    }
    for (const [session, pages] of this.#viewers) {
      for (const mount of pages.values()) {
        const shared = this.#shared.get(mount.page)?.lastTti ?? null;
        const own = this.#own.get(session) ?? new Set<number>();
        this.#own.set(session, own);
        const before = mount.lastTti;
        const text = mount.dirty ? mount.render() : before;
        if (text === null) {
          continue;
        }
        if (text === shared) {
          if (own.delete(mount.page)) {
            context.release(session, mount.page);
          }
        } else if (text !== before || !own.has(mount.page)) {
          context.publishTti(session, mount.page, text);
          own.add(mount.page);
        }
      }
    }
  }

  /** The page's mount for `session`, if it has one: for tests. */
  viewerMount(session: number, page: number): Mount | undefined {
    return this.#viewers.get(session)?.get(page);
  }
}

/** Gives `input` to `mount`: answers whether a component took it. */
function dispatch(mount: Mount, session: number, input: Input): boolean {
  switch (input.kind) {
    case "key":
      return mount.key({ session, key: input.key, action: input.action, ...(input.text === undefined ? {} : { text: input.text }) });
    case "text":
      return mount.text({ session, text: input.text });
    case "paste":
      return mount.paste({ session, text: input.text });
    case "mouse":
      return input.action === "move"
        ? mount.pointer(session, "move", input.row, input.column, 0, input.buttons)
        : mount.pointer(session, input.action, input.row, input.column, input.button, input.action === "press" ? 1 << input.button : 0);
    case "wheel":
      return mount.wheel(session, input.row, input.column, input.dx, input.dy);
    case "hover":
      return mount.hover(session, input.action === "enter", input.row, input.column);
    case "focus":
      mount.focusChanged(true);
      return false;
    case "blur":
      mount.focusChanged(false);
      return false;
  }
}
