# Implementation log

The questions met while implementing without the owner, and the answer taken, as `~/devel/neoteletext-common/DEVELOPMENT.md` section 3 asks: number, question, decision, reason. A row is amended in place or superseded by a later row that says so.

## 25 October 2026: the repository and its process

| Row | Question | Decision | Reason |
|---|---|---|---|
| 1 | Where the workspace sits: at the root, as the engine has it, or in `native/`, as the browser does | At the root | The browser's `native/` sets the program apart from its documents and prototypes; this repository holds only the server. |
| 2 | Where the engine's and the browser's process forms differ, which | The browser's, as DEVELOPMENT.md Appendix A prefers | Staged-only commit gate, nothing skipped, pinned tools, State lines. |
| 3 | `rust-version` | 1.97, three releases behind the pinned 1.99.0 | DEVELOPMENT.md section 13's limit. |
| 4 | Install the pinned tools, or share the browser's | Link the browser's `tools/`, the same versions | The same binaries; building them again takes minutes and gains nothing. |

## 25 October 2026: each game on one page

| Row | Question | Decision | Reason |
|---|---|---|---|
| 5 | Which keys choose in The Lost Crown on its one page, where the coloured keys reach the channel instead of following links | F1 to F4, the remote's colour keys (codes 92 to 95), the engine viewer's r, g, y and c, and a left click on a choice or its prompt | While keys go to a channel every key goes (INPUT.md section 9.1), so the browser's own Fastext keys never act; these are the keys a viewer reaches for. |
| 6 | Whether a viewer's place in the story survives leaving the page | It survives another tab or page; choosing "Menu" at an ending begins the story again | Switching tabs sends PAGE 0 and back (INPUT.md section 9.1), which must not lose the story; the menu's link always led to the first node before. |
| 7 | Whether the one page keeps Fastext links | None for the choices, only Index | A link to the page itself would only reload it. A receiver without the input protocol sees the first node and cannot choose: the cost of one page a game. |

## 25 October 2026: the sound area in TypeScript

| Row | Question | Decision | Reason |
|---|---|---|---|
| 8 | What NeoTeletext Radio (501) sends a listener who joins a running stream, now that the protocol carries PCM and the server encodes it | The samples of the blocks it has yet to hear, numbered as the stream numbered them, in one `blocks` message of its own; the stream's own ticks still go in one message each to everyone with the page open | The Rust radio sent the joiner the other listeners' very records; a program cannot, as it never sees records. The samples are the same, so the joiner hears the same stream, its catch-up encoded afresh: one step-size restart at joining (TECH-DEBT.md), against one every tick were each block sent alone. |

## 25 October 2026: the gallery in TypeScript

