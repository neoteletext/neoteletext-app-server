//! Animation on a channel (neoteletext-common `docs/animation.md` ANIM-2,
//! ANIM-3, ANIM-17, ANIM-21; REQ-6533): the record built from each of the
//! protocol's messages, each refusal, frames held to their end, and
//! timelines at their times; and in the second form (DEC-6587, DEC-6599)
//! nothing kept for a later viewer, a sheet one page's, and a timeline
//! ended with its page (page-rules.md RULE-40, RULE-111, RULE-112;
//! REQ-6615).

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_rules::animate::{Operation, OperationKind};
use neoteletext_rules::table::Target;
use neoteletext_web::consts::{MAX_FRAME_BYTES, MAX_SHEET_BYTES};
use neoteletext_web::records::frame::FrameMark;
use neoteletext_web::records::naplps_objects::ObjectKind;
use neoteletext_web::records::sheet::SheetKind;
use neoteletext_web::records::{
    TYPE_ANIMATE, TYPE_FRAME, TYPE_NAPLPS_OBJECTS, TYPE_RULES, TYPE_SHEET, TYPE_SPRITES, animate, frame, naplps_objects, sheet, sprites,
};
use serde_json::{Value, json};

use super::super::animation::FRAME_LIMIT_MS;
use super::{
    ACKNOWLEDGEMENT_WAIT_MS, CONNECTION, Channel, ChannelConfig, DefaultRules, Effect, InitialPage, String, Vec, all_sent_to, bare, hello, line, message,
    record_types, select, selected, sent_to, subpage, told, with,
};
use crate::animation::tick_ms;
use crate::protocol::tests::base64;
use crate::protocol::tests::rules::ANIMATION_EXAMPLES;

/// A ReGIS sheet: one hidden drawing, committed.
const REGIS_SHEET: &[u8] = b"D(S1(V0),C)";

