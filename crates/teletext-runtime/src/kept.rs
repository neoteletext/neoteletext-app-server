//! Kept pictures (neoteletext-common `docs/renderer-parity.md` PAR-16 to
//! PAR-19, PAR-25; REQ-6766, REQ-6769): what a program sends to the
//! audience `"kept"` of a page and sub-code (DEC-6599: a page is one page
//! number and one sub-code together), held so that every connection that
//! selects that page later is given it, from the beginning of its stream.
//!
//! A kept picture is page content, as a teletext page's rows are: its
//! stream, its NAPLPS objects (the definitions, never their animation),
//! its links and its cursor. Animation, the pointer's looks and a rule
//! table's state are a viewer's local state and are never kept (DEC-6587
//! item 3, PAR-18).
//!
//! What is kept is bounded and refused at its bounds, never evicted
//! (PAR-17): a program that needs room forgets a picture.
//!
//! **Readers** (TODO-6836, TODO-6837). Each kept picture's stream is read
//! by the receiver's reader as it comes, and the reader is kept to read
//! the next append. Readers are counted at what they hold
//! (`Checker::held_bytes`: each reader as its renderer reports it,
//! measured again after every append it takes). When holding one would pass `MOST_KEPT_COST`, the
//! readers of the pictures least recently written are let go; and a
//! reader that refused an append, having read bytes that are not kept, is
//! let go too. A picture without its reader is still kept and shown, but
//! takes no append until the program begins it again with `restart`: the
//! server never reads a kept stream again, which would cost what every
//! message of it cost together.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use neoteletext_core::glyphs::{JOINED_SETS, JoiningClass, joining};

use crate::protocol::{DrawingLink, NaplpsObject, NaplpsTitle, Rule};

