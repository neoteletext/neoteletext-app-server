// An area of the showcase: its entries in the index on page 100 and its
// applications. Each area registers in areas.ts, in the order its sections
// appear on the index.

import type { Application } from "./kit.ts";
import type { Picture } from "./pictures.ts";

/** A section of the index: its first page, its name and what it holds, in a few words. */
export interface Section {
  readonly page: number;
  readonly name: string;
  readonly about: string;
}

/** An area: crates/teletext-showcase's `sections()` and `applications()` of one module. */
export interface Area {
  /** Its entries in the index. */
  sections(): readonly Section[];
  /** Its applications, each made afresh, with nothing typed, posted or played yet; `pictures` are the converted pictures, none when the folder is not given. */
  applications(pictures: readonly Picture[]): readonly Application[];
}