/// A channel whose connection is told to its program and shows `page`,
/// sub-page 0001, with no rule table.
fn showing(page: u16) -> Channel {
    let config = ChannelConfig {
        name: "Test".into(),
        address: "http://127.0.0.1/test/".into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: DefaultRules::None,
    };
    let mut channel = Channel::new(config, &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    channel.opened(CONNECTION, None, 0);
    channel.received(CONNECTION, &bare(&[hello(), select(page, 1)]), 0);
    // HELLO is acknowledged now, so that no later wake sends its record.
    channel.deadline(ACKNOWLEDGEMENT_WAIT_MS);
    let _ = channel.take_effects();
    channel
}

/// The records in `bytes`, each as its type and body, reassembled from its
/// fragments and its frame taken off.
fn records(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut found = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for packet in bytes.as_chunks::<PACKET_BYTES>().0 {
        let Some(Ok(fragment)) = neoteletext_audio::idl::fragment(&Packet { bytes: *packet }) else { continue };
        if fragment.index == 0 {
            record.clear();
        }
        record.extend_from_slice(&fragment.data);
        if fragment.last {
            let (kind, body) = neoteletext_audio::record::split(&record[..neoteletext_audio::record::framed_length(&record).unwrap()]).unwrap();
            found.push((kind, body.to_vec()));
        }
    }
    found
}

/// The types of the records `effects` send the connection, in order.
fn kinds(effects: &[Effect]) -> Vec<u8> {
    records(&all_sent_to(effects, CONNECTION)).into_iter().map(|(kind, _)| kind).collect()
}

/// The `refused` lines among `effects`: each one's rule and path.
fn refusals(effects: &[Effect]) -> Vec<(String, String)> {
    told(effects)
        .iter()
        .filter(|line| line["type"] == "refused")
        .map(|line| (line["rule"].as_str().unwrap().into(), line["path"].as_str().unwrap().into()))
        .collect()
}

fn named(rule: &str, path: &str) -> Vec<(String, String)> {
    alloc::vec![(rule.into(), path.into())]
}

/// The example of animation.md ANIM-21 of type `kind`, for the test's
/// connection; the first of that type.
fn example(kind: &str) -> Value {
    let mut found = ANIMATION_EXAMPLES.lines().map(|line| serde_json::from_str::<Value>(line).unwrap()).find(|message| message["type"] == kind).unwrap();
    found["connections"] = json!([CONNECTION]);
    found
}

/// The operations of an ANIMATE record's body.
fn operations(body: &[u8]) -> Vec<Operation> {
    animate::read(body).unwrap().operations().collect()
}

fn object(number: u16, kind: OperationKind) -> Operation {
    Operation { target: Target::object(number), kind }
}

/// REQ-6533, ANIM-21, ANIM-6: the `animate` example is one ANIMATE record
/// of its four operations, which the engine's reader takes.
#[test]
fn animate_is_an_animate_record() {
    let mut channel = showing(0x150);
    channel.from_program(&line(&example("animate")), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = records(&all_sent_to(&effects, CONNECTION));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, TYPE_ANIMATE);
    let record = animate::read(&sent[0].1).unwrap();
    assert_eq!((record.page.mpp(), record.subcode.value()), (0x150, 1));
    assert_eq!(
        operations(&sent[0].1),
        [
            object(10, OperationKind::Move { relative: false, x: 120, y: 80 }),
            object(10, OperationKind::Cycle { first: 0, last: 3, period: 6, once: false }),
            object(20, OperationKind::Hide),
            object(21, OperationKind::Reset),
        ]
    );
    // 3F7F names no page (DEC-6599): refused, and nothing sent.
    channel.from_program(&line(&with(&example("animate"), "/sub", json!("3F7F"))), 2);
    let effects = channel.take_effects();
    assert_eq!(rules_refused(&effects), [json!("out_of_range")]);
    assert_eq!(records(&all_sent_to(&effects, CONNECTION)), []);
}

/// REQ-6533, ANIM-21, ANIM-7: the `sprites` example is one SPRITES record
/// with the sprite's fields and its two object references.
#[test]
fn sprites_are_a_sprites_record() {
    let mut channel = showing(0x160);
    channel.from_program(&line(&example("sprites")), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = records(&all_sent_to(&effects, CONNECTION));
    assert_eq!(sent.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(), [TYPE_SPRITES]);
    let record = sprites::read(&sent[0].1).unwrap();
    assert_eq!((record.page.mpp(), record.subcode.value(), record.count), (0x160, 1, 1));
    let sprite = record.sprites().next().unwrap();
    assert_eq!((sprite.number, sprite.shown, sprite.detectable, sprite.layer, sprite.order), (1, true, false, 2, 0));
    assert_eq!((sprite.column, sprite.row, sprite.width, sprite.height), (10, 5, 4, 3));
    // A local passive object: source 1, type 3, numbers 0 and 1.
    let frames: Vec<(u8, u8, u16)> = sprite.frames().map(|frame| (frame.source(), frame.kind(), frame.number())).collect();
    assert_eq!(frames, [(1, 3, 0), (1, 3, 1)]);
}

/// REQ-6533, ANIM-21, ANIM-11: the `naplps_objects` example is one
/// `NAPLPS_OBJECTS` record of a macro object and a region, for the
/// generation the connection's NAPLPS stream of the sub-page has, or the
/// one its next stream will take.
#[test]
fn naplps_objects_are_a_naplps_objects_record() {
    let mut channel = showing(0x170);
    channel.from_program(&line(&example("naplps_objects")), 1);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = records(&all_sent_to(&effects, CONNECTION));
    assert_eq!(sent.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(), [TYPE_NAPLPS_OBJECTS]);
    let record = naplps_objects::read(&sent[0].1).unwrap();
    assert_eq!((record.page.mpp(), record.subcode.value(), record.generation, record.count), (0x170, 1, 1, 2), "the stream its next naplps begins");
    let objects: Vec<_> = record.objects().collect();
    let first = &objects[0];
    assert_eq!((first.number, first.kind, first.shown, first.detectable, first.layer, first.order), (1, ObjectKind::Macro, true, true, 2, 0));
    assert_eq!((first.x, first.y, first.width, first.height, first.origin), (300, 200, 64, 64, (0, 416)));
    assert_eq!((first.frames, first.cover, first.looks), (&[32, 33][..], 0, [34, 0, 0]));
    let second = &objects[1];
    assert_eq!((second.number, second.kind, second.layer, second.cover, second.frames.len()), (2, ObjectKind::Region, 1, 0, 0));
    // The stream begun, and its objects given again: they are of it.
    let stream = json!({"type": "naplps", "connections": [CONNECTION], "page": "170", "sub": "0001", "restart": true, "data": base64(b"")});
    channel.from_program(&line(&stream), 2);
    channel.from_program(&line(&example("naplps_objects")), 3);
    let sent = records(&all_sent_to(&channel.take_effects(), CONNECTION));
    let again = sent.iter().find(|(kind, _)| *kind == TYPE_NAPLPS_OBJECTS).unwrap();
    assert_eq!(naplps_objects::read(&again.1).unwrap().generation, 1);
}

/// REQ-6533, ANIM-21, ANIM-17: `sprite_sheet` is SHEET records of one
/// stream for the page and kind, which `restart` begins again at a new
/// generation.
#[test]
fn a_sprite_sheet_is_sheet_records() {
    let mut channel = showing(0x150);
    let sheet_of = |data: &[u8], restart: bool| {
        line(
            &json!({"type": "sprite_sheet", "connections": [CONNECTION], "page": "150", "sub": "0001", "kind": "regis", "data": base64(data), "restart": restart}),
        )
    };
    channel.from_program(&sheet_of(b"D(S1(V0),", true), 1);
    channel.from_program(&sheet_of(b"C)", false), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), []);
    let sent = records(&all_sent_to(&effects, CONNECTION));
    let read: Vec<_> = sent.iter().map(|(kind, body)| (*kind, sheet::read(body).unwrap())).collect();
    assert_eq!(read.len(), 2);
    assert_eq!(
        (read[0].0, read[0].1.page.mpp(), read[0].1.kind, read[0].1.generation, read[0].1.offset, read[0].1.data),
        (TYPE_SHEET, 0x150, SheetKind::Regis, 1, 0, &b"D(S1(V0),"[..])
    );
    assert_eq!((read[1].1.generation, read[1].1.offset, read[1].1.data), (1, 9, &b"C)"[..]));
    channel.from_program(&line(&example("sprite_sheet")), 3);
    let sent = records(&all_sent_to(&channel.take_effects(), CONNECTION));
    let again = sheet::read(&sent[0].1).unwrap();
    assert_eq!((again.generation, again.offset, again.data), (2, 0, REGIS_SHEET));
}

/// REQ-6533, ANIM-17: the server reads a sub-page's ReGIS after its page's
/// sheet, as a receiver's renderer is fed them, so that a commit that
/// fails only with the sheet's drawings is refused here first: the sheet's
/// hidden drawing 1 takes its contents from drawing 2, and a stream whose
/// drawing 2 takes its own from drawing 1 closes a ring.
#[test]
fn a_regis_stream_is_read_after_its_pages_sheet() {
    let stream =
        |data: &[u8]| line(&json!({"type": "regis", "connections": [CONNECTION], "page": "150", "sub": "0001", "restart": true, "data": base64(data)}));
    let closes_the_ring = b"D(S2(R1),C)";
    let mut channel = showing(0x150);
    channel.from_program(&stream(closes_the_ring), 1);
    assert_eq!(refusals(&channel.take_effects()), [], "without the sheet there is no ring");
    let sheet = json!({"type": "sprite_sheet", "connections": [CONNECTION], "page": "150", "sub": "0001", "kind": "regis", "data": base64(b"D(S1(V0,R2),C)"), "restart": true});
    channel.from_program(&line(&sheet), 2);
    assert_eq!(refusals(&channel.take_effects()), []);
    channel.from_program(&stream(closes_the_ring), 3);
    assert_eq!(refusals(&channel.take_effects()), named("invalid_regis", "data"));
}

/// REQ-6533, ANIM-2, ANIM-21: between a `frame` begin and its end,
/// everything that changes what the page shows is held, and at the end
/// goes between the two FRAME records in one message; sound passes as it
/// comes.
#[test]
fn a_frame_is_held_to_its_end() {
    let mut channel = showing(0x150);
    let mark = |mark: &str| line(&json!({"type": "frame", "connections": [CONNECTION], "page": "150", "sub": "0001", "mark": mark}));
    channel.from_program(&mark("begin"), 1);
    channel.from_program(&line(&example("animate")), 2);
    channel.from_program(&line(&example("sprite_sheet")), 3);
    let table = json!({"rules": [{"name": "typed", "on": {"event": "text"}, "do": [{"type": "report"}]}]});
    channel.from_program(&line(&json!({"type": "rules", "connections": [CONNECTION], "page": "150", "sub": "0001", "table": table})), 4);
    let page = with(&with(&subpage(), "/page", json!("150")), "/sub", json!("0001"));
    channel.from_program(&line(&json!({"type": "page", "connections": [CONNECTION], "request": 1, "page": page})), 5);
    let stop = json!({"type": "sound", "kind": "stop", "connections": [CONNECTION], "channel": 0, "fade_ms": 5});
    channel.from_program(&line(&stop), 6);
    let held = channel.take_effects();
    assert_eq!(refusals(&held), []);
    assert_eq!(kinds(&held), [neoteletext_audio::record::TYPE_CONTROL], "the sound's command alone: the rest is held");
    channel.from_program(&mark("end"), 7);
    let effects = channel.take_effects();
    let messages = effects.iter().filter(|effect| matches!(effect, Effect::Send { .. })).count();
    assert_eq!(messages, 1, "the whole frame in one message");
    let sent = records(&all_sent_to(&effects, CONNECTION));
    let types: Vec<u8> = sent.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(types[..4], [TYPE_FRAME, TYPE_ANIMATE, TYPE_SHEET, TYPE_RULES]);
    assert_eq!(types.last(), Some(&TYPE_FRAME));
    let (begin, end) = (frame::read(&sent[0].1).unwrap(), frame::read(&sent.last().unwrap().1).unwrap());
    assert_eq!((begin.page.mpp(), begin.subcode.value(), begin.mark, end.mark), (0x150, 1, FrameMark::Begin, FrameMark::End));
    assert_eq!(begin.identifier, end.identifier, "the end repeats its begin's identifier");
    // The page's own packets are inside it too.
    assert!(sent_to(&effects, CONNECTION).len() > all_sent_to(&effects, CONNECTION).len() / 2);
    // The next frame has another identifier.
    channel.from_program(&mark("begin"), 8);
    channel.from_program(&mark("end"), 9);
    let next = records(&all_sent_to(&channel.take_effects(), CONNECTION));
    assert_ne!(frame::read(&next[0].1).unwrap().identifier, begin.identifier);
}

/// REQ-6533, ANIM-21: an end with no begin, and a begin inside a frame,
/// are refused `frame_order`; a frame left open is ended by the server
/// after two seconds, or when its program ends, so that a fault delays a
/// picture and never hides one.
#[test]
fn frames_are_paired_or_ended_by_the_server() {
    let mark = |mark: &str| line(&json!({"type": "frame", "connections": [CONNECTION], "page": "150", "sub": "0001", "mark": mark}));
    let mut channel = showing(0x150);
    channel.from_program(&mark("end"), 1);
    assert_eq!(refusals(&channel.take_effects()), named("frame_order", "mark"));
    channel.from_program(&mark("begin"), 1_000);
    channel.from_program(&mark("begin"), 1_001);
    assert_eq!(refusals(&channel.take_effects()), named("frame_order", "mark"));
    channel.from_program(&line(&example("animate")), 1_002);
    assert_eq!(kinds(&channel.take_effects()), [] as [u8; 0]);
    assert!(channel.next_wake().is_some_and(|wake| wake <= 1_000 + FRAME_LIMIT_MS));
    channel.deadline(1_000 + FRAME_LIMIT_MS - 1);
    assert_eq!(kinds(&channel.take_effects()), [] as [u8; 0]);
    channel.deadline(1_000 + FRAME_LIMIT_MS);
    assert_eq!(kinds(&channel.take_effects()), [TYPE_FRAME, TYPE_ANIMATE, TYPE_FRAME], "ended by the server, with what it held");
    channel.from_program(&mark("end"), 5_000);
    assert_eq!(refusals(&channel.take_effects()), named("frame_order", "mark"), "it was ended already");
    // The program ends with a frame open.
    channel.from_program(&mark("begin"), 6_000);
    channel.from_program(&line(&example("animate")), 6_001);
    let _ = channel.take_effects();
    channel.program_ended();
    // The frame's end, then the page's table dropped (RULE-111 case 3).
    assert_eq!(kinds(&channel.take_effects()), [TYPE_FRAME, TYPE_ANIMATE, TYPE_FRAME, TYPE_RULES]);
    // A frame's sub-page is one that can be shown.
    let mut channel = showing(0x150);
    channel.from_program(&line(&json!({"type": "frame", "connections": [CONNECTION], "page": "150", "sub": "3F7F", "mark": "begin"})), 1);
    assert_eq!(refusals(&channel.take_effects()), named("out_of_range", "sub"));
}

/// REQ-6533, ANIM-3, ANIM-21: a frame whose records pass 262,144 bytes is
/// refused `frame_too_large`, and what it held is sent as it is, with no
/// frame.
#[test]
fn a_frame_too_large_is_sent_unframed_and_refused() {
    let mut channel = showing(0x150);
    let mark = |mark: &str| line(&json!({"type": "frame", "connections": [CONNECTION], "page": "150", "sub": "0001", "mark": mark}));
    channel.from_program(&mark("begin"), 1);
    // A megabyte of ReGIS in one message: more packets than a frame holds.
    let regis = json!({"type": "regis", "connections": [CONNECTION], "page": "150", "sub": "0001", "restart": true, "data": base64(&alloc::vec![b' '; MAX_FRAME_BYTES])});
    channel.from_program(&line(&regis), 2);
    let effects = channel.take_effects();
    assert_eq!(refusals(&effects), named("frame_too_large", ""));
    let sent = records(&all_sent_to(&effects, CONNECTION));
    assert!(!sent.is_empty() && sent.iter().all(|(kind, _)| *kind != TYPE_FRAME), "what it held, with no frame");
    channel.from_program(&mark("end"), 3);
    assert_eq!(refusals(&channel.take_effects()), named("frame_order", "mark"), "the frame was given up");
}

/// REQ-6533, ANIM-21: each refusal of an animation message, with its rule
/// and the path of its fault; nothing of a refused message is sent.
#[test]
fn each_fault_of_animation_is_refused() {
    let mut channel = showing(0x150);
    let mut refused = |message: Value| {
        channel.from_program(&line(&message), 1);
        let effects = channel.take_effects();
        assert_eq!(all_sent_to(&effects, CONNECTION), [] as [u8; 0], "{message}");
        refusals(&effects)
    };
    let animate = |op: Value| json!({"type": "animate", "connections": [CONNECTION], "page": "150", "sub": "0001", "ops": [op]});
    // invalid_animation: on "any", on the page or a rectangle, on a
    // control where an object is needed, and a cycle that runs backwards.
    assert_eq!(refused(animate(json!({"action": "show", "target": {"object": "any"}}))), named("invalid_animation", "ops[0].target"));
    assert_eq!(refused(animate(json!({"action": "hide", "target": "page"}))), named("invalid_animation", "ops[0].target"));
    assert_eq!(refused(animate(json!({"action": "reset", "target": {"rect": "pad"}}))), named("invalid_animation", "ops[0].target"));
    assert_eq!(refused(animate(json!({"action": "frame", "target": {"control": 2}, "frame": 1}))), named("invalid_animation", "ops[0].target"));
    let backwards = json!({"action": "cycle", "target": {"object": 1}, "first": 3, "last": 2, "period": 6});
    assert_eq!(refused(animate(backwards)), named("invalid_animation", "ops[0].first"));
    // What the schema states is refused by its own rule.
    assert_eq!(refused(animate(json!({"action": "frame", "target": {"object": 1}, "frame": 16}))), named("out_of_range", "ops[0].frame"));
    assert_eq!(refused(json!({"type": "animate", "connections": [CONNECTION], "page": "150", "sub": "0001", "ops": []})), named("too_short", "ops"));
    // invalid_sprites: a number given twice.
    let twice = with(&example("sprites"), "/sprites", json!([example("sprites")["sprites"][0], example("sprites")["sprites"][0]]));
    assert_eq!(refused(twice), named("invalid_sprites", "sprites[1].number"));
    assert_eq!(refused(with(&example("sprites"), "/sprites/0/frames", json!([]))), named("invalid_sprites", "sprites[0].frames"));
    assert_eq!(refused(with(&example("sprites"), "/sprites/0/layer", json!(0))), named("out_of_range", "sprites[0].layer"));
    // invalid_naplps_objects: a region with frames, a macro object with
    // none or with a cover, a number given twice.
    assert_eq!(refused(with(&example("naplps_objects"), "/objects/1/frames", json!([32]))), named("invalid_naplps_objects", "objects[1].frames"));
    assert_eq!(refused(with(&example("naplps_objects"), "/objects/0/frames", json!([]))), named("invalid_naplps_objects", "objects[0].frames"));
    assert_eq!(refused(with(&example("naplps_objects"), "/objects/0/cover", json!(3))), named("invalid_naplps_objects", "objects[0].cover"));
    assert_eq!(refused(with(&example("naplps_objects"), "/objects/1/number", json!(1))), named("invalid_naplps_objects", "objects[1].number"));
    assert_eq!(refused(with(&example("naplps_objects"), "/objects/0/hover", json!(31))), named("out_of_range", "objects[0].hover"));
    // invalid_sheet: ReGIS whose commit fails, and NAPLPS that draws.
    let sheet = |kind: &str, data: &[u8]| json!({"type": "sprite_sheet", "connections": [CONNECTION], "page": "150", "sub": "0001", "kind": kind, "data": base64(data), "restart": true});
    assert_eq!(refused(sheet("regis", b"D(S1(H2),S2(A1),C)")), named("invalid_sheet", "data"));
    // SO, then RECT with two operands: half the screen across, a quarter
    // up (ANSI X3.110-1983 section 5.3.1.4).
    assert_eq!(refused(sheet("naplps", &[0x0E, 0x31, 0x51, 0x40, 0x40])), named("invalid_sheet", "data"));
    // sheet_too_large: in one message, and over several.
    assert_eq!(refused(sheet("regis", &alloc::vec![b' '; MAX_SHEET_BYTES + 1])), named("sheet_too_large", "data"));
    channel.from_program(&line(&sheet("naplps", &[0x80, 0x21, 0x0E, 0x31, 0x51, 0x40, 0x40, 0x85])), 1);
    assert_eq!(refusals(&channel.take_effects()), [], "the same rectangle defined as a macro draws nothing");
    channel.from_program(&line(&sheet("regis", &alloc::vec![b' '; MAX_SHEET_BYTES])), 2);
    assert_eq!(refusals(&channel.take_effects()), []);
    let more = with(&sheet("regis", b" "), "/restart", json!(false));
    channel.from_program(&line(&more), 3);
    assert_eq!(refusals(&channel.take_effects()), named("sheet_too_large", "data"));
    // A timeline whose steps do not ascend, or reach the tick it begins
    // again at.
    let timeline = example("timeline");
    channel.from_program(&line(&with(&timeline, "/steps/1/tick", json!(0))), 4);
    assert_eq!(refusals(&channel.take_effects()), named("not_ascending", "steps[1].tick"));
    channel.from_program(&line(&with(&timeline, "/loop_ticks", json!(30))), 5);
    assert_eq!(refusals(&channel.take_effects()), named("invalid_animation", "steps[1].tick"));
    channel.from_program(&line(&with(&timeline, "/steps/0/ops/0/target", json!({"control": "any"}))), 6);
    assert_eq!(refusals(&channel.take_effects()), named("invalid_animation", "steps[0].ops[0].target"));
}

/// REQ-6533, ANIM-21: a seventeenth timeline playing for a page is
/// refused `too_many_timelines`; a timeline with a playing one's number
/// replaces it.
#[test]
fn a_page_has_at_most_sixteen_timelines() {
    let mut channel = showing(0x150);
    let looping = with(&example("timeline"), "/loop_ticks", json!(60));
    for id in 1..=16 {
        channel.from_program(&line(&with(&looping, "/id", json!(id))), 1);
    }
    assert_eq!(refusals(&channel.take_effects()), []);
    channel.from_program(&line(&with(&looping, "/id", json!(17))), 2);
    assert_eq!(refusals(&channel.take_effects()), named("too_many_timelines", "id"));
    channel.from_program(&line(&with(&looping, "/id", json!(16))), 3);
    assert_eq!(refusals(&channel.take_effects()), [], "it replaces the one playing");
    channel.from_program(&line(&example("timeline_stop")), 4);
    channel.from_program(&line(&with(&looping, "/id", json!(17))), 5);
    assert_eq!(refusals(&channel.take_effects()), [], "one was stopped");
}

/// REQ-6533, ANIM-21: the server plays a timeline on its own clock: each
/// step's ANIMATE record goes when its tick is due, counted from the
/// message's arrival at a sixtieth of a second a tick, and again from the
/// start every `loop_ticks`, until it is stopped.
#[test]
fn a_timeline_is_played_on_the_servers_clock() {
    const ARRIVED: u64 = 1_000;
    let mut channel = showing(0x150);
    let show = [object(10, OperationKind::Show)];
    let moved = [object(10, OperationKind::Move { relative: true, x: 8, y: 0 })];
    let played = |channel: &mut Channel| -> Vec<Vec<Operation>> {
        records(&all_sent_to(&channel.take_effects(), CONNECTION)).iter().filter(|(kind, _)| *kind == TYPE_ANIMATE).map(|(_, body)| operations(body)).collect()
    };
    // The example: a step at tick 0 and one at tick 30, played once.
    channel.from_program(&line(&example("timeline")), ARRIVED);
    assert_eq!(played(&mut channel), [show.to_vec()], "tick 0 is due as it arrives");
    assert_eq!(tick_ms(30), 500);
    assert_eq!(channel.timelines.next_due(), Some(ARRIVED + 500));
    assert!(channel.next_wake().is_some_and(|wake| wake <= ARRIVED + 500));
    channel.deadline(ARRIVED + 499);
    assert_eq!(played(&mut channel), [] as [Vec<Operation>; 0]);
    channel.deadline(ARRIVED + 500);
    assert_eq!(played(&mut channel), [moved.to_vec()]);
    channel.deadline(ARRIVED + 60_000);
    assert_eq!(played(&mut channel), [] as [Vec<Operation>; 0], "it played once");
    // Looping every 60 ticks: tick 0 again a second after it began.
    let begun = 100_000;
    channel.from_program(&line(&with(&example("timeline"), "/loop_ticks", json!(60))), begun);
    assert_eq!(played(&mut channel), [show.to_vec()]);
    channel.deadline(begun + 500);
    assert_eq!(played(&mut channel), [moved.to_vec()]);
    channel.deadline(begun + 999);
    assert_eq!(played(&mut channel), [] as [Vec<Operation>; 0]);
    channel.deadline(begun + 1_000);
    assert_eq!(played(&mut channel), [show.to_vec()]);
    channel.deadline(begun + 1_500);
    assert_eq!(played(&mut channel), [moved.to_vec()]);
    // A host that slept through ten turns sends one turn's steps, not ten.
    channel.deadline(begun + 11_600);
    assert_eq!(played(&mut channel).len(), 2);
    channel.deadline(begun + 12_000);
    assert_eq!(played(&mut channel), [show.to_vec()], "and goes on in step with its start");
    channel.from_program(&line(&example("timeline_stop")), begun + 12_100);
    channel.deadline(begun + 60_000);
    assert_eq!(played(&mut channel), [] as [Vec<Operation>; 0], "stopped");
    // A connection that closes is played nothing.
    channel.from_program(&line(&with(&example("timeline"), "/loop_ticks", json!(60))), 200_000);
    channel.closed(CONNECTION, 200_001);
    assert_eq!(channel.timelines.next_due(), None);
}

/// REQ-6533, ANIM-21: a record of animation cannot be sent as raw packets,
/// past the check.
#[test]
fn an_animation_record_in_raw_packets_is_refused() {
    let mut channel = showing(0x150);
    channel.from_program(&line(&example("animate")), 1);
    let record = all_sent_to(&channel.take_effects(), CONNECTION);
    channel.from_program(&line(&json!({"type": "packets", "connections": [CONNECTION], "packets": base64(&record[..PACKET_BYTES])})), 2);
    assert_eq!(refusals(&channel.take_effects()), named("invalid_packet", "packets[0]"));
}

/// The packets of a row of the engine's wire vectors of kind `records`:
/// each 42 bytes in hexadecimal, separated by a space.
fn vector_packets(name: &str) -> Vec<u8> {
    let row = neoteletext_web::wire_vectors::vectors().find(|row| row.kind == "records" && row.name == name).unwrap_or_else(|| panic!("no records {name}"));
    row.value
        .split(' ')
        .flat_map(|packet| {
            let (bytes, length) = neoteletext_web::wire_vectors::bytes_of(packet).unwrap();
            bytes[..length].to_vec()
        })
        .collect()
}

/// REQ-6533, ANIM-23, RULE-85: the engine's wire vectors of the five
/// records of animation. What the server writes for the same frame, the
/// same operations and the same sheet is byte for byte the vector, and the
/// vectors of sprites and of NAPLPS objects are read by the reader the
/// server checks its own with.
#[test]
fn the_servers_records_are_the_engines_wire_vectors() {
    use crate::protocol::{FrameMarkName, SheetKindName};
    for (name, mark) in [("frame_begin", FrameMarkName::Begin), ("frame_end", FrameMarkName::End)] {
        let vector = vector_packets(name);
        let read = frame::read(&records(&vector)[0].1).unwrap();
        let written = crate::animation::frame_record(read.page.mpp(), read.subcode.value(), mark, read.identifier, &mut 0).unwrap();
        assert_eq!(written, vector, "{name}");
    }
    let vector = vector_packets("animate");
    let body = &records(&vector)[0].1;
    let read = animate::read(body).unwrap();
    let sub = read.subcode.value();
    assert_eq!(crate::animation::animate_records(read.page.mpp(), sub, &operations(body), &mut 0).unwrap(), vector);
    let vector = vector_packets("sheet");
    let read_records = records(&vector);
    let read = sheet::read(&read_records[0].1).unwrap();
    assert_eq!(read.kind, SheetKind::Regis);
    let written =
        crate::animation::sheet_records((read.page.mpp(), read.subcode.value()), SheetKindName::Regis, read.generation, read.offset, read.data, &mut 0)
            .unwrap();
    assert_eq!(written, vector);
    assert_eq!(sprites::read(&records(&vector_packets("sprites"))[0].1).map(|record| record.count), Ok(1));
    assert!(naplps_objects::read(&records(&vector_packets("naplps_objects"))[0].1).is_ok());
}

/// A `sprite_sheet` message of a ReGIS sheet for page 100 and `sub`.
fn sheet_message(sub: &str, data: &[u8], restart: bool) -> Vec<u8> {
    line(&json!({"type":"sprite_sheet","connections":"all","page":"100","sub":sub,"kind":"regis","data":base64(data),"restart":restart}))
}

/// The refusals among `effects`, by rule.
fn rules_refused(effects: &[Effect]) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == "refused").map(|line| line["rule"].clone()).collect()
}

