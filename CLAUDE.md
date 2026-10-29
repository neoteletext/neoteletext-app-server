# teletext-server

The NeoTeletext server. The design is `~/devel/neoteletext-common/SERVER.md`; the development process is `~/devel/neoteletext-common/DEVELOPMENT.md`, shared with the engine and the browser.

## Rules particular to this repository

- **Import, never copy.** Teletext parsing, encoding, the page model, the input protocol and audio come from `neoteletext-engine`, as git dependencies pinned at one `rev`, as the browser pins them. Code is never copied out of the engine or the browser.
- **What the engine lacks for a server is written here, once** (the owner's decision of 25 October 2026, option B of `SERVER.md` section 3.3): the engine's scope is unchanged.
- **`runtime` does no I/O and reads no clock**: it works through a `Host` trait; `native` holds the sockets, TLS and processes.

## Requirements, decisions, checklists and to-dos: rdm only

They are records in rdm, reached through its MCP tools (search, get_record, list_records, create_record, revise_record, set_status, link_records), since 30 October 2026 (rdm decision DEC-1389). **Never create or edit a file to record one:** there is no `docs/requirements.md`, `TECH-DEBT.md`, `PENDINGTASKSFORANDREW.md` or decisions section in `docs/architecture.md` any more, and a hook refuses edits to those files. Old identifiers still work in rdm: "Requirement 1.3", "Decision 10", "TECH-DEBT 4", "Pending task 2". A requirement has its Statement, Reason, Source, Verification and State in its body. Work owed is an rdm to-do, a task only the owner can do among them; a decision taken without the owner is created with `needs_confirmation`. If a merge brings one of those files back, record what it adds in rdm and delete the file.

## Key documents

- The requirements and the owner's decisions: rdm, above.
- `docs/architecture.md`: section 3 how the server runs on each host (section numbers unchanged; section 2, the decisions, is in rdm).
- `docs/licensing.md`, `CREDITS.md`, `IMPLEMENTATION-LOG.md`.

## Commands

- Tools: `scripts/tools.sh` installs the pinned tools into the ignored `tools/`; the check script refuses other versions.
- Edit loop: `scripts/try.sh` (Clippy on every save) or `scripts/try.sh <crate> [filter]`.
- Gates and the work branch: `scripts/check.sh commit | push | full`; the Linux stage runs on `TELETEXT_LINUX_HOST`, `foundintelquiet4060` by default. How they work and the day-to-day routine: @~/devel/neoteletext-common/gates.md
