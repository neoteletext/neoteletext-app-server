# teletext-server: applications in any language

How a channel's application, a program in any language, talks to the server. It is `SERVER.md` section 7 as built, protocol version 1. The server is Rust; a channel's application is not (the owner, 25 October 2026: "the neoteletext server itself must be RUST. showcase channel must be typescript, calling the rust server. the hackaday channel must be python calling the rust server").

## 1. Process

- **Two transports carry the same lines** (requirements 6.1, 6.2 and 6.4):
  - **standard input and output:** the server starts each channel's program named by a `command` in the channels file (section 6), in its own folder, with `NEOTELETEXT_CHANNEL` set to the channel's folder;
  - **a Unix domain socket:** a channel named with a `socket` is served to the program that connects to it, which runs as a service of its own and so outlives a restart of the server. The socket and its folder are their owner's alone. One program at a time; when it goes, or the server restarts, it connects again and declares its pages again.
- **One program serves every viewer** of its channel, multiplexed by session (`SERVER.md` section 7.1).
- **Lines:** one JSON object a line (RFC 8259), UTF-8, ending in `\n`, at most 16 MiB. A program's standard error becomes the server's log, a line an event with its channel, cut at 64 KiB (requirement 7.5).
- **The first line the program writes declares its pages** (section 3.1), within 30 seconds, each time it starts. The server then sends `start`. Until then the channel serves the pages the server kept from before (requirement 5.2); a page the program no longer declares is withdrawn.
- **`start` ends every session the program knew** (requirement 9.1): it is then told of every session there is.
- **A program that ends** is started again after 1 s, then 2, 4 … up to 30 s; one that ran 5 s or more after declaring its pages was ended, not failing, and is started again at once. Its channel pages stay as last published meanwhile. When it is running again it declares its pages again and is sent `start`, and publishes afresh. Each viewer's own copies of its pages are dropped, and it is sent `join` for every viewer connected and `open` for each viewer on one of its pages, so it draws them again.
- **The network loop never waits for a program.** At most 4,096 lines wait for it; beyond that the newest are dropped and the server says so in its log (`SERVER.md` section 7.5). Lines for a program that is not running are dropped, not kept for the next: it is told of every viewer when it starts.
- **Every line the program sends is checked whole** and refused whole when it is not valid, with the reason in the server's log. Nothing of a refused line is acted on.
- **Timers are the program's own:** it has a clock, so the protocol has no `tick`.

## 2. Server to program

| `type` | Fields | When |
|---|---|---|
| `start` | `protocol` (1), `epoch_ms`; to come (decision 10): `standard`, the standard's maxima, and `budgets`, the server's | The program is to publish its channel pages |
| `join` | `session` | A session began: a viewer connected without a session to continue |
| `open` | `session`, `page` | The viewer opened one of the program's pages |
| `leave` | `session`, `page` | The viewer left it, or their connection closed |
| `input` | `session`, `page`, `event` | Input on the page open, from INPUT.md |
| `close` | `session` | The session ended: 5 minutes after its connection closed with no new connection to continue it. Forget it |
| `stats` | `server`, `channel` | Every 10 seconds: the figures of section 7, for a page to show |

`session` is a whole number, never reused, across restarts too, and below 2^53, so a TypeScript program reads it exactly (requirement 5.4). A session outlives its connection (requirement 5.5): a viewer who comes back within 5 minutes, or later while their browser keeps the session's cookie (30 days, renewed at each connection), continues it, and the program is told `open` when they open a page, not `join` again. A program may keep what it knows of a viewer under the session's number (requirement 9.2). `page` is three hexadecimal digits, `"100"` to `"8FF"`.

`event.kind` is one of:

| `kind` | Fields |
|---|---|
| `text` | `text`: whole characters that came from no key: an input method's committed text, or a phone's (INPUT.md section 5.3) |
| `key` | `action` (`press`, `repeat`, `release`), `key`: a W3C `KeyboardEvent.key` name such as `ArrowUp`, `PageDown`, `F1`, `ColorF0Red`, or the character of a character key; `text` when the key produced text, `base` when it differs from `key` (INPUT.md section 5.5). Every press, repeat and release is sent, the keys that only modify or lock and the dead key included: a program that types takes `text` from presses and repeats, and ignores releases for typing |
| `mouse` | `action` (`press`, `release`, `move`), `column`, `row`; on a press or release `pressed` and `button` (0 left, 1 middle, 2 right); on a move `buttons`, those held, a bit each (1 left, 2 middle, 4 right). Every press and release over the page, and every move over it, a button held or not (INPUT.md section 6.3) |
| `wheel` | `dx`, `dy` in 1/120ths of a line (down positive), `column`, `row`, while the pointer is over the page |
| `hover` | `action` (`enter`, `leave`), `entered` (true on `enter`), `column`, `row`: the pointer came onto one of the page's controls or went off it, and the cell is the control's first, its top-left, as the page declares it (INPUT.md sections 5.4 and 7.3). Moving from one control to the next is a `leave`, then an `enter`. The browser draws the control's look under the pointer itself; a program uses a hover for anything else, such as saying on the page what the control does. One whose page has changed since may find no control there now, and ignores it |
| `paste` | `text` |
| `focus`, `blur` | The keys came to the page or left it |

