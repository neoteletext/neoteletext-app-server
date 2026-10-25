// What the gallery's files share (crates/teletext-showcase/src/
// gallery.rs): the groups of pictures, the collections that are channels
// of their own, and the showcase gallery's menu page.

/** A group of pictures, as the levels page names it. */
export interface Group {
  /** Its letter in captions.tsv. */
  readonly letter: string;
  /** Its name. */
  readonly name: string;
  /** Its name as a Fastext label: at most 9 characters. */
  readonly short: string;
}

/** The groups the showcase channel shows itself, in page order. */
export const SHOWCASE_GROUPS: readonly Group[] = [
  { letter: "e", name: "Your examples", short: "Examples" },
  { letter: "s", name: "Sci-fi sprites", short: "Sprites" },
];

/** A collection served as a channel of its own: its name, its folder, the channel's name and its group. */
export interface Collection {
  /** The collection's name, and the Rust server's folder for it. */
  readonly key: string;
  /** The folder the channel is served at: the key with "-ts" while the Rust server still serves the key itself (PORTING.md section 3.6). */
  readonly folder: string;
  /** The channel's name. */
  readonly channel: string;
  /** Its pictures. */
  readonly group: Group;
}

/** The folder of a collection's TypeScript channel, beside the Rust server's own of the same key. */
const FOLDER_SUFFIX = "-ts";
/** A collection's Fastext label for its first picture. */
const FIRST = "First";

function collection(key: string, channel: string, letter: string, name: string): Collection {
  return { key, folder: `${key}${FOLDER_SUFFIX}`, channel, group: { letter, name, short: FIRST } };
}

/**
 * The collections with channels of their own: each is kept to a few
 * hundred pages and sub-pages, so that it opens quickly in a browser
 * whose page store starts at 256.
 */
export const COLLECTIONS: readonly Collection[] = [
  collection("characters", "NeoTeletext characters", "c", "Characters"),
  collection("art", "NeoTeletext art", "a", "Art"),
  collection("people", "NeoTeletext people", "p", "People"),
  collection("travel", "NeoTeletext travel", "t", "Travel"),
  collection("gameart", "NeoTeletext game art", "g", "Game art"),
];

/** The showcase gallery's menu page. */
export const MENU = 0x600;
