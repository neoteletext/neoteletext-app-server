// One picture collection as a channel of its own, a program of its own
// (crates/teletext-showcase/src/gallery.rs, channels()): its index at page
// 100, a page for each picture and the credits, which the server starts
// from channels.json.
//
//   node src/collection.ts --collection NAME [--pictures FOLDER]
//
// NAME is one of the collections' keys (characters, art, people, travel,
// gameart); FOLDER holds the converted pictures (pictures.ts).

import { parseArgs } from "node:util";

import { collection, ofGroup } from "./gallery.ts";
import { COLLECTIONS } from "./gallery/pages.ts";
import { runChannel } from "./kit.ts";
import { loadPictures } from "./pictures.ts";

const { values } = parseArgs({ options: { pictures: { type: "string" }, collection: { type: "string" } }, strict: true });
const spec = COLLECTIONS.find(({ key }) => key === values.collection);
if (spec === undefined) {
  process.stderr.write(`collection: --collection is one of ${COLLECTIONS.map(({ key }) => key).join(", ")}\n`);
  process.exit(1);
}
const pictures = values.pictures === undefined ? [] : loadPictures(values.pictures);
process.stderr.write(`${spec.key}: ${ofGroup(pictures, spec.group).length} pictures\n`);

await runChannel([collection(pictures, spec)]);
