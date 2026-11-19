//! The program protocol's snapshot messages (REQ-6900; neoteletext-common
//! `docs/tab-thumbnails.md` section 6; DEC-6875 version 2 item 3, DEC-6878
//! version 2 items 2 and 3): the server asks a program to draw one page for
//! no one in particular with `snapshot`, and the program answers with that
//! page and its picture (`snapshot_page`) or declines (`snapshot_declined`).
//!
//! **Nothing here names a connection.** `snapshot` carries a snapshot's
//! number, a page, a sub-code and a path, and nothing of any receiver: no
//! connection, viewer handle, `remembered`, query or request number. The
//! answers carry the number back with a page and a picture, and have no
//! field for an audience, rules, sound, links, a cursor, sprites, a
//! timeline, a sub-page list or an index, so a program cannot send any of
//! them with a snapshot. A snapshot's number is not a connection's: a
//! message naming it as a connection is refused `unknown_connection`, as
//! any number the server did not announce with `connected` is.
//!
//! **Why protocol 16 is not raised** (REQ-6900 item 7): see [`super::PROTOCOL`].

use alloc::string::String;
use alloc::vec::Vec;

use schemars::JsonSchema;
use serde::Serialize;

use super::animation::NaplpsObject;
use super::picture::{self, KeptPictureName, PictureLanguageName, PicturePart};
use super::{Base64, Page, PageNumber, SAFE_INTEGER_LIMIT, SubCode};

/// A snapshot's number: 1 to 2^53 less one, rising, never reused while the
/// server runs, and counted apart from connection numbers.
pub type SnapshotNumber = u64;

/// `snapshot` (REQ-6900 item 1): a receiver asks for a picture of this page
/// as a stranger who has just arrived would see it. Draw it for no one in
/// particular, as for a connection with no viewer and no query, and answer
/// with `snapshot_page` or `snapshot_declined` within
/// `start.budgets.snapshot_limit_ms`. The answer may be given to any
/// stranger for up to a minute.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// The snapshot's number, which the answer carries back.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub snapshot: SnapshotNumber,
    /// The page asked for, a display page.
    pub page: PageNumber,
    /// The sub-code asked for, or 3F7F for the page as a whole.
    pub sub: SubCode,
    /// The place as an address names it: `302` or `302/0002`.
    pub path: String,
}

/// `snapshot_page` (REQ-6900 item 2): the page a program drew for a
/// snapshot, and its picture.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotPage {
    /// The number of a `snapshot` the server awaits.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub snapshot: SnapshotNumber,
    /// One sub-page, as every page message carries one: of the page asked
    /// for, and of the sub-code asked for, or any of the page's when 3F7F
    /// was asked.
    pub page: Page,
    /// Its picture, shown in place of the sub-page's teletext page by a
    /// receiver that agreed its language: the kept picture of the same
    /// sub-page, or bytes; none for none.
    #[serde(default)]
    pub picture: Option<SnapshotPicture>,
}

/// `snapshot_declined` (REQ-6900 item 3): the program will not draw this
/// snapshot. The server may answer from the page's public description.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotDeclined {
    /// The number of a `snapshot` the server awaits.
    #[schemars(range(min = 1, max = SAFE_INTEGER_LIMIT))]
    pub snapshot: SnapshotNumber,
}

/// A snapshot's picture: `{"kept": "<sub-code>"}`, the picture kept for the
/// same sub-page (PAR-16), or bytes.
// Read by the reader, which chooses by the field it holds (`read.rs`).
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(untagged)]
pub enum SnapshotPicture {
    /// The kept picture of the answered sub-page: its sub-code.
    Kept(KeptPictureName),
    /// These bytes.
    Bytes(SnapshotPictureBytes),
}

/// A snapshot picture's bytes: `data` or `parts`, exactly one (PAR-21), at
/// most 262,144 bytes once decoded and written, as `description.picture`'s
/// (PAR-26); for NAPLPS, its objects.
#[derive(Clone, PartialEq, Eq, Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = picture::data_or_parts)]
pub struct SnapshotPictureBytes {
    /// Its language.
    pub language: PictureLanguageName,
    /// Its bytes, exact base64.
    #[serde(default)]
    pub data: Option<Base64>,
    /// Its bytes as parts, in place of `data` (PAR-21).
    #[serde(default)]
    #[schemars(with = "Option<picture::Parts>")]
    pub parts: Option<Vec<PicturePart>>,
    /// For NAPLPS, its objects, of which the shown ones are composed (PAR-26);
    /// null for none.
    #[serde(default)]
    #[schemars(length(max = super::MOST_NAPLPS_OBJECTS))]
    pub objects: Option<Vec<NaplpsObject>>,
}
