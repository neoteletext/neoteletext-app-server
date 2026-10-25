# Porting an area of the showcase to TypeScript

The owner decided the server is Rust and the showcase is TypeScript, a separate program speaking `docs/applications.md`. `crates/teletext-showcase` (Rust) was ported area by area into `channels/showcase` (this folder), page for page and behaviour for behaviour, with its tests ported too. Every area is ported: `common.rs`, `index.rs`, `sound.rs`, `services/` (101 to 110), `interactive/` (200 to 207), `games/` (300 to 305), `adventure/` (400 to 402), `audio/` (500 to 504) and `gallery/` (600 to 699, and the collections' own channels). The Rust showcase stays until it is removed in a step of its own; do not delete or change it meanwhile.

This file is all you need besides the Rust area you port. Read section 1 to 3 before writing code, and section 7 before committing.

## 1. Layout

```
kits/typescript/src/       the reference kit (docs/applications.md section 5)
  kit.ts                   everything below but testing, in one import
  screen.ts                Screen, Sixels, colours, attributes, cursor flags, modes, TTI writer
  english.ts               text to the English G0 set (EN 300 706 Table 36)
  input.ts                 Input events, Key names, pressed/released/character, decimalDigit
  channel.ts               Application, Context, Channel, Random, runChannel
  navigation.ts            page selection by digits and colour keys (INPUT.md section 9.4)
  sound.ts                 sound members (object, live, blocks, play, stop, gain), pcmBase64
  app.ts                   the protocol: App (declare, publish, publishTti, goto, sound, log), line loop
  read-tti.ts              reads back the TTI the kit writes (rows, links and their sub-codes, cursor, modes)
  packed-links.ts          X/27/0 links that name a sub-page, as a packed OL,27 line, and read back
  testing.ts               Harness: drives a channel as the server does, for tests
channels/showcase/
  src/main.ts              the program: node src/main.ts [--pictures FOLDER]
  src/areas.ts             the registry: AREAS, in the index's order
  src/area.ts              the Area and Section types
  src/kit.ts               re-exports the kit; every module imports "../kit.ts" or "./kit.ts"
  src/common.ts            page(title, band, prompts), FASTEXT_ROW, SERVICE, INDEX, labelColumns, labelAt
  src/index-page.ts        page 100
  src/sound.ts             the synthesized library: Effect, Sounds (load, play, forget)
  src/pictures.ts          Picture and loadPictures (the --pictures folder)
  src/services.ts, src/services/*.ts       ported area
  src/interactive.ts, src/interactive/*.ts ported area
  test/*.test.ts           tests, run by node --test
```

An area is `src/<area>.ts` (the Rust `<area>.rs`: its `Area` object, and its application class if the Rust module holds it) plus `src/<area>/*.ts`, one file per Rust file, kebab-case names (`long_read.rs` is `long-read.ts`). Put the area's page numbers and anything its files share in `src/<area>/pages.ts`, not in `src/<area>.ts`: the area file imports its parts, and a part that imported a constant back from it would form an import cycle in which the constant is not yet defined when the part's top-level code runs.

## 2. Registering an area

`src/area.ts`:

```ts
export interface Section { readonly page: number; readonly name: string; readonly about: string }
export interface Area {
  sections(): readonly Section[];                                       // the Rust sections()
  applications(pictures: readonly Picture[]): readonly Application[];  // the Rust applications(pictures)
}
```

Export one `Area` from `src/<area>.ts`:

```ts
export const games: Area = {
  applications: (pictures) => [new Arcade(pictures)],
  sections: (): readonly Section[] => [{ page: MENU, name: "Action games", about: "invaders, snake, breakout, cave" }],
};
```

and add it to `AREAS` in `src/areas.ts`, in the Rust order, which is the index's order (its Fastext links the first four sections): `[services, interactive, games, adventure, audio, gallery]`. Four of you edit that one line in parallel; keep the order and the merge is trivial. `applications(AREAS, pictures)` makes the index from every area's sections and then every area's applications; the first application to declare a page serves it.

## 3. The kit, against the Rust runtime

The kit hosts the Rust runtime's `Application` and `Context` shape in the program, so a Rust area ports nearly line for line.

### 3.1 Applications

```ts
interface PageEntry { page: number; title: string; ownsDigits?: boolean; ownsColourKeys?: boolean }
interface Application {
  pages(): readonly PageEntry[];
  start(context: Context): void;
  joined?(session: number, context: Context): void;
  opened?(session: number, page: number, context: Context): void;
  leftPage?(session: number, page: number, context: Context): void;
  input?(session: number, page: number, input: Input, context: Context): void;
  wake?(context: Context): void;
  ended?(session: number, context: Context): void;
}
```

A Rust `impl Application for X` becomes `class X implements Application`; unused methods are left out. Titles are at most 32 characters without controls (checked at the declaration). Session state lives in a `Map<number, State>` and is deleted in `ended`, as the Rust `BTreeMap` is.

| Rust (`Context`) | TypeScript (`Context`) |
|---|---|
| `context.now()` (ms since the server started) | `context.now()` (ms since the program started; the server starts it when it starts) |
| `context.epoch_ms()` | `context.epochMs()` (the server's clock, from `start`) |
| `context.publish(Owner::Channel, n, &screen)` | `context.publish("channel", n, screen)` (sub-page 0) |
| `context.publish(Owner::Session(s), n, &screen)` | `context.publish(s, n, screen)` |
| `context.publish_subpages(owner, n, pages)` | `context.publishSubpages(owner, n, [{ screen, subpage }, ...])`, 1 to 64 |
| a `PageFile` read from TTI (gallery) | `context.publishTti(owner, n, ttiText)` (section 3.6) |
| `context.goto(s, n)` | `context.goto(s, n)` |
| `context.sound(audience, records)` | one `context.sound(to, member)` per sound (section 3.5) |
| `context.wake_at(t)`, `context.wake_in(d)` | `context.wakeAt(t)`, `context.wakeIn(d)`; one wake per application, the earliest asked for wins, as in Rust |
| `context.random()`, `context.below(n)` | `context.random()`, `context.below(n)`: the same xorshift64*, one generator per channel, so the same seed and the same order of calls give the same numbers |
| `context.log(text)` | `context.log(text)` (the server's log; `console.error` also reaches it, but never `console.log`: standard output is protocol only) |

A session's own copy of a page replaces the channel's for that viewer from its first publish until the session ends. An application that throws is reported on standard error and the channel goes on; do not rely on it.

### 3.2 Screen and Sixels (`screen.ts`)

| Rust | TypeScript |
|---|---|
| `Screen::new()` | `new Screen()`; `screen.clone()` |
| `codes`, `code`, `text`, `coloured`, `centred`, `fill`, `at`, `row`, `header`, `background` | the same names |
| `double_height`, `row_text`, `index_link` | `doubleHeight`, `rowText`, `indexLink` |
| `fastext([Some(0x101), None, ..])` | `fastext([0x101, null, null, null])` |
| `cursor(row, column, width, flags)`, `modes(m)` | the same; read back with `cursorLine()`, `inputModes()`, `fastextLinks()`, `index()` |
| `PageFile.fastext_row_24 = true` | `screen.showFastextRow()` (a page with any link shows row 24 anyway) |
| `Colour::Red`, `.text()`, `.mosaic()` | `Colour.Red`, `textColour(c)`, `mosaicColour(c)` |
| `attribute::NEW_BACKGROUND` | `Attribute.NewBackground` (and `Flash`, `Steady`, `NormalHeight`, `DoubleHeight`, `Conceal`, `Contiguous`, `Separated`, `BlackBackground`, `HoldMosaics`, `ReleaseMosaics`) |
| `neoteletext_input::cursor::{ECHO, MASKED, BLOCK}` | `CursorFlag.Echo`, `.Masked`, `.Block` |
| `modes::{ALL_KEYS, KEY_RELEASE, BASE_KEY, MOUSE_BUTTONS, ..}` | `Mode.AllKeys`, `.KeyRelease`, `.BaseKey`, `.MouseButtons`, `.MouseDrag`, `.MouseAll`, `.MousePixels`, `.Compose` |
| `COLUMNS`, `ROWS`, `SIXELS_ACROSS`, `SIXELS_DOWN`, `SPACE`, `HEADER_TEXT_COLUMN` | the same |
| `Sixels::new(w, h)`, `set`, `get`, `clear`, `cell`, `draw` | the same; `size()` is the fields `width` and `height` |
| `format!("{:03X}", page)` | `pageText(page)` |

`screen.toTti(page, subpage)` and `tti(page, subpages)` write the TTI; you never call them yourself, `publish` does. They write every row in the escape form (control codes as ESC and the code plus 0x40), `SC` as the sub-page's decimal digits, `FL` with `8FF` for no link and the index link sixth (link 5, where the engine's viewer looks; the Rust runtime's `Screen::page` puts it fifth, which is a Rust bug, harmless to a port), then `CU` and `IM`. `crates/teletext-native/tests/typescript_channel.rs` checks the server reads all of it.

### 3.3 Text: the English G0 set (`english.ts`)

`screen.text` folds every character to the English national option set (EN 300 706 Table 36): `#` is written at 0x5F and `£` at 0x23; `←` `½` `→` `↑` `—` `¼` `‖` `¾` `÷` at their codes; curly quotes and dashes become ASCII, accented Latin letters lose their accents, Cyrillic is romanised, combining marks are dropped, a character the set cannot show becomes `?` and a control a space. Two differences from the Rust `Screen::text`, which wrote printable ASCII as it is and anything else as a space:

- ASCII `[ \ ] ^ _ \` { | } ~` are folded to `( / ) space ' -` rather than written at codes where a receiver shows arrows, fractions and `#`. If the Rust text you port contains any of them on purpose (a raw arrow or fraction), write the sign itself (`"←"`) or the code with `screen.code`. Grep the Rust area for them before you start.
- Unknown characters become `?`, not a space.

Text measured for layout (centring, right-aligning) is measured in codes, `englishCodes(text).length`, which equals the character count for ASCII. `foldCharacter(c)` gives one character's fold, for a gallery-style caption fold that also collapses whitespace (port `gallery/text.rs` on top of it; note the kit folds `—` to the set's dash, 0x60, where `gallery/text.rs` folds it to `-`: keep the Rust behaviour inside the gallery's own fold).

### 3.4 Input (`input.ts`) and page selection (`navigation.ts`)

```ts
type Input =
  | { kind: "text"; text } | { kind: "paste"; text }
  | { kind: "key"; action: "press" | "repeat" | "release"; key; text?; base? }
  | { kind: "mouse"; action: "press" | "release"; button; column; row }
  | { kind: "mouse"; action: "move"; buttons; column; row }
  | { kind: "wheel"; dx; dy; column; row }
  | { kind: "focus" } | { kind: "blur" };
```

| Rust | TypeScript |
|---|---|
| `input.pressed(Named::Up)` | `pressed(input, Key.Up)` |
| `input.released(Named::Enter)` | `released(input, Key.Enter)` |
| `input.character()` | `character(input)` (one code point, or undefined) |
| `Input::Mouse { pressed: true, button, column, row }` | `input.kind === "mouse" && input.action === "press"`, `Button.Left` 0, `Button.Middle` 1, `Button.Right` 2; `clicked(input)` for a press of the left button |
| `Input::Moved { buttons, column, row }` | `input.kind === "mouse" && input.action === "move"`, `input.buttons` a bit each: `Held.Left` 1, `Held.Middle` 2, `Held.Right` 4; sent in `Mode.MouseDrag` while a button is held, in `Mode.MouseAll` always |
| `Input::Wheel { dy, .. }` | `input.kind === "wheel"`, `input.dy` (1/120ths of a line) |
| `Named::Character(c)` | `input.kind === "key" && isCharacterKey(input.key)`; the key is the character |
| `Named::Other(code)` | the key's W3C name: `Key.F1` to `Key.F4`, `Key.Red` (`ColorF0Red`), `Key.Green`, `Key.Yellow`, `Key.Blue`, or any other name from the engine's `crates/neoteletext-input/src/names.rs` (code n is the n-th name there: 92 to 95 are the colour keys) |
| `base` (`BASE_KEY`) | `input.base`, a string |

Key names in `Key`: `Up` `Down` `Left` `Right` (the `Arrow…` names), `Enter`, `Escape`, `Backspace`, `Delete`, `Tab`, `Home`, `End`, `PageUp`, `PageDown`, `F1`–`F4`, `Red`, `Green`, `Yellow`, `Blue`. Any other name is a plain string.

**Page selection is the kit's default, and it is new behaviour.** While keys go to a channel every key goes (INPUT.md section 9.1), so on every page the kit takes three digits (of any script, `decimalDigit`) and goes to that page of the channel if the channel has it (the first digit a magazine, 1 to 8), and takes F1–F4 and the coloured keys and follows the page's own Fastext links, as the viewer sees them (the session's own copy if it has one). The Rust showcase never did this. A page that uses digits or colour keys itself must say so in its `PageEntry`, or it never receives them:

- `ownsDigits: true` for a page that takes typed text or numbers (Station Zero, 401, typed; the sound board's number keys, 503; any page whose Rust input reads digits or text). In `ALL_KEYS` a digit arrives as a key press whose key is the digit, and is taken the same way.
- `ownsColourKeys: true` for a page that acts on F1–F4 or the colour keys (The Lost Crown, 402: `IMPLEMENTATION-LOG.md` row 5).

Everything else (letters, arrows, the mouse, paste) always reaches the application. Write a test that digits and colour keys navigate where the page does not use them, as `test/services.test.ts` and `test/interactive.test.ts` do at their ends.

### 3.5 Sound

`src/sound.ts` is the Rust `sound.rs`: `Effect.Laser` … `Effect.Bell` and `{ note: 0..12 }`; `new Sounds()` per application as in Rust; `sounds.play(session, effect, context)` sends the library once a session, then a play command; `sounds.forget(session)` in `ended`. The samples are the Rust integer synthesis exactly; `samples(effect)` and `handle(effect)` are exported.

Your own sounds go through `context.sound(to, member)`, `to` being `{ session }` or `{ page }` (everyone with the page open):

| Rust | TypeScript member |
|---|---|
| `Sound::new(h, g, Codec::ImaAdpcm, rate, IMA_BLOCK_SMALL, &pcm)` and its records | `{ object: { handle, generation, rate, samples: Int16Array } }` |
| `Sound::live(h, g, Codec::ImaAdpcm, rate, IMA_BLOCK_LARGE)`'s object record | `{ live: { handle, generation, rate } }` |
| `sound.live_records(first, &pcm)` | `{ blocks: { handle, generation, rate, first, samples } }`: blocks of 1,017 samples, which the server encodes in 512-byte IMA ADPCM blocks (`IMA_BLOCK_LARGE`, what the Rust radio uses) |
| `Command { operation: PLAY, channel, handle, generation, gain, fade_ms, flags, duck, page, sequence }` | `{ play: { channel, handle, generation, gain?, fadeMs?, loop?, after?, duck?, page? } }`; `FLAG_LOOP` is `loop: true`, `FLAG_AFTER_CURRENT` is `after: true` |
| `Command` with `STOP` / `GAIN` | `{ stop: { channel, fadeMs? } }`, `{ gain: { channel, gain, fadeMs? } }` |

Rates are 8000, 11025, 16000, 22050, 24000, 32000, 44100 or 48000; channels 0 to 3; gain 0 to 100; duck 0 to 6; at most two minutes of samples. Every field is checked when the line is written and a bad one throws. **The server numbers each channel's commands itself**, so drop the Rust `sequence` and the process-wide `SEQUENCE` counter. The Rust records were encoded in the process; the protocol carries PCM, so the encoding happens in the server. The audio streaming design is paused: port the radio as the Rust radio is, and do not touch `docs/streaming-audio.md`.

### 3.6 Pictures, raw TTI, and the gallery's channels

`node src/main.ts --pictures FOLDER`. `loadPictures(folder)` (`src/pictures.ts`) reads it as `crates/teletext-native/src/pictures.rs` does and every area's `applications(pictures)` receives the result (none without the folder):

```ts
interface Picture { name: string; caption: string; group: string; levels: readonly { level: "l15" | "l25" | "l35"; tti: string }[] }
```

`group` is the Rust `char` as a one-letter string (`e` examples, `s` sci-fi sprites, `c` characters, `a` art, `p` people, `t` travel, `g` game art). The folder is built by `scripts/pictures.py` into the ignored `tmp/pictures` (decision 6: never committed); `channels.json` passes `../../tmp/pictures`, relative to `channels/showcase`, and the deploy host's service will pass its own folder. The picture files are seven-bit TTI with control codes in the escape form (checked over all 2,223 files on 25 October 2026), with `DE`, `PN`, `SC`, `PS`, `FL` and `OL` lines, including the packed `OL,26` and `OL,28` enhancement lines.

- **Games** read a sprite from a picture's Level 1.5 rows: `readTti(picture.levels.find(l => l.level === "l15")?.tti ?? "")[0]?.rows` (`kits/typescript/src/read-tti.ts`) gives the 25 rows of codes, ignoring the packed lines, as the Rust `extract` reads the page's rows.
- **The gallery** publishes the files themselves with `context.publishTti(owner, page, text)`: ASCII only, 1 to 64 sub-pages each from a `PN` line; the server addresses every sub-page to `page` whatever its `PN` says, and the `FL` line of the first sub-page gives the coloured keys their links. Port `gallery/levels.rs` as text: from each file take its first page (from its `PN` line to the next), set the `PN` line's sub-page digits and the `SC` line (`decimalSubcode`, written as four hexadecimal digits), replace its `FL` line with the gallery's links (`FL,r,g,y,c,8FF,index`, `8FF` for none; add one if the file has none), and join the sub-pages. The Rust credit links name a sub-page (`levels::link(page, Some(subpage))`), which the engine writes as a packed `OL,27` line, not `FL`; the kit's `packedLinksLine(page, links, displayRow24)` writes that packed line (EN 300 706 clause 9.6.1), and `readTti` reads it back, so the coloured keys follow those links too.
- **The collections' own channels** (`gallery::channels`: characters, art, people, travel, game art) are separate programs, one entry each in the root `channels.json`, each with its index at page 100:

  ```json
  {"folder":"characters-ts","name":"NeoTeletext characters","command":["node","src/collection.ts","--pictures","../../tmp/pictures","--collection","characters"],"directory":"channels/showcase"}
  ```

  `src/collection.ts` runs `runChannel([new Gallery(...)])` for the collection named; a collection without pictures should still declare its index, since the server starts every program in the file. Use `-ts` folders while the Rust server still serves `characters` and the rest itself (`--channels` refuses a folder that is already a channel), and show those folders on the showcase gallery's menu, which names each collection's folder.

## 4. Porting the arithmetic exactly

The services and the input area came out byte for byte the same as the Rust (section 6.3). Keep that:

- Rust `/` on unsigned integers is `Math.floor(a / b)`; on signed integers it truncates: `Math.trunc(a / b)`. `%` behaves as JavaScript's (the sign of the dividend).
- `saturating_sub` on unsigned types never goes below 0: `Math.max(0, a - b)`. Rust code leans on this silently; look for it in every subtraction.
- `checked_div(..).unwrap_or(0)`: test for a zero divisor.
- `u64` wrapping arithmetic (hashes, SplitMix64, xorshift): `BigInt` with `BigInt.asUintN(64, ..)`. `u32` wrapping: `>>> 0`. Anything that must stay 64-bit signed (the calculator): `BigInt`, range-checked.
- Numbers above 2^53 lose precision in `number`; use `BigInt` there.
- `format!("{x:>3}")` is `x.toString().padStart(3)`, `{:<14}` is `padEnd(14)`, `{:02}` is `padStart(2, "0")`; Rust never cuts a longer value, nor does `padStart`.
- `chars().count()` is `[...text].length`, not `text.length` (UTF-16 units).
- `split_whitespace` is `text.split(/\s+/u).filter((piece) => piece.length > 0)`.
- Random draws must happen in the same order as in Rust, or games diverge from their tests' expectations.

## 5. TypeScript rules (the gate enforces them)

- **Erasable syntax only** (`erasableSyntaxOnly`): Node runs the `.ts` files by stripping types. No `enum` (use an `as const` object and a union type, as `Effect`, `Colour`, `Problem` are), no `namespace`, **no constructor parameter properties** (`constructor(readonly x: number)` is refused; declare the field and assign it), no `import x = require`.
- Imports name the `.ts` extension; type-only imports use `import type` or `type` in the braces (`verbatimModuleSyntax`).
- `noUncheckedIndexedAccess`: `array[i]` is `T | undefined`; answer with `?? default` or a check, never `!`.
- `exactOptionalPropertyTypes`: never assign `undefined` to an optional property; spread it in conditionally (`...(x === undefined ? {} : { x })`).
- `strict`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, `noUnusedLocals`, `noUnusedParameters` (prefix an unused parameter with `_`), `noPropertyAccessFromIndexSignature` (`record["key"]`), `noImplicitOverride`.
- Private state is `#field`. No `.js` files; no dependencies beyond the pinned `typescript` and `@types/node` (a library needs the owner's approval by name).
- The owner's rules: no magic numbers (name each literal, or cite its source or derivation in a comment: "EN 300 706 Table 26"); comments say why; descriptive names. The Rust constants and their doc comments carry over; a layout's row numbers written inline in a drawing function are as the Rust has them.
- **Page numbers are hexadecimal numbers**: page 301 is `0x301`, never `301`. Show one with `pageText`. Typed digits are the hexadecimal digits of a page (typing 1, 0, 9 goes to `0x109`).
- Nothing but protocol lines may reach standard output: no `console.log`.

## 6. Tests

### 6.1 Writing them

Port every Rust test of the area into `test/<area>.test.ts` (several files if the Rust has several), with Node's runner: `import { test } from "node:test"`, `import assert from "node:assert/strict"`. Keep the Rust test's name as the test's sentence and its doc comment as a comment. The Rust harness is `kits/typescript/src/testing.ts`'s `Harness`:

| Rust (`crate::testing::Harness`) | TypeScript (`Harness`) |
|---|---|
| `Harness::new(applications(&[]))` | `new Harness(area.applications([]))`; options `{ epochMs, seed }`, by default 0 and the Rust harness's seed 7 |
| `join(s, page)`, `open(s, page)` | the same; `close(s)` for `channel.closed(s, now)` |
| `type_text(s, text)`, `press(s, "Name")`, `click(s, button, column, row)`, `wheel(s, dy, column, row)` | `typeText`, `press`, `click`, `wheel`; also `paste`, `key(s, name, action)`, `mouse(s, "press" \| "release", button, column, row)`, `move(s, buttons, column, row)`, `send(s, input)` |
| `advance(by)` (one deadline if due) | `advance(by)`; `runUntil(t)` wakes at every time asked, as the services tests' `run` does |
| `shared(page)`, `own(s, page)` (rows of the last sub-page) | the same; `sharedPages(page)` and `ownPages(s, page)` give every sub-page with its links, `fastextRow`, cursor and modes |
| `cursor(s)` | `cursor(s)`: row, column, width, flags of the page the session is at; the acknowledged count is the server's and is not seen here |
| `cursor_pages(s)` | `gotos(s)`, the pages the session was sent to |
| `page_modes(s)` | `pageModes(s)` |
| `shared_publications(page)` | `sharedPublications(page)` |
| `text(rows, row)`, `row_of`, `shows` | `text`, `rowOf`, `shows` |
| sounds sent | `sounds(s)`, the sound lines for the session, parsed |
| | `sent` (every line), `errors` (what the channel reported; assert it empty) |

The harness does not follow a goto: open the page yourself, as the Rust tests do.

### 6.2 Running them

- Once: `scripts/typescript.sh` installs the pinned tools (`npm ci`, no install scripts) and runs the whole TypeScript stage: Node 22.18 or later, `tsc`, every test.
- While working: `node_modules/.bin/tsc -p tsconfig.json` (type check, the repository root's `tsconfig.json`), `node --test 'channels/**/*.test.ts'`, or one file, `node --test channels/showcase/test/games.test.ts`.
- The server end to end: `cargo nextest run -p teletext-native --test typescript_channel` (after `. scripts/tools.sh` in bash), which also serves the root `channels.json`.
- By hand: `cargo run -p teletext-native -- --channels channels.json`, then the browser at `http://127.0.0.1:8090/showcase-ts/`. The program alone: `printf '{"type":"start","protocol":1,"epoch_ms":0}\n' | node channels/showcase/src/main.ts` prints its lines.

### 6.3 Proving the port is exact

The services and the input area were checked byte for byte against the Rust, and this is how; do it for your area before you commit, and never commit the Rust half:

1. Add a temporary `#[cfg(test)] mod parity;` to `crates/teletext-showcase/src/lib.rs` and a `src/parity.rs` with a test that builds a `Channel` (or the crate's `testing::Harness`) of your area's applications, seed 7 and a fixed `epoch_at_start_ms`, starts it, runs a script of joins, typing, keys and clicks, and writes every shared page's rows (`testing::pages_in`, rows 1 to 24 in hexadecimal) and the session pages and cursors you care about to the file named by an environment variable. Run it with `cargo nextest run -p teletext-showcase <test name>`.
2. Write the same script in a TypeScript file in your scratch folder against `Harness` with `{ seed: 7n, epochMs }`, printing the same format (`readTti` for the rows).
3. `diff` the two files. Every difference is a porting mistake or a deliberate change (the English fold, page selection); fix the first kind.
4. Remove `parity.rs` and the two lines from `lib.rs`, and check `git status` shows nothing under `crates/` before you commit.

## 7. The gates and committing

- `scripts/check.sh commit` runs as the pre-commit hook. With anything under `kits/`, `channels/`, `channels.json`, `package.json`, `package-lock.json`, `tsconfig.json`, `scripts/` or `.githooks/` staged, it runs the TypeScript stage and the end-to-end test; with anything under `crates/` (or the Rust configuration) staged, the Rust stage too. It **refuses to run while those paths hold unstaged changes or untracked files**, so what is checked is what is committed: commit a whole area at once, or move what is not yet committed out of the tree meanwhile. Never `--no-verify`.
- `scripts/check.sh full` (pre-push) runs everything, including the TypeScript stage on the Linux host under its Node 22. Do not push or merge unless asked.
- One concept a commit, the message saying why. No AI credit of any kind in a message, even if a tool suggests a trailer.
- Do not change the kit's public behaviour without its tests; a kit change touches every area. If your area needs something the kit lacks, add it to the kit in a commit of its own, with tests, and say so in your report.

## 8. Pitfalls met so far

- A Rust `const _: () = assert!(..)` becomes a check at the module's top level that throws, as `long-read.ts` does.
- A Rust module that is `pub(super)` everywhere still exports in TypeScript what its siblings and tests need; tests import from `../src/<area>/<file>.ts` directly.
- A shared page republished on every change costs every viewer; the input area's `Republish` (`src/interactive/pages.ts`) is the Rust coalescing, reusable.
- `page(title, band, prompts)` takes prompts as `[label, page | null]` pairs; `labelAt(labels(prompts), column)` finds the prompt clicked on row 24 of a page that takes the mouse.
- The server clock (105) now counts from the program's start, which is the server's start but for a restart of the program.
- A page the Rust published only per session still needs a shared copy published at start if a receiver can show the page before saying it opened it; the Rust areas already do this (`first_screen`), keep it.
