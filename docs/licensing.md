# teletext-server: licensing

## 1. This project's licence

MIT, as the engine's and the browser's are (decision 4, for the owner to confirm).

## 2. What may be used

As `~/devel/neoteletext-common/DEVELOPMENT.md` section 7 says:
- MIT, Apache-2.0 (with or without the LLVM exception), BSD-2-Clause, BSD-3-Clause, Zlib, ISC, CC0-1.0 and Unicode-3.0;
- MPL or EPL only by the owner's decision.

## 3. What may not be used

GPL and AGPL, in any form.

## 4. How it is enforced

- `cargo deny check` in the full gate, against `deny.toml`. An exception names one crate and one licence, with its reason.
- The owner's approval of each library by name, recorded in a decision (requirement 2.3).

## 5. What is in use

| Library | Licence | For | Approved |
|---|---|---|---|
| The engine's crates (`neoteletext-core`, `-formats`, `-input`, `-audio`, `-web`) | MIT | Teletext itself | Decision 1 |
| `tokio`, `hyper`, `hyper-util` | MIT | The native host | Decision 5 |
| `libfuzzer-sys` | (MIT or Apache-2.0) and NCSA, for the LLVM libFuzzer it bundles | `fuzz/`, outside the shipped server | Decision 5 |

### 5.1 What the channels use

The channels are programs of their own (decision 8). What they run is not linked into the server.

| What | Licence | Used by | Why |
|---|---|---|---|
| ruff, mypy, pytest (`uv.lock`) | MIT | The Python checks | Development only, not deployed (`uv run --no-dev`) |
| numpy, Pillow (the engine's `scripts/image-to-teletext.py` pins them) | BSD-3-Clause; MIT-CMU (the historical PIL licence, permissive) | The Hackaday channel's pictures, through the engine's converter run by uv | The engine's own tool, not copied; MIT-CMU is outside `DEVELOPMENT.md` section 7's list, so the owner confirms it |
| tslib (through oxlint-tsgolint) | 0BSD | The TypeScript channels' and kits' lint, an optional fallback not installed on macOS or Linux x64 | Development only, never linked into a channel; 0BSD is outside `DEVELOPMENT.md` section 7's list, approved by the owner on 28 October 2026 (decision 13) |

## 6. Approved, not yet used

| Library | Licence | For | Approved |
|---|---|---|---|
| `rustls`, `tokio-rustls`, `rustls-acme` | MIT or Apache-2.0 | TLS and certificates | Decision 5 |
| `symphonia` (0.6.x) | MPL-2.0, by the owner's decision (section 2) | Decoding streamed audio: MP3, AAC, FLAC and Vorbis (`streaming-audio.md`) | Decision 5, 25 October 2026 |

When `symphonia` is added, `deny.toml` allows MPL-2.0 and `CREDITS.md` names it. MPL-2.0 is copyleft per file: its own files stay under MPL-2.0 and their source stays available; this project's files keep their own licence.
