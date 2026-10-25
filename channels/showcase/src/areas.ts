// The showcase's areas, registered in one place: their sections make the
// index on page 100, in this order (its Fastext links the first four), and
// their applications serve the rest of the channel's pages
// (crates/teletext-showcase/src/channels.rs). An area still in Rust,
// in crates/teletext-showcase, is registered here when it is ported; see
// PORTING.md.

import { adventure } from "./adventure.ts";
import type { Area, Section } from "./area.ts";
import { Index } from "./index-page.ts";
import { interactive } from "./interactive.ts";
import type { Application } from "./kit.ts";
import type { Picture } from "./pictures.ts";
import { services } from "./services.ts";

/** The areas, in the index's order: the Rust showcase's, as each is ported (games, audio and gallery to come). */
export const AREAS: readonly Area[] = [services, interactive, adventure];

/** Every area's sections, in order. */
export function sections(areas: readonly Area[] = AREAS): Section[] {
  return areas.flatMap((area) => area.sections());
}

/** The channel's applications: the index first, then each area's, made afresh, given the pictures. */
export function applications(areas: readonly Area[] = AREAS, pictures: readonly Picture[] = []): Application[] {
  return [new Index(sections(areas)), ...areas.flatMap((area) => area.applications(pictures))];
}
