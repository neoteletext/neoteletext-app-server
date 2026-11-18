//! A Public or Global Public Object Page (EN 300 706 clause 10.5.1, Figure
//! 19) written from the objects a program defines (`object_page`,
//! animation.md ANIM-7 and ANIM-17; DEC-6654 item 1): its pointer table
//! computed, each object's definition placed in the triplets that follow
//! it, and the page encoded by the engine's page encoder. Every triplet is
//! Hamming 24/18 coded by the engine (clause 8.3); none of the page is
//! text, so none of it takes odd parity.
//!
//! This is the server's one writer of an object page. The engine's own
//! `neoteletext_formats::author::ObjectPage` needs the standard library,
//! which the runtime does not link; this follows the same clauses with the
//! engine core's triplet functions, and its tests read what it writes
//! with the engine's reader of object pages (`ObjectPacket`,
//! `triplet_location`, `Triplet::function`).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::{
    LAST_PACKET_ENHANCEMENT_BASE, OBJECT_DEFINITION_FIRST_PACKET, OBJECT_NUMBER_N4_SHIFT, OBJECT_NUMBER_N6_N5_SHIFT, OBJECT_NUMBER_N8_N7_SHIFT,
    OBJECT_NUMBER_NIBBLE, OBJECT_POINTER_BITS, OBJECT_POINTER_DATA_BIT, OBJECT_POINTER_UNUSED, OBJECT_POINTERS_PER_TRIPLET, OBJECT_TRIPLET_LAST, OBJECT_TYPES,
    OBJECTS_PER_POINTER_PACKET, PAGE_CODING_TRIPLETS, PAGE_FUNCTION_GPOP, PAGE_FUNCTION_POP, TERMINATION_FINAL_LAST, TERMINATION_FINAL_MORE,
    TERMINATION_INTERMEDIATE_LAST, TERMINATION_INTERMEDIATE_MORE, TRIPLETS_PER_PACKET,
};
use neoteletext_core::packet::encode::pointer_triplet;
use neoteletext_core::packet::{ObjectKind, ObjectNumber, ObjectUsage, Triplet, TripletFunction, TripletLocation, triplet_location};
use neoteletext_core::types::SubCode;
use neoteletext_formats::encode::packed_row;
use neoteletext_formats::page::{EnhancementPacket, PageFile, PageFunction};

use crate::page::address;
use crate::protocol::{ObjectPageContent, ObjectType};

/// Why an object page cannot be written: the detail of its refusal.
pub(crate) type Refused = String;

/// Byte 6 of a packet of object definitions, packets 3 to 25: B1 clear
/// says "definitions" in packets 3 and 4, and the rest is reserved, sent
/// as 0 (clause 10.5.1.2, clause 10.5.1.3).
const DEFINITIONS_BYTE_6: u8 = 0;
/// Whether an object is required at Levels 2.5 and 3.5: both, bits S0 and
/// S1 of its definition's address (Table 28).
const USAGE_BOTH_LEVELS: u8 = 0b11;

/// The engine's type of an object.
const fn kind_of(kind: ObjectType) -> ObjectKind {
    match kind {
        ObjectType::Active => ObjectKind::Active,
        ObjectType::Adaptive => ObjectKind::Adaptive,
        ObjectType::Passive => ObjectKind::Passive,
    }
}

/// The place of a type among the three of each group of pointer triplets:
/// two Active, then two Adaptive, then two Passive pointers (clause
/// 10.5.1.2).
const fn type_offset(kind: ObjectType) -> usize {
    match kind {
        ObjectType::Active => 0,
        ObjectType::Adaptive => 1,
        ObjectType::Passive => 2,
    }
}

