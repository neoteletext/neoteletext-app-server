# Technical debt

What is owed, and why, as `~/devel/neoteletext-common/DEVELOPMENT.md` section 2.2 asks. An item is removed when done, or moved to an issue when it needs the owner.

## 25 October 2026: the repository started

- **No GitHub issues yet** (DEVELOPMENT.md section 3): the work of `SERVER.md` section 17 is to be opened as issues once the owner confirms decisions 2, 4 and 5.
- **`tools/` is a link to the browser's** pinned tools, the same versions; `scripts/tools.sh` builds this repository's own when the link is removed.

## 25 October 2026: the showcase

- **Applications cannot see the capabilities agreed in WELCOME**, so the form page (201) shows what it asks for, not what was agreed. The runtime could pass the agreed set to `opened`.
- **No overlay rows on a shared page for one session** (`SERVER.md` section 7.3): a viewer's typing on the guestbook goes to a page of its own (207), and the poll cannot show a viewer their own vote. The runtime needs an overlay of a session's rows over a channel page.
- **The engine's live audio encoder restarts its IMA ADPCM step size at every `live_records` call**, costing 2 to 7 dB at each restart (measured by the sound pages' builder). The radio encodes each tick's blocks in one call; a live encoder that keeps its state between calls is an engine change, for the owner under the engine's process (decision 1: the engine's scope is unchanged). The browser's reference podcast, which encodes one block a call, pays it on every block.

## The standard's maxima and sub-pages on the wire (26 October 2026)

Owed by decision 10: every item of `neoteletext-common/STANDARD-LIMITS.md` section 7 that names this repository, in its order; none is started. The owner's questions are its section 6.

## Surviving a restart, logging and figures (28 October 2026)

- **The kits do not connect to a Unix socket** (requirement 6.2): `neottx-server-ts-protocol`'s `wire/streams.ts` and the Python kit speak standard input and output only; each needs a transport that connects to the socket named in its environment and connects again when it goes.
- **Requirements 9.1 to 9.4 are owed in the channels' and the libraries' repositories:** kits' tests that `start` ends every session; the demo site's polls, guestbook and hall of fame in `node:sqlite`; kept component state in the component library; open pages published first after `start`.
- **The memory of sessions waiting for their viewer** (5 minutes each, requirement 5.5) under a churn of connections is not measured; `teletext-load` opens and holds connections, it does not churn them.
- **The browser has not been seen continuing a session**: its agent keeps cookies set on the push upgrade (the engine's `agent/push.rs`), but no run of the browser against this server has shown it.
- **`scripts/deploy.sh` is changed and not run:** its unit gains `StateDirectory=`, `--admin-listen 127.0.0.1:9464` and `TimeoutStopSec=15`.

## The Cloudflare host (28 October 2026)

- **The deployment is by hand** (`npx wrangler deploy` in `crates/teletext-cloudflare`, with `CLOUDFLARE_API_TOKEN` from `NEOTELETEXT_CF_TOKEN`); a deploy script, as `scripts/deploy.sh` is for the native host, is owed.
- **Static pages come from the object,** waking it; the edge's cache in front of `NNN.t42`, `index.t42` and `contents.txt` is owed (architecture section 3.3).
- **No `stats` line to the program and no `/metrics`** on Cloudflare: Workers Analytics or the object's own figures are to be chosen.
- **Sessions waiting for their viewer are lost when an object is evicted;** their grace holds the object awake meanwhile through its alarm.
- **The bridge stands in for the kits' WebSocket transport** (requirement 1.5): the kits are to connect themselves and the bridge to go.
- **`npm test` in `crates/teletext-cloudflare` is not in `scripts/check.sh`:** it needs Node and starts `wrangler dev`; a gate step for it is owed, with its time measured.

## Opus for the desktop (28 October 2026)

- **The kits do not encode Opus:** `neottx-server-ts-protocol` and the Python kit send PCM, which the desktop browser cannot play; each needs an `opus` path, with `ffmpeg` and libopus or an encoder of its language, and its channels' sounds moved to it.
- **The engine has no public profile check:** the server decodes each packet to judge it (`IMPLEMENTATION-LOG.md` row 35); a `neoteletext_opus` function that reads a packet without decoding it is an engine change, for its process.
- **The radio's transcoder** (requirement 10.4) is not built; its Opus encoder is the owner's to choose (`docs/streaming-audio.md` section 10 item 8).
