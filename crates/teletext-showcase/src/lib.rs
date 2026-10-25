//! The NeoTeletext showcase (`docs/architecture.md`): the channels and
//! their applications, each a plain state machine of the runtime, so they
//! run on every host. Pictures arrive from the host as TTI files (they
//! are kept out of git, decision 6).

#![no_std]
#![forbid(unsafe_code)]
// No panicking constructs: the applications are driven by what viewers
// send (DEVELOPMENT.md section 6).
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

pub mod adventure;
pub mod audio;
pub mod channels;
pub mod common;
pub mod gallery;
pub mod games;
pub mod index;
pub mod interactive;
pub mod services;
pub mod sound;
#[cfg(test)]
mod testing;

pub use channels::{ChannelSpec, Picture, channels};
