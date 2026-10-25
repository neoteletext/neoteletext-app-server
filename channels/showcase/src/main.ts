// The NeoTeletext showcase as its own program (docs/applications.md),
// which the server starts from channels.json: every area's applications
// on one channel, over standard input and output.

import { applications } from "./areas.ts";
import { runChannel } from "./kit.ts";

await runChannel(applications());
