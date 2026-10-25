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