/// The number an Object Invocation triplet gives the object of `index`
/// (0 to 31) of its type on sub-page `s1`: N3 to N0 the sub-page, N4 the
/// half of the pointer triplet, N6 N5 the group, N8 N7 the pointer packet
/// (Table 28; clause 10.5.1.2). A sprite's frame of source `public` or
/// `global` names an object by this number (ANIM-7).
#[must_use]
pub(crate) fn object_number(s1: u8, index: u8) -> u16 {
    let index = usize::from(index);
    let packet = index / OBJECTS_PER_POINTER_PACKET;
    let within = index % OBJECTS_PER_POINTER_PACKET;
    let group = within / OBJECT_POINTERS_PER_TRIPLET;
    let odd = within % OBJECT_POINTERS_PER_TRIPLET;
    let field = |value: usize, shift: u32| u16::try_from(value).unwrap_or(0) << shift;
    field(packet, OBJECT_NUMBER_N8_N7_SHIFT)
        | field(group, OBJECT_NUMBER_N6_N5_SHIFT)
        | field(odd, OBJECT_NUMBER_N4_SHIFT)
        | (u16::from(s1) & OBJECT_NUMBER_NIBBLE)
}

/// Where the pointer of the object of `index` of `kind` lies: the pointer
/// packet (1 to 4), the triplet (1 to 12) and whether it is the odd half,
/// bits 10 to 18 (clause 10.5.1.2).
fn pointer_place(kind: ObjectType, index: u8) -> (usize, usize, bool) {
    let index = usize::from(index);
    let within = index % OBJECTS_PER_POINTER_PACKET;
    let group = within / OBJECT_POINTERS_PER_TRIPLET;
    let packet = (index / OBJECTS_PER_POINTER_PACKET).saturating_add(1);
    let triplet = group.saturating_mul(OBJECT_TYPES).saturating_add(type_offset(kind)).saturating_add(1);
    (packet, triplet, within % OBJECT_POINTERS_PER_TRIPLET == 1)
}