/// The bytes of one kept stream since its restart (PAR-17): the most a
/// static channel's picture file carries too, by the engine's one
/// constant.
pub const MOST_KEPT_STREAM_BYTES: usize = neoteletext_formats::picture_text::MAX_PICTURE_STREAM_BYTES;
/// The pages and sub-codes with a kept picture, for one channel (PAR-17).
pub const MOST_KEPT_PICTURES: usize = 512;
/// The cost of all of a channel's kept pictures (PAR-17): each stream's
/// bytes, its reader's memory as `Checker::held_bytes` states it, and each
/// companion's message bytes as received: 32 MiB for one channel, the
/// figure DEC-6842 set.
pub const MOST_KEPT_COST: usize = 33_554_432;
/// The memory of the readers of every stream a program sends to
/// connections themselves (PAR-23), each as `Checker::held_bytes`
/// states it after the message it read, with the `ReaderFonts` they
/// share counted once, for all of a channel's connections together; a
/// message that would pass it is refused (TODO-6836, TODO-6845).
///
/// Derived for the native host (TODO-6895): at least
/// [`CONNECTIONS_WITH_OWN_PICTURES`] connections of one channel must each
/// be able to hold its own stream of an ordinary page. Measured on 19
/// November 2026 (`x86_64` Linux, the renderers' `held_bytes` with the
/// reader's box): an ordinary NAPLPS page's reader holds 1,160 bytes (a
/// page of text, the corpus's and the samples' median), the median ReGIS
/// demonstration page's 15,151 and the largest ordinary one's 60,927. The
/// median ReGIS page sets the figure: [`ORDINARY_READER_BYTES`] for each of
/// the connections is 50,000 times 16,384, 819,200,000 bytes, rounded up
/// to a gibibyte, within which about 70,800 median ReGIS readers fit, the
/// channel's every connection (`MOST_VIEWERS_PER_CHANNEL`) with an
/// ordinary NAPLPS one, or 17,600 of the largest ordinary ReGIS page. The
/// budget's basis: a native host serving 50,000 viewers of one channel
/// gives that channel about two gigabytes, the readers' gibibyte and about
/// 13 to 16 KB of each connection's own state (measured at 10,000 and
/// 30,000 connections with `teletext-load`).
pub const MOST_CONNECTION_READER_BYTES: usize = 1_073_741_824;
/// The connections of one channel, each with its own stream of an
/// ordinary page, that [`MOST_CONNECTION_READER_BYTES`] must hold on the
/// native host (TODO-6895).
pub const CONNECTIONS_WITH_OWN_PICTURES: usize = 50_000;
/// What an ordinary page's reader is allowed in deriving
/// [`MOST_CONNECTION_READER_BYTES`]: the median ReGIS demonstration page's
/// 15,151 bytes, measured, rounded up to 16 KiB; an ordinary NAPLPS
/// page's is 1,160.
pub const ORDINARY_READER_BYTES: usize = 16_384;
const _: () = assert!(CONNECTIONS_WITH_OWN_PICTURES * ORDINARY_READER_BYTES <= MOST_CONNECTION_READER_BYTES);
/// The least time between two fallback drawings of one picture (PAR-25).
pub const FALLBACK_INTERVAL_MS: u64 = 500;
/// The fallback drawings of one channel a second, sustained (PAR-25).
pub const MOST_FALLBACK_RENDERS_PER_SECOND: u32 = 20;
/// The stream bytes a channel's fallback drawings may draw in a second,
/// each drawing counted as its stream's bytes and
/// `FALLBACK_DRAWING_FLOOR_BYTES` (TODO-6839). Measured on 19 November
/// 2026 (release build, `x86_64` Linux, chunko): one ReGIS drawing of a
/// 1 MiB stream of whole-screen fills takes 89 ms, at the renderer's
/// `WORK_CAP`, and with numbered drawings composed up to
/// `MAX_REBUILD_WORK` 98 ms; twenty of those a second would take two
/// processor cores. Two mebibytes a second hold a channel's ReGIS
/// fallbacks to about a quarter of one core. A NAPLPS drawing is counted
/// by the pixel work it spends instead ([`NAPLPS_WORK_PER_FALLBACK_BYTE`]
/// units a byte; TODO-6845, the NAPLPS renderer's DEC-6851).
pub const MOST_FALLBACK_BYTES_PER_SECOND: usize = 2_097_152;
/// What a fallback drawing is counted as besides its stream's bytes: the
/// composition of its numbered drawings, which a short stream can make as
/// dear as a long one's drawing (measured 19 November 2026: 100 numbered
/// drawings of whole-screen fills, 4,800 bytes of stream, drawn and
/// composed in 19 ms, what about 220 KB of base fills cost), rounded up
/// to a quarter of a mebibyte.
pub const FALLBACK_DRAWING_FLOOR_BYTES: usize = 262_144;
/// The NAPLPS renderer's pixel work units (DEC-6851) one byte of
/// `MOST_FALLBACK_BYTES_PER_SECOND` stands for, so that a NAPLPS fallback
/// drawing is counted by the work it spends at about what ReGIS's bytes
/// cost in time (TODO-6845). Measured 19 November 2026 (release build,
/// `x86_64` Linux, chunko): one work unit costs about 12.5 ns
/// (DEC-6851's measurement), and the costliest ReGIS megabyte about 98 ms,
/// some 93 ns a byte; 93 / 12.5 is 7.5, rounded up to 8, so that the
/// budget's two mebibytes a second hold a channel's NAPLPS fallbacks too
/// to about a quarter of one core (16,777,216 units, about 210 ms).
pub const NAPLPS_WORK_PER_FALLBACK_BYTE: u64 = 8;
/// The bytes of per-connection streams kept for fallbacks, for all of a
/// channel's connections together (PAR-25).
pub const MOST_FALLBACK_POOL_BYTES: usize = 16_777_216;

/// What a box holding a reader costs besides the reader's own figure: the
/// allocator's bookkeeping of one allocation, as both renderers count it
/// (their `limits::ALLOCATION_OVERHEAD_BYTES`, asserted equal).
const READER_BOX_BYTES: usize = regis::limits::ALLOCATION_OVERHEAD_BYTES;
const _: () = assert!(regis::limits::ALLOCATION_OVERHEAD_BYTES == naplps::limits::ALLOCATION_OVERHEAD_BYTES);

// One reader at the most a ReGIS renderer says it can hold (its
// `MAX_HELD_BYTES`, DEC-6850) and a whole stream fit `MOST_KEPT_COST`
// alone, so every stream within `MOST_KEPT_STREAM_BYTES` can be kept.
const _: () = assert!(regis::limits::MAX_HELD_BYTES + READER_BOX_BYTES + MOST_KEPT_STREAM_BYTES <= MOST_KEPT_COST);
// The same for NAPLPS without the stroke font and joined sets it is given,
// whose figures are known only once they are built (the renderer's
// `MAX_HELD_BYTES`, TODO-6854); `kept::tests` checks it with them.
const _: () = assert!(naplps::limits::MAX_HELD_BYTES + READER_BOX_BYTES + MOST_KEPT_STREAM_BYTES <= MOST_KEPT_COST);

/// A picture language.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PictureLanguage {
    /// ReGIS (DEC-6335).
    Regis,
    /// NAPLPS (the NAPLPS renderer's REQ-6408).
    Naplps,
}

