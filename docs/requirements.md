# teletext-server: requirements

| | |
|---|---|
| **Status** | Started 25 October 2026 |
| **Companion documents** | `docs/architecture.md` (decisions), `~/devel/neoteletext-common/SERVER.md` (the design), `~/devel/neoteletext-common/DEVELOPMENT.md` (the process) |

## How to read

- No abbreviations.
- Numbers are permanent; a gap is deliberate.
- "Must" is mandatory; "should" is expected unless a reason is recorded; "may" is an option.
- Each requirement has a Statement, a Reason, a Source, a Verification and a dated State.

The behaviour of `SERVER.md` becomes requirements here as each part is adopted and built.

## 1. Where it runs

### Requirement 1.1: Native on the common systems
- **Statement:** The server must build and run as one program on Linux, macOS and Windows, on x86-64 and arm64.
- **Reason:** The owner asked for it to run locally on a variety of operating systems.
- **Source:** Decision 2.
- **Verification:** Test: the full gate builds and tests it on macOS and on the Linux host; Windows by hand in the virtual machine.
- **State:** Not met, 25 October 2026: no native host yet.

### Requirement 1.2: On Cloudflare
- **Statement:** The server must run on Cloudflare Workers, each channel in a Durable Object of its own, with its WebSockets hibernatable.
- **Reason:** The owner asked for it to run on Cloudflare.
- **Source:** Decision 2.
- **Verification:** Demonstration on Cloudflare; test with `wrangler dev` locally.
- **State:** Partly met, 28 October 2026: `crates/teletext-cloudflare` built (decision 20); with `wrangler dev`, `npm test` in that folder passes its four tests: a program's pages served through the bridge with their section and new title; a program without the secret refused with 401 and a second with 409; a viewer sent the pages and a clock header each second, given a session cookie and continuing its session with it; and the host restarted serving the page it kept before its program connects again. Deployed on 28 October 2026 at `https://cf.neoteletext.com` (814 KiB uploaded, 285 KiB compressed, 4 ms start-up), answering over HTTPS with its certificate; `npm test` against it (`NEOTELETEXT_URL=https://cf.neoteletext.com`) passed its three tests that run on a deployment, the kit's fixture channel bridged from macOS over `wss://`, and with the program gone its pages were still served (page 100, 1,092 bytes, in 0.10 s). The name did not yet resolve through the local network's resolver, which still held the failure it cached before the delegation, so the test resolved it to the address Google's public resolver gave. Redeployed the same day with decision 21's Opus (962 KiB uploaded, 356 KiB compressed, 4 ms start-up) and tested again three times, clean, after the first test was made to wait for the program's titles: a deployment serves page 100 kept from before while a newly connected program is still declaring (requirement 5.2), which the local run, starting from empty storage, never met. Hibernation with viewers connected is given up while the clock is kept (decision 20).
- *Amended 28 October 2026 (decision 17):* the Durable Object must be the server of every WebSocket it holds, its viewers' and its programs' (requirement 1.5), and must hold no outbound socket, since an object that opened a connection cannot hibernate for up to 15 minutes a connection (Cloudflare's Durable Objects documentation, "WebSockets"). Whether a channel's object can hibernate while it is watched also depends on the time-filling header the native host sends every second, which keeps an object awake: whether the client draws its own clock is the owner's to decide (`SERVER.md` section 20 item 6).

### Requirement 1.3: One runtime, with no input or output, that builds for WebAssembly
- **Statement:** The server's runtime must do no input or output: no clock, no socket, no file, no process, no thread and no async executor. It must be `no_std` with `alloc`, and build for `wasm32-unknown-unknown`. Every host must drive this same runtime, and no host's code may be in it.
- **Reason:** One runtime can then run under tokio, in a Durable Object and in WebAssembly, and its tests set their own time with no network.
- **Source:** Decision 2.
- **Verification:** Check: the commit gate runs Clippy for `wasm32-unknown-unknown` over the runtime, and `no_std` keeps the standard library's input and output out.
- **State:** Met for what exists, 25 October 2026: `teletext-runtime` is `no_std` and passes Clippy for `wasm32-unknown-unknown`.

### Requirement 1.4: As many connections as possible
- **Statement:** The native server must hold as many WebSocket connections as the host's memory and open-file limit allow. Its costs:
  - a shared page's change costs the channel the same whatever the number of viewers;
  - each connection's idle memory is measured and kept small;
  - a slow connection never makes the server hold more than one version of each page for it.

  The numbers must be measured on `foundintelquiet4060` and recorded here: connections held, memory per connection, and time from a change to the last viewer's send.
- **Reason:** The owner: scale is critical.
- **Source:** Decision 7.
- **Verification:** Test: the load client (`teletext-load`) against the server on the host; the numbers in this State line.
- **State:** Met, 25 October 2026, on `foundintelquiet4060` (20 cores, Linux), the server as a systemd service with an open-file limit of 524,288, and the load clients on the same host against loopback addresses. Each connection received the clock page (105, 1,092 bytes) once a second.

  | Connections | Server memory | Per connection | Change to the last viewer | Sent |
  |---|---|---|---|---|
  | 10,000 | 95 MB | 8.6 KB | 11 to 16 ms | 10.9 MB/s |
  | 50,000 | 440 MB | 8.6 KB | 61 to 64 ms | 54.6 MB/s |
  | 100,000 | 961 MB | 9.5 KB | 123 to 130 ms | 109 MB/s |

  The server's memory before any connection was 8.9 MB. A run beyond 100,000 is limited by the load clients' ports, about 28,000 per loopback address, and their own memory, not by the server.

  Measured again on 28 October 2026, after decisions 17 to 19 (logging, kept pages, session cookies), against the commit before them, both release builds on `foundintelquiet4060`, 10,000 connections from `teletext-load`, a program publishing page 105 each second: 8,772 bytes a connection before, 8,855 after (+0.9 %); the fan-out the load client reported was the same for both (1,953 and 1,983 ms) and is not comparable with the table's, whose clock page came from the Rust showcase. A closed session now waits 5 minutes for its viewer (requirement 5.5); the memory those waiting sessions hold under a churn of connections is not yet measured.

### Requirement 1.5: On Cloudflare, programs connect to their channel
- **Statement:** On Cloudflare, a channel's program must reach its channel by opening a WebSocket to the channel's Durable Object over `wss://`, presenting the channel's secret on the upgrade; the object accepts it with the hibernation interface, as it accepts its viewers. Each WebSocket message carries one line of `docs/applications.md`'s protocol, the same lines as every other transport (requirement 6.4), at most 16 MiB, within the object's 32 MiB limit for a received message. The program runs wherever its owner runs it; nothing on Cloudflare starts it.
- **Reason:** Workers cannot start processes, so the channels' programs run elsewhere. An object that is the server of its connections can hibernate with them open; one that opens connections itself cannot. The program connecting out also needs no tunnel through the network address translation in front of the owner's machines.
- **Source:** Decision 17.
- **Verification:** Test with `wrangler dev`: a program connects, declares its pages, is sent `start`, and publishes; the object hibernates with the program's connection open and wakes on its next line.
- **State:** Partly met, 28 October 2026: the object accepts its program's WebSocket with the hibernation interface after the Worker checks the secret, and the kit's fixture channel ran through `scripts/bridge.ts` against `wrangler dev` (`npm test` in `crates/teletext-cloudflare`); the kits have no WebSocket transport of their own yet, and nothing is deployed.

## 2. Build and verification

### Requirement 2.1: One check script and the git hooks
- **Statement:** `scripts/check.sh` must run every gate:
  - `commit` before each commit: formatting, the no-panic lists, Clippy in every feature shape and for WebAssembly, and tests;
  - `full` before each push, on a clean tree at the pushed commit: the commit gate's steps, documentation tests, release tests, tests with optional features compiled out, licences and advisories, unused dependencies, fuzzing, and the Linux host.

  The hooks in `.githooks/` must run it (`git config core.hooksPath .githooks`). Nothing may be skipped, and nothing may bypass the hooks.
- **Reason:** The gates are local; there is no CI (DEVELOPMENT.md section 9).
- **Source:** Decision 3.
- **Verification:** Check: the hooks call the script; review of the script's steps.
- **State:** Met, 25 October 2026.

### Requirement 2.2: The strictest lints
- **Statement:** The workspace's lints must forbid unsafe code; warn, as errors in the gates, on missing documentation, unused qualifications, Clippy's pedantic group, `allow` without a reason, `dbg!`, `todo!` and `unimplemented!`. Every crate root must carry the same no-panic list: `indexing_slicing`, `string_slice`, `unwrap_used`, `expect_used`, `panic`, `arithmetic_side_effects`, `unreachable`, `todo` and `unimplemented`. A suppression must be narrow, at its site, with its reason.
- **Reason:** Everything the server reads comes from clients and applications that may be hostile (DEVELOPMENT.md section 6).
- **Source:** Decision 3.
- **Verification:** Check: the commit gate compares every root's list.
- **State:** Met, 25 October 2026.

### Requirement 2.3: Dependencies by approval, licences allowed
- **Statement:** Every library must have the owner's approval by name, recorded in a decision, and a licence that `deny.toml` allows. `Cargo.lock` must be committed. Programs must pin `=x.y.z`, with the licence and the reason beside each. `cargo deny` and `cargo machete` must pass.
- **Reason:** DEVELOPMENT.md section 7.
- **Source:** Decisions 3 and 5.
- **Verification:** Check: the full gate.
- **State:** Met, 25 October 2026: no library yet.
- *Amended 28 October 2026 (decision 18):* `tracing` 0.1.44 and `tracing-subscriber` 0.3.23 added, pinned `=x.y.z` with their licence and reason in `Cargo.toml`; `cargo deny check` and `cargo machete` pass. The release binary grew from 1,723,728 to 2,457,496 bytes on `foundintelquiet4060`.

### Requirement 2.4: Pinned tools
- **Statement:** The development tools must be installed at exact versions into the ignored `tools/` by `scripts/tools.sh`, each crate checked against its recorded SHA-256, and the check script must refuse any other version.
- **Reason:** DEVELOPMENT.md section 13.
- **Source:** Decision 3.
- **Verification:** Check: the check script's first step.
- **State:** Met, 25 October 2026: `cargo-nextest` 0.9.148, `bacon` 3.26.0, `cargo-deny` 0.20.2, `cargo-fuzz` 0.13.2, `cargo-machete` 0.9.2, and the fuzzing compiler `nightly-2026-10-01`, the browser's. *28 October 2026:* `worker-build` 0.8.7 added (decision 20); `tools/` is excluded from the workspace, since a tool's crate built there was taken for one of its packages.

### Requirement 2.5: The engine imported, never copied
- **Statement:** Every engine crate must be a git dependency on the engine at one commit, the same for every manifest. No code may be copied from the engine or the browser.
- **Reason:** The owner's instruction to share code, not rebuild it (decision 1).
- **Source:** Decision 1.
- **Verification:** Check: the commit gate's engine pin step; review.
- **State:** Met, 25 October 2026: no engine crate imported yet. *28 October 2026:* every engine crate at `e09a589`, the browser's pin (decision 21), `neoteletext-opus` among them. Later that day at `8c59a89` (E.133), the browser's pin again, so both count input units alike: a move, gone, hover, COMPOSE and RESUME count none.

### Requirement 2.6: Every reader of outside input fuzzed
- **Statement:** Every reader of bytes from clients, applications or files must have a fuzz target in `fuzz/`, and every input that ever found a fault must be kept in `fuzz/regressions/` and replayed first by the full gate.
- **Reason:** DEVELOPMENT.md section 8.
- **Source:** Decision 3.
- **Verification:** Check: the full gate's fuzzing step.
- **State:** Met, 25 October 2026: `websocket_reader` (the server's WebSocket frames, in any pieces) and `channel_input` (a channel whose application is a program, the runtime's `Remote`, as every channel's is since the Rust showcase was removed: two sessions' input streams and the program's lines, with time passing); each ran 20 seconds clean (906,063 runs; `channel_input` 447,006 runs in its new form). *28 October 2026:* `store_reader` (what the server kept, under any key: requirement 5.2) and `session_cookie` (a viewer's `Cookie` header: requirement 5.5) added; 20 seconds each on macOS: `store_reader` 2,491,165 runs, `session_cookie` 3,142,361, `channel_input` 234,015, all clean. *28 October 2026, decision 21:* `opus_sound` (a program's Opus packets, any bytes at any packet length the protocol takes: requirement 10.1) added; on the engine at `e09a589`, 20 seconds each on macOS: `opus_sound` 228,910 runs, `channel_input` 145,977, `store_reader` 1,997,811, `session_cookie` 3,059,254, `websocket_reader` 743,683, all clean.

## 3. Pages and sub-pages

### Requirement 3.1: The standard's maxima, from the engine
- **Statement:** The server must accept every page number the standard allows for content (00 to FE in magazines 1 to 8, less Annex A.10's reserved numbers: 2,022) and every sub-page a link can name (sub-codes 0001 to 3F7E: 8,190 a page), naming each bound from the engine's `neoteletext_core::limits` and writing none of its own. What it holds and sends at once is bounded by budgets, never by refusing a page or sub-page the standard allows.
- **Reason:** The owner's instruction that the engine, the server and the browser have the standard's maximum from one piece of code.
- **Source:** Decision 10; `neoteletext-common/STANDARD-LIMITS.md`.
- **Verification:** Test: a channel that declares 2,022 pages and publishes 8,190 sub-pages on one; check: no literal bound in the runtime.
- **State:** Not met, 26 October 2026: `MOST_PAGES` is 800 and `MOST_SUBPAGES` 64 in `teletext-runtime`.

### Requirement 3.2: Sub-pages pass the wire only when needed
- **Statement:** A client must receive a sub-page's content only while it tunes that sub-page; for the rest of a tuned page it receives the page's catalogue (sub-codes and versions) and its changes, and titles only for the places it asks for (`SERVER.md` sections 5.4, 5.5 and 6). An application must be able to publish and remove single sub-pages without resending the others (`SERVER.md` section 7.3).
- **Reason:** The owner's instruction that sub-pages pass the wire only when needed; a page at the standard's maximum is about 9 MB.
- **Source:** Decision 10.
- **Verification:** Test: bytes sent to a client tuned to one sub-page of a page of 8,190, when it opens, when another sub-page changes and when it steps; measure before and after.
- **State:** Not met, 26 October 2026: a page is held and sent whole, to every connection, on every change.

### Requirement 3.3: Applications are told the limits
- **Statement:** `start` must carry the standard's maxima and the server's budgets, written from the engine's constants; the Python and TypeScript kits must expose them and hold no limit of their own, and a test of each kit must check that none is written as a literal.
- **Reason:** One definition, for programs in languages that cannot import the engine.
- **Source:** Decision 10.
- **Verification:** Test of each kit; review of the channels.
- **State:** Not met, 26 October 2026: the Hackaday channel writes `MOST_SUBPAGES = 64`, and `start` carries `protocol` and `epoch_ms` only.

## 4. Channels' programs

### Requirement 4.1: A program started again picks up its viewers
- **Statement:** A channel's program that ran 5 seconds or more after declaring its pages, and then ended, must be started again at once; one that ends sooner keeps the back-off of 1, 2, 4 … 30 seconds. When it is running again it must be sent `join` for every viewer connected and `open` for each viewer on one of its pages, and each viewer's own copies of its pages must be dropped, so it draws them afresh (`docs/applications.md` section 1).
- **Reason:** The owner, 27 October 2026, on fast iteration: a program restarted as its author saves a change must show the change to the viewer already on the page, without a reload and without waiting; a program that crashed gets its viewers back the same way.
- **Source:** The owner's request for fast iteration, 27 October 2026.
- **Verification:** Tests: a restarted program is told of its viewers and redraws each one's own page; a steady program starts again at once and a failing one waits.
- **State:** Met, 27 October 2026: `a_restarted_program_is_told_of_its_viewers`, `a_steady_program_starts_again_at_once_and_a_failing_one_waits`; by hand, a saved change showed in the browser in 1.3 to 2 seconds.

### Requirement 4.2: The mouse modes a page may ask for are agreed
- **State:** Withdrawn, 27 October 2026, superseded by INPUT.md section 6.3: the input modes no longer exist, every event is sent whenever keys go to the channel, and WELCOME agrees only the capabilities (`a_receiver_is_welcomed_and_its_input_acknowledged`, `welcome_agrees_no_mode_bits`).

## 5. Surviving a restart

What the server holds is a cache of what its programs published, and its viewers' sessions; what matters lives in the programs (`SERVER.md` section 8.4). A restart must cost the viewer a reconnection and nothing more, and must never present one viewer's session as another's.

### Requirement 5.1: A stopped server closes its connections cleanly
- **Statement:** On `SIGTERM` or `SIGINT` the native server must stop accepting connections, send every viewer's WebSocket a Close frame with status 1012, "service restart" (RFC 6455 section 7.4, the IANA WebSocket Close Code registry), write what it keeps (requirements 5.2 and 8.3), and exit with status 0 within 10 seconds. A connection closed for breaking the protocol is sent status 1002, "protocol error", before it closes.
- **Reason:** A viewer told that the service restarts reconnects at once; a connection reset tells it nothing. What is kept must be written before the process ends.
- **Source:** Decision 17.
- **Verification:** Test: the server binary sent `SIGTERM` with a viewer connected; the viewer reads a Close frame carrying 1012, and the server exits with status 0.
- **State:** Met, 28 October 2026: `a_restart_closes_viewers_as_a_restart_and_serves_what_was_kept` (the binary, on macOS and on `foundintelquiet4060`) and `a_stopping_channel_closes_its_connections_and_keeps_its_pages`; the 1002 on a protocol error by review of `connection.rs` and `hub.rs`. Measured on `foundintelquiet4060`: the server before this change exited with status 143 on `SIGTERM`, this one with 0.

### Requirement 5.2: Shared pages survive a restart
- **Statement:** Every shared page a channel publishes, with its cursor, and every page its programs declare, with its title and section, must be kept through the runtime's effects, never by the runtime itself: `Store` with a key and bytes, `Forget` with a key, and `load`, which a host calls with what it kept before the channel starts. The native host must keep them as files in its state directory (`--state`, or `$STATE_DIRECTORY` as systemd sets it), each written to a temporary file and renamed into place, within 5 seconds of the change and at shutdown. A restarted server must serve what it kept at once, until each program publishes afresh; a page its program no longer declares must be withdrawn. What is kept carries a format version; a record of another version, or one that does not read whole, must be logged and ignored, never acted on in part. A server with no state directory keeps nothing.
- **Reason:** A program needs time to start and publish; meanwhile the viewers would see nothing. The same effects are how a Durable Object keeps its pages through hibernation (requirement 1.2), so the runtime gains one interface for both hosts.
- **Source:** Decision 17; `docs/architecture.md` section 3.1.
- **Verification:** Tests: a channel's published pages, loaded into a new channel, are served before its program declares; a page no longer declared is withdrawn; a record of another version or cut short is refused; the server binary, restarted with its state directory, serves a page its program has not yet published again. Fuzzing: the record reader (requirement 2.6).
- **State:** Met, 28 October 2026: `kept_pages_are_served_before_the_program_declares`, `a_page_record_reads_back`, `a_page_record_not_whole_is_refused` (which found that a record cut at a packet's end read as a shorter page, so a record now carries its frame's length), `a_pages_record_reads_back_and_keeps_the_declarations_rules`, `a_key_is_named_and_read_back`, `records_are_kept_read_back_and_forgotten`, and the binary restarted in `a_restart_closes_viewers_as_a_restart_and_serves_what_was_kept`; fuzzing: `store_reader`. The Durable Object's half waits for the Cloudflare host.

### Requirement 5.3: The server serves before its programs declare
- **Statement:** The native server must listen as soon as it has read its channels file and what it kept, without waiting for any program. A program's declaration of its pages must reach its channel as an event each time the program starts, the first time and every time after, so that a program started again with pages added or removed is served with its new pages. Lines for a program that is not running must be dropped, not queued, since a program that starts is told of every viewer afresh (requirement 4.1).
- **Reason:** One slow or failing program must not keep every channel off the air; a program's author who adds a page and saves must see it without the server starting again. A line queued for a program that ended would reach the next one before `start`, about sessions it never knew.
- **Source:** Decision 17.
- **Verification:** Tests: a channel answers before its program has declared; a program started again with a new page is served that page; no line reaches a program before `start`.
- **State:** Met, 28 October 2026: `probes_and_metrics_are_on_the_administration_listener` (the channel served while its program has not declared), `a_program_is_started_heard_and_started_again` (declared again when started again), `lines_reach_a_program_only_while_it_runs`, `lines_for_an_ended_program_are_dropped`.

### Requirement 5.4: Session numbers are never reused, across restarts too
- **Statement:** The native server must number sessions from the number of milliseconds since the Unix epoch at its start, times 1,024, so that a number issued after a restart is greater than any issued before it while the clock moves forward. Every session number must be below 2^53, the largest integer a TypeScript program reads exactly from JSON (ECMA-262 section 21.1.2.6, `Number.MAX_SAFE_INTEGER`).
- **Reason:** `docs/applications.md` promises programs that a session number is never reused; numbering from 1 at every start broke that promise across restarts, which matters as soon as a program outlives the server (requirement 6.2) or keeps what it knows of a session (requirement 9.2).
- **Source:** Decision 17.
- **Verification:** Test: the first number is the start time times 1,024, and below 2^53.
- **State:** Met, 28 October 2026: `session_numbers_start_from_the_start_time`; `http.rs` numbers from `session_cookie::first_session`.

### Requirement 5.5: A session outlives its connection
- **Statement:** A session must continue across connections for as long as the viewer's browser keeps its session cookie (`SERVER.md` sections 8.1 and 8.2):
  - The server sets the cookie `_session` on the push upgrade's response: the session's number, its expiry, and an HMAC-SHA-256 (RFC 2104, FIPS 180-4) over the channel, the number and the expiry, with the attributes `Path=/<channel>/`, `HttpOnly`, `SameSite=Strict`, `Max-Age` of 30 days, renewed at each connection, and `Secure` when the origin is `https`.
  - A connection presenting a cookie that verifies under the current or the previous key, and has not expired, continues that session. A cookie that does not verify is ignored, never shown to a program. A session already connected is not shared: a second connection presenting its cookie is given a new session of its own, and no cookie.
  - The key is 32 random bytes from the system's source, kept in the state directory readable by its owner alone, or made afresh at each start when there is none; `session.key.previous`, when present, is the previous key, for rotation.
  - A session whose connection closes stays for 5 minutes: its program is told `leave` for the page it had open, and `close` only when the 5 minutes pass without a new connection. A connection that continues it is a new input session (INPUT.md section 4): the program is told `open` when its viewer opens a page, not `join` again, and the session's own pages are sent to it again.
- **Reason:** Phones close background sockets, and a server restart closes every socket: a viewer coming back must find their game, form or story where they left it, and a program must be able to keep what it knows of a viewer under a number that lasts (requirement 9.2). Identity that travels with every request is what lets a stateless server rebuild a viewer's state.
- **Source:** Decision 17; `SERVER.md` sections 8.1 and 8.2.
- **Verification:** Tests: a closed session's program is told `leave` at once and `close` only after 5 minutes; a connection with the session's cookie continues it, sent its own pages, with no second `join`; a forged, expired or other channel's cookie starts a new session; a session connected twice gives the second connection a new session; the cookie's attributes. Fuzzing: the cookie reader (requirement 2.6).
- **State:** Met, 28 October 2026, on Linux and macOS: `a_closed_session_waits_for_its_viewer`, `a_viewer_who_comes_back_continues_their_session`, `a_cookie_names_its_session_while_it_lasts`, `an_altered_cookie_names_nothing`, `the_cookie_is_scoped_to_its_channel`, `the_session_key_is_made_once_and_kept`, and the binary in `a_session_continues_with_its_cookie`; fuzzing: `session_cookie`. Not on Windows: the standard library has no source of random bytes there, and `getrandom` awaits the owner's approval (decision 5); a session there ends with its connection. Not yet seen in the browser, whose agent sends the channel's cookies on the push upgrade and keeps those its response sets (the engine's `agent/push.rs`).

### Requirement 5.6: The listening socket outlives a restart
- **Statement:** The native server should accept its listening socket from systemd's socket activation (`sd_listen_fds`), so that connections made while it restarts wait instead of being refused.
- **Reason:** A restart refuses connections from the old process's exit until the new one listens.
- **Source:** Decision 17.
- **Verification:** Test on the Linux host: connections made during a restart are answered once it is running.
- **State:** Not met, 28 October 2026: taking a socket by its descriptor needs `unsafe` (`FromRawFd`), which requirement 2.2 forbids, or a library the owner has not approved; for the owner (`PENDINGTASKSFORANDREW.md`).

## 6. How programs reach their channel

### Requirement 6.1: Standard input and output
- **Statement:** A channel named in the channels file with a `command` must have that program started by the server and spoken to over its standard input and output, one line a message (`docs/applications.md`).
- **Reason:** The simplest deployment, and the edit loop: the server starts the program again as its author saves a change (requirement 4.1).
- **Source:** Decisions 8 and 17.
- **Verification:** Test: `crates/teletext-native/tests/channel_program.rs`.
- **State:** Met, 25 October 2026.

### Requirement 6.2: A Unix domain socket, for a program that runs as a service of its own
- **Statement:** A channel named in the channels file with a `socket` (a path, relative to the file's folder) must be served to a program that connects to that socket, on Linux and macOS:
  - The server makes the socket's folder, readable by its owner alone, when it does not exist, removes a socket left at the path, and makes the socket readable and writable by its owner alone. The folder's permissions are the authentication.
  - One program at a time: a second connection while one is live is closed.
  - The program's first line declares its pages, within 30 seconds, as over standard input and output; it is then sent `start` and told of every viewer (requirement 4.1).
  - When the program disconnects, its channel keeps its pages as last published; when it connects again, it declares its pages again. When the server restarts, the program connects again.
- **Reason:** A program that runs as a service of its own outlives a restart of the server, and keeps what it holds in memory. The socket needs no secret and no port, and Python's and Node's standard libraries speak it. Its speed is not the reason: measured on 28 October 2026, a round trip over a Unix socket was 5 to 16 microseconds faster than over loopback TCP, against a 20-millisecond budget (`SERVER.md` section 10).
- **Source:** Decision 17.
- **Verification:** Test: a program connects to the socket, declares, is sent `start`, publishes a page the server then serves over HTTP, disconnects, connects again and is sent `start` again; the socket's mode is 0600.
- **State:** Met, 28 October 2026, in the server: `a_program_connects_to_its_socket`, on macOS and on `foundintelquiet4060`. The kits do not yet connect to a socket (TECH-DEBT.md).

### Requirement 6.3: A WebSocket, for a program elsewhere
- **Statement:** The native server should accept a channel's program over a WebSocket, as the Cloudflare host does (requirement 1.5), authenticated by the channel's secret, for a program on another machine.
- **Reason:** One network transport for both hosts; a Unix socket reaches only a program on the same machine.
- **Source:** Decision 17.
- **Verification:** Test: a program connects over a WebSocket with the secret and is served; one without it is refused.
- **State:** Not met, 28 October 2026: built with the Cloudflare host, which needs it first.

### Requirement 6.4: One protocol on every transport
- **Statement:** Every transport must carry the same lines of `docs/applications.md`, with the same 16 MiB limit, the same order of `pages` first and `start` after, and the same checks of every line; the server's handling of a program's lines must not know which transport carried them.
- **Reason:** The kits and the channels then change only the code that opens the connection.
- **Source:** Decision 17.
- **Verification:** Review: one relay, generic over its reader and writer, serves every transport.
- **State:** Met, 28 October 2026: `apps.rs` has one `relay`, generic over its reader and writer, and one `declare`, for both transports.

## 7. Logging

### Requirement 7.1: Structured events
- **Statement:** Everything the native server reports must be an event with a time (RFC 3339, UTC), a level (`ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`), a target naming the part of the server that reports it, a message, and fields, written one event a line to standard error: as JSON objects when standard error is not a terminal, as text when it is, or as `--log-format json` or `text` says. Nothing may be written to standard error or standard output except through it.
- **Reason:** One event a line with named fields is what journald, Loki, Vector and Cloudflare's Workers Logs read and search; text is for a person at a terminal.
- **Source:** Decision 18.
- **Verification:** Test: the server binary with standard error a pipe writes its start as one JSON object with the fields `timestamp`, `level`, `target` and `fields`; review: no `eprintln!` or `println!` in the server's code.
- **State:** Met, 28 October 2026: `the_log_is_json_lines_and_rust_log_chooses_the_level` (the binary), `an_event_is_a_line_of_json_with_its_fields`; no `eprintln!` or `println!` in the server's code but the load client's.

### Requirement 7.2: Levels chosen at start
- **Statement:** Which events are written must be chosen by `RUST_LOG`, as targets and levels (`info,teletext_native::connection=debug`), `info` when it is unset or does not read.
- **Reason:** The convention every Rust server's operator knows; detail is turned on for one part without a rebuild.
- **Source:** Decision 18.
- **Verification:** Test: `RUST_LOG=warn` leaves out the `INFO` start event.
- **State:** Met, 28 October 2026: `the_filter_reads_rust_log`, and `RUST_LOG=warn` in `the_log_is_json_lines_and_rust_log_chooses_the_level`.

### Requirement 7.3: What is reported
- **Statement:** The server must report at least:
  - at `INFO`: its start (version, listening address, origin, channels, state directory), its stop, each program's start, declaration and end with the reason and the pause before it starts again, what it kept and loaded, and its figures once a minute as fields;
  - at `WARN`: a line refused from a program, with the reason; lines for a program dropped because it fell behind, at the first and then at each power of two; a record of what it kept that it ignored; a cookie key it could not keep;
  - at `ERROR`: a listener that fails, a state directory it cannot write;
  - at `DEBUG`: each connection's opening and closing, with its session number, and a connection closed for breaking the protocol.

  Each event about a channel carries the field `channel`, its folder.
- **Reason:** What an operator needs to know a restart went well, and to find why a channel is off the air, without reading code.
- **Source:** Decision 18.
- **Verification:** Review of the events against this list; tests of the start event and a refused line.
- **State:** Met, 28 October 2026, by review of the events in `main.rs`, `apps.rs`, `hub.rs` and `connection.rs` against this list, and the tests of the start event, a refused line and a program's standard error.

### Requirement 7.4: Nothing that identifies a viewer
- **Statement:** No event may carry a viewer's address, anything a viewer typed, a cookie or any part of one, or a session key. A session's number may be carried.
- **Reason:** The figures' rule (`docs/applications.md` section 7) applied to logs, which are kept longer and copied further.
- **Source:** Decision 18.
- **Verification:** Review of every event's fields.
- **State:** Met, 28 October 2026, by review: the fields are channels, session numbers, counts, codes, files of the state directory and reasons. A program's own `log` lines and standard error are its own words, logged as they come: a program that logs what its viewers type breaks this rule itself.

### Requirement 7.5: A program's standard error is logged
- **Statement:** Each line a program writes to its standard error must become an `INFO` event of the target `program` with the field `channel`; a line longer than 64 KiB is cut there and marked so.
- **Reason:** A program's own messages, inherited until now, reached the journal unattributed, interleaved with the server's.
- **Source:** Decision 18.
- **Verification:** Test: a program's line on standard error becomes an event with its channel.
- **State:** Met, 28 October 2026: `a_program_is_started_heard_and_started_again`, `a_long_line_of_standard_error_is_cut`.

### Requirement 7.6: The runtime's reports are events too
- **Statement:** The runtime must report through an effect carrying a level, a stable event name and its text, which each host writes in its own way, the native host as a `tracing` event.
- **Reason:** The runtime does no output (requirement 1.3); the level and name must still reach the operator.
- **Source:** Decision 18.
- **Verification:** Test: a refused line is reported at `WARN` with its event name.
- **State:** Met, 28 October 2026: `reports_carry_their_level_and_name`, `an_event_is_a_line_of_json_with_its_fields`.

## 8. Figures

### Requirement 8.1: Metrics in the Prometheus text format
- **Statement:** The native server must serve `/metrics` in the Prometheus text exposition format, version 0.0.4, on its administration listener (`--admin-listen ADDRESS`; none unless it is given), never on the listener viewers reach. The names follow Prometheus's conventions: the prefix `teletext_`, base units (seconds, bytes), `_total` on every counter, the channel as the label `channel`; and the standard `process_start_time_seconds` and, where the system says, `process_resident_memory_bytes`. Counters count from the process's start, as Prometheus expects.
- **Reason:** The format every collector reads (Prometheus, the OpenTelemetry Collector, Grafana Agent, Datadog, Cloudflare's own); written by the server in a few lines, with no library. Figures about viewers stay off the public address.
- **Source:** Decision 19.
- **Verification:** Test: `/metrics` answers with every name of requirement 8.2, each with its `# TYPE` line; the viewers' listener answers `/metrics` with 404.
- **State:** Met, 28 October 2026: `metrics_are_in_the_prometheus_text_format`, and `probes_and_metrics_are_on_the_administration_listener` (404 on the viewers' listener).

### Requirement 8.2: What is measured
- **Statement:** At least:
  - the server: connections now and their peak, connections made, WebSocket messages and bytes sent, HTTP requests and those answered "not modified", programs started again;
  - each channel: viewers now, their peak and in all, sessions waiting for their viewer to come back and sessions continued, pages opened, inputs and key presses, shared and own pages published, viewers sent to a page, sound records, lines to and from its program and lines dropped, whether it is ready;
  - each channel, as a histogram, the time its task takes over one event and its effects, in buckets from 50 microseconds to 100 milliseconds.
- **Reason:** What decides whether the server keeps up (decision 7) and whether a restart went well.
- **Source:** Decision 19.
- **Verification:** Test, with requirement 8.1.
- **State:** Met, 28 October 2026: `metrics_are_in_the_prometheus_text_format`. A channel's figures are recorded each second, so `/metrics` is at most a second behind.

### Requirement 8.3: Totals accumulated across restarts
- **Statement:** The server's counters and each channel's (requirement 8.2, the counters and the peaks, and each page's opens) must also be kept as totals since the server was first run with its state directory: kept in `figures.json` there, written every minute and at shutdown, each time to a temporary file and renamed into place. `/stats.json` and the `stats` line sent to programs must carry them as `all_time` beside the figures since the start. A file that does not read is logged, kept aside as `figures.json.unreadable`, and the totals start again from zero.
- **Reason:** A page that shows "viewers in all" or the most read pages should not start again from nothing at every deployment; Prometheus's counters reset at a restart by convention, so the totals are kept apart from them.
- **Source:** Decision 19.
- **Verification:** Test: totals written and read again are added to by the next run's figures; an unreadable file is kept aside and the totals start from zero.
- **State:** Met, 28 October 2026: `totals_accumulate_across_restarts`, `figures_are_kept_and_an_unreadable_file_is_kept_aside`.

### Requirement 8.4: Health and readiness
- **Statement:** The administration listener must answer `/healthz` with 200 while the server runs, and `/readyz` with 200 once every channel has pages to serve, kept from before (requirement 5.2) or declared by its program, and with 503 naming the channels that have none until then.
- **Reason:** The standard probes of a supervisor or a load balancer, which then sends viewers only to a server that has something to show them.
- **Source:** Decision 19.
- **Verification:** Test: `/readyz` is 503 before a channel's program declares and 200 after.
- **State:** Met, 28 October 2026: `probes_and_metrics_are_on_the_administration_listener`.

## 9. Programs' own state

What a program holds must survive the program's restart and the server's, as a page rendered on a web server survives that server's: the viewer's identity comes with every request (requirement 5.5), and the page is rendered again from what is kept. These requirements are on the kits and the channels, in their own repositories; this repository records them because the protocol they rely on is its own.

### Requirement 9.1: `start` ends every session a program knew
- **Statement:** A program sent `start` must forget every session it knew, as `docs/applications.md` section 1 says; it is then told of every viewer connected (requirement 4.1).
- **Reason:** A program that outlives the server (requirement 6.2) would otherwise keep sessions the server no longer has.
- **Source:** Decision 17.
- **Verification:** Test of each kit.
- **State:** Partly met, 28 October 2026: `docs/applications.md` section 1 says it; the TypeScript component library's runtime unmounts everything on `start`; the kits' tests are owed in their repositories.

### Requirement 9.2: What lasts is kept, keyed by the session's number
- **Statement:** A channel must keep in storage of its own (SQLite: `node:sqlite` in TypeScript, `sqlite3` in Python, both in the language's standard library) what must outlast its process: what every viewer shares, such as polls, guestbooks and halls of fame, and what one viewer owns, keyed by the session's number (requirement 5.5). Component state holds only what may be lost: the cursor, a scroll position, an animation's frame.
- **Reason:** Every restart of a program, including each save of its source, otherwise loses what its viewers did; the Hackaday channel, which keeps everything in SQLite, already loses nothing.
- **Source:** Decision 17.
- **Verification:** Test of each channel: what it keeps is there after it starts again.
- **State:** Not met, 28 October 2026: the demo site's polls, guestbook and hall of fame are in memory.

### Requirement 9.3: Component state kept when asked
- **Statement:** The TypeScript component library should let a component name a key under which its state, plain JSON with a schema version, is kept for its session and given back when it is mounted again for that session; state of another version is dropped.
- **Reason:** A game or a half-written form survives a restart without each channel writing its own storage; a component's state stays its own, in its class (decision 15).
- **Source:** Decision 17.
- **Verification:** Test of the library: a component's kept state is given back when it is mounted again for its session, and dropped at a new version.
- **State:** Not met, 28 October 2026.

### Requirement 9.4: The pages viewers have open are published first
- **Statement:** A program sent `start` should publish first the pages its viewers have open, as `open` tells it, then the most opened (`stats`), then the rest.
- **Reason:** A program with hundreds of pages (the Hackaday channel publishes 347) otherwise keeps its viewers waiting behind pages nobody is reading.
- **Source:** Decision 17.
- **Verification:** Test of each kit: after `start`, the opened pages are published first.
- **State:** Not met, 28 October 2026.

## 10. Audio

### Requirement 10.1: Opus in the profile, checked
- **Statement:** A program's Opus sound must be refused whole unless every packet is one the receivers decode, as the engine's own decoder judges it from a reset state: mono, one 20 ms frame, at most 160 bytes, bare or padded (the engine's decision E.123 and its REQ-AUD-022); unless every packet of the sound has one length, at most 40 bytes, so that the sound keeps the profile's 16 kbit/s at constant rate; and unless the packets fit the sound's stated length (E.124). The server must not encode Opus, and must not write the profile's rules itself.
- **Reason:** The desktop browsers play Opus alone, in one profile; a packet outside it would be refused by every receiver, one at a time, and a program's mistake is better found once, at the server, with its reason in the log.
- **Source:** Decision 21.
- **Verification:** Test: packets over 40 bytes, stereo, of two frames, of a 10 ms frame, empty, or not whole packets are refused with their reasons and nothing is sent; a sound whose stated length its packets do not hold is refused.
- **State:** Met, 28 October 2026: `opus_outside_the_profile_is_refused`.

### Requirement 10.2: Opus carried unchanged as codec 2
- **Statement:** A program's Opus sound must reach its listeners as the engine carries Opus, codec 2 (E.124): a stored sound (`opus`) as its OBJECT and DATA records, a live sound as its OBJECT (`opus_live`) and then its packets' records as they come (`opus_packets`), the packets unchanged.
- **Reason:** The receivers decode what the encoder wrote; nothing between should change it.
- **Source:** Decision 21.
- **Verification:** Test: a constant-rate encoder's sound (one second of a tone from `ffmpeg` and libopus, 51 packets of 40 bytes) reaches the viewer as exactly the records `Sound::opus` makes; a live sound's object and packets as `Sound::live` and `Sound::packet_records` make them.
- **State:** Met, 28 October 2026: `an_encoders_opus_sound_reaches_the_viewer_as_codec_2`, `a_live_opus_sound_goes_as_its_packets_come`. The kits do not yet encode Opus (TECH-DEBT.md).

### Requirement 10.3: PCM as IMA ADPCM, for microcontrollers
- **Statement:** A program's PCM samples (`object`, `live`, `blocks`) must still be encoded as IMA ADPCM and sent as before, for microcontroller receivers such as tellyboy, which decode no Opus (the engine's E.131).
- **Reason:** The owner's instruction to keep the path for microcontroller clients.
- **Source:** Decision 21.
- **Verification:** Test: `sounds_are_encoded_and_commands_numbered`.
- **State:** Met, 28 October 2026.

### Requirement 10.4: The radio's output is Opus
- **Statement:** The streaming-audio transcoder (`docs/streaming-audio.md`) must resample to 48 kHz and encode Opus in the profile of requirement 10.1, carried as live Opus sounds.
- **Reason:** Its listeners are the desktop browsers, which play Opus alone; and 16 kbit/s a listener is a quarter of the IMA ADPCM output's 64.
- **Source:** Decision 21.
- **Verification:** Test, when the transcoder is built: a station's output passes requirement 10.1's check.
- **State:** Not met, 28 October 2026: the transcoder is not built, and its encoder is the owner's to choose (`docs/streaming-audio.md` section 10 item 8).