/// The packets of the object page `content`: its header with the sub-code
/// whose S1 is the sub-page's and whose last-packet field names its last
/// packet (Annex A.1), packet X/28/0 with the page function POP or GPOP and
/// the coding "triplets" (clause 9.4.2.1), the pointer packets, and the
/// object definitions from the packet after them, each its Object
/// Definition triplet, its own triplets and a Termination Marker, the last
/// packet filled with repetitions of the last marker (clause 10.5.1.3).
///
/// # Errors
/// An object defined twice, a triplet that would end or begin a
/// definition inside one (an Object Definition or a Termination Marker),
/// one that invokes a Local Object, which a public object never does
/// (clause 13.6 note), or definitions past the page's last triplet, 506.
pub(crate) fn packets(content: &ObjectPageContent) -> Result<Vec<u8>, Refused> {
    let s1 = content.s1.get();
    let highest = content.objects.iter().map(|object| object.index.get()).max().unwrap_or(0);
    let pointer_packets = (usize::from(highest) / OBJECTS_PER_POINTER_PACKET).saturating_add(1);
    // Definitions begin at the packet after the pointer table, never
    // before packet 3; triplet 0 is the first triplet of packet 3.
    let first_packet = u8::try_from(pointer_packets.saturating_add(1)).unwrap_or(u8::MAX).max(OBJECT_DEFINITION_FIRST_PACKET);
    let start = usize::from(first_packet.saturating_sub(OBJECT_DEFINITION_FIRST_PACKET)).saturating_mul(TRIPLETS_PER_PACKET);
    let mut sequence: Vec<Triplet> = Vec::new();
    let mut pointers = alloc::vec![[pointer_triplet(OBJECT_POINTER_UNUSED, OBJECT_POINTER_UNUSED); TRIPLETS_PER_PACKET]; pointer_packets];
    let count = content.objects.len();
    for (position, object) in content.objects.iter().enumerate() {
        let (kind, index) = (object.kind, object.index.get());
        if content.objects.iter().take(position).any(|earlier| earlier.kind == kind && earlier.index.get() == index) {
            return Err(format!("objects[{position}]: the {kind:?} object {index} is defined twice"));
        }
        let absolute = start.saturating_add(sequence.len());
        let pointer = u16::try_from(absolute).ok().filter(|pointer| *pointer <= OBJECT_TRIPLET_LAST).ok_or_else(|| full(position))?;
        let (packet, triplet, odd) = pointer_place(kind, index);
        if let Some(slot) = pointers.get_mut(packet.saturating_sub(1)).and_then(|triplets| triplets.get_mut(triplet)) {
            let held = slot.value();
            let mask = u32::from(OBJECT_POINTER_UNUSED);
            let (even, odd_value) = (held & mask, (held >> OBJECT_POINTER_BITS) & mask);
            let (even, odd_value) = (u16::try_from(even).unwrap_or(OBJECT_POINTER_UNUSED), u16::try_from(odd_value).unwrap_or(OBJECT_POINTER_UNUSED));
            *slot = if odd { pointer_triplet(even, pointer) } else { pointer_triplet(pointer, odd_value) };
        }
        let number = ObjectNumber(object_number(s1, index));
        sequence.push(TripletFunction::ObjectDefinition { usage: ObjectUsage(USAGE_BOTH_LEVELS), kind: kind_of(kind), number }.to_triplet());
        for (place, given) in object.triplets.iter().enumerate() {
            let triplet = Triplet { address: given.address.get(), mode: given.mode.get(), data: given.data.get() };
            match triplet.function() {
                TripletFunction::ObjectDefinition { .. } | TripletFunction::Termination { .. } => {
                    return Err(format!("objects[{position}].triplets[{place}]: the server writes each definition's Object Definition and Termination Marker"));
                }
                TripletFunction::ObjectInvocation { source: neoteletext_core::packet::ObjectSource::Local, .. } => {
                    return Err(format!("objects[{position}].triplets[{place}]: a public object never invokes a Local Object (clause 13.6)"));
                }
                _ => sequence.push(triplet),
            }
        }
        // Each object ends with a marker that says whether more objects
        // follow on this sub-page and whether this is the page's last
        // sub-page (Table 28, mode 11111).
        let marker = match (content.last, position.saturating_add(1) < count) {
            (false, true) => TERMINATION_INTERMEDIATE_MORE,
            (false, false) => TERMINATION_INTERMEDIATE_LAST,
            (true, true) => TERMINATION_FINAL_MORE,
            (true, false) => TERMINATION_FINAL_LAST,
        };
        sequence.push(TripletFunction::Termination { kind: marker }.to_triplet());
    }
    if let Some(&marker) = sequence.last() {
        while !start.saturating_add(sequence.len()).is_multiple_of(TRIPLETS_PER_PACKET) {
            sequence.push(marker);
        }
    }
    if start.saturating_add(sequence.len()) > usize::from(OBJECT_TRIPLET_LAST).saturating_add(1) {
        return Err(full(count.saturating_sub(1)));
    }
    let mut model = PageFile::new();
    model.address = address(content.page.0);
    model.function = Some(PageFunction { function: if content.global { PAGE_FUNCTION_GPOP } else { PAGE_FUNCTION_POP }, coding: PAGE_CODING_TRIPLETS });
    let mut last_packet = 0_u8;
    for (place, triplets) in pointers.iter().enumerate() {
        let packet = place.saturating_add(1);
        model.set_row(packet, &packed_row(OBJECT_POINTER_DATA_BIT, triplets));
        last_packet = last_packet.max(u8::try_from(packet).unwrap_or(u8::MAX));
    }
    let mut rows: Vec<(u8, [Triplet; TRIPLETS_PER_PACKET])> = Vec::new();
    let mut enhancements: Vec<(u8, [Triplet; TRIPLETS_PER_PACKET])> = Vec::new();
    for (offset, &triplet) in sequence.iter().enumerate() {
        let number = u16::try_from(start.saturating_add(offset)).unwrap_or(u16::MAX);
        let (list, packet, index) = match triplet_location(number).ok_or_else(|| full(count.saturating_sub(1)))? {
            TripletLocation::Row { packet, triplet } => (&mut rows, packet, triplet),
            TripletLocation::Enhancement { designation, triplet } => (&mut enhancements, designation, triplet),
        };
        if !list.iter().any(|(held, _)| *held == packet) {
            list.push((packet, [Triplet::default(); TRIPLETS_PER_PACKET]));
        }
        let slots = list.iter_mut().find(|(held, _)| *held == packet).map(|(_, slots)| slots).ok_or_else(|| full(count))?;
        if let Some(slot) = slots.get_mut(usize::from(index)) {
            *slot = triplet;
        }
    }
    for (packet, triplets) in &rows {
        model.set_row(usize::from(*packet), &packed_row(DEFINITIONS_BYTE_6, triplets));
        last_packet = last_packet.max(*packet);
    }
    for (designation, triplets) in &enhancements {
        if !model.enhancements.push(EnhancementPacket { designation: *designation, triplets: *triplets }) {
            return Err(full(count.saturating_sub(1)));
        }
        last_packet = LAST_PACKET_ENHANCEMENT_BASE.saturating_add(*designation);
    }
    // The sub-code: S1 the sub-page, S4 S3 the last packet sent, S2 the
    // update indicator 0 (Annex A.1).
    model.subcode = SubCode::new(neoteletext_core::limits::enhancement_sub_code(last_packet, 0, s1));
    let mut bytes = Vec::new();
    crate::frame::append_page(&mut bytes, &model);
    Ok(bytes)
}

