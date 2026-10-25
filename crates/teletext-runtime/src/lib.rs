//! The server's runtime, without input or output (`docs/architecture.md`
//! decision 2): one channel's state, driven by the events a host gives it
//! (a connection opened, a frame arrived, an application wrote, a deadline
//! passed) and answering with the effects the host carries out (send these
//! bytes, close that connection, wake me at this time). It reads no clock,
//! opens no socket and starts no thread, so the same code runs under tokio
//! on every desktop and server system, in a Cloudflare Durable Object, and
//! in WebAssembly, and its tests run with no network and a clock they set.

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
