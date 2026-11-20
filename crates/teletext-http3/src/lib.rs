//! HTTP/3 without input or output (DEC-6964 item 3, REQ-6967): what a
//! server, and a client, read from and write to QUIC streams, as code that
//! takes byte slices and gives byte vectors. It opens no socket, reads no
//! clock and keeps no QUIC state: the host carries the bytes.
//!
//! - [`varint`]: QUIC's variable-length integers (RFC 9000 section 16).
//! - [`frame`]: RFC 9114 section 7's frames, read as their bytes arrive.
//! - [`settings`]: the SETTINGS frame's payload (RFC 9114 section 7.2.4).
//! - [`stream`]: unidirectional stream types and the control stream (RFC
//!   9114 section 6.2), and a request stream's sequence of frames (section
//!   4.1).
//! - [`qpack`]: field sections (RFC 9204) with the static table alone. A
//!   reference to the dynamic table is a connection error, since
//!   [`settings::ours`] offers a table of no capacity.
//! - [`message`]: a request's control data (RFC 9114 section 4.3.1), with
//!   the extended CONNECT of RFC 9220.
//!
//! Every bound is a constant of [`limits`], and input over one is the
//! connection error RFC 9114 section 8.1 names.

#![no_std]
#![forbid(unsafe_code)]
// No panicking constructs: everything here reads what a peer sends
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

extern crate alloc;

pub mod error;
pub mod frame;
pub mod limits;
pub mod message;
pub mod qpack;
pub mod settings;
pub mod stream;
pub mod varint;

pub use error::{Code, Error, Scope};
