//! Integers with their range in their type: read refusing a value outside
//! it with `out_of_range` and its path (server.md SRV-49 item 1), and
//! written into the schema as an integer with that minimum and maximum
//! (SRV-96), inline, so that no generic name reaches the schema's `$defs`
//! (SRV-3).

use alloc::borrow::Cow;
use alloc::format;
use core::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};

use crate::protocol::read::INNER_PATH;

/// The error a value outside its range gives, in the form the reader
/// carries a rule out of serde with: no inner path, the rule, the detail.
fn out_of_range<E: de::Error>(value: impl fmt::Display, least: u64, most: u64) -> E {
    E::custom(format!("{INNER_PATH}{INNER_PATH}OutOfRange{INNER_PATH}{value}: {least} to {most}"))
}

/// Reads an integer of any JSON form serde gives, as `u64`, or the error
/// of a negative one.
struct Whole {
    least: u64,
    most: u64,
}

impl Visitor<'_> for Whole {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "an integer from {} to {}", self.least, self.most)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        if (self.least..=self.most).contains(&value) { Ok(value) } else { Err(out_of_range(value, self.least, self.most)) }
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| out_of_range(value, self.least, self.most)).and_then(|value| self.visit_u64(value))
    }
}

macro_rules! ranged {
    ($(#[$meta:meta])* $name:ident, $inner:ty, $format:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, serde::Serialize)]
        #[serde(transparent)]
        pub struct $name<const LEAST: $inner, const MOST: $inner>(pub $inner);

        impl<const LEAST: $inner, const MOST: $inner> $name<LEAST, MOST> {
            /// The value.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl<'de, const LEAST: $inner, const MOST: $inner> Deserialize<'de> for $name<LEAST, MOST> {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = deserializer.deserialize_u64(Whole { least: u64::from(LEAST), most: u64::from(MOST) })?;
                <$inner>::try_from(value).map(Self).map_err(|_| out_of_range(value, u64::from(LEAST), u64::from(MOST)))
            }
        }

        impl<const LEAST: $inner, const MOST: $inner> JsonSchema for $name<LEAST, MOST> {
            fn inline_schema() -> bool {
                true
            }

            fn schema_name() -> Cow<'static, str> {
                format!(concat!(stringify!($name), "_{}_{}"), LEAST, MOST).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "integer", "format": $format, "minimum": LEAST, "maximum": MOST })
            }
        }
    };
}

ranged!(
    /// A `u8` from `LEAST` to `MOST`.
    U8In,
    u8,
    "uint8"
);
ranged!(
    /// A `u16` from `LEAST` to `MOST`.
    U16In,
    u16,
    "uint16"
);
ranged!(
    /// A `u64` from `LEAST` to `MOST`.
    U64In,
    u64,
    "uint64"
);

/// Reads a signed integer of any JSON form serde gives, as `i64`.
struct Signed {
    least: i32,
    most: i32,
}

impl Signed {
    /// The error of a value outside the range, in the reader's form.
    fn outside<E: de::Error>(&self, value: impl fmt::Display) -> E {
        E::custom(format!("{INNER_PATH}{INNER_PATH}OutOfRange{INNER_PATH}{value}: {} to {}", self.least, self.most))
    }
}

impl Visitor<'_> for Signed {
    type Value = i32;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "an integer from {} to {}", self.least, self.most)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<i32, E> {
        i32::try_from(value).ok().filter(|value| (self.least..=self.most).contains(value)).ok_or_else(|| self.outside(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<i32, E> {
        i64::try_from(value).map_err(|_| self.outside(value)).and_then(|value| self.visit_i64(value))
    }
}

/// An `i32` from `LEAST` to `MOST`: what the rule table's signed fields
/// are (page-rules.md RULE-35), whatever their width in the record.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct I32In<const LEAST: i32, const MOST: i32>(pub i32);

impl<const LEAST: i32, const MOST: i32> I32In<LEAST, MOST> {
    /// The value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl<'de, const LEAST: i32, const MOST: i32> Deserialize<'de> for I32In<LEAST, MOST> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_i64(Signed { least: LEAST, most: MOST }).map(Self)
    }
}

impl<const LEAST: i32, const MOST: i32> JsonSchema for I32In<LEAST, MOST> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("I32In_{LEAST}_{MOST}").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "format": "int32", "minimum": LEAST, "maximum": MOST })
    }
}

/// A MIDI data byte, 0 to 127 (MIDI 1.0 4.2, printed page 4).
pub type Data = U8In<0, 127>;
/// A MIDI channel as MIDI 1.0's prose numbers it, 1 to 16 (server.md
/// SRV-7); the server writes channel `c` as `c - 1`.
pub type MidiChannel = U8In<1, 16>;
/// A fourteen-bit value or parameter number, 0 to 16,383 (MIDI 1.0 4.2,
/// Table III, controllers 6 and 38, 98 to 101).
pub type Data14 = U16In<0, 16_383>;