What reaches the program: while the browser sends keys to the channel, every event goes, every key, digits and colour keys included, every click and move over the page, and every hover of a control (INPUT.md sections 6.3 and 9.1). There are no input modes to ask for (withdrawn on 27 October 2026): a program ignores what it does not use. A program selects pages for the viewer itself: three digits or the coloured keys on a page that has no other use for them answer with `goto` (INPUT.md section 9.4).

## 3. Program to server

### 3.1 `pages`, the first line

```json
{"type":"pages","pages":[{"page":"100","title":"Front page"},{"page":"101","title":"News"}]}
```

At most 800 pages, each once; a title at most 120 characters (the engine's `contents::MAX_TITLE`), without control characters. The channel's `contents.txt` lists them. A program publishes only the pages it declared.

A page may start a section: `{"page":"200","title":"Retrocomputing","section":"Retrocomputing"}`. `contents.txt` then heads it and the pages after it with the section's name, at most 120 characters, which a receiver shows as the name of the page's magazine.

### 3.2 `publish`

```json
{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,..."}
```

- **`owner`** is `"channel"`, one shared copy sent to every viewer, encoded once; or a session number, that viewer's own copy of the page from now on (`SERVER.md` section 7.3).
- **`tti`** is the page in TTI, the engine's page format (`neoteletext-engine` REQ-INP-013), read by the engine's TTI reader:
  - `PN,mppss`: starts each sub-page. The page number comes from `page`; the sub-page from `ss`. At most 64 sub-pages. *To change (decision 10, not built):* up to 8,190, each named by `SC` with its four-digit sub-code, a publication putting only the sub-pages it carries and `remove` ending others; the bound comes from `start`.
  - `OL,row,text`: a row, 0 to 24. Control codes (0x00 to 0x1F) are written as ESC (0x1B) followed by the code plus 0x40; every other character is 0x20 to 0x7F of the page's character set.
  - `FL,red,green,yellow,cyan,link5,index`: Fastext links, hexadecimal; `8FF` for none. It also shows row 24.
  - `CU,row,column,width,flags`: the input cursor and its echo (INPUT.md section 7.1).
  - `SC`, `PS`, `LK`, `LT` and the rest as the engine reads them.
- The server sends the page with the erase bit set and in serial magazine mode, so each copy replaces the last.
- Row 0's columns 8 to 39 are the program's (EN 300 706 clause 9.3.1.4; decision 22, `docs/requirements.md` section 11): what an `OL,0` line puts there reaches the viewer as written, and the server writes nothing there. A program that wants a name, a date or a clock in the header writes it; without an `OL,0` line the row is blank. Columns 0 to 7 are the receiver's, where it shows the page's number; what a program puts there is never shown. The server sends no time-filling header: a clock on a page changes when the program publishes the page again.

### 3.3 `goto`

```json
{"type":"goto","session":7,"page":"300"}
```

Sends the viewer to a page of the channel (INPUT.md section 7.1, REQ-INT-113).

### 3.4 `sound`

The engine's audio profile (`SERVER.md` section 9; the engine's `audio.md`). `to` is `{"session":7}` for one viewer or `{"page":"501"}` for everyone with that page open.

