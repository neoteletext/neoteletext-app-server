//! The server's runtime, without input or output (`docs/architecture.md`
//! decision 2): one channel's state, driven by the events a host gives it
//! (a connection opened, a message arrived, a deadline passed) and
//! answering with the bytes the host sends. It reads no clock, opens no
//! socket and starts no thread, so the same code runs under tokio on every
//! desktop and server system, in a Cloudflare Durable Object, and in
//! WebAssembly, and its tests run with no network and a clock they set.
//!
//! It speaks what the NeoTeletext browser speaks today (the engine's
//! channel layout, decision E.70; INPUT.md version 2), so the browser
//! needs no change: pages go as T42 packets, one page to a WebSocket
//! message, which the browser's decoder completes at the message's end.

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

pub mod application;
pub mod channel;
pub mod frame;
pub mod input;
pub mod json;
pub mod layout;
pub mod remote;
pub mod screen;
mod session;
pub mod stats;

pub use application::{Application, Audience, Context, Owner, PageEntry};
pub use channel::{Channel, ChannelConfig, Effect, SessionId};
pub use input::{Input, Named};
pub use remote::Remote;
pub use screen::{Colour, Screen};
pub use stats::ChannelStats;

/// The engine's packet, 42 bytes on the wire (EN 300 706 clause 7.1).
pub use neoteletext_core::packet::Packet;
/// The engine's page model, which applications publish.
pub use neoteletext_formats::page::PageFile;