/// A ReGIS sheet as it should be: a hidden master, then a commit.
const MASTER: &[u8] = b"D(B41)P[10,10]V[20,20]D(E)D(S41(V0),C)";

/// ANIM-17, DEC-6599 (REQ-6533): a sheet belongs to one page and
/// sub-code: it reaches the connection that has that page selected and no
/// other sub-code of the page number, and a sheet that leaves a drawing
/// visible is refused `invalid_sheet` with the ReGIS renderer's own
/// validator (TODO-6570).
#[test]
fn a_sheet_is_one_pages_and_draws_nothing() {
    let mut channel = selected();
    channel.from_program(&sheet_message("0001", MASTER, true), 1);
    let effects = channel.take_effects();
    assert!(!record_types(&sent_to(&effects, CONNECTION)).contains(&TYPE_SHEET), "another sub-code is another page");
    channel.from_program(&sheet_message("0000", MASTER, true), 2);
    let effects = channel.take_effects();
    assert_eq!(rules_refused(&effects), [] as [Value; 0]);
    assert!(record_types(&sent_to(&effects, CONNECTION)).contains(&TYPE_SHEET));
    channel.from_program(&sheet_message("0000", b"D(B42)P[10,10]V[20,20]D(E)D(C)", true), 3);
    assert_eq!(rules_refused(&channel.take_effects()), [json!("invalid_sheet")]);
}

