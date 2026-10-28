# Pending tasks for Andrew

What only Andrew can do: decisions, accounts, money, keys, people, machines, and checks that need a hand. Each item says what it unblocks. Remove an item here when it is done.

## Decisions

2. **Confirm decision 4:** the MIT licence.
6. **systemd's socket activation** (requirement 5.6): taking the listening socket from systemd needs `unsafe` or a library (such as `listenfd`, MIT or Apache-2.0); approve one by name, or leave connections refused for the moment a restart takes.
8. **Narrow `NEOTELETEXT_CF_TOKEN` to the `neoteletext.com` zone** if you want: it was made for every zone of the account (it lists all seven), and the Cloudflare host needs only that one.
9. **Choose the radio's Opus encoder** (`docs/streaming-audio.md` section 10 item 8): libopus through a Rust binding (C, BSD-3-Clause) is recommended; it needs your approval by name.
7. **Review the choices taken on 28 October 2026** while building decisions 17 to 19: `IMPLEMENTATION-LOG.md` rows 17 to 28 (5 minutes' grace, a 30-day cookie, the administration port, the log's format and the rest).

## Accounts

