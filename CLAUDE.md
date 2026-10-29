# teletext-server

The NeoTeletext server. The design is `~/devel/neoteletext-common/SERVER.md`; the development process is `~/devel/neoteletext-common/DEVELOPMENT.md`, shared with the engine and the browser.

## Rules particular to this repository

- **Import, never copy.** Teletext parsing, encoding, the page model, the input protocol and audio come from `neoteletext-engine`, as git dependencies pinned at one `rev`, as the browser pins them. Code is never copied out of the engine or the browser.
- **What the engine lacks for a server is written here, once** (the owner's decision of 25 October 2026, option B of `SERVER.md` section 3.3): the engine's scope is unchanged.
- **`runtime` does no I/O and reads no clock**: it works through a `Host` trait; `native` holds the sockets, TLS and processes.

## Key documents

- `docs/requirements.md`: every requirement, numbered by section ("Requirement 1.3"), with Statement, Reason, Source, Verification and a dated State.
- `docs/architecture.md`: section 2 the owner's decisions ("Decision n", with a "Now" column), section 3 how the server runs on each host.
- `docs/licensing.md`, `CREDITS.md`, `TECH-DEBT.md`, `IMPLEMENTATION-LOG.md`, `PENDINGTASKSFORANDREW.md`.

## Commands

- Tools: `scripts/tools.sh` installs the pinned tools into the ignored `tools/`; the check script refuses other versions.
- Edit loop: `scripts/try.sh` (Clippy on every save) or `scripts/try.sh <crate> [filter]`.
- Gates and the work branch: `scripts/check.sh commit | push | full`; the Linux stage runs on `TELETEXT_LINUX_HOST`, `foundintelquiet4060` by default. How they work and the day-to-day routine: @~/devel/neoteletext-common/gates.md