impl PictureLanguage {
    /// Its name, for a refusal's detail.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Regis => "ReGIS",
            Self::Naplps => "NAPLPS",
        }
    }
}

/// A stream read as the receiver reads it: the receiver's own reader of
/// its language, which refuses what the receiver would refuse (REQ-6332,
/// PAR-23).
pub(crate) enum Checker {
    /// `regis::Validator`.
    Regis(alloc::boxed::Box<regis::Validator>),
    /// `naplps::Validator`.
    Naplps(alloc::boxed::Box<naplps::Validator>),
}

/// Why a checker refused bytes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct CheckRefusal {
    /// `invalid_regis` or `invalid_naplps`.
    pub rule: Rule,
    /// What the receiver would refuse, and where.
    pub detail: String,
}

/// What a checker found in bytes it took: lines for the author's log.
pub(crate) type Notes = Vec<String>;

impl Checker {
    /// A new one, for a stream begun again, given the channel's shared
    /// `fonts`.
    pub(crate) fn new(language: PictureLanguage, fonts: &ReaderFonts) -> Self {
        match language {
            PictureLanguage::Regis => Self::Regis(alloc::boxed::Box::new(regis::Validator::new())),
            PictureLanguage::Naplps => Self::Naplps(alloc::boxed::Box::new(fonts.naplps_validator())),
        }
    }

    /// The memory it holds, in bytes (TODO-6845, TODO-6860): each reader as
    /// its renderer reports it, which grows and shrinks with what the stream
    /// defines, and the box it is kept in. A ReGIS reader is
    /// `regis::Validator::held_bytes` (DEC-6850); a NAPLPS reader
    /// `naplps::Validator::held_bytes` (DEC-6858), never more than the
    /// renderer's `MAX_HELD_BYTES`; the stroke font and the joined sets it
    /// is given are the channel's [`ReaderFonts`], shared by every reader
    /// and counted once by whoever totals the readers (TODO-6895). Callers
    /// ask again after every message the reader reads.
    pub(crate) fn held_bytes(&self) -> usize {
        match self {
            Self::Regis(validator) => validator.held_bytes().saturating_add(READER_BOX_BYTES),
            Self::Naplps(validator) => validator.held_bytes().saturating_add(READER_BOX_BYTES),
        }
    }

    /// The pixel work the stream's drawing spends on the receiver's
    /// canvas (DEC-6851); none for ReGIS.
    pub(crate) fn work_spent(&self) -> Option<u64> {
        match self {
            Self::Regis(_) => None,
            Self::Naplps(validator) => Some(validator.work_spent()),
        }
    }

