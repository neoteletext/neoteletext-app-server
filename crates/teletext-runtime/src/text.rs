//! A page's Unicode text, encoded by the engine's one encoder (DEC-2199,
//! REQ-2214): the runs a program sends are written over the page's rows
//! with `neoteletext_formats::write_text::write_runs`, their triplets
//! merged with the program's own packets X/26, and the page's designation
//! written into its character set and into the program's own X/28/0 Format
//! 1, if it sent one. What it cost is the `measured` answer (REQ-2216). The
//! page has been checked ([`crate::protocol::check`]) before it gets here,
//! and leaves as a page of codes alone, which the rest of the server sends
//! as it always has. A run from outside the program (`outside`) shows a
//! stand-in for a character its language cannot show, rather than refuse
//! the page (DEC-2326, REQ-2150 item 2).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::consts::{COLUMNS, DESIGNATION_GROUP_SHIFT, NATIONAL_INDEX_MASK, NO_SECOND_G0, ROWS, SPACE};
use neoteletext_core::packet::Triplet;
use neoteletext_formats::write_text::{FIRST_TEXT_ROW, Level, PageText, RunError, RunFault, TextRun, designate, packets, write_runs};
use neoteletext_web::base64;

use crate::protocol::{
    self, CharacterSet, Enhancement, MessageType, PackedLine, Page, PageDesignation, ProgramMessage, Refusal, Replaced, Rule, RunCost, TRIPLET_LIMIT,
    TextLevel, X28Line,
};

/// The page-related packet X/28 of designation code 0: X/28/0 Format 1
/// (EN 300 706 clause 9.4.2).
const PAGE_DATA_DESIGNATION: u8 = 0;

/// What a run from outside shows for a character its language cannot, the
/// first of these its language shows (REQ-2150 item 2, DEC-2326): the
/// question mark, which the engine writes through Latin English inside a
/// run whose own set lacks it (the engine's DEC-2473), so that the Arabic
/// script's own question mark is no longer needed; then a hyphen or a full
/// stop, should the question mark not fit.
const STAND_INS: [char; 3] = ['?', '-', '.'];

/// A run's language as the encoder reads it: its own, else the page's.
fn run_language<'a>(run: &'a protocol::TextRun, page: Option<&'a str>) -> Option<&'a str> {
    run.language.as_ref().map(|tag| tag.0.as_str()).or(page)
}

/// What encoding a page cost (REQ-2216 item 2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Cost {
    /// The triplets of its local enhancement data.
    pub triplets: u16,
    /// Its designation.
    pub designation: PageDesignation,
    /// Each run's cost.
    pub runs: Vec<RunCost>,
    /// The characters runs from outside showed as a stand-in, each once.
    pub replaced: Vec<Replaced>,
}

/// A fault the encoder found, at a path under the page.
pub(crate) struct Fault {
    /// The path from the page: `text[3].language`.
    pub path: String,
    /// The rule broken.
    pub rule: Rule,
    /// The fault in words.
    pub detail: String,
}

