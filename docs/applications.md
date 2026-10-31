# teletext-server: applications in any language

How a channel's application, a program in any language, talks to the server. It is `SERVER.md` section 7 as built, protocol version 3 (DEC-1482, DEC-1683). The server is Rust; a channel's application is not (the owner, 25 October 2026: "the neoteletext server itself must be RUST. showcase channel must be typescript, calling the rust server. the hackaday channel must be python calling the rust server").

## 1. Process

- **Two transports carry the same lines** (requirements 6.1, 6.2 and 6.4):
  - **standard input and output:** the server starts each channel's program named by a `command` in the channels file (section 6), in its own folder, with `NEOTELETEXT_CHANNEL` set to the channel's folder;
  - **a Unix domain socket:** a channel named with a `socket` is served to the program that connects to it, which runs as a service of its own and so outlives a restart of the server. The socket and its folder are their owner's alone. One program at a time; when it goes, or the server restarts, it connects again and is sent `start` again.
- **One program serves every viewer** of its channel, multiplexed by session (`SERVER.md` section 7.1).
- **Lines:** one JSON object a line (RFC 8259), UTF-8, ending in `\n`, at most 16 MiB. A program's standard error becomes the server's log, a line an event with its channel, cut at 64 KiB (requirement 7.5).
- **When the program starts** (or connects to its socket) the server sends it `start`, then `connected` for every connection open. While no program runs, the channel has nothing to show (DEC-1482).
- **A program that ends** is started again after 1 s, then 2, 4 … up to 30 s; one that ran 5 s or more was ended, not failing, and is started again at once.
- **The network loop never waits for a program.** At most 4,096 lines wait for it; beyond that the newest are dropped and the server says so in its log (`SERVER.md` section 7.5). Lines for a program that is not running are dropped, not kept for the next: it is told of every viewer when it starts.
- **Every line the program sends is checked whole** and refused whole when it is not valid: the program is sent `refused`, and the reason goes to the server's log. Nothing of a refused line is acted on.
- **Timers are the program's own:** it has a clock, so the protocol has no `tick`.

## 2. Server to program

Protocol version 3 (DEC-1482, DEC-1683). Every message in both directions is defined once by the server's Rust types (`crates/teletext-runtime/src/protocol.rs`) and published as a JSON Schema, draft 2020-12, generated from them: `protocol/schema-3.json`, whose `$defs/ServerMessage` is what the server sends and `$defs/ProgramMessage` what a program sends. The schema is the exact contract, with every bound; this section and the next say what each message is for. Every message is an object tagged by `type`; an unknown `type` or field is refused, never skipped.

Common shapes:
- `connection`: a whole number from 1, naming one open connection: one browser tab, with one selection (DEC-1527); `session`: a whole number, the viewer's session, which outlives connections and is shared by a viewer's tabs (requirement 5.5). Both below 2^53.
- `page`: three hexadecimal digits in capitals, magazine 1 to 8 first, `"100"` to `"8FF"`; `sub`: four, `"0000"` to `"3F7F"`, where `"3F7F"` means the page and none of its sub-pages in particular (EN 300 706 clause 9.6.1).
- A connection's **selection** is a page with `sub` `"3F7F"`, answered with the page's first sub-page in ascending sub-code order (0000 when the page has it), which then becomes the selection; or one sub-page. A sub-page never changes by itself: no rotation (DEC-1526).
- Text, in every field, is UTF-8 in Unicode normalisation form C, with no control character (C0, DEL, C1) and no bidirectional formatting character, within the field's length.
- `connections`: `"all"`, every open connection, or a list of 1 to 4,096 connections; a connection named that has closed is passed over.

