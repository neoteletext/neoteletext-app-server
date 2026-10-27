// The kitchensink as its own program (docs/applications.md), which the
// server starts from channels.json: every component of the class-component
// framework (src/ui) and every technique of the showcase and the Hackaday
// channel, on one channel, over standard input and output.
//
//   node src/main.ts [--pictures FOLDER]
//
// FOLDER holds the converted pictures the showcase reads too; without it
// the picture pages say so. QR codes are made at the start with the
// engine's neottx (NEOTTX names another).

import { parseArgs } from "node:util";

import { ComponentChannel } from "../../../kits/typescript-components/src/index.ts";
import { pictures, qrCodes } from "./assets.ts";
import { ARTICLES } from "./content.ts";
import { runChannel } from "./kit.ts";
import { pages } from "./pages.ts";

const { values } = parseArgs({ options: { pictures: { type: "string" } }, strict: true });
const found = pictures(values.pictures);
const qr = qrCodes(ARTICLES.map((article) => article.link));
process.stderr.write(`kitchensink: ${found.length} pictures, ${qr.size} QR codes\n`);

await runChannel([new ComponentChannel(pages({ pictures: found, qr }))]);
