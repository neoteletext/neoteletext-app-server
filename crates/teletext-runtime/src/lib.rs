//! The server's runtime, without input or output (`docs/architecture.md`
//! decision 2): one channel's state, driven by the events a host gives it
//! (a connection opened, a message arrived, a deadline passed) and
//! answering with the bytes the host sends. It reads no clock, opens no
//! socket and starts no thread, so the same code runs under tokio on every
//! desktop and server system, in a Cloudflare Durable Object, and in
//! WebAssembly, and its tests run with no network and a clock they set.
//!
//! Downstream to a receiver is a T42 stream and nothing else (DEC-1459);
//! upstream is DOC-1931's events. The channel's program speaks the typed
//! JSON protocol of [`protocol`] (DEC-1482), and the server keeps none of
//! a channel's content: the program owns every page.

#![no_std]
#![forbid(unsafe_code)]
// No panicking constructs: everything here is driven by what clients and
// applications send (DEVELOPMENT.md section 6).
#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod channel;
pub mod descriptions;
pub mod frame;
pub mod input;
pub mod json;
pub mod landing;
mod page;
pub mod protocol;
mod records;
pub mod session_cookie;
mod sound;
pub mod stats;

pub use channel::{Channel, ChannelConfig, Effect, Level, PROGRAM_REPLY_LIMIT_MS, SESSION_GRACE_MS};
pub use input::{Input, Named};
pub use protocol::{ConnectionId, SessionId};
pub use stats::ChannelStats;

/// The engine's packet, 42 bytes on the wire (EN 300 706 clause 7.1).
pub use neoteletext_core::packet::Packet;
