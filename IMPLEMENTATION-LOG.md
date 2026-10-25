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
