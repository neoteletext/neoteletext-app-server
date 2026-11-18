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

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::protocol::{DrawingLink, NaplpsObject, NaplpsTitle, Rule};

/// The bytes of one kept stream since its restart (PAR-17): the most a
/// static channel's picture file carries too, by the engine's one
/// constant.
pub const MOST_KEPT_STREAM_BYTES: usize = neoteletext_formats::picture_text::MAX_PICTURE_STREAM_BYTES;
/// The pages and sub-codes with a kept picture, for one channel (PAR-17).
pub const MOST_KEPT_PICTURES: usize = 512;
/// The cost of all of a channel's kept pictures (PAR-17): each stream's
/// bytes counted twice, for the bytes and its validator's copy of what it
/// holds, plus each companion's message bytes as received. A quarter of
/// the Cloudflare host's 128 MB for one channel.
pub const MOST_KEPT_COST: usize = 33_554_432;
/// The least time between two fallback drawings of one picture (PAR-25).
pub const FALLBACK_INTERVAL_MS: u64 = 500;
/// The fallback drawings of one channel a second, sustained (PAR-25).
pub const MOST_FALLBACK_RENDERS_PER_SECOND: u32 = 20;
/// The bytes of per-connection streams kept for fallbacks, for all of a
/// channel's connections together (PAR-25).
pub const MOST_FALLBACK_POOL_BYTES: usize = 16_777_216;

