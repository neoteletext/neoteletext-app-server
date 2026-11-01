# teletext-server: architecture

The design of the server's behaviour is `~/devel/neoteletext-common/SERVER.md`, shared with the engine and the browser. This document holds the owner's decisions for this repository (section 2), how the server runs where it runs (section 3), and what is built (section 5).

## 1. What it is

The NeoTeletext server is a teletext application server: applications publish pages and streams, and each client receives, as 42-byte teletext packets, only what changed in the pages it shows. It imports its teletext from `neoteletext-engine` at one pinned commit and writes here, once, what the engine lacks for a server.

## 3. How the server runs where it runs

### 3.1 The runtime and its hosts

The owner asked for async Rust that runs on several desktop and server systems, on Cloudflare, and in WebAssembly. Those three pull against each other:
- **Native:** tokio, many threads, processes and files.
- **Cloudflare Workers:** `wasm32-unknown-unknown`, one thread, no tokio, futures that are not `Send`, no processes, and an object evicted when idle (hibernation).
- **WebAssembly generally:** no threads and no clock unless the host gives them.

Code written against one async runtime does not run on the others.

**The answer is to keep the async out of the runtime.** `teletext-runtime` is a state machine, one per channel:

```
host → runtime:  Opened(connection, cookies) | Frame(connection, bytes) | Closed(connection)
                 FromApplication(application, message) | Deadline(now) | Loaded(key, bytes)
runtime → host:  Send(connection, bytes) | Close(connection, reason)
                 ToApplication(application, message) | WakeAt(time) | Load(key) | Store(key, bytes)
```

Each host is a short async loop: wait for whatever comes next (a socket, an application, a timer), give it to the runtime with the time, and carry out the effects. The engine's own web agent works this way (`neoteletext-web`, sans I/O).

| | A synchronous runtime driven by async hosts (chosen) | An async runtime generic over a host trait |
|---|---|---|
| Native, Cloudflare, WebAssembly | One code, three short hosts | One code, if every future avoids `Send` and every executor's quirks |
| Tests | Deterministic: the test sets the time and feeds events; no network | Need an executor and fake I/O |
| Hibernation on Cloudflare | Natural: the state is plain data, saved and reloaded | Awaits in flight are lost when the object is evicted |
| Timers | One "wake me at" effect, which is exactly a Durable Object's single alarm | One timer per task, which Cloudflare does not have |
| Cost | Event and effect types to define; a host loop each | Less code at first |

### 3.2 The native host