| `type` | Fields | When |
|---|---|---|
| `start` | `protocol` (2), `epoch_ms`, `standard`, `budgets` | The program started (or connected); `connected` follows for each connection open |
| `connected` | `connection`, `session`, `cookies` (`name`, `value` pairs), `selection` (`page` and `sub`, or `null` for the channel's initial page) | A viewer's connection opened; answer with `reply` (REQ-1471) |
| `selected` | `connection`, `page`, `sub` | The connection selected a page (`sub` 3F7F: send its first sub-page and its `subpages` list) or one sub-page (send that sub-page) (REQ-1472, DEC-1526) |
| `input` | `connection`, `page`, `sub`, `lost`, `event` | Input on the page on screen (INPUT.md). `page` and `sub` are the page and sub-code the viewer was looking at, the connection's selection: input stamped with anything else is dropped before it reaches you (REQ-1557), except a release, which names the page that held it. `lost` is `true` on a release the page lost, the key or button not having come up: the viewer moved away, the keys left the page, or the connection went (REQ-207). A game can tell "the player let go" from "the player left". A lost mouse release is no click |
| `left` | `connection`, `page` | The connection's selection left a page, or the connection closed |
| `closed` | `session` | The session ended: 5 minutes after its connection closed with no new connection to continue it. Forget it |
| `stats` | `server`, `channel` | Every 10 seconds: the figures of section 7 |
| `refused` | `message` (its `type`, or `null`), `path` of the first fault (such as `page.rows[3]` or `magazines[2].pages[14].title`; for an unknown field, the field itself; empty for the message as a whole), `rule` (a closed list: `syntax`, `unknown_field`, `missing_field`, `wrong_type`, `not_listed`, `out_of_range`, `too_long`, `too_short`, `not_ascending`, `duplicate`, `reserved`, `wrong_magazine`, `forbidden_character`, `not_nfc`, `not_base64`, `wrong_length`, `not_in_index`, `unknown_connection`, `not_awaited`, `outside_profile`, `invalid_packet`, and the rest the schema lists), `detail` | A line the program sent was refused whole; nothing of it was done (REQ-1475) |

`standard` is the standard's maxima, from the engine's `neoteletext_core::limits` (decision 10; requirement 3.3), so a program holds none of its own: `magazines` 8, `pages` 2,022, `pages_per_magazine` 253, `pages_in_magazine_1` 251, `keyable_pages` 800, `subpages` 8,190, `decimal_subpages` 3,199. `budgets` is the server's own: `line_bytes` (16 MiB), `title_characters` (120), `author_log_bytes` (200), `log_characters` (1,000), `named_connections` (4,096), `reply_limit_ms` (how long the server waits for an answer to `connected` or `selected`).

`event.kind` is one of `key` (`action` `press`, `repeat` or `release`; `key`, a W3C `KeyboardEvent.key` name from the engine's table or one character; `text` and `base`, or `null`), `mouse` (`action` `press`, `release` or `move`; `button` 0 left, 1 middle, 2 right, `null` on a move; `buttons` `{left, middle, right}` on a move, else `null`; `column` 0 to 39, `row` 0 to 24), `wheel` (`dx`, `dy` in 1/120ths of a line, down positive; `column`, `row`), `hover` (`action` `enter` or `leave`; the control's first `column` and `row`), `text` and `paste` (`text`), `focus` and `blur`. Every key, click, move and hover goes; a program ignores what it does not use, and selects pages for the viewer with `redirect` (INPUT.md section 9.4).

## 3. Program to server

| `type` | Fields | What |
|---|---|---|
| `reply` | `connection`, `magazines`, `selected` (one sub-page, or `null`) | The answer to `connected`, exactly once: the channel's index for this viewer, and the sub-page its selection shows, or the initial page's first. The page must be listed in `magazines` |
| `page` | `connections`, `page`: one sub-page | A sub-page. It reaches only those connections whose selection shows it: those that selected it, and those that selected its page with 3F7F and await its first sub-page, whose selection it becomes (DEC-1526) |
| `subpages` | `connections`, `page`, `version` (0 to 65,535), `subpages` (`sub`, `title` of at most 40 characters or `null`), each sub-code once, in ascending order | A page's sub-page list, with the answer to a page selection and whenever it changes; sent to the connections named whose selection shows the page and that do not hold that version |
| `index` | `connections`, `magazines` | The index changed: a page added, removed, retitled, its section or sub-page count changed |
| `redirect` | `connection`, and `page` and `sub`, or `address` (at most 1,015 bytes, an `http` or `https` address, `MAX_ADDRESS` of `INPUT.md` section 7.11) | Sends the viewer elsewhere, with no action from them (DEC-1683). With a page, a page redirect: a selection, so the program is then sent `selected` for it. With an address, to another channel, after the receiver's one-second notice (REQ-1686): no selection changes, and the program is sent `left` when the viewer goes. Naming both or neither is refused. It is neither an HTTP redirect (REQ-701) nor anything a viewer can refuse, but a viewer's own choice overrides it |
| `sound` | `kind`, `connections`, and the kind's fields (below) | A sound, its data, or a command |
| `cookie` | `connection`, `name`, `value`, `path`, `max_age` (seconds, 0 removing it, `null` for the browser's session), `same_site` (`Strict`, `Lax`, `None`), `secure`, `http_only` | Sets one of the program's cookies; names starting `_` are the server's |
| `author_log` | `connections`, `level` (`E`, `W` or `I`), `text` (1 to 200 bytes), `page` or `null` | A line for the developer console of the viewer's browser |
| `log` | `level` (`error`, `warn`, `info`, `debug`), `event` (lower case, digits and `_`), `text` | A line for the server's log |
| `packets` | `connections`, `packets`: base64 of whole 42-byte packets | Raw packets (`SERVER.md` section 7.4) |

**`magazines`** (REQ-1477): at most 8, in ascending `magazine` (1 to 8), each with a `name` (at most 120 characters) and `pages`: 1 to 253 (251 in magazine 1), ascending, each `page` in that magazine and never reserved (mFD, mFE, mFF, 1BE, 1F0), with a `title` and a `section` (at most 120 characters, `section` `null` for none) and `subpages`, its count, 1 to 8,190. The index is page-level: sub-pages are never listed in it.

**A sub-page** (`page`, REQ-1478), every field required:
- `page`, and `sub`, 0000 to 3F7E;
- `rows`: 25 entries, rows 0 to 24, each exact base64 (RFC 4648 section 4, padded) of 40 seven-bit character codes, or `null` for a blank row. Row 0's columns 8 to 39 are the program's header text; columns 0 to 7 are the receiver's;
- `links`: the six X/27/0 links, red, green, yellow, cyan, the fifth and the index, each `{page, sub}` or `null`; `display_row_24`, X/27/0's flag;
- `link_extension` (the engine's Appendix E, REQ-1494): `keys` (each `key` `red`, `green`, `yellow`, `cyan` or `index` at most once, a `target` and a `label` of at most 9 characters, 8 for another service, `null` for the Index key), `numbers` (at most 8: `number`, a page number on the page, and a `target`, never `web:`) and `texts` (at most 8: `row` 1 to 23, `column` 0 to 39, `length` keeping the run within 40 columns, and a `target`). A target is `mpp`, `mpp:ssss`, `name:mpp`, `name:mpp:ssss`, an `http` or `https` address, or `web:` and one: printable ASCII without spaces or commas, at most 256 bytes;
- `control`: C4 to C11 as `erase`, `newsflash`, `subtitle`, `suppress_header`, `update`, `interrupted_sequence`, `inhibit_display`, `magazine_serial`. The server sets C4 and C11 itself;
- `character_set`: `region` 0 to 15, carried in X/28/0, or `null`; `national_option` 0 to 7, C12 to C14;
- `enhancements`: at most 16 packets X/26, each `designation` 0 to 15 once, and 13 `triplets` of `address` (6 bits), `mode` (5 bits) and `data` (7 bits);
- `packets`: at most 16 other packed lines by `packet`: `x27_nibbles` (designation 1 to 3, 39 values of 4 bits), `x27` (designation 4 to 15), `x28` and `m29` (designation 0 to 15), each of 13 triplets;
- `cursor`: `{row, column, block}` or `null` (INPUT.md section 7.1).

The server encodes the sub-page with the engine's encoder and sends it alone (REQ-1501). A connection is sent it whole, with the erase bit, the first time after its selection changes or when it is another sub-page than the one last sent; after that, the same sub-code goes as only its header, the rows that changed and the enhancement group when any of it changed, and nothing when nothing changed (DEC-1499). Every frame of a sub-page with Link Extension links is followed by its LINKS record (DEC-1517).

**`sound`** (`SERVER.md` section 9; the engine's audio profile), by `kind`, each with `connections`:

| `kind` | Fields |
|---|---|
| `opus` | `handle` 0–255, `generation`, `packet_bytes` 1–40, `samples`, `packets`: base64 of the packets back to back, at most two minutes. Every packet is checked with the engine's decoder against the profile (decision 21) |
| `opus_live` | `handle`, `generation`, `packet_bytes` |
| `opus_packets` | `handle`, `generation`, `packet_bytes`, `first`, `packets` |
| `object` | `handle`, `generation`, `rate` (8000, 11025, 16000, 22050, 24000, 32000, 44100 or 48000), `pcm`: base64 of 16-bit little-endian mono samples, at most two minutes; encoded here as IMA ADPCM |
| `live` | `handle`, `generation`, `rate` |
| `blocks` | `handle`, `generation`, `rate`, `first`, `pcm` |
| `play` | `channel` 0–3, `handle`, `generation`, `gain` 0–100, `fade_ms`, `loop`, `after`, `duck` 0–6, `page` or `null` |
| `stop` | `channel`, `fade_ms` |
| `gain` | `channel`, `gain`, `fade_ms` |

**What reaches the viewer** (REQ-1480), every packet checked with the engine's one stream check (REQ-1462): sub-pages as page frames; the index of `reply` and `index` as index records; `subpages` as SUBPAGES records; `author_log` as LOG records; `cookie` as a SET_COOKIE record; the Link Extension as LINKS records; sounds as the audio profile's records; `packets` as they are. **Not yet:** DENIED "unavailable" when an answer is late (TODO-1522); the operator's log says so meanwhile.

## 4. Pages and sessions

- The server keeps no page (REQ-1474): only what each open connection is, and the last version of the sub-page its selection shows, to send its changes (DEC-1499). A program sends a sub-page again to the connections that should see it whenever it changes; a connection that opens later is sent nothing of it until the program answers its `connected`.
- A channel can differ for every viewer: the program answers each connection and selection as it chooses.
- A program keeps each session's state itself and forgets it on `closed` (`SERVER.md` section 8.4).

## 5. Kits

The protocol is the interface; a kit is a convenience (`SERVER.md` section 3.4). The channels file, the old Hackaday channel in Python and the Python kit are in their own repository, `~/devel/neoteletext-server-channels` (decision 12); the TypeScript kits are libraries in repositories of their own, released as built packages (decision 13); and each TypeScript channel is a repository of its own beside it: `neoteletext-server-channel-hackaday` (decision 14), `-demosite` and `-uicomponentsdemo` (decision 16).

- **TypeScript** (`neottx-server-ts-protocol`, in `~/devel/neottx-server-ts-protocol`), the reference kit: a 25 by 40 screen buffer per page, drawing helpers (text, colours, mosaics, double height), sounds, and the line loop; it is being rewritten to protocol version 2 (TODO-1484). The component library over it, `neottx-server-ts-ui-components`, is in `~/devel/neottx-server-ts-ui-components`.
- **Python** (`kits/python`): the same, thinner.

## 6. The channels file

```json
{"channels":[
  {"folder":"hackaday","name":"Hackaday","command":["node","--disable-warning=ExperimentalWarning","src/main.ts"],"directory":"../neoteletext-server-channel-hackaday"},
  {"folder":"oldhackaday","name":"Hackaday (old)","command":["uv","run","--frozen","--no-dev","python","-m","hackaday"],"directory":"hackaday"}
]}
```

`teletext-server --channels ~/devel/neoteletext-server-channels/channels.json`. A folder is letters, digits, `-` and `_`. A relative `directory` is taken from the file's own folder, so `../<repository>` names a channel's repository beside the channels'.

A channel whose program runs as a service of its own names a socket in place of a command and a directory, on Linux and macOS:

```json
{"folder":"hackaday","name":"Hackaday","socket":"sockets/hackaday.sock"}
```

## 7. Figures

The server counts, and never records who a viewer is: no address, no text typed. Every 10 seconds each program is sent `stats`; `/stats.json` on the administration listener holds the same for every channel (DEC-1467); the log has an event with the figures each minute. Each of `server` and `channel` also carries `all_time`: its counters and peaks since the server was first run with its state directory (requirement 8.3), and, for a channel, `pages_opened`, each page's opens. The same figures, since the process started, are at `/metrics` on the administration listener, in the Prometheus text format (requirement 8.1).

- **`server`:**
  - `uptime_s`;
  - `connections`, `connections_peak` and `connections_total`;
  - `messages_sent` and `bytes_sent` (WebSocket);
  - `requests` (HTTP: `channel.json` and the WebSocket upgrade);
  - `restarts` (of programs);
  - `memory_kb` (resident, where the system says; -1 elsewhere);
  - `channels`, and `viewers` by channel folder.
- **`channel`:**
  - `viewers`, `viewers_peak` and `viewers_total`;
  - `opens`, and `most_opened`: ten page numbers with how often each was opened;
  - `inputs` and `keys`;
  - `pages_sent` (sub-pages sent, each copy counted) and `refused` (lines from the program refused);
  - `redirects`, `sound_records`, `lines_to_application`, `lines_from_application` and `lines_dropped`;
  - `sessions_waiting`, sessions whose viewer may come back, and `sessions_resumed`.
