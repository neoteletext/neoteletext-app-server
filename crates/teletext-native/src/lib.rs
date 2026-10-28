//! The server on the common systems (`docs/architecture.md` section 3.2):
//! tokio and hyper around the runtime. Each channel's runtime is one task;
//! every connection is a task of its own that catches up on the channel's
//! shared pages by itself, so a page's change costs the channel the same
//! whatever the number of viewers (decision 7).

#![forbid(unsafe_code)]
// No panicking constructs: everything here reads what clients send
// (DEVELOPMENT.md section 6).
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

pub mod admin;
pub mod apps;
pub mod connection;
pub mod http;
pub mod hub;
pub mod logging;
pub mod state;
pub mod stats;
pub mod websocket;