fn narrow(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// The fault of an encoder's refusal, by the rule REQ-2214 and REQ-2216
/// give it.
fn fault(error: &RunError) -> Fault {
    let at = |run: Option<usize>, field: &str| match run {
        Some(index) if field.is_empty() => format!("text[{index}]"),
        Some(index) => format!("text[{index}].{field}"),
        None => field.to_string(),
    };
    let detail = error.fault.to_string();
    let (path, rule) = match &error.fault {
        RunFault::UnknownLanguage(_) => (at(error.run, "language"), Rule::UnknownLanguage),
        RunFault::OutOfRows => (at(error.run, ""), Rule::OutOfRange),
        RunFault::OutOfColumns(_) => (at(error.run, "text"), Rule::TextOutOfColumns),
        RunFault::Overlap(_) => (at(error.run, ""), Rule::TextOverlap),
        RunFault::Unshown(_) => (at(error.run, "text"), Rule::TextUnshown),
        RunFault::CellTaken { .. } => (at(error.run, ""), Rule::TextCellTaken),
        RunFault::SetChanged { .. } => (at(error.run, ""), Rule::TextSetChanged),
        RunFault::OverBudget(_) => (at(error.run, ""), Rule::TripletsOverBudget),
        RunFault::ProgramObjects => ("enhancements".to_string(), Rule::TextAndObjects),
    };
    let path = if path.is_empty() { "language".to_string() } else { path };
    Fault { path, rule, detail }
}

/// The page's rows as codes: a blank row as spaces.
fn grid(page: &Page) -> [[u8; COLUMNS]; ROWS] {
    let mut grid = [[SPACE; COLUMNS]; ROWS];
    for (slot, row) in grid.iter_mut().zip(&page.rows) {
        let Some(row) = row else { continue };
        let mut bytes = [0_u8; COLUMNS];
        if let Ok(length) = base64::decode(row.as_bytes(), &mut bytes)
            && length == COLUMNS
        {
            *slot = bytes;
        }
    }
    grid
}

/// One row's codes as the protocol carries them.
fn row_text(codes: &[u8; COLUMNS]) -> String {
    let mut out = alloc::vec![0_u8; base64::encoded_len(COLUMNS)];
    let written = base64::encode(codes, &mut out).unwrap_or(0);
    out.truncate(written);
    String::from_utf8(out).unwrap_or_default()
}

/// The program's local enhancement data: its packets X/26 in order of
/// designation code.
fn program_triplets(page: &Page) -> Vec<Triplet> {
    let mut ordered: Vec<&Enhancement> = page.enhancements.iter().collect();
    ordered.sort_by_key(|packet| packet.designation);
    ordered
        .iter()
        .flat_map(|packet| packet.triplets.iter().map(|triplet| Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data }))
        .collect()
}

fn protocol_triplet(triplet: Triplet) -> protocol::Triplet {
    protocol::Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data }
}

/// Writes the page's text into it, and gives what that cost: the runs over
/// its rows, their triplets merged into its packets X/26, and its
/// language's designation into its character set and its own X/28/0
/// Format 1. After it the page has no `language`, `level` or `text`.
pub(crate) fn encode(page: &mut Page) -> Result<Cost, Fault> {
    let runs = page.text.take().unwrap_or_default();
    let language = page.language.take();
    let level = match page.level.take().unwrap_or_default() {
        TextLevel::NeoTeletext => Level::NeoTeletext,
        TextLevel::Level35 => Level::Standard,
    };
    let given = page.character_set.map_or(0, |set| (set.region.unwrap_or(0) << DESIGNATION_GROUP_SHIFT) | set.national_option);
    let page_language = language.as_ref().map(|tag| tag.0.as_str());
    let before = grid(page);
    let program = program_triplets(page);
    let mut texts: Vec<String> = runs.iter().map(|run| run.text.clone()).collect();
    let mut replaced: Vec<Replaced> = Vec::new();
    // Each pass replaces one character of one run from outside, so the
    // passes are bounded by the characters sent (REQ-2214 item 4).
    let most_passes = texts.iter().map(|text| text.chars().count()).sum::<usize>().saturating_add(1);
    let mut passes = 0_usize;
    let (codes, encoded) = loop {
        let text_runs: Vec<TextRun<'_>> = runs
            .iter()
            .zip(&texts)
            .map(|(run, text)| TextRun { row: run.row, column: run.column, text, language: run.language.as_ref().map(|tag| tag.0.as_str()) })
            .collect();
        let text = PageText { language: page_language, designation: given, level, runs: &text_runs };
        let mut codes = before;
        let error = match write_runs(&mut codes, &text, &program) {
            Ok(encoded) => break (codes, encoded),
            Err(error) => error,
        };
        passes = passes.saturating_add(1);
        let (Some(index), RunFault::Unshown(character)) = (error.run, &error.fault) else { return Err(fault(&error)) };
        let character = *character;
        let Some(run) = runs.get(index).filter(|run| run.outside) else { return Err(fault(&error)) };
        let run_tag = run_language(run, page_language);
        let shows = |candidate: char| candidate != character && shows(page_language, given, level, run_tag, candidate);
        let (Some(stand_in), true) = (STAND_INS.into_iter().find(|&candidate| shows(candidate)), passes < most_passes) else { return Err(fault(&error)) };
        let Some(slot) = texts.get_mut(index) else { return Err(fault(&error)) };
        *slot = slot.replace(character, stand_in.encode_utf8(&mut [0_u8; 4]));
        let tag = run_tag.unwrap_or(ENGLISH).to_string();
        if !replaced.iter().any(|seen: &Replaced| seen.character == character && seen.language == tag) {
            replaced.push(Replaced { page: page.page, character, stand_in, language: tag });
        }
    };
    for (slot, (new, old)) in page.rows.iter_mut().zip(codes.iter().zip(&before)) {
        if new != old {
            *slot = Some(protocol::Base64(row_text(new)));
        }
    }
    if encoded.changed {
        page.enhancements = (0_u8..)
            .zip(packets(&encoded.triplets))
            .map(|(designation, triplets)| Enhancement { designation, triplets: triplets.map(protocol_triplet) })
            .collect();
    }
    let designations = encoded.designations;
    if language.is_some() {
        page.character_set =
            Some(CharacterSet { region: Some(designations.default >> DESIGNATION_GROUP_SHIFT), national_option: designations.default & NATIONAL_INDEX_MASK });
        // REQ-2214 item 7: the program's own X/28/0 Format 1 keeps every
        // field but the designation.
        for line in &mut page.packets {
            if let PackedLine::X28(X28Line { designation: PAGE_DATA_DESIGNATION, triplets }) = line {
                let mut engine = triplets.map(|triplet| Triplet { address: triplet.address, mode: triplet.mode, data: triplet.data });
                designate(&mut engine, designations);
                *triplets = engine.map(protocol_triplet);
            }
        }
    }
    let second = (designations.second != NO_SECOND_G0).then_some(designations.second);
    let runs = encoded.runs.iter().map(|report| RunCost { cells: report.cells, triplets: narrow(report.triplets) }).collect();
    // The encoder refuses a page over the budget, so this is at most 208.
    let triplets = narrow(encoded.count).min(TRIPLET_LIMIT);
    Ok(Cost { triplets, designation: PageDesignation { default: designations.default, second }, runs, replaced })
}

