// Hackaday, built on the component library: the whole archive of posts in
// its areas and subjects, the podcast's episodes and stories to listen to,
// and What's That Sound, as its own program (docs/applications.md), which
// the server starts from channels.json.
//
//   node --disable-warning=ExperimentalWarning src/main.ts --analysis DIR [--rounds DIR] [--ffmpeg PATH]
//
// DIR holds analysis.sqlite and its story clips in audio/ (the host's
// ~/hackaday-analysis); --rounds names whatsthatsound's clips folder, which
// holds manifest.json and the rounds' clips. Node's own SQLite reads the
// database, which is experimental in Node 22, hence the flag.

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { parseArgs } from "node:util";

import { ComponentChannel } from "../../../kits/typescript-components/src/index.ts";
import { Clips } from "./clips.ts";
import { runChannel } from "./kit.ts";
import { Library } from "./library.ts";
import { pages } from "./pages.ts";
import { SiteMap } from "./site.ts";

const { values } = parseArgs({ options: { analysis: { type: "string" }, rounds: { type: "string" }, ffmpeg: { type: "string" } }, strict: true });
const analysis = values.analysis ?? join(homedir(), "hackaday-analysis");
const roundClips = values.rounds ?? join(homedir(), "whatsthatsound", "data", "clips");
const manifest = join(roundClips, "manifest.json");
const started = performance.now();
const library = new Library(join(analysis, "analysis.sqlite"), existsSync(manifest) ? manifest : null);
const site = { library, map: new SiteMap(library), clips: new Clips(values.ffmpeg ?? "ffmpeg"), analysis, roundClips };
const specs = pages(site);
process.stderr.write(`hackaday-next: ${library.newest.length} posts, ${library.episodes.length} episodes, ${library.stories.length} stories, ${library.rounds.length} rounds, ${specs.length} pages, loaded in ${Math.round(performance.now() - started)} ms\n`);

await runChannel([new ComponentChannel(specs)]);
