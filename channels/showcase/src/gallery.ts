// Picture galleries (crates/teletext-showcase/src/gallery.rs): on the
// showcase channel (pages 600 to 699) your examples and the sci-fi
// sprites, a page that compares the levels and their credits; and a
// channel of its own for each larger collection (characters, art, people,
// travel, game art). Every picture is one page whose sub-pages are its
// Levels 1.5, 2.5 and 3.5, and every caption, with its attribution, is on
// the gallery's credits pages (decision 6). The pages are shared and
// static: they are published once, at start.

import type { Area, Section } from "./area.ts";
import { COLLECTION_PLAN, Gallery, SHOWCASE_PLAN } from "./gallery/application.ts";
import { COLLECTIONS, type Collection, type Group, MENU, SHOWCASE_GROUPS } from "./gallery/pages.ts";
import type { Picture } from "./pictures.ts";

/** The pictures of `group`. */
export function ofGroup(pictures: readonly Picture[], group: Group): Picture[] {
  return pictures.filter((picture) => picture.group === group.letter);
}

/** The showcase's own gallery, of {@link SHOWCASE_GROUPS}, naming the collections that have channels of their own. */
export function showcase(pictures: readonly Picture[]): Gallery {
  const shown = SHOWCASE_GROUPS.map((group) => ({ group, members: ofGroup(pictures, group) }));
  const elsewhere = COLLECTIONS.map((collection) => ({ collection, count: ofGroup(pictures, collection.group).length })).filter(({ count }) => count > 0);
  return new Gallery("Picture galleries", SHOWCASE_PLAN, shown, elsewhere);
}

/**
 * A collection's channel's gallery, with its index at page 100. The Rust
 * server made no channel for a collection without pictures, but
 * channels.json starts every program it names, so one without pictures
 * still declares its index.
 */
export function collection(pictures: readonly Picture[], spec: Collection): Gallery {
  return new Gallery(spec.group.name, COLLECTION_PLAN, [{ group: spec.group, members: ofGroup(pictures, spec.group) }], []);
}

/** The gallery area: its entry in the index and one application for pages 600 to 699. */
export const gallery: Area = {
  applications: (pictures) => [showcase(pictures)],
  sections: (): readonly Section[] => [{ page: MENU, name: "Picture galleries", about: "Levels 1.5, 2.5 and 3.5 compared" }],
};