/// The language of a page that names none: Latin English.
const ENGLISH: &str = "en";

/// Whether `character` alone, in a run of `language` on a blank page of
/// `page`'s language or designation at `level`, is shown: the encoder's own
/// answer, so the stand-in is one the run's sets hold.
fn shows(page: Option<&str>, designation: u8, level: Level, language: Option<&str>, character: char) -> bool {
    let mut grid = [[SPACE; COLUMNS]; ROWS];
    let mut buffer = [0_u8; 4];
    let run = [TextRun { row: FIRST_TEXT_ROW, column: 0, text: character.encode_utf8(&mut buffer), language }];
    write_runs(&mut grid, &PageText { language: page, designation, level, runs: &run }, &[]).is_ok()
}

/// A page of `message` encoded in place, a fault refusing the message with
/// the page's path before the fault's.
fn encode_at(kind: MessageType, at: &str, page: &mut Page) -> Result<Cost, Refusal> {
    encode(page).map_err(|Fault { path, rule, detail }| Refusal { message: Some(kind), path: format!("{at}.{path}"), rule, detail })
}

/// Every page `message` carries for viewers, encoded (REQ-2214): `page`'s,
/// `reply`'s selected sub-page and `description`'s content, with the
/// characters runs from outside showed as a stand-in (DEC-2326). `measure`
/// is encoded where it is answered.
///
/// # Errors
/// The encoder's refusal, at the path of the run at fault.
pub(crate) fn encode_message(message: &mut ProgramMessage) -> Result<Vec<Replaced>, Refusal> {
    let kind = message.kind();
    let cost = match message {
        ProgramMessage::Page(message) => Some(encode_at(kind, "page", &mut message.page)?),
        ProgramMessage::Reply(reply) => reply.selected.as_mut().map(|page| encode_at(kind, "selected", page)).transpose()?,
        ProgramMessage::Description(description) => description.content.as_mut().map(|page| encode_at(kind, "content", page)).transpose()?,
        _ => None,
    };
    Ok(cost.map(|cost| cost.replaced).unwrap_or_default())
}

/// `measure`'s page encoded, and what it cost (REQ-2216 item 1).
///
/// # Errors
/// The encoder's refusal, at `page.` and the path of the run at fault.
pub(crate) fn measure(page: &mut Page) -> Result<Cost, Refusal> {
    encode_at(MessageType::Measure, "page", page)
}

#[cfg(test)]
mod tests;
