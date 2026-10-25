# teletext-server

The NeoTeletext server. The design is `~/devel/neoteletext-common/SERVER.md`; the development process is `~/devel/neoteletext-common/DEVELOPMENT.md`, shared with the engine and the browser.

## Rules particular to this repository

- **Import, never copy.** Teletext parsing, encoding, the page model, the input protocol and audio come from `neoteletext-engine`, as git dependencies pinned at one `rev`, as the browser pins them. Code is never copied out of the engine or the browser.
- **What the engine lacks for a server is written here, once** (the owner's decision of 25 October 2026, option B of `SERVER.md` section 3.3): the engine's scope is unchanged.
- **`runtime` does no I/O and reads no clock**: it works through a `Host` trait; `native` holds the sockets, TLS and processes.