    /// Reads `bytes` on from what it has read.
    ///
    /// # Errors
    /// For ReGIS a commit that fails, with its code and drawing; for NAPLPS
    /// any refusal of the receiver's reader (PAR-23), named by
    /// `Refusal::name` with the stream offset of the first. Bytes passed
    /// over and characters written back over their neighbour in colour
    /// mode 2 are no refusal, and are noted for the author's log.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Notes, CheckRefusal> {
        match self {
            Self::Regis(validator) => {
                for event in validator.feed_regis(bytes) {
                    if let regis::Event::CommitFailed { error, drawing } = event {
                        // DEC-6850: code 8 is the scene's drawings passing
                        // the renderer's bound on their records, which the
                        // author can act on only if it is named.
                        let why = if matches!(error, regis::CommitError::Drawings) {
                            format!(
                                ": the scene's numbered drawings would pass MAX_DRAWING_RECORD_BYTES ({}), about {} drawings",
                                regis::limits::MAX_DRAWING_RECORD_BYTES,
                                regis::limits::MOST_DRAWINGS_HELD
                            )
                        } else {
                            String::new()
                        };
                        return Err(CheckRefusal {
                            rule: Rule::InvalidRegis,
                            detail: format!("a commit fails with code {} ({error:?}), drawing {drawing}{why}", error.code()),
                        });
                    }
                }
                Ok(Vec::new())
            }
            Self::Naplps(validator) => {
                let before = *validator.diagnostics();
                let _ = validator.feed(bytes);
                let after = *validator.diagnostics();
                if after.refused() > before.refused() {
                    let names: Vec<&str> =
                        naplps::Refusal::ALL.into_iter().filter(|&kind| after.count(kind) > before.count(kind)).map(naplps::Refusal::name).collect();
                    let offset = after.first_refused.map_or_else(|| String::from("unknown"), |offset| format!("{offset}"));
                    // DEC-6851: the work limit stops the receiver's drawing
                    // for good, so the allowance is named for the author.
                    let allowance = if after.count(naplps::Refusal::WorkLimit) > before.count(naplps::Refusal::WorkLimit) {
                        format!(
                            "; the stream's drawing passed its pixel work allowance, WORK_CAP ({}) and WORK_PER_BYTE ({}) a byte, and the receiver draws none of the rest",
                            naplps::limits::WORK_CAP,
                            naplps::limits::WORK_PER_BYTE
                        )
                    } else {
                        String::new()
                    };
                    return Err(CheckRefusal {
                        rule: Rule::InvalidNaplps,
                        detail: format!("{}; the first at stream offset {offset}{allowance}", names.join(", ")),
                    });
                }
                let mut notes = Vec::new();
                let passed = after.passed_over.saturating_sub(before.passed_over);
                if passed > 0 {
                    let first = after.first_passed_over.map_or_else(|| String::from("unknown"), |offset| format!("{offset}"));
                    notes.push(format!("NAPLPS: {passed} bytes had no effect and were passed over; the stream's first at offset {first}"));
                }
                // PAR-23, PAR-5: in colour mode 2 a character's field is
                // filled with the background (ANSI X3.110-1983 section
                // 5.3.2.6.3), so a mark written back over its letter erases
                // the letter. The author is told; the bytes are not refused.
                let overwritten = after.overwritten_in_mode_two.saturating_sub(before.overwritten_in_mode_two);
                if overwritten > 0 {
                    let first = after.first_overwritten_in_mode_two.map_or_else(|| String::from("unknown"), |offset| format!("{offset}"));
                    notes.push(format!(
                        "NAPLPS: {overwritten} characters were written back over the one before in colour mode 2, which erases it, as a text part's mark does; the stream's first at offset {first}"
                    ));
                }
                Ok(notes)
            }
        }
    }

    /// The NAPLPS macros its stream has defined, by code less 0x20; none
    /// for ReGIS.
    pub(crate) fn defined_macros(&self) -> Option<[bool; MACRO_CODES]> {
        match self {
            Self::Regis(_) => None,
            Self::Naplps(validator) => Some(validator.defined_macros()),
        }
    }
}

/// What a channel's NAPLPS readers are given, built once and shared by
/// every one of them (TODO-6895): the receiver's stroke font, whose
/// strokes' pixel work a reader counts, and the joined sets 44 to 51 with
/// their joining data, which decide when a held letter is drawn (DEC-6851,
/// TODO-6845). Only a set's joining data matters to a validator, so each
/// joined set is a stand-in of one-pixel glyphs. Each reader held its own
/// copy before, 20,208 of an ordinary page's 32,568 bytes. The font and the
/// sets are this build's, so a refusal of them is a fault of the build and
/// leaves the readers without them, which counts the stream's end the same.
#[derive(Clone)]
pub(crate) struct ReaderFonts {
    font: Option<Arc<naplps::StrokeFont>>,
    joined: Vec<(u8, Arc<naplps::BitmapSet>)>,
}

impl Default for ReaderFonts {
    fn default() -> Self {
        let font = naplps::StrokeFont::parse(naplps_fonts::NEOTELETEXT).ok().map(Arc::new);
        let joined = JOINED_SETS
            .into_iter()
            .filter_map(|number| {
                let mut set = naplps::BitmapSet::new(1, 1).ok()?;
                for code in FIRST_MACRO_CODE..=LAST_SET_CODE {
                    let Some(joined) = joining(number, code) else { continue };
                    let kind = match joined.class {
                        JoiningClass::None => naplps::JoiningType::None,
                        JoiningClass::Right => naplps::JoiningType::Right,
                        JoiningClass::Dual => naplps::JoiningType::Dual,
                        JoiningClass::Transparent => naplps::JoiningType::Transparent,
                    };
                    let _ = set.set_joining(code, kind);
                }
                Some((number, Arc::new(set)))
            })
            .collect();
        Self { font, joined }
    }
}

impl ReaderFonts {
    /// A NAPLPS reader as the receiver's is: on the receiver's 480 by 460
    /// canvas (`Validator::new`, RAS-11), given the shared font and sets.
    pub(crate) fn naplps_validator(&self) -> naplps::Validator {
        let mut validator = naplps::Validator::new();
        if let Some(font) = &self.font {
            validator.set_font(Arc::clone(font));
        }
        for (number, set) in &self.joined {
            let _ = validator.set_teletext_set(*number, Arc::clone(set));
        }
        validator
    }