/// The refusal of definitions past the page's last triplet.
fn full(position: usize) -> Refused {
    format!("objects[{position}]: the definitions pass the page's last triplet, {OBJECT_TRIPLET_LAST}; put the rest on another sub-page")
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use neoteletext_core::consts::{DATA_BYTES, OBJECT_POINTER_UNUSED, PACKET_BYTES, TERMINATION_FINAL_LAST, TERMINATION_FINAL_MORE};
    use neoteletext_core::packet::{
        ObjectKind, ObjectNumber, ObjectPacket, Packet, Triplet, TripletFunction, TripletLocation, decode_address, triplet_location,
    };
    use serde_json::json;

    use super::{object_number, packets};
    use crate::protocol::ObjectPageContent;

    /// The content `value` reads as.
    fn content(value: &serde_json::Value) -> ObjectPageContent {
        serde_json::from_value(value.clone()).unwrap()
    }

    /// Each packet of `bytes` by its number, X/26 by 26 plus its
    /// designation code, with its data read as an object page's packet.
    fn read(bytes: &[u8]) -> Vec<(u8, ObjectPacket)> {
        bytes
            .as_chunks::<PACKET_BYTES>()
            .0
            .iter()
            .filter_map(|bytes| {
                let packet = Packet { bytes: *bytes };
                let number = decode_address(&packet).ok()?.number;
                let data: [u8; DATA_BYTES] = packet.data();
                Some((number, ObjectPacket::new(&data)))
            })
            .collect()
    }

    /// The triplet at absolute number `at` of the page `bytes`, read back.
    fn triplet_at(read: &[(u8, ObjectPacket)], at: u16) -> Option<Triplet> {
        match triplet_location(at)? {
            TripletLocation::Row { packet, triplet } => {
                read.iter().find(|(number, _)| *number == packet).and_then(|(_, packet)| packet.triplets.get(usize::from(triplet)).copied().flatten())
            }
            TripletLocation::Enhancement { designation, triplet } => read
                .iter()
                .find(|(number, packet)| *number == 26 && packet.byte_6 == Some(designation))
                .and_then(|(_, packet)| packet.triplets.get(usize::from(triplet)).copied().flatten()),
        }
    }

    /// EN 300 706 clause 10.5.1.2, Table 28 (ANIM-7, ANIM-17): an object's
    /// pointer, read by the engine's reader of object pages, points to its
    /// Object Definition triplet with the number an invocation gives it,
    /// followed by its own triplets and a Termination Marker; the pointer of
    /// an object of index 9 is the odd half of packet 2's triplet 3, every
    /// other pointer unused, and every triplet reads back without error.
    #[test]
    fn each_object_is_found_by_its_pointer() {
        let character = json!({"address":0,"mode":15,"data":65});
        let page = content(&json!({"page":"1C0","s1":3,"global":false,"last":true,"objects":[
            {"kind":"active","index":0,"triplets":[character, character]},
            {"kind":"passive","index":9,"triplets":[character]}]}));
        let bytes = packets(&page).unwrap();
        let read = read(&bytes);
        let pointer_packet = |number: u8| read.iter().find(|(held, _)| *held == number).map(|(_, packet)| *packet).unwrap();
        assert!(pointer_packet(1).carries_pointers() && pointer_packet(2).carries_pointers());
        let active = pointer_packet(1).pointer(1, false);
        let passive = pointer_packet(2).pointer(3, true);
        for (place, odd) in [(1, true), (2, false), (3, false)] {
            assert_eq!(pointer_packet(1).pointer(place, odd), OBJECT_POINTER_UNUSED, "packet 1 triplet {place}");
        }
        for (pointer, kind, index, body, marker) in
            [(active, ObjectKind::Active, 0, 2, TERMINATION_FINAL_MORE), (passive, ObjectKind::Passive, 9, 1, TERMINATION_FINAL_LAST)]
        {
            let definition = triplet_at(&read, pointer).unwrap().function();
            assert_eq!(
                definition,
                TripletFunction::ObjectDefinition {
                    usage: neoteletext_core::packet::ObjectUsage(super::USAGE_BOTH_LEVELS),
                    kind,
                    number: ObjectNumber(object_number(3, index))
                }
            );
            for offset in 1..=body {
                assert_eq!(triplet_at(&read, pointer + offset), Some(Triplet { address: 0, mode: 15, data: 65 }));
            }
            assert_eq!(triplet_at(&read, pointer + body + 1).unwrap().function(), TripletFunction::Termination { kind: marker });
        }
        // Passive 9: N8 N7 = 1, N6 N5 = 0, N4 = 1, N3 to N0 = S1 3.
        assert_eq!(object_number(3, 9), (1 << 7) | (1 << 4) | 3);
        // Every packet of rows 1 to 25 reads without an uncorrectable byte.
        assert!(
            read.iter()
                .filter(|(number, _)| (1..=25).contains(number))
                .all(|(_, packet)| packet.byte_6.is_some() && packet.triplets.iter().all(Option::is_some))
        );
    }

    /// Clause 10.5.1.3, clause 13.6: an object defined twice, a body that
    /// writes its own definition or marker or invokes a Local Object, and
    /// definitions past triplet 506 are refused.
    #[test]
    fn what_cannot_be_written_is_refused() {
        let character = json!({"address":0,"mode":15,"data":65});
        let one = |triplets: serde_json::Value| {
            content(&json!({"page":"1C0","s1":0,"global":true,"last":true,"objects":[{"kind":"active","index":0,"triplets":triplets}]}))
        };
        let twice = content(&json!({"page":"1C0","s1":0,"global":true,"last":true,"objects":[
            {"kind":"active","index":0,"triplets":[character]},{"kind":"active","index":0,"triplets":[character]}]}));
        assert!(packets(&twice).unwrap_err().contains("twice"));
        let termination = TripletFunction::Termination { kind: TERMINATION_FINAL_LAST }.to_triplet();
        let own = json!([{"address":termination.address,"mode":termination.mode,"data":termination.data}]);
        assert!(packets(&one(own)).is_err());
        let local =
            TripletFunction::ObjectInvocation { source: neoteletext_core::packet::ObjectSource::Local, kind: ObjectKind::Active, number: ObjectNumber(0) }
                .to_triplet();
        assert!(packets(&one(json!([{"address":local.address,"mode":local.mode,"data":local.data}]))).unwrap_err().contains("Local"));
        let many: Vec<serde_json::Value> = (0..520).map(|_| character.clone()).collect();
        assert!(packets(&one(json!(many))).unwrap_err().contains("506"));
        assert!(packets(&one(json!([character]))).is_ok());
    }
}