**Sound for the desktop is Opus** (decision 21). The browser on macOS, Windows and Linux decodes Opus alone (the engine's decision E.131, the browser's decision 121), in one profile (the engine's E.123): mono, 48 kHz, one 20 ms frame a packet, at most 160 bytes, bare or padded, and at constant rate, every packet of a sound one length, at most 40 bytes, the profile's 16 kbit/s. The program encodes; the server checks every packet with the engine's own decoder and refuses the whole line if one is outside the profile, then carries the packets unchanged as codec 2 (E.124). A constant-rate encoder writes them, such as `ffmpeg -c:a libopus -b:a 16k -vbr off -frame_duration 20`, its packets read out of the Ogg file after its two header packets.

**PCM becomes IMA ADPCM, for microcontroller receivers** such as tellyboy, which decode no Opus (E.131): `object`, `live` and `blocks` are kept for them, and are silent in the desktop browser.

One of:

| Member | Fields | Becomes |
|---|---|---|
| `opus` | `handle` 0–255, `generation`, `packet_bytes` 1–40, `samples` (the sound's length: its last packet's frame may run past it), `packets`: base64 of the packets back to back, at most two minutes | A stored Opus sound, codec 2 |
| `opus_live` | `handle`, `generation`, `packet_bytes` 1–40 | A live Opus sound's object |
| `opus_packets` | `handle`, `generation`, `packet_bytes`, `first` (the first packet's block number), `packets` | A live Opus sound's next packets, one a block, in order |
| `object` | `handle` 0–255, `generation`, `rate` (8000, 11025, 16000, 22050, 24000, 32000, 44100 or 48000), `pcm`: base64 of 16-bit little-endian mono samples, at most two minutes | A stored sound, encoded here as IMA ADPCM in 256-byte blocks |
| `live` | `handle`, `generation`, `rate` | A live sound's object (`audio.md` section 8.3) |
| `blocks` | `handle`, `generation`, `rate`, `first` (block number), `pcm` | Live blocks of 1,017 samples each, encoded here in 512-byte blocks |
| `play` | `channel` 0–3, `handle`, `generation`, `gain` 0–100 (100 if absent), `fade_ms`, `loop`, `after`, `duck` 0–6, `page` (the page it belongs to, or none for the whole service) | A command. The server numbers each channel's commands, so the latest wins |
| `stop` | `channel`, `fade_ms` | A command |
| `gain` | `channel`, `gain`, `fade_ms` | A command |

### 3.5 `title`

```json
{"type":"title","page":"201","title":"A Robot Arm From Bits Of Wo..."}
```

Gives one of the program's pages a new title, as `pages` does, for what the page holds now: `contents.txt` lists it from then on.

### 3.6 `release`

```json
{"type":"release","session":7,"page":"100"}
```

Drops the session's own copy of a page: from now on it sees the channel's copy, as every viewer does, and is sent it at once. For a page shown to one viewer for a while, such as a greeting.

### 3.7 `log`

```json
{"type":"log","text":"fetched 7 posts"}
```

A line for the server's log.

## 4. Pages and sessions

- A session's own copy of a page replaces the channel's for that viewer from its first `publish`, until the session ends or the program releases it (section 3.6).
- A program keeps each session's state itself and forgets it on `close` (`SERVER.md` section 8.4).
- The server keeps each page's latest copy. A program publishes again only when the page changes. A shared page's change costs the same whatever the number of viewers (decision 7).

## 5. Kits

The protocol is the interface; a kit is a convenience (`SERVER.md` section 3.4). The channels file, the old Hackaday channel in Python and the Python kit are in their own repository, `~/devel/neoteletext-server-channels` (decision 12); the TypeScript kits are libraries in repositories of their own, released as built packages (decision 13); and each TypeScript channel is a repository of its own beside it: `neoteletext-server-channel-hackaday` (decision 14), `-demosite` and `-uicomponentsdemo` (decision 16).

- **TypeScript** (`neottx-server-ts-protocol`, in `~/devel/neottx-server-ts-protocol`), the reference kit: a 25 by 40 screen buffer per page, drawing helpers (text, colours, mosaics, double height), the TTI writer, sounds, and the line loop. The component library over it, `neottx-server-ts-ui-components`, is in `~/devel/neottx-server-ts-ui-components`.
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

The server counts, and never records who a viewer is: no address, no text typed. Every 10 seconds each program is sent `stats`; `/stats.json` at the server's root holds the same for every channel; the log has an event with the figures each minute. Each of `server` and `channel` also carries `all_time`: its counters and peaks since the server was first run with its state directory (requirement 8.3), and, for a channel, `pages_opened`, each page's opens. The same figures, since the process started, are at `/metrics` on the administration listener, in the Prometheus text format (requirement 8.1).

- **`server`:**
  - `uptime_s`;
  - `connections`, `connections_peak` and `connections_total`;
  - `messages_sent` and `bytes_sent` (WebSocket);
  - `requests` and `not_modified` (HTTP; answered from the client's `ETag`);
  - `restarts` (of programs);
  - `memory_kb` (resident, where the system says; -1 elsewhere);
  - `channels`, and `viewers` by channel folder.
- **`channel`:**
  - `viewers`, `viewers_peak` and `viewers_total`;
  - `opens`, and `most_opened`: ten page numbers with how often each was opened;
  - `inputs` and `keys`;
  - `shared_published` and `own_published`;
  - `gotos`, `sound_records`, `lines_to_application`, `lines_from_application` and `lines_dropped`;
  - `sessions_waiting`, sessions whose viewer may come back, and `sessions_resumed`.
