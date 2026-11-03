//! A page's Unicode text, encoded by the engine's one encoder (DEC-2199,
//! REQ-2214): the runs a program sends are written over the page's rows
//! with `neoteletext_formats::write_text::write_runs`, their triplets
//! merged with the program's own packets X/26, and the page's designation
//! written into its character set and into the program's own X/28/0 Format
//! 1, if it sent one. What it cost is the `measured` answer (REQ-2216). The
//! page has been checked ([`crate::protocol::check`]) before it gets here,
//! and leaves as a page of codes alone, which the rest of the server sends
//! as it always has.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use neoteletext_core::consts::{COLUMNS, DESIGNATION_GROUP_SHIFT, NATIONAL_INDEX_MASK, NO_SECOND_G0, ROWS, SPACE};
use neoteletext_core::packet::Triplet;
use neoteletext_formats::write_text::{Level, PageText, RunError, RunFault, TextRun, designate, packets, write_runs};
use neoteletext_web::base64;

use crate::protocol::{
    self, CharacterSet, Enhancement, MessageType, PackedLine, Page, PageDesignation, ProgramMessage, Refusal, Rule, RunCost, TRIPLET_LIMIT, TextLevel, X28Line,
};

/// The page-related packet X/28 of designation code 0: X/28/0 Format 1
/// (EN 300 706 clause 9.4.2).
const PAGE_DATA_DESIGNATION: u8 = 0;

/// What encoding a page cost (REQ-2216 item 2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Cost {
    /// The triplets of its local enhancement data.
    pub triplets: u16,
    /// Its designation.
    pub designation: PageDesignation,
    /// Each run's cost.
    pub runs: Vec<RunCost>,
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
        // The pinned engine still lays Thai and Lao out with mark rows; the
        // protocol no longer names that fault (DEC-2239), so it is the run's
        // row out of range until the pin moves to an engine without them.
        RunFault::MarkRowsOutOfRange => (at(error.run, "row"), Rule::OutOfRange),
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
    let before = grid(page);
    let mut codes = before;
    let text_runs: Vec<TextRun<'_>> = runs
        .iter()
        .map(|run| TextRun { row: run.row, column: run.column, text: &run.text, language: run.language.as_ref().map(|tag| tag.0.as_str()) })
        .collect();
    let text = PageText { language: language.as_ref().map(|tag| tag.0.as_str()), designation: given, level, runs: &text_runs };
    let encoded = write_runs(&mut codes, &text, &program_triplets(page)).map_err(|error| fault(&error))?;
    for (slot, (new, old)) in page.rows.iter_mut().zip(codes.iter().zip(&before)) {
        if new != old {
            *slot = Some(row_text(new));
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
    Ok(Cost { triplets, designation: PageDesignation { default: designations.default, second }, runs })
}

/// A page of `message` encoded in place, a fault refusing the message with
/// the page's path before the fault's.
fn encode_at(kind: MessageType, at: &str, page: &mut Page) -> Result<Cost, Refusal> {
    encode(page).map_err(|Fault { path, rule, detail }| Refusal { message: Some(kind), path: format!("{at}.{path}"), rule, detail })
}

/// Every page `message` carries for viewers, encoded (REQ-2214): `page`'s,
/// `reply`'s selected sub-page and `description`'s content. `measure` is
/// encoded where it is answered.
///
/// # Errors
/// The encoder's refusal, at the path of the run at fault.
pub(crate) fn encode_message(message: &mut ProgramMessage) -> Result<(), Refusal> {
    let kind = message.kind();
    match message {
        ProgramMessage::Page(message) => encode_at(kind, "page", &mut message.page).map(drop),
        ProgramMessage::Reply(reply) => reply.selected.as_mut().map_or(Ok(()), |page| encode_at(kind, "selected", page).map(drop)),
        ProgramMessage::Description(description) => description.content.as_mut().map_or(Ok(()), |page| encode_at(kind, "content", page).map(drop)),
        _ => Ok(()),
    }
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
