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
| The engine's crates (`neoteletext-core`, `-formats`, `-input`, `-audio`, `-audio-instruments`, `-web`, `-opus`, `-rules`) | MIT | Teletext itself; `neoteletext-opus`, the engine's integer port of libopus's decoder, checks programs' Opus packets (decision 21); `neoteletext-rules` encodes and checks each page's rule table with the code every receiver decodes it with (REQ-6532, page-rules.md RULE-59) | Decision 1 |
| The ReGIS renderer (`regis`) and the NAPLPS renderer (`naplps`), NeoTeletext's own, each pinned at one commit; neither has a dependency | MIT | A program's ReGIS, and a page's ReGIS or NAPLPS sprite sheet, are read with the receiver's own code before they are sent (DEC-6335, REQ-6533, animation.md ANIM-17 and ANIM-21) | DEC-6335 for `regis`; `naplps` was taken on 18 November 2026 while the owner was away, and waits for the owner's approval by name (the decision recorded with REQ-6533) |
| `tokio`, `hyper`, `hyper-util` | MIT | The native host | Decision 5 |
| `rustls`, `tokio-rustls` (and what they bring: `rustls-webpki`, `rustls-pki-types`, `subtle`, `zeroize`, checked by `cargo deny`) | Apache-2.0 or ISC or MIT; MIT or Apache-2.0 | TLS served by the native host itself (DEC-6964) | Decision 5 |
| `quinn` (and what it brings: `quinn-proto`, `quinn-udp`, `rand`, `rustc-hash`, `thiserror`, `tinyvec`, `lru-slab`, `socket2` and others, all MIT, Apache-2.0, BSD or Zlib, checked by `cargo deny`) | MIT or Apache-2.0 | QUIC, which carries HTTP/3 (DEC-6964) | DEC-6964, under the owner's standing approval of pure-Rust libraries (DEC-6579) |
| `h2`, through hyper's `http2` feature (and what it brings: `indexmap`, `tokio-util`, `fnv` and others, checked by `cargo deny`) | MIT | HTTP/2 served by the native host itself (DEC-6964); also the tests' HTTP/2 client | DEC-6964, under DEC-6579 |
| `rcgen` (and what it brings: `yasna`, `time`, `pem`, checked by `cargo deny`) | MIT or Apache-2.0 | The certificate and key each test of TLS makes for itself; tests only, never linked into the server | DEC-6964, under DEC-6579 |
| `ring` | Apache-2.0 and ISC | Viewer state's cryptography (DEC-6471), and the cryptography under `rustls` and `quinn` (DEC-6964) | DEC-6471, DEC-6478 |
| `listenfd` (and on Windows alone what it brings, `uuid` and `winapi`, checked by `cargo deny`) | Apache-2.0 | systemd's socket activation in the native host | DEC-1767 |
| `tracing`, `tracing-subscriber` (and what they bring: `tracing-core`, `tracing-serde`, `serde`, `serde_json`, `sharded-slab`, `thread_local`, `smallvec` and others, all MIT or Apache-2.0, checked by `cargo deny`) | MIT | The native host's log | Decision 18 |
| `worker` (workers-rs), `wasm-bindgen`, `getrandom` (and what they bring: `js-sys`, `web-sys`, `wasm-bindgen-futures`, `serde`, `serde-wasm-bindgen` and others, checked by `cargo deny`) | Apache-2.0; MIT or Apache-2.0 | The Cloudflare host, `wasm32-unknown-unknown` only | Decision 20 |
| `wrangler`, `worker-build` | MIT or Apache-2.0; Apache-2.0 | Building, running locally and deploying the Cloudflare host; development only, never linked | Decision 20 |
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
| `rustls-acme` | MIT or Apache-2.0 | Obtaining certificates | Decision 5 |
| `symphonia` (0.6.x) | MPL-2.0, by the owner's decision (section 2) | Decoding streamed audio: MP3, AAC, FLAC and Vorbis (`streaming-audio.md`) | Decision 5, 25 October 2026 |

When `symphonia` is added, `deny.toml` allows MPL-2.0 and `CREDITS.md` names it. MPL-2.0 is copyleft per file: its own files stay under MPL-2.0 and their source stays available; this project's files keep their own licence.
