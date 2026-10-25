# Implementation log

The questions met while implementing without the owner, and the answer taken, as `~/devel/neoteletext-common/DEVELOPMENT.md` section 3 asks: number, question, decision, reason. A row is amended in place or superseded by a later row that says so.

## 25 October 2026: the repository and its process

| Row | Question | Decision | Reason |
|---|---|---|---|
| 1 | Where the workspace sits: at the root, as the engine has it, or in `native/`, as the browser does | At the root | The browser's `native/` sets the program apart from its documents and prototypes; this repository holds only the server. |
| 2 | Where the engine's and the browser's process forms differ, which | The browser's, as DEVELOPMENT.md Appendix A prefers | Staged-only commit gate, nothing skipped, pinned tools, State lines. |
| 3 | `rust-version` | 1.97, three releases behind the pinned 1.99.0 | DEVELOPMENT.md section 13's limit. |
| 4 | Install the pinned tools, or share the browser's | Link the browser's `tools/`, the same versions | The same binaries; building them again takes minutes and gains nothing. |