| Row | Question | Decision | Reason |
|---|---|---|---|
| 9 | How a picture's credit link, which names a sub-page, is published from TTI, whose `FL` line names pages only | Packet X/27/0 written whole as the packed `OL,27` line the engine's TTI writer uses for such links (the kit's `packedLinksLine`), read back by the kit for page selection | It keeps the Rust behaviour exactly: the server reads the same links, sub-codes and all; linking to any sub-page instead would land on the credits' first sub-page. |
| 10 | Where the showcase menu puts the collections' folders, now `characters-ts` and the like, which overrun the Rust column 27 | At column 27, or further left as far as the widest folder needs to end at column 39: 26 today | The menu names the folder a viewer opens; cut short it names none. The Rust column comes back when the folders lose `-ts`. |
| 11 | What a level's TTI file keeps when it is rewritten as a sub-page | Its page's lines but for `PN`, `SC`, `FL` and a packed X/27/0 line, its description after its `PN` line; any other line the engine's reader does not take as a page's (an unknown command, the withdrawn `FH` and `TF`) is left out | The engine read past such a line when the Rust read the file, but the server refuses a publication that holds one, which would lose the picture's page. Any other fault the reader reports in a file still refuses that picture's page, where the Rust showed it; none of the 2,223 converted files has one. |
| 12 | Which characters of a caption the gallery folds | The kit's English fold, but for the em dash, a hyphen as in the Rust, and the set's arrows, fractions and other signs, unshown as in the Rust | The kit's fold is the deliberate change PORTING.md section 3.3 describes; it also folds U+2011, U+2012, U+2212, U+201A and U+2033, which the Rust left unshown and no caption holds. |
| 13 | Whether a collection without pictures has a channel | Yes: its index at 100, with no pictures listed | The Rust server made no channel for it, but `channels.json` starts every program it names, and a program declares its pages. |

## 25 October 2026: the Rust showcase removed

| Row | Question | Decision | Reason |
|---|---|---|---|
| 14 | Where the showcase menu puts the collections' folders, now that they are `characters` and the like again | Column 27, the Rust's, with a check at load that every folder ends within the row; row 10's workaround is gone | The `-ts` folders existed only while the Rust server served the plain names; the TypeScript programs serve them now. |
| 15 | What `channel_input` fuzzes once no application runs in the server | A channel whose application is the runtime's `Remote`, with a fixed declaration, fed two sessions' input and the program's lines | Every channel is a program now, so that is what a viewer's bytes reach; the program's lines are outside input too (requirement 2.6), which no target read before. |
| 16 | What becomes of `teletext-pages` and the server's `--pictures` | Removed with the Rust showcase | Both served only the in-process channels; the showcase's programs read the pictures themselves (`channels.json`), and the deploy script copies them to the host's `tmp/pictures`, where those programs look. |

## 28 October 2026: surviving a restart, logging and figures

Taken on the owner's instruction to go ahead with all of it ("i agree with all ... go ahead, do all of this then write the code and build and test"), and, for logging and figures, "i automatically accept what you propose for these". For the owner's review.

| Row | Question | Decision | Reason |
|---|---|---|---|
| 17 | How long a closed session waits for its viewer | 5 minutes | `SERVER.md` section 8.1's "a grace period of minutes": long enough for a phone's background socket and a server's restart; the memory waiting sessions hold under churn is not yet measured. |
| 18 | How long the session cookie lasts | 30 days, renewed at each connection | Long enough for a program to keep a viewer's game or place under the session's number (requirement 9.2); short of RFC 6265bis's 400-day cap, since it identifies a returning viewer to the channel's program. |
| 19 | What a second connection presenting a connected session's cookie gets | A new session of its own and no cookie | One session per connection keeps the runtime's input state per session; setting a cookie would take the first tab's session from it. Two tabs in one browser therefore share nothing until one closes. |
| 20 | How session numbers stay unique across restarts without keeping a counter | The start time in milliseconds times 1,024 | No file to keep or lose; 1,024 a millisecond is far above any rate of connection; the product stays below 2^53 until about the year 2248. |
| 21 | How often what a channel keeps is written | Within 5 seconds of a change, and at shutdown | Pages a program republishes every second would otherwise write a file a second each; 5 seconds lose at most that much on a crash, nothing on a clean stop. |
| 22 | Where `/metrics` is served, and on which port | An administration listener, none unless `--admin-listen` is given; the deploy uses `127.0.0.1:9464` | Figures about viewers stay off the public address; none by default keeps two servers on one machine (the tests) from colliding; 9464 is the port the OpenTelemetry Prometheus exporter registers. |
| 23 | The log's format by default | JSON when standard error is not a terminal, text when it is | What a supervisor or collector reads, and what a person at a terminal reads, without a flag. |
| 24 | How `RUST_LOG` is read | `tracing-subscriber`'s `Targets` filter, not `env-filter` | Targets and levels are all the server needs; `env-filter` brings a regular expression engine. |
| 25 | At which level a program's standard error is logged | `INFO`, target `program` | Programs write progress there as often as faults; a level from the line's text would be a guess. |
| 26 | The buckets of the channel's event-time histogram | 50 µs to 100 ms, eleven bounds | From a key's cost to a republication of hundreds of pages; Prometheus's default buckets start at 5 ms, above what a channel's task should take. |
| 27 | What a kept page's record holds | Version, kind, page, cursor, the frame's length, the frame | The frame is what is served, so nothing is encoded again at load; the length was added when `a_page_record_not_whole_is_refused` found that a record cut at a packet's end read as a shorter page. |
| 28 | Whether a page's record is the published sub-pages or the encoded frame | The encoded frame | The frame carries the header as the inserter stamped it; the time-filling header corrects the clock within a second. |

## 28 October 2026: the Cloudflare host

| Row | Question | Decision | Reason |
|---|---|---|---|
| 29 | Why `wasm-bindgen` failed with "externref table required for catch wrappers" | The Worker's release build is not stripped (`[profile.release.package.teletext-cloudflare] strip = false`) | The workspace's `strip = true` removed the module's `target_features` section, from which `wasm-bindgen` learns that it has reference types; built unstripped, it bound at once. |
| 30 | Where the Cloudflare host's secrets for programs live | A Worker secret a channel, `PROGRAM_SECRET_<FOLDER>`, sent by the program as `Authorization: Bearer`; locally `.dev.vars`, ignored by git | Requirement 1.5's channel's secret; a header, never the address, which logs keep. |
| 31 | How the object serves `channel.json`, `contents.txt` and the pages | The object answers them from the runtime, as the native host does | The edge cache in front (architecture section 3.3) is an optimisation once deployed; owed in `TECH-DEBT.md`. |
| 32 | What becomes of viewers connected when an object is evicted | They start afresh: each is sent every shared page and its session's join | While the clock is kept a watched object is not evicted for being idle; eviction then comes with a deployment, which closes the connections anyway. |
| 33 | Whether the object sends its program the `stats` line | Not yet | The figures are the native host's (`ServerStats`); the object's are the runtime's alone. Owed in `TECH-DEBT.md`. |
| 34 | The subdomain | `cf.neoteletext.com`, unless the owner names another | Short, and says which host serves it beside a native one. |

## 28 October 2026: Opus for the desktop

| Row | Question | Decision | Reason |
|---|---|---|---|
| 35 | How the server checks a packet against the profile when the engine's packet reader is private | Each packet decoded by the engine's public `Decoder` from a reset state; only its "outside the profile" refusal refuses the line | Writing the TOC rules here would copy the engine; a reset decoder judges the profile without state, and a frame the decoder finds corrupt from a reset state may be sound in its stream, so that is not refused. Each packet is decoded once more than needed, about 50 a second a live sound. |
| 36 | What "16 kbit/s on average" means for a constant-rate sound | Packets of at most 40 bytes | E.123 checks the average where a sound is published; at constant rate a packet's length is its rate, 40 bytes being 16 kbit/s exactly. |
| 37 | The protocol's names for Opus | `opus`, `opus_live`, `opus_packets`, beside `object`, `live`, `blocks` | Parallel to the PCM members, so a kit keeps both paths side by side; `packet_bytes` named as the engine names it. |
| 38 | The test's Opus | One second of a 440 Hz tone from `ffmpeg` with libopus 1.5, with E.124's own command, kept as `crates/teletext-runtime/src/remote/opus-tone.packets` | Real encoder output, every packet 40 bytes in five configurations (SILK, Hybrid and CELT, bare and padded); generated, so no one's rights in it. |

## 29 October 2026: row 0 the page's, and the keepalive (decision 22)

| Row | Question | Decision | Reason |
|---|---|---|---|
| 39 | Row 28's reason, now that nothing stamps the header | Row 28 stands: the record is the encoded frame | The frame now carries row 0 as the program published it, which is what a kept page must be served as. Row 28's reason is superseded. |
| 40 | When the native host pings | After 35 seconds of hearing nothing: the receiver's 25 and its own 10 added; dropped 10 seconds after an unanswered Ping | A live receiver on a quiet connection always pings first, so this side pings only a connection busy with pages or a dead one: a quiet connection carries the receiver's heartbeat alone. The bounds are the engine's constants, imported, so the two cannot drift. |
| 41 | Close a connection that answered nothing, or drop it | Drop it, without a Close | RFC 6455 section 7.1.7: a peer that answers nothing is failed, not closed; a Close would wait on the same dead socket. |
| 42 | What restarts the keepalive's wait | Any byte read | Any byte shows the receiver is there; frames need not be whole for that. |
| 43 | The Cloudflare host, where a Worker cannot send a Ping | Leave the receiver's Pings to Cloudflare's runtime, which answers them without waking the object | Measured under `wrangler dev`: a Ping answered with its payload. An object that woke for each would cost what the clock cost. |