/// ANIM-21, RULE-111 (REQ-6533): a timeline ends for a connection that
/// leaves its page: a later step of it is never sent there.
#[test]
fn a_timeline_ends_when_its_page_is_left() {
    let mut channel = selected();
    let timeline = json!({"type":"timeline","connections":"all","page":"100","sub":"0000","id":1,
        "steps":[{"tick":60,"ops":[{"action":"show","target":{"object":1}}]}]});
    channel.from_program(&serde_json::to_vec(&timeline).unwrap(), 0);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 1)]), 10);
    let _ = channel.take_effects();
    channel.deadline(2_000);
    let effects = channel.take_effects();
    assert!(!record_types(&sent_to(&effects, CONNECTION)).contains(&TYPE_ANIMATE), "the step went to a page left");
}

/// ANIM-17, TODO-6569 (REQ-6533): an object page reaches the connections
/// that have its page selected, as packets, and no other.
#[test]
fn an_object_page_reaches_its_pages_viewers() {
    let mut channel = selected();
    let object = with(&subpage(), "/page", json!("1C0"));
    let to = |page: &str| serde_json::to_vec(&json!({"type":"object_page","connections":"all","page":page,"sub":"0000","object":object})).unwrap();
    channel.from_program(&to("200"), 1);
    assert_eq!(sent_to(&channel.take_effects(), CONNECTION), Vec::<u8>::new());
    channel.from_program(&to("100"), 2);
    let effects = channel.take_effects();
    assert_eq!(rules_refused(&effects), [] as [Value; 0]);
    assert_ne!(sent_to(&effects, CONNECTION), [] as [u8; 0]);
}
