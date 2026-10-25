# teletext-server: architecture

The design of the server's behaviour is `~/devel/neoteletext-common/SERVER.md`, shared with the engine and the browser. This document holds the owner's decisions for this repository (section 2), how the server runs where it runs (section 3), and what is built (section 5).

## 1. What it is

The NeoTeletext server is a teletext application server: applications publish pages and streams, and each client receives, as 42-byte teletext packets, only what changed in the pages it shows. It imports its teletext from `neoteletext-engine` at one pinned commit and writes here, once, what the engine lacks for a server.

## 2. The owner's decisions

| Number | Date | Decision | Now |
|---|---|---|---|
| Decision 1 | 25 October 2026 | **A repository of its own**, decided by the owner (option B of `SERVER.md` section 3.3): `~/devel/teletext-server`, private on GitHub (`crowdwave/teletext-server`). It imports what it needs from the engine at one pinned commit, as the browser does, and never copies it. The engine's scope is unchanged. What the engine lacks for a server is written here, once (`SERVER.md` section 3.2), including the push protocol's two sides, which the browser will import from here. | **Kept** |
| Decision 2 | 25 October 2026 | **Async Rust on every host, around a runtime with no input or output** (section 3). On the owner's instruction: "write it async rust - ideally it will support wasm, run on cloudflare, run locally on a variety of operating systems - you advise me what to do in this area". The advice taken:<br>1. `teletext-runtime` is synchronous and sans I/O: `no_std` with `alloc`, no clock, no socket, no thread, no executor. It is driven by events and answers with effects.<br>2. The async lives in the hosts:<br>• a native host on tokio, for Linux, macOS and Windows on x86-64 and arm64;<br>• a Cloudflare host on `workers-rs`, one Durable Object per channel, with hibernatable WebSockets.<br>3. The runtime builds for `wasm32-unknown-unknown` at every commit gate.<br>4. Applications reach the server by each host's own means: stdio processes natively; HTTP, a WebSocket or a service binding on Cloudflare; in-process WebAssembly later. | Recommended 25 October 2026; **owner to confirm** |
| Decision 3 | 25 October 2026 | **The neoteletext development process** (`~/devel/neoteletext-common/DEVELOPMENT.md`), on the owner's instruction: "analyse the rust development processes for neoteletext so far and ensure this new project has the same". The browser's forms are used where the repositories differ, as that document's Appendix A prefers:<br>• the commit gate checks only what is staged and refuses unstaged changes in the parts it checks;<br>• nothing is skipped;<br>• tools are pinned and checked by SHA-256;<br>• the full gate runs on a clean tree at the pushed commit;<br>• requirements carry State lines;<br>• decisions are a table with a "Now" column.<br>Particular to this repository:<br>• one Cargo workspace at the root, as the engine has;<br>• Clippy for WebAssembly in the commit gate (requirement 1.3);<br>• no Windows step until there is a native host to build. | **Kept** |
| Decision 4 | 25 October 2026 | **The licence is MIT**, as the engine's and the browser's are. | **Owner to confirm** |
| Decision 5 | 25 October 2026 | **Libraries proposed, none approved yet** (DEVELOPMENT.md section 7: each needs the owner's approval by name). Nothing is added until approved. For the native host:<br>• `tokio`, the async runtime;<br>• `hyper` with `hyper-util`, HTTP/1.1 serving and the upgrade;<br>• `rustls` with `tokio-rustls`, TLS without OpenSSL.<br>For the Cloudflare host: `worker` (`workers-rs`, 0.8.x). For signing cookies: nothing (an HMAC over the engine's SHA-256). For the application protocol's JSON: nothing (the engine's `neoteletext_web::json` reader, with a small writer here). All are MIT or Apache-2.0. | Proposed 25 October 2026; **owner to approve each by name** |

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
- **Applications:** child processes over stdio.
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
- **Static pages** are served by the Worker itself from Workers static assets or R2, cached at the edge, without waking the channel's object.
- **Applications:** there are no processes. An application is reached over HTTP or a WebSocket to wherever it runs, or is another Worker reached by a service binding.
- **Limits to design within** (Cloudflare's published limits):
  - 128 MB of memory per isolate;
  - 30 seconds of CPU per request by default, up to 5 minutes;
  - the Worker's compressed size, which the dependencies must keep small.
- **Clock and randomness:** the clock comes from the host as every event's time. Random session tokens come from `getrandom`, with the WebAssembly back end Cloudflare needs (a library to approve).

### 3.4 WebAssembly beyond Cloudflare

The runtime and the engine's `core` and `formats` already build for `wasm32-unknown-unknown`. A WASI host (`wasm32-wasip2`), or a runtime in a web page for testing, is a further host if needed; nothing in the runtime changes.

## 4. The engine

Not yet imported. When the first crate needs it, every `neoteletext-*` dependency is a git dependency on `file:///Users/andrewstuart/devel/neoteletext-engine` at one `rev`, with `default-features = false` and the features named. The check script refuses two revisions, and the full gate's Linux stage reaches the engine as the browser's does.

## 5. What is built

- The repository and its process (decision 3), 25 October 2026.
- `teletext-runtime`: an empty `no_std` crate with the no-panic lint list, building for WebAssembly.

Not built: everything of `SERVER.md`.
