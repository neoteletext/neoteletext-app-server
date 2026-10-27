# teletext-server: applications in any language

How a channel's application, a program in any language, talks to the server. It is `SERVER.md` section 7 as built, protocol version 1. The server is Rust; a channel's application is not (the owner, 25 October 2026: "the neoteletext server itself must be RUST. showcase channel must be typescript, calling the rust server. the hackaday channel must be python calling the rust server").

## 1. Process

- The server starts each channel's program, named in the channels file (section 6), in its own folder, with `NEOTELETEXT_CHANNEL` set to the channel's folder.
- **One program serves every viewer** of its channel, multiplexed by session (`SERVER.md` section 7.1).
- **Lines:** one JSON object a line (RFC 8259), UTF-8, ending in `\n`, at most 16 MiB, on the program's standard input and output. Standard error goes to the server's log.
- **The first line the program writes declares its pages** (section 3.1), within 30 seconds. The server then sends `start`.
- **A program that ends** is started again after 1 s, then 2, 4 … up to 30 s; one that ran 5 s or more after declaring its pages was ended, not failing, and is started again at once. Its channel pages stay as last published meanwhile. When it is running again it declares its pages again and is sent `start`, and publishes afresh. Each viewer's own copies of its pages are dropped, and it is sent `join` for every viewer connected and `open` for each viewer on one of its pages, so it draws them again.
- **The network loop never waits for a program.** At most 4,096 lines wait for it; beyond that the newest are dropped and the server says so in its log (`SERVER.md` section 7.5).
- **Every line the program sends is checked whole** and refused whole when it is not valid, with the reason in the server's log. Nothing of a refused line is acted on.
- **Timers are the program's own:** it has a clock, so the protocol has no `tick`.

## 2. Server to program

| `type` | Fields | When |
|---|---|---|
| `start` | `protocol` (1), `epoch_ms` | The program is to publish its channel pages |
| `join` | `session` | A viewer connected |
| `open` | `session`, `page` | The viewer opened one of the program's pages |
| `leave` | `session`, `page` | The viewer left it |
| `input` | `session`, `page`, `event` | Input on the page open, from INPUT.md |
| `close` | `session` | The viewer went: forget the session |
| `stats` | `server`, `channel` | Every 10 seconds: the figures of section 7, for a page to show |

`session` is a whole number, never reused. `page` is three hexadecimal digits, `"100"` to `"8FF"`.

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

At most 800 pages, each once; a title at most 32 characters, without control characters. The channel's `contents.txt` lists them. A program publishes only the pages it declared.

A page may start a section: `{"page":"200","title":"Retrocomputing","section":"Retrocomputing"}`. `contents.txt` then heads it and the pages after it with the section's name, at most 32 characters, which a receiver shows as the name of the page's magazine.

### 3.2 `publish`

```json
{"type":"publish","page":"100","owner":"channel","tti":"PN,10000\nOL,1,..."}
```

- **`owner`** is `"channel"`, one shared copy sent to every viewer, encoded once; or a session number, that viewer's own copy of the page from now on (`SERVER.md` section 7.3).
- **`tti`** is the page in TTI, the engine's page format (`neoteletext-engine` REQ-INP-013), read by the engine's TTI reader:
  - `PN,mppss`: starts each sub-page. The page number comes from `page`; the sub-page from `ss`. At most 64 sub-pages.
  - `OL,row,text`: a row, 0 to 24. Control codes (0x00 to 0x1F) are written as ESC (0x1B) followed by the code plus 0x40; every other character is 0x20 to 0x7F of the page's character set.
  - `FL,red,green,yellow,cyan,link5,index`: Fastext links, hexadecimal; `8FF` for none. It also shows row 24.
  - `CU,row,column,width,flags`: the input cursor and its echo (INPUT.md section 7.1).
  - `SC`, `PS`, `LK`, `LT` and the rest as the engine reads them.
- The server sends the page with the erase bit set and in serial magazine mode, so each copy replaces the last.

### 3.3 `goto`

```json
{"type":"goto","session":7,"page":"300"}
```

Sends the viewer to a page of the channel (INPUT.md section 7.1, REQ-INT-113).

### 3.4 `sound`

The engine's audio profile (`SERVER.md` section 9; the engine's `audio.md`). `to` is `{"session":7}` for one viewer or `{"page":"501"}` for everyone with that page open. One of:

| Member | Fields | Becomes |
|---|---|---|
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

The protocol is the interface; a kit is a convenience (`SERVER.md` section 3.4).

- **TypeScript** (`kits/typescript`), the reference kit: a 25 by 40 screen buffer per page, drawing helpers (text, colours, mosaics, double height), the TTI writer, sounds, and the line loop.
- **Python** (`kits/python`): the same, thinner.

## 6. The channels file

```json
{"channels":[
  {"folder":"hackaday","name":"Hackaday","command":["uv","run","--frozen","python","-m","hackaday"],"directory":"channels/hackaday"},
  {"folder":"showcase","name":"NeoTeletext showcase","command":["node","src/main.ts"],"directory":"channels/showcase"}
]}
```

`teletext-server --channels channels.json`. A folder is letters, digits, `-` and `_`. A relative `directory` is taken from the file's own folder.

## 7. Figures

The server counts, and never records who a viewer is: no address, no text typed. Every 10 seconds each program is sent `stats`; `/stats.json` at the server's root holds the same for every channel; the log has a one-line summary each minute.

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
  - `gotos`, `sound_records`, `lines_to_application` and `lines_from_application`.