/// What a stream's validator holds besides the bytes is at most the bytes
/// (PAR-17's reason), so a stream costs twice its length.
const STREAM_COST_FACTOR: usize = 2;

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
    /// A new one, for a stream begun again.
    pub(crate) fn new(language: PictureLanguage) -> Self {
        match language {
            PictureLanguage::Regis => Self::Regis(alloc::boxed::Box::new(regis::Validator::new())),
            PictureLanguage::Naplps => Self::Naplps(alloc::boxed::Box::new(naplps::Validator::new())),
        }
    }

    /// Reads `bytes` on from what it has read.
    ///
    /// # Errors
    /// For ReGIS a commit that fails, with its code and drawing; for NAPLPS
    /// any refusal of the receiver's reader (PAR-23), named by
    /// `Refusal::name` with the stream offset of the first. Bytes passed
    /// over are no refusal, and are noted for the author's log.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Notes, CheckRefusal> {
        match self {
            Self::Regis(validator) => {
                for event in validator.feed_regis(bytes) {
                    if let regis::Event::CommitFailed { error, drawing } = event {
                        return Err(CheckRefusal {
                            rule: Rule::InvalidRegis,
                            detail: format!("a commit fails with code {} ({error:?}), drawing {drawing}", error.code()),
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
                    return Err(CheckRefusal { rule: Rule::InvalidNaplps, detail: format!("{}; the first at stream offset {offset}", names.join(", ")) });
                }
                let passed = after.passed_over.saturating_sub(before.passed_over);
                if passed == 0 {
                    return Ok(Vec::new());
                }
                let first = after.first_passed_over.map_or_else(|| String::from("unknown"), |offset| format!("{offset}"));
                Ok(alloc::vec![format!("NAPLPS: {passed} bytes had no effect and were passed over; the stream's first at offset {first}")])
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
    /// The length of each message's bytes, in order: the pieces the
    /// checker read, so that it can be read again as it was.
    pieces: Vec<usize>,
    checker: Checker,
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
    fn new(language: PictureLanguage) -> Self {
        Self {
            language,
            stream: Vec::new(),
            pieces: Vec::new(),
            checker: Checker::new(language),
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

    /// What it costs (PAR-17).
    fn cost(&self) -> usize {
        self.stream.len().saturating_mul(STREAM_COST_FACTOR).saturating_add(self.objects_cost).saturating_add(self.links_cost).saturating_add(self.cursor_cost)
    }

    /// Its checker read from the start again, after a refusal left it
    /// having read bytes that are not kept.
    fn reread(&mut self) {
        self.checker = Checker::new(self.language);
        let mut at = 0_usize;
        for &length in &self.pieces {
            let end = at.saturating_add(length);
            if let Some(piece) = self.stream.get(at..end) {
                let _ = self.checker.feed(piece);
            }
            at = end;
        }
    }

    /// The NAPLPS macros its stream has defined.
    pub(crate) fn defined_macros(&self) -> Option<[bool; MACRO_CODES]> {
        self.checker.defined_macros()
    }
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
}

impl KeptPictures {
    /// The kept picture of a page and sub-code.
    pub(crate) fn get(&self, page: u16, sub: u16) -> Option<&KeptPicture> {
        self.pictures.get(&(page, sub))
    }

    /// What all of them cost.
    pub(crate) fn cost(&self) -> usize {
        self.pictures.values().fold(0_usize, |sum, picture| sum.saturating_add(picture.cost()))
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
        let picture = self.pictures.entry((page, sub)).or_insert_with(|| KeptPicture::new(language));
        if restart || picture.language != language {
            *picture = KeptPicture::new(language);
        }
        Ok(picture)
    }

    /// Bytes of a kept stream (PAR-16, PAR-17): appended, or with
    /// `restart` a new kept generation. A stream is counted by its bytes.
    ///
    /// # Errors
    /// `wrong_picture_language`, a bound of PAR-17, or the receiver's
    /// reader's refusal; nothing is kept then.
    pub(crate) fn stream(&mut self, page: u16, sub: u16, language: PictureLanguage, restart: bool, bytes: &[u8]) -> Result<(StreamChange, Notes), KeptRefusal> {
        let other_cost = self.pictures.iter().filter(|(key, _)| **key != (page, sub)).fold(0_usize, |sum, (_, picture)| sum.saturating_add(picture.cost()));
        let existing = self.pictures.get(&(page, sub));
        let kept_before = existing.filter(|picture| !restart && picture.language == language);
        let length = kept_before.map_or(0, |picture| picture.stream.len()).saturating_add(bytes.len());
        if length > MOST_KEPT_STREAM_BYTES {
            return Err(KeptRefusal::over("data", "MOST_KEPT_STREAM_BYTES", MOST_KEPT_STREAM_BYTES));
        }
        let companions = kept_before.map_or(0, |picture| picture.cost().saturating_sub(picture.stream.len().saturating_mul(STREAM_COST_FACTOR)));
        let cost = other_cost.saturating_add(length.saturating_mul(STREAM_COST_FACTOR)).saturating_add(companions);
        if cost > MOST_KEPT_COST {
            return Err(KeptRefusal::over("data", "MOST_KEPT_COST", MOST_KEPT_COST));
        }
        // A restart refused by its bytes must keep what was kept: it is
        // read on a checker of its own first.
        if restart || existing.is_none() {
            let mut checker = Checker::new(language);
            let notes = checker.feed(bytes).map_err(|refused| KeptRefusal { path: "data", rule: refused.rule, detail: refused.detail })?;
            let picture = self.entry(page, sub, language, true, "data")?;
            picture.checker = checker;
            picture.stream = bytes.to_vec();
            picture.pieces = alloc::vec![bytes.len()];
            picture.version = picture.version.wrapping_add(1);
            return Ok((StreamChange::Restarted, notes));
        }
        let picture = self.entry(page, sub, language, false, "data")?;
        match picture.checker.feed(bytes) {
            Ok(notes) => {
                picture.stream.extend_from_slice(bytes);
                picture.pieces.push(bytes.len());
                picture.version = picture.version.wrapping_add(1);
                Ok((StreamChange::Appended, notes))
            }
            Err(refused) => {
                picture.reread();
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
        let added = if one { line_bytes } else { line_bytes.saturating_sub(before) };
        if self.cost().saturating_add(added) > MOST_KEPT_COST {
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
mod tests;