    /// What the font and the sets hold, counted once for every reader that
    /// shares them: each as its renderer states it, in its shared
    /// allocation.
    pub(crate) fn held_bytes(&self) -> usize {
        let shared = |held: usize| held.saturating_add(ARC_COUNT_BYTES).saturating_add(READER_BOX_BYTES);
        self.joined.iter().map(|(_, set)| shared(set.held_bytes())).fold(self.font.as_ref().map_or(0, |font| shared(font.held_bytes())), usize::saturating_add)
    }
}

/// The two reference counts an `Arc`'s allocation holds before its value.
const ARC_COUNT_BYTES: usize = 2 * size_of::<usize>();

/// The last code of a provided set (REQ-6668).
const LAST_SET_CODE: u8 = 0x7F;

/// The NAPLPS macro names: the 96 codes 2/0 to 7/15 (ANSI X3.110-1983
/// section 6.2).
pub(crate) const MACRO_CODES: usize = 96;
/// The first macro code, 2/0.
pub(crate) const FIRST_MACRO_CODE: u8 = 0x20;

/// The objects kept with a NAPLPS picture: the last whole set, with each
/// later `one` redefinition applied, and the title.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct KeptObjects {
    pub objects: Vec<NaplpsObject>,
    pub title: Option<NaplpsTitle>,
}

/// One kept picture (PAR-16).
pub(crate) struct KeptPicture {
    pub language: PictureLanguage,
    /// Every byte since its last restart, in order.
    pub stream: Vec<u8>,
    /// The receiver's reader, having read `stream`; none once it was let
    /// go, and then why.
    checker: Result<Checker, Released>,
    /// What `checker` holds, as it said after the last bytes it took
    /// (TODO-6845); its renderer's figure is not asked for again until it
    /// reads more.
    reader_held: usize,
    /// When it was last written, by [`KeptPictures`]'s count of writes: the
    /// pictures written longest ago let their readers go first.
    written: u64,
    /// The NAPLPS macros `stream` defines, as its reader last reported.
    defined: Option<[bool; MACRO_CODES]>,
    pub objects: Option<KeptObjects>,
    /// Each companion's message bytes as received, for the cost.
    objects_cost: usize,
    pub links: Option<Vec<DrawingLink>>,
    links_cost: usize,
    pub cursor: Option<u16>,
    cursor_cost: usize,
    /// Moves on with every change, for the fallback.
    pub version: u64,
    /// The objects' generation was logged as naming an undefined macro.
    pub undefined_macro_logged: bool,
}

impl KeptPicture {
    fn new(language: PictureLanguage, fonts: &ReaderFonts) -> Self {
        Self::with_checker(language, Checker::new(language, fonts))
    }

    /// An empty one whose reader is `checker`.
    fn with_checker(language: PictureLanguage, checker: Checker) -> Self {
        Self {
            language,
            stream: Vec::new(),
            reader_held: checker.held_bytes(),
            checker: Ok(checker),
            written: 0,
            defined: None,
            objects: None,
            objects_cost: 0,
            links: None,
            links_cost: 0,
            cursor: None,
            cursor_cost: 0,
            version: 0,
            undefined_macro_logged: false,
        }
    }

    /// What its companions cost.
    const fn companions_cost(&self) -> usize {
        self.objects_cost.saturating_add(self.links_cost).saturating_add(self.cursor_cost)
    }

    /// What its reader is counted as holding: none once it is let go.
    const fn reader_cost(&self) -> usize {
        if self.checker.is_ok() { self.reader_held } else { 0 }
    }

    /// What it costs besides its reader.
    const fn content_cost(&self) -> usize {
        self.stream.len().saturating_add(self.companions_cost())
    }

    /// What it costs (PAR-17, TODO-6836).
    const fn cost(&self) -> usize {
        self.content_cost().saturating_add(self.reader_cost())
    }

    /// The NAPLPS macros its stream has defined.
    pub(crate) const fn defined_macros(&self) -> Option<[bool; MACRO_CODES]> {
        self.defined
    }

    /// The pixel work its NAPLPS stream's drawing spends on the receiver's
    /// canvas, as its reader counted it (DEC-6851); none for ReGIS or once
    /// its reader was let go.
    pub(crate) fn work_spent(&self) -> Option<u64> {
        self.checker.as_ref().ok().and_then(Checker::work_spent)
    }
}

/// Why a kept picture's reader was let go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Released {
    /// It refused an append, having read part of it.
    Refused,
    /// Its room was wanted under `MOST_KEPT_COST` by a picture written
    /// later.
    ForRoom,
}

