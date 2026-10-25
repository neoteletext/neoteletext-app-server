# Technical debt

What is owed, and why, as `~/devel/neoteletext-common/DEVELOPMENT.md` section 2.2 asks. An item is removed when done, or moved to an issue when it needs the owner.

## 25 October 2026: the repository started

- **No GitHub issues yet** (DEVELOPMENT.md section 3): the work of `SERVER.md` section 17 is to be opened as issues once the owner confirms decisions 2, 4 and 5.
- **`tools/` is a link to the browser's** pinned tools, the same versions; `scripts/tools.sh` builds this repository's own when the link is removed.

## 25 October 2026: the showcase

- **Applications cannot see the capabilities agreed in WELCOME**, so the form page (201) shows what it asks for, not what was agreed. The runtime could pass the agreed set to `opened`.
- **No overlay rows on a shared page for one session** (`SERVER.md` section 7.3): a viewer's typing on the guestbook goes to a page of its own (207), and the poll cannot show a viewer their own vote. The runtime needs an overlay of a session's rows over a channel page.
- **The engine's live audio encoder restarts its IMA ADPCM step size at every `live_records` call**, costing 2 to 7 dB at each restart (measured by the sound pages' builder). The radio encodes each tick's blocks in one call; a live encoder that keeps its state between calls is an engine change, for the owner under the engine's process (decision 1: the engine's scope is unchanged). The browser's reference podcast, which encodes one block a call, pays it on every block.
