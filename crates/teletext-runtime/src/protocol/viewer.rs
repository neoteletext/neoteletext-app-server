//! Program protocol 13's messages of viewer state (neoteletext-common's
//! `docs/viewer-state.md`, adopted by DEC-6471: VS-351 to VS-353): what a
//! program is told of the viewer on each connection, and the three things
//! it may ask. A program sees only a handle, never a key, a proof or a
//! token, and can choose, set or extend none of them (VS-353).

use alloc::borrow::Cow;
use alloc::string::String;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{ConnectionId, SAFE_INTEGER_LIMIT};
use crate::viewer::HANDLE_BYTES;

/// The hexadecimal digits of a handle: two a byte.
pub const HANDLE_DIGITS: usize = HANDLE_BYTES * 2;
/// The radix of a handle's digits.
const HEXADECIMAL: u32 = 16;
/// The bits of one hexadecimal digit.
const NIBBLE_BITS: u32 = 4;
/// The low digit of a byte.
const LOW_NIBBLE: u8 = 0x0F;
/// A handle's form in the schema (VS-351 item 1).
const HANDLE_PATTERN: &str = "^[0-9a-f]{32}$";

/// A viewer's handle on one channel (VS-305): 128 bits derived by the
/// server, written as 32 lower-case hexadecimal digits. It is meaningless
/// outside its channel and its server's secret, compares by value, and
/// prints only a placeholder for `Debug` (VS-209), so that no log line can
/// carry one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle([u8; HANDLE_BYTES]);

impl Handle {
    /// The handle of `bytes`, as the server derived them.
    pub(crate) const fn of_bytes(bytes: [u8; HANDLE_BYTES]) -> Self {
        Self(bytes)
    }

    /// The handle `text` writes, exactly 32 lower-case hexadecimal digits,
    /// or `None`.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        if text.len() != HANDLE_DIGITS {
            return None;
        }
        let mut bytes = [0_u8; HANDLE_BYTES];
        let mut digits = text.chars();
        for byte in &mut bytes {
            let high = digits.next().and_then(lower_hex)?;
            let low = digits.next().and_then(lower_hex)?;
            *byte = u8::try_from((high << NIBBLE_BITS) | low).ok()?;
        }
        Some(Self(bytes))
    }

    /// The handle as the protocol writes it.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::with_capacity(HANDLE_DIGITS);
        for byte in self.0 {
            for nibble in [byte >> NIBBLE_BITS, byte & LOW_NIBBLE] {
                text.push(char::from_digit(u32::from(nibble), HEXADECIMAL).unwrap_or('0'));
            }
        }
        text
    }
}

/// One lower-case hexadecimal digit's value.
fn lower_hex(digit: char) -> Option<u32> {
    if digit.is_ascii_uppercase() { None } else { digit.to_digit(HEXADECIMAL) }
}

impl core::fmt::Debug for Handle {
    /// Never the handle (VS-209).
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Handle(..)")
    }
}

impl Serialize for Handle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

struct HandleVisitor;

impl Visitor<'_> for HandleVisitor {
    type Value = Handle;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a viewer's handle: 32 lower-case hexadecimal digits")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Handle, E> {
        // The text is not repeated in the error, which reaches the
        // operator's log: it may be a handle (VS-209).
        Handle::read(text).ok_or_else(|| E::invalid_value(de::Unexpected::Other("text that is not a handle"), &self))
    }
}

impl<'de> Deserialize<'de> for Handle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(HandleVisitor)
    }
}

impl JsonSchema for Handle {
    fn schema_name() -> Cow<'static, str> {
        "Handle".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A viewer's handle on this channel (VS-305): 32 lower-case hexadecimal digits, meaningless outside the channel.",
            "type": "string",
            "pattern": HANDLE_PATTERN
        })
    }
}

// ------------------------------------------------------- server to program

/// `viewer_began` (VS-351 item 2): a connection that had no viewer now has
/// one, after a state request or after the browser offered its key.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewerBegan {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// The viewer's handle.
    pub viewer: Handle,
    /// The viewer chose, in the browser, to be remembered on this channel.
    pub remembered: bool,
}

/// `viewer_left` (VS-351 item 3): the connection no longer carries a
/// viewer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewerLeft {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
}

/// Why a state request or a `STATE_KEEP_OFFER` was not granted (VS-351
/// item 4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StateRefusal {
    /// The viewer blocked the channel or refused state, or already chose
    /// to be remembered.
    Declined,
    /// The client never keeps state.
    Stateless,
    /// No input from the viewer was recent enough (VS-212).
    NoGesture,
    /// The connection has no key and none was made.
    NoKey,
    /// The proof did not verify (VS-206).
    Invalid,
    /// A bound on challenges or on new viewers was reached (VS-205,
    /// VS-304), or the offer was already made on the connection.
    Rate,
    /// No answer came within the server's bound (VS-202).
    Timeout,
    /// The connection is a private window's.
    Private,
}

/// `state_refused` (VS-351 item 4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateRefused {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
    /// Why.
    pub reason: StateRefusal,
}

/// `viewers_reset` (VS-351 item 5): every handle the program knows is void,
/// because the server's handle secret was rotated (VS-306).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewersReset {}

/// `gone` (VS-351 item 6, VS-354): the connection closed and will not come
/// back; the program may forget it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gone {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
}

// ------------------------------------------------------- program to server

/// `request_state` (VS-352 item 1): ask that the viewer on the connection
/// be given state; the server answers later with `viewer_began` or
/// `state_refused`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestState {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
}

/// `end_state` (VS-352 item 2): end the viewer's state, on every
/// connection that presents it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndState {
    /// The viewer's handle.
    pub viewer: Handle,
}

/// `offer_remember` (VS-352 item 3): ask the browser to offer "remember
/// me"; refused with `state_refused` when it cannot be offered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfferRemember {
    /// The connection.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub connection: ConnectionId,
}
