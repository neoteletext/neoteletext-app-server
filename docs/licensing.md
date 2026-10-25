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

The engine's crates (MIT), when imported. No other library yet.
