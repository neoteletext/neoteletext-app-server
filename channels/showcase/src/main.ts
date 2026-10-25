// The NeoTeletext showcase as its own program (docs/applications.md),
// which the server starts from channels.json: every area's applications
// on one channel, over standard input and output.
//
//   node src/main.ts [--pictures FOLDER]
//
// FOLDER holds the converted pictures (pictures.ts); without it the
// galleries and the games' sprites have none.

import { parseArgs } from "node:util";

import { AREAS, applications } from "./areas.ts";
import { runChannel } from "./kit.ts";
import { loadPictures } from "./pictures.ts";

const { values } = parseArgs({ options: { pictures: { type: "string" } }, strict: true });
const pictures = values.pictures === undefined ? [] : loadPictures(values.pictures);
process.stderr.write(`showcase: ${pictures.length} pictures\n`);

await runChannel(applications(AREAS, pictures));