/// Why something cannot be kept.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct KeptRefusal {
    /// The field at fault.
    pub path: &'static str,
    pub rule: Rule,
    pub detail: String,
}

impl KeptRefusal {
    fn over(path: &'static str, constant: &str, value: usize) -> Self {
        Self { path, rule: Rule::KeptOverLimit, detail: format!("{constant} ({value}) would be passed") }
    }
}

/// The refusal of bytes after which a kept picture's reader would hold
/// `held` bytes, past `MOST_KEPT_COST` with the rest (TODO-6845).
fn reader_over(held: usize) -> KeptRefusal {
    KeptRefusal {
        path: "data",
        rule: Rule::KeptOverLimit,
        detail: format!("MOST_KEPT_COST ({MOST_KEPT_COST}) would be passed: the picture's reader would hold {held} bytes for what its stream defines"),
    }
}

/// What a kept stream message changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StreamChange {
    /// A new kept generation: everything of before is gone.
    Restarted,
    /// Bytes added to the end.
    Appended,
}

/// A companion of a kept picture.
pub(crate) enum Companion {
    /// `naplps_objects`: a whole set, or with `one` a redefinition.
    Objects { objects: Vec<NaplpsObject>, title: Option<NaplpsTitle>, one: bool },
    /// `regis_links` or `naplps_links`.
    Links(Vec<DrawingLink>),
    /// `regis_cursor` or `naplps_cursor`.
    Cursor(u16),
}

/// A channel's kept pictures, by page and sub-code.
#[derive(Default)]
pub(crate) struct KeptPictures {
    pictures: BTreeMap<(u16, u16), KeptPicture>,
    /// Counts the writes, to order the pictures by their last.
    writes: u64,
    /// What every reader of the channel is given, shared (TODO-6895).
    fonts: ReaderFonts,
}

impl KeptPictures {
    /// The font and sets every reader of the channel shares.
    pub(crate) const fn fonts(&self) -> &ReaderFonts {
        &self.fonts
    }

    /// The kept picture of a page and sub-code.
    pub(crate) fn get(&self, page: u16, sub: u16) -> Option<&KeptPicture> {
        self.pictures.get(&(page, sub))
    }

    /// What all of them cost.
    pub(crate) fn cost(&self) -> usize {
        self.pictures.values().fold(0_usize, |sum, picture| sum.saturating_add(picture.cost()))
    }

    /// What every picture but `key`'s costs besides its reader: the least
    /// they can be made to cost, by letting their readers go.
    fn others_content_cost(&self, key: (u16, u16)) -> usize {
        self.pictures.iter().filter(|(other, _)| **other != key).fold(0_usize, |sum, (_, picture)| sum.saturating_add(picture.content_cost()))
    }

    /// The readers held, for the tests.
    #[cfg(test)]
    pub(crate) fn readers_held(&self) -> usize {
        self.pictures.values().filter(|picture| picture.checker.is_ok()).count()
    }

    /// `key`'s picture was just written: it is the most recent, and the
    /// readers of the pictures written longest ago are let go until the
    /// whole cost is within `MOST_KEPT_COST` (TODO-6836).
    fn written(&mut self, key: (u16, u16)) {
        self.writes = self.writes.wrapping_add(1);
        let writes = self.writes;
        if let Some(picture) = self.pictures.get_mut(&key) {
            picture.written = writes;
        }
        while self.cost() > MOST_KEPT_COST {
            let oldest = self
                .pictures
                .iter()
                .filter(|(other, picture)| **other != key && picture.checker.is_ok())
                .min_by_key(|(_, picture)| picture.written)
                .map(|(other, _)| *other);
            let Some(picture) = oldest.and_then(|other| self.pictures.get_mut(&other)) else { break };
            picture.checker = Err(Released::ForRoom);
        }
    }

    /// The kept picture of `page` and `sub` in `language`, made empty if
    /// there is none and room for one.
    fn entry(&mut self, page: u16, sub: u16, language: PictureLanguage, restart: bool, path: &'static str) -> Result<&mut KeptPicture, KeptRefusal> {
        if let Some(picture) = self.pictures.get(&(page, sub))
            && picture.language != language
            && !restart
        {
            return Err(KeptRefusal {
                path: "type",
                rule: Rule::WrongPictureLanguage,
                detail: format!("the kept picture of {page:03X}/{sub:04X} is {}; begin it again with restart", picture.language.name()),
            });
        }
        if !self.pictures.contains_key(&(page, sub)) && self.pictures.len() >= MOST_KEPT_PICTURES {
            return Err(KeptRefusal::over(path, "MOST_KEPT_PICTURES", MOST_KEPT_PICTURES));
        }
        let fonts = &self.fonts;
        let picture = self.pictures.entry((page, sub)).or_insert_with(|| KeptPicture::new(language, fonts));
        if restart || picture.language != language {
            *picture = KeptPicture::new(language, fonts);
        }
        Ok(picture)
    }