- **Async runtime:** tokio, one task per channel holding that channel's runtime. A channel is single-threaded, as the design asks; channels run in parallel.
- **HTTP and WebSocket:** `hyper` serves the engine's channel layout (`SERVER.md` section 4) with `ETag`, and performs the WebSocket upgrade with the engine's `ws::accept_for`. The server-side WebSocket framing is this repository's own code, sans I/O, as `SERVER.md` section 3.2 item 4 plans.
- **TLS:** `rustls`, with certificates from files. A proxy such as Caddy may front it instead.
- **Applications:** child processes over stdio, or programs of their own over a Unix domain socket (decision 17).
- **What survives a restart:** the state directory (`--state`, or systemd's `StateDirectory=`): each channel's kept pages and declarations, the session cookie's keys, the accumulated figures (decisions 17 and 19). On `SIGTERM` every viewer is sent a Close with 4100, try again (`INPUT.md` section 5.1), and what is kept is written.
- **Log and figures:** structured events through `tracing` on standard error (decision 18); `/metrics`, `/healthz` and `/readyz` on the administration listener (decision 19).
- **Builds:** one static binary each for Linux, macOS and Windows, on x86-64 and arm64.

### 3.3 The Cloudflare host

`workers-rs` (0.8.7, 25 September 2026) has what the server needs:
- Durable Objects;
- hibernatable WebSockets: `State::accept_web_socket`, `accept_websocket_with_tags`, `get_websockets`, and the `websocket_message`, `websocket_close` and `websocket_error` handlers;
- alarms;
- SQLite storage.

Its `DurableObject` trait's async handlers do not need `Send`.

- **One Durable Object per channel** is the design's single actor per channel, with Cloudflare's routing to it: every connection to a channel reaches the same object, which `SERVER.md` section 20 item 3 asks of a native deployment too.
- **Hibernation:** an idle channel's object is evicted while its WebSockets stay open.
  - What must survive goes to the object's SQLite storage through the runtime's `Store` effects: published pages, sessions, each connection's tuned pages.
  - Each connection's last-sent versions are lost, so after a wake every tuned page goes whole once (`SERVER.md` section 6.3), about 1 KB a page.
  - A live channel keeps its object awake, and is billed for it.
- **Static pages** should be served by the Worker itself from the edge's cache or R2, without waking the channel's object; as built (decision 20) the object answers them (an rdm to-do).
- **Applications:** there are no processes. A program runs wherever its owner runs it and opens a WebSocket to its channel's Durable Object over `wss://`, with the channel's secret, so the object stays the server of every connection it holds and can hibernate (decision 17, requirement 1.5).
- **Limits to design within** (Cloudflare's published limits):
  - 128 MB of memory per isolate;
  - 30 seconds of CPU per request by default, up to 5 minutes;
  - the Worker's compressed size, which the dependencies must keep small.
- **Clock and randomness:** the clock comes from the host as every event's time. Random session tokens come from `getrandom`, with the WebAssembly back end Cloudflare needs (a library to approve).

### 3.4 WebAssembly beyond Cloudflare

The runtime and the engine's `core` and `formats` already build for `wasm32-unknown-unknown`. A WASI host (`wasm32-wasip2`), or a runtime in a web page for testing, is a further host if needed; nothing in the runtime changes.

## 4. The engine

Every `neoteletext-*` dependency is a git dependency on the engine's public repository, `https://github.com/neoteletext/neoteletext-engine`, at one `rev`, with `default-features = false` and the features named; the check script refuses two revisions. Until 28 October 2026 it was the engine's local repository, `file:///Users/andrewstuart/devel/neoteletext-engine`, whose history the gates and deploys copied to the Linux host; the engine went public that day, on the owner's instruction, and the dependency moved to it at the same commit, `e09a589`.

## 5. What is built

- The repository and its process (decision 3), 25 October 2026.
- `teletext-runtime` (decision 2): a channel as a state machine, with no input or output.
  - It speaks what the browser speaks today: the engine's channel layout, pages as T42 page frames, and INPUT.md version 2 with WELCOME and cursor records (no input modes since 27 October 2026, INPUT.md section 6.3).
  - Applications are state machines in the server's process; the `Remote` one stands for a program of its own (decision 8), which every channel now is.
  - Shared pages are published once; a session's own pages go to it alone.
- `teletext-native`: the native host on tokio and hyper.
  - Each channel's runtime runs in a task.
  - Every connection catches up on the shared pages from the change log by itself (decision 7).
  - `teletext-load` measures scale.
  - `scripts/deploy.sh` deploys as a systemd service.
  - Requirement 1.4's measurement: 100,000 connections at 9.5 KB each.
- **The showcase** (`showcase` in `~/devel/neoteletext-server-channels` since decision 12, TypeScript on the reference kit, `neottx-server-ts-protocol` since decision 13), served from that repository's `channels.json` at `showcase` and its collections at `characters`, `art`, `people`, `travel` and `gameart`. It was first built in Rust (`crates/teletext-showcase`), on `foundintelquiet4060` at `http://192.168.0.166:8090/` and seen in the macOS browser on 25 October 2026, then ported page for page (`showcase/PORTING.md` there) and the Rust removed.
  - **Live services (101 to 150):** news, weather, world clock, markets, server clock, sport, TV guide, about, teletext facts, a long read scrolled within its page (Page Up and Page Down, the arrows, the wheel and a mosaic scroll bar whose block can be pulled), each reader's own, and behind the scenes (150), the server's own figures, live.
  - **Input (200 to 207):** form, guestbook, live poll, quiz, calculator, mouse paint.
  - **Action games (300 to 305):** invaders with the sci-fi sprites, snake, breakout, cave flyer, a shared hall of fame.
  - **Adventures (400 to 402):** one page a game, each viewer's own: Station Zero, typed; The Lost Crown, on the coloured keys.
  - **Sound (500 to 504):** a live radio stream for every listener, piano, sound board, drum machine.
  - **Galleries (600 to 695):** your examples and the sci-fi sprites at Levels 1.5, 2.5 and 3.5, with credits; and channels for the characters, art, people, travel and game art.

- **Applications in any language** (decision 8; `docs/applications.md`), 25 October 2026: the channels file, the supervisor that starts each program and starts it again when it ends, and the runtime's `Remote` application that turns the channel's calls into lines and the program's lines into pages and sounds.
- **Surviving a restart** (decision 17), 28 October 2026: a clean stop with a code that says try again; shared pages and declarations kept through the runtime's `Store`, `Forget` and `load`, served at once by a restarted server; serving before programs declare, and declarations as events each time a program starts; session numbers that are never reused; sessions that outlive their connection with a signed cookie and 5 minutes' grace; programs over a Unix domain socket.
- **Logging and figures** (decisions 18 and 19), 28 October 2026: JSON events on standard error, `RUST_LOG`, a program's standard error logged; `/metrics` in the Prometheus text format, `/healthz`, `/readyz`, and totals accumulated across restarts.
- **The Python kit** (`kits/python`) and **the Hackaday channel** (`hackaday`), both in `~/devel/neoteletext-server-channels` since decision 12, 25 October 2026, on the host at `http://192.168.0.166:8090/hackaday/`: the latest 100 posts as stories (2NN), their lead pictures in Level 2.5 mosaics made by the engine's converter (3NN), threaded comments (4NN) and QR codes to the article and its videos made by `neottx qr` (5NN); a front page with the Jolly Wrencher, a ticker and a newsflash with a chime; headlines, sections and tags; search, on this day and roulette over the whole archive; DOOM watch, a quiz, a live poll, statistics, writers, the columns, the Retrocomputing Challenge's countdown and My Hackaday. Rebuilt the same day, on the owner's word that it should be "nicely structured ... into magazines and a logical hierarchy", as eight named magazines (`channels/hackaday/src/hackaday/site.py`): 1 Hackaday (front, headlines, the magazines, popular tags, tags A to Z, search, columns, the people, the archive, the fun, and the columns and news that name no subject, 160 to 189), then one subject each, 2 Retrocomputing, 3 Hardware, 4 Making, 5 Radio and wireless, 6 Science and space, 7 Software and security, 8 Games, music and art, each named in `contents.txt` by a section heading. In a subject magazine N00 is its front, N01 its feature (201 the Retrocomputing Challenge, 701 This Week in Security, 801 DOOM watch), N02 every story, N03 its tags, N04 to N09 its six busiest tags and N10 to N99 its stories, newest first, each titled by its post. The channel holds the latest 300 posts, and a viewer arriving at 100 sees the whole-page Jolly Wrencher for two seconds. Restructured on 26 October 2026 for pages and sub-pages, on the owner's instruction to "analyse ALL the content in our entire hackaday site and come up with a fresh new well organised structure designed for magazine/pages/subpages", from what the site holds (about nine posts a day, a median of 281 words or three screens and up to forty, a median of 12 comments and up to 99, half the posts with videos): a page is one thing and its sub-pages are its parts, each named in its band (row 1) so that the browser lists them by name (its decision 111). A story is one page: "Story 1/n" and on, "Picture", "Comments 1/m" and on, "Links" and a QR page for each video, at most 64 (the runtime's `MOST_SUBPAGES`). Headlines (101) are a sub-page a day; the columns (110) a sub-page a series, and each column's edition (111, 112, 701) its screens; 106 "Talking points" is the most discussed and then the latest comments; 108 "Facts and figures" is the statistics, the writers and the server's own figures; 132 is "Then" and "Now"; 133 the archive reader's screens; 190 "Help" four parts; each person's page their profile, their latest posts and their hackaday.io profile. Only a list to choose from scrolls: a magazine's stories and its tag pages. The whole site is about 3,000 sub-pages on 390 pages.

Not built: tuning (`SERVER.md` section 6.1, the browser's decision 104), cookies set by applications and identity (section 8.2 and 8.3), WebSocket and WebAssembly applications (section 7.1), TLS and ACME (decision 5), the Cloudflare host (section 3.3), systemd's socket activation (requirement 5.6), the carousel.
