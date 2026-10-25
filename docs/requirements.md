# teletext-server: requirements

| | |
|---|---|
| **Status** | Started 25 October 2026 |
| **Companion documents** | `docs/architecture.md` (decisions), `~/devel/neoteletext-common/SERVER.md` (the design), `~/devel/neoteletext-common/DEVELOPMENT.md` (the process) |

## How to read

- No abbreviations.
- Numbers are permanent; a gap is deliberate.
- "Must" is mandatory; "should" is expected unless a reason is recorded; "may" is an option.
- Each requirement has a Statement, a Reason, a Source, a Verification and a dated State.

The behaviour of `SERVER.md` becomes requirements here as each part is adopted and built.

## 1. Where it runs

### Requirement 1.1: Native on the common systems
- **Statement:** The server must build and run as one program on Linux, macOS and Windows, on x86-64 and arm64.
- **Reason:** The owner asked for it to run locally on a variety of operating systems.
- **Source:** Decision 2.
- **Verification:** Test: the full gate builds and tests it on macOS and on the Linux host; Windows by hand in the virtual machine.
- **State:** Not met, 25 October 2026: no native host yet.

### Requirement 1.2: On Cloudflare
- **Statement:** The server must run on Cloudflare Workers, each channel in a Durable Object of its own, with its WebSockets hibernatable.
- **Reason:** The owner asked for it to run on Cloudflare.
- **Source:** Decision 2.
- **Verification:** Demonstration on Cloudflare; test with `wrangler dev` locally.
- **State:** Not met, 25 October 2026: no Cloudflare host yet.

### Requirement 1.3: One runtime, with no input or output, that builds for WebAssembly
- **Statement:** The server's runtime must do no input or output: no clock, no socket, no file, no process, no thread and no async executor. It must be `no_std` with `alloc`, and build for `wasm32-unknown-unknown`. Every host must drive this same runtime, and no host's code may be in it.
- **Reason:** One runtime can then run under tokio, in a Durable Object and in WebAssembly, and its tests set their own time with no network.
- **Source:** Decision 2.
- **Verification:** Check: the commit gate runs Clippy for `wasm32-unknown-unknown` over the runtime, and `no_std` keeps the standard library's input and output out.
- **State:** Met for what exists, 25 October 2026: `teletext-runtime` is `no_std` and passes Clippy for `wasm32-unknown-unknown`.

## 2. Build and verification

### Requirement 2.1: One check script and the git hooks
- **Statement:** `scripts/check.sh` must run every gate:
  - `commit` before each commit: formatting, the no-panic lists, Clippy in every feature shape and for WebAssembly, and tests;
  - `full` before each push, on a clean tree at the pushed commit: the commit gate's steps, documentation tests, release tests, tests with optional features compiled out, licences and advisories, unused dependencies, fuzzing, and the Linux host.

  The hooks in `.githooks/` must run it (`git config core.hooksPath .githooks`). Nothing may be skipped, and nothing may bypass the hooks.
- **Reason:** The gates are local; there is no CI (DEVELOPMENT.md section 9).
- **Source:** Decision 3.
- **Verification:** Check: the hooks call the script; review of the script's steps.
- **State:** Met, 25 October 2026.

### Requirement 2.2: The strictest lints
- **Statement:** The workspace's lints must forbid unsafe code; warn, as errors in the gates, on missing documentation, unused qualifications, Clippy's pedantic group, `allow` without a reason, `dbg!`, `todo!` and `unimplemented!`. Every crate root must carry the same no-panic list: `indexing_slicing`, `string_slice`, `unwrap_used`, `expect_used`, `panic`, `arithmetic_side_effects`, `unreachable`, `todo` and `unimplemented`. A suppression must be narrow, at its site, with its reason.
- **Reason:** Everything the server reads comes from clients and applications that may be hostile (DEVELOPMENT.md section 6).
- **Source:** Decision 3.
- **Verification:** Check: the commit gate compares every root's list.
- **State:** Met, 25 October 2026.

### Requirement 2.3: Dependencies by approval, licences allowed
- **Statement:** Every library must have the owner's approval by name, recorded in a decision, and a licence that `deny.toml` allows. `Cargo.lock` must be committed. Programs must pin `=x.y.z`, with the licence and the reason beside each. `cargo deny` and `cargo machete` must pass.
- **Reason:** DEVELOPMENT.md section 7.
- **Source:** Decisions 3 and 5.
- **Verification:** Check: the full gate.
- **State:** Met, 25 October 2026: no library yet.

### Requirement 2.4: Pinned tools
- **Statement:** The development tools must be installed at exact versions into the ignored `tools/` by `scripts/tools.sh`, each crate checked against its recorded SHA-256, and the check script must refuse any other version.
- **Reason:** DEVELOPMENT.md section 13.
- **Source:** Decision 3.
- **Verification:** Check: the check script's first step.
- **State:** Met, 25 October 2026: `cargo-nextest` 0.9.148, `bacon` 3.26.0, `cargo-deny` 0.20.2, `cargo-fuzz` 0.13.2, `cargo-machete` 0.9.2, and the fuzzing compiler `nightly-2026-10-01`, the browser's.

### Requirement 2.5: The engine imported, never copied
- **Statement:** Every engine crate must be a git dependency on the engine at one commit, the same for every manifest. No code may be copied from the engine or the browser.
- **Reason:** The owner's instruction to share code, not rebuild it (decision 1).
- **Source:** Decision 1.
- **Verification:** Check: the commit gate's engine pin step; review.
- **State:** Met, 25 October 2026: no engine crate imported yet.

### Requirement 2.6: Every reader of outside input fuzzed
- **Statement:** Every reader of bytes from clients, applications or files must have a fuzz target in `fuzz/`, and every input that ever found a fault must be kept in `fuzz/regressions/` and replayed first by the full gate.
- **Reason:** DEVELOPMENT.md section 8.
- **Source:** Decision 3.
- **Verification:** Check: the full gate's fuzzing step.
- **State:** Met, 25 October 2026: no reader yet.