    /// Bytes of a kept stream (PAR-16, PAR-17): appended, or with
    /// `restart` a new kept generation. A stream is counted by its bytes
    /// and its reader's memory.
    ///
    /// # Errors
    /// `wrong_picture_language`, a bound of PAR-17, or the receiver's
    /// reader's refusal; nothing is kept then. Once its reader has refused
    /// an append, a kept picture refuses every append until it is begun
    /// again with `restart` (TODO-6837).
    pub(crate) fn stream(&mut self, page: u16, sub: u16, language: PictureLanguage, restart: bool, bytes: &[u8]) -> Result<(StreamChange, Notes), KeptRefusal> {
        let key = (page, sub);
        let existing = self.pictures.get(&key);
        let kept_before = existing.filter(|picture| !restart && picture.language == language);
        let length = kept_before.map_or(0, |picture| picture.stream.len()).saturating_add(bytes.len());
        if length > MOST_KEPT_STREAM_BYTES {
            return Err(KeptRefusal::over("data", "MOST_KEPT_STREAM_BYTES", MOST_KEPT_STREAM_BYTES));
        }
        // The others' readers may be let go for room; this one's reader is
        // needed to read the bytes, and is counted at what it holds before
        // and again after it reads them (TODO-6845).
        let companions = kept_before.map_or(0, KeptPicture::companions_cost);
        let without_reader = self.others_content_cost(key).saturating_add(length).saturating_add(companions);
        let over = |held: usize| without_reader.saturating_add(held) > MOST_KEPT_COST;
        // A restart refused by its bytes must keep what was kept: it is
        // read on a checker of its own first.
        if restart || existing.is_none() {
            let mut checker = Checker::new(language, &self.fonts);
            if over(checker.held_bytes()) {
                return Err(KeptRefusal::over("data", "MOST_KEPT_COST", MOST_KEPT_COST));
            }
            let notes = checker.feed(bytes).map_err(|refused| KeptRefusal { path: "data", rule: refused.rule, detail: refused.detail })?;
            let held = checker.held_bytes();
            if over(held) {
                return Err(reader_over(held));
            }
            let picture = self.entry(page, sub, language, true, "data")?;
            picture.defined = checker.defined_macros();
            picture.reader_held = held;
            picture.checker = Ok(checker);
            picture.stream = bytes.to_vec();
            picture.version = picture.version.wrapping_add(1);
            self.written(key);
            return Ok((StreamChange::Restarted, notes));
        }
        if over(kept_before.map_or(0, |picture| picture.reader_held)) {
            return Err(KeptRefusal::over("data", "MOST_KEPT_COST", MOST_KEPT_COST));
        }
        let picture = self.entry(page, sub, language, false, "data")?;
        let checker = match picture.checker.as_mut() {
            Ok(checker) => checker,
            Err(released) => {
                let why = match released {
                    Released::Refused => "its reader refused an earlier append",
                    Released::ForRoom => "its reader was let go to make room under MOST_KEPT_COST for pictures written since",
                };
                return Err(KeptRefusal {
                    path: "restart",
                    rule: Rule::KeptOverLimit,
                    detail: format!("the kept picture of {page:03X}/{sub:04X} takes no append, since {why}; begin it again with restart"),
                });
            }
        };
        match checker.feed(bytes) {
            Ok(notes) => {
                let held = checker.held_bytes();
                if over(held) {
                    // As for a refusal: the reader has read bytes that are
                    // not kept, and is let go (TODO-6837).
                    picture.checker = Err(Released::Refused);
                    return Err(reader_over(held));
                }
                picture.defined = checker.defined_macros();
                picture.reader_held = held;
                picture.stream.extend_from_slice(bytes);
                picture.version = picture.version.wrapping_add(1);
                self.written(key);
                Ok((StreamChange::Appended, notes))
            }
            Err(refused) => {
                // The reader has read bytes that are not kept: it no longer
                // reads as a receiver given the kept stream would, and is
                // let go rather than read again (TODO-6837).
                picture.checker = Err(Released::Refused);
                Err(KeptRefusal { path: "data", rule: refused.rule, detail: refused.detail })
            }
        }
    }

    /// A companion of a kept picture (PAR-16): its objects, links or
    /// cursor, `line_bytes` the message's line as received, which is its
    /// cost. A companion of a page with nothing kept begins an empty kept
    /// picture of its language.
    ///
    /// # Errors
    /// `wrong_picture_language`, or a bound of PAR-17.
    pub(crate) fn companion(&mut self, page: u16, sub: u16, language: PictureLanguage, companion: Companion, line_bytes: usize) -> Result<(), KeptRefusal> {
        let (path, before) = {
            let existing = self.pictures.get(&(page, sub)).filter(|picture| picture.language == language);
            match &companion {
                Companion::Objects { .. } => ("objects", existing.map_or(0, |picture| picture.objects_cost)),
                Companion::Links(_) => ("links", existing.map_or(0, |picture| picture.links_cost)),
                Companion::Cursor(_) => ("cursor", existing.map_or(0, |picture| picture.cursor_cost)),
            }
        };
        // A `one` adds to the objects' cost; a whole set replaces it.
        let one = matches!(companion, Companion::Objects { one: true, .. });
        let key = (page, sub);
        let own = self.pictures.get(&key).filter(|picture| picture.language == language);
        let companion_after = if one { before.saturating_add(line_bytes) } else { line_bytes };
        let after =
            own.map_or_else(|| Checker::new(language, &self.fonts).held_bytes(), KeptPicture::cost).saturating_sub(before).saturating_add(companion_after);
        if self.others_content_cost(key).saturating_add(after) > MOST_KEPT_COST {
            return Err(KeptRefusal::over(path, "MOST_KEPT_COST", MOST_KEPT_COST));
        }
        let picture = self.entry(page, sub, language, false, path)?;
        match companion {
            Companion::Objects { objects, title, one: true } => {
                let kept = picture.objects.get_or_insert_with(KeptObjects::default);
                for object in objects {
                    match kept.objects.iter_mut().find(|held| held.number == object.number) {
                        Some(held) => *held = object,
                        None => kept.objects.push(object),
                    }
                }
                // ANIM-11: `one` keeps the title.
                let _ = title;
                picture.objects_cost = picture.objects_cost.saturating_add(line_bytes);
            }
            Companion::Objects { objects, title, one: false } => {
                picture.objects = Some(KeptObjects { objects, title });
                picture.objects_cost = line_bytes;
                picture.undefined_macro_logged = false;
            }
            Companion::Links(links) => {
                picture.links = Some(links);
                picture.links_cost = line_bytes;
            }
            Companion::Cursor(number) => {
                picture.cursor = Some(number);
                picture.cursor_cost = line_bytes;
            }
        }
        picture.version = picture.version.wrapping_add(1);
        self.written(key);
        Ok(())
    }

    /// Marks a kept NAPLPS picture's objects as logged for naming a macro
    /// its stream has not defined.
    pub(crate) fn mark_logged(&mut self, page: u16, sub: u16) {
        if let Some(picture) = self.pictures.get_mut(&(page, sub)) {
            picture.undefined_macro_logged = true;
        }
    }

    /// Drops the kept picture of `page` and `sub` (PAR-19).
    ///
    /// # Errors
    /// `not_kept` when nothing is kept there.
    pub(crate) fn forget(&mut self, page: u16, sub: u16) -> Result<KeptPicture, KeptRefusal> {
        self.pictures.remove(&(page, sub)).ok_or_else(|| KeptRefusal {
            path: "sub",
            rule: Rule::NotKept,
            detail: format!("nothing is kept of {page:03X}/{sub:04X}"),
        })
    }

    /// Drops every kept picture: the program ended or started again.
    pub(crate) fn clear(&mut self) {
        self.pictures.clear();
    }
}

/// The macros `objects` name in their frames or looks that `defined` does not hold
/// (PAR-23: logged, never refused, since a macro may come later).
pub(crate) fn undefined_macros(objects: &[NaplpsObject], defined: &[bool; MACRO_CODES]) -> Vec<u8> {
    let mut missing: Vec<u8> = Vec::new();
    for object in objects {
        let looks = [object.hover, object.pressed, object.alternative];
        for code in object.frames.iter().chain(looks.iter().flatten()).map(|code| code.get()) {
            let known = code.checked_sub(FIRST_MACRO_CODE).and_then(|index| defined.get(usize::from(index))).copied().unwrap_or(false);
            if !known && !missing.contains(&code) {
                missing.push(code);
            }
        }
    }
    missing
}

#[cfg(test)]
pub(crate) mod tests;
