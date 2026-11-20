//! Viewer state on a channel (DEC-6471): the opening challenge and
//! `connected` (VS-202), proofs and their refusals (VS-205, VS-206,
//! VS-301), the handle (VS-305, VS-207, VS-211), the program's requests
//! (VS-212, VS-352, VS-353), the table's bounds and idling (VS-303,
//! VS-304), rotation (VS-306), `gone` (VS-354) and what is logged (VS-209).

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_core::consts::PACKET_BYTES;
use neoteletext_core::packet::Packet;
use neoteletext_input::event::{Action, Event, Item};
use neoteletext_input::state::{Algorithm, Lifetime, NoneReason, StateProof};
use neoteletext_web::records::{ChannelRecord, Read, RecordReader};
use neoteletext_web::viewer_state;
use serde_json::{Value, json};

use super::{
    ACKNOWLEDGEMENT_WAIT_MS, ANY, CONNECTION, Channel, ChannelConfig, Effect, InitialPage, ON, bare, connecteds, hello, key, line, message, select,
    select_with, sent_to, told,
};
use crate::viewer::testing::{random, secret};
use crate::viewer::tests::sign;
use crate::viewer::{
    CHALLENGE_BYTES, HandleSecret, MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE, MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR, STATE_ANSWER_MS, STATE_GESTURE_MS,
    VIEWER_IDLE_MS,
};

/// The channel's address.
const ADDRESS: &str = "http://127.0.0.1/test/";
/// A viewer's seed.
const ALICE: [u8; 32] = [1; 32];
/// Another's.
const BOB: [u8; 32] = [2; 32];

fn config(address: &str) -> ChannelConfig {
    ChannelConfig {
        name: "Test".into(),
        address: address.into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        // A key press is heard, as a gesture (VS-212): the pages have the
        // default table.
        default_rules: crate::channel::DefaultRules::SendAll,
    }
}

/// A running channel at `address`, under `secret`.
fn channel_at(address: &str, secret: &HandleSecret) -> Channel {
    let mut channel = Channel::new(config(address), secret, random());
    channel.program_started(0);
    let _ = channel.take_effects();
    channel
}

/// The viewer-state records in `bytes`.
fn state_records(bytes: &[u8]) -> Vec<ChannelRecord<'static>> {
    let mut reader = RecordReader::new();
    let mut found = Vec::new();
    for packet in bytes.as_chunks::<PACKET_BYTES>().0 {
        match reader.push(&Packet { bytes: *packet }) {
            Read::Record(ChannelRecord::StateChallenge(challenge)) => found.push(ChannelRecord::StateChallenge(challenge)),
            Read::Record(ChannelRecord::StateEnd(end)) => found.push(ChannelRecord::StateEnd(end)),
            Read::Record(ChannelRecord::StateKeepOffer) => found.push(ChannelRecord::StateKeepOffer),
            _ => {}
        }
    }
    found
}

/// The challenges `effects` send `connection`, with whether each asks to
/// create.
fn challenges(effects: &[Effect], connection: u64) -> Vec<([u8; CHALLENGE_BYTES], bool)> {
    state_records(&sent_to(effects, connection))
        .into_iter()
        .filter_map(|record| if let ChannelRecord::StateChallenge(challenge) = record { Some((challenge.challenge, challenge.create)) } else { None })
        .collect()
}

/// A connection opened and said HELLO: its opening challenge.
fn open(channel: &mut Channel, connection: u64, now: u64) -> [u8; CHALLENGE_BYTES] {
    channel.opened(connection, None, now);
    channel.received(connection, &message(channel, connection, &[hello()]), now);
    let sent = challenges(&channel.take_effects(), connection);
    assert_eq!(sent.len(), 1, "one challenge after WELCOME");
    assert!(!sent[0].1, "the opening challenge does not ask to create");
    sent[0].0
}

/// The proof of `seed` for `challenge` on `address`, as one message.
fn proof(seed: &[u8; 32], lifetime: Lifetime, challenge: &[u8; CHALLENGE_BYTES], address: &str) -> Vec<u8> {
    let (public_key, signature) = sign(seed, lifetime, challenge, address);
    bare(&[Item::Event(Event::StateProof(StateProof { algorithm: Algorithm::Ed25519, lifetime, public_key: &public_key, signature: &signature }))])
}

fn none(reason: NoneReason) -> Vec<u8> {
    bare(&[Item::Event(Event::StateNone(reason))])
}

/// The lines of `effects` of type `kind`.
fn lines_of(effects: &[Effect], kind: &str) -> Vec<Value> {
    told(effects).into_iter().filter(|line| line["type"] == kind).collect()
}

/// A connection opened, proven with `seed` and selecting page 100: its
/// handle as `connected` gave it.
fn proven(channel: &mut Channel, connection: u64, seed: &[u8; 32], now: u64) -> Value {
    let challenge = open(channel, connection, now);
    channel.received(connection, &proof(seed, Lifetime::Visit, &challenge, ADDRESS), now);
    channel.received(connection, &message(channel, connection, &[select(0x100, 0)]), now);
    let lines = connecteds(&channel.take_effects());
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines[0]["viewer"].clone()
}

fn is_handle(value: &Value) -> bool {
    value.as_str().is_some_and(|text| text.len() == 32 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
}

/// VS-202, VS-351 item 1: the first record after WELCOME is the challenge,
/// `connected` waits for its answer, and a proof gives it a handle.
#[test]
fn connected_waits_for_the_answer_and_carries_the_handle() {
    let mut channel = channel_at(ADDRESS, &secret());
    let challenge = open(&mut channel, CONNECTION, 0);
    channel.deadline(super::FIRST_SELECTION_WAIT_MS);
    assert_eq!(connecteds(&channel.take_effects()), [] as [Value; 0], "the first selection's wait is over; the answer's is not");
    channel.received(CONNECTION, &proof(&ALICE, Lifetime::Remembered, &challenge, ADDRESS), 10);
    let lines = connecteds(&channel.take_effects());
    assert_eq!(lines.len(), 1);
    assert!(is_handle(&lines[0]["viewer"]), "{}", lines[0]);
    assert_eq!(lines[0]["remembered"], json!(true));
    assert!(lines[0].get("session").is_none() && lines[0].get("cookies").is_none());
    assert_eq!(channel.stats().proofs_accepted, 1);
}

/// VS-202: a SELECT before the answer is an answer with no key; `STATE_NONE`
/// is no viewer; and no answer at all is no viewer after `STATE_ANSWER_MS`.
#[test]
fn a_connection_without_a_proof_has_no_viewer() {
    let mut channel = channel_at(ADDRESS, &secret());
    open(&mut channel, 1, 0);
    channel.received(1, &message(&channel, 1, &[select(0x100, ANY)]), 0);
    let lines = connecteds(&channel.take_effects());
    assert_eq!((&lines[0]["viewer"], &lines[0]["remembered"]), (&Value::Null, &json!(false)));

    open(&mut channel, 2, 0);
    channel.received(2, &none(NoneReason::Stateless), 0);
    channel.received(2, &message(&channel, 2, &[select(0x100, ANY)]), 0);
    assert_eq!(connecteds(&channel.take_effects())[0]["viewer"], Value::Null);

    let challenge = open(&mut channel, 3, 0);
    channel.deadline(super::FIRST_SELECTION_WAIT_MS);
    assert_eq!(connecteds(&channel.take_effects()), [] as [Value; 0], "the answer is still awaited");
    channel.deadline(STATE_ANSWER_MS);
    let lines = connecteds(&channel.take_effects());
    assert_eq!(lines.len(), 1, "told when the answer's wait is over");
    assert_eq!(lines[0]["viewer"], Value::Null);
    channel.received(3, &proof(&ALICE, Lifetime::Visit, &challenge, ADDRESS), STATE_ANSWER_MS + 1);
    assert_eq!(lines_of(&channel.take_effects(), "viewer_began"), [] as [Value; 0], "a proof after its time is refused (VS-205)");
}

/// VS-206: a proof that does not verify is no viewer, counted, and the
/// connection stays open; VS-205: a proof with no challenge outstanding,
/// such as a second for one challenge, is refused.
#[test]
fn a_failed_or_unasked_proof_is_refused_and_the_connection_stays() {
    let mut channel = channel_at(ADDRESS, &secret());
    let challenge = open(&mut channel, CONNECTION, 0);
    channel.received(CONNECTION, &proof(&ALICE, Lifetime::Visit, &challenge, "http://127.0.0.1/other/"), 0);
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, ANY)]), 0);
    let effects = channel.take_effects();
    assert_eq!(connecteds(&effects)[0]["viewer"], Value::Null);
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Close { .. })), "not closed for that alone");
    assert_eq!(channel.stats().proofs_refused, 1);
    channel.received(CONNECTION, &proof(&ALICE, Lifetime::Visit, &challenge, ADDRESS), 1);
    assert_eq!(told(&channel.take_effects()), [] as [Value; 0], "the challenge was used");
    assert_eq!(channel.stats().proofs_refused, 2);
}

/// VS-204's preamble: a malformed state message is a protocol error.
#[test]
fn a_malformed_state_message_closes_the_connection() {
    let mut channel = channel_at(ADDRESS, &secret());
    open(&mut channel, CONNECTION, 0);
    // STATE_NONE with reason 5, which no reason has: 0xFF, the type, the
    // length, the reason.
    channel.received(CONNECTION, &[0xFF, neoteletext_input::event::STATE_NONE, 1, 5], 0);
    assert!(channel.take_effects().iter().any(|effect| matches!(effect, Effect::Close { .. })));
}

/// VS-111, VS-207, VS-303, VS-305: one key is one handle on every
/// connection and after a restart; VS-211: another channel's handle of the
/// same key differs; another key's differs.
#[test]
fn one_key_is_one_handle_on_its_channel_alone() {
    let mut channel = channel_at(ADDRESS, &secret());
    let first = proven(&mut channel, 1, &ALICE, 0);
    let second = proven(&mut channel, 2, &ALICE, 0);
    let bob = proven(&mut channel, 3, &BOB, 0);
    assert!(is_handle(&first));
    assert_eq!(first, second, "two tabs, one viewer");
    assert_ne!(first, bob);
    let mut restarted = channel_at(ADDRESS, &secret());
    assert_eq!(proven(&mut restarted, 1, &ALICE, 0), first, "the same after a restart");
    let other = "http://127.0.0.1/other/";
    let mut elsewhere = channel_at(other, &secret());
    let challenge = open(&mut elsewhere, 1, 0);
    elsewhere.received(1, &proof(&ALICE, Lifetime::Visit, &challenge, other), 0);
    elsewhere.received(1, &message(&elsewhere, 1, &[select(0x100, 0)]), 0);
    let there = connecteds(&elsewhere.take_effects())[0]["viewer"].clone();
    assert!(is_handle(&there));
    assert_ne!(there, first, "channels cannot relate one another's handles");
}

/// A key press on page 100, which is a gesture (VS-212).
fn press(channel: &Channel) -> Vec<u8> {
    message(channel, CONNECTION, &[key(ON, Action::Press, false)])
}

/// A connection with no viewer on page 100, told to its program.
fn anonymous(channel: &mut Channel, connection: u64, now: u64) {
    open(channel, connection, now);
    channel.received(connection, &none(NoneReason::NoKey), now);
    channel.received(connection, &message(channel, connection, &[select(0x100, 0)]), now);
    let _ = channel.take_effects();
}

fn request_state(connection: u64) -> Vec<u8> {
    line(&json!({"type": "request_state", "connection": connection}))
}

/// The rule of the refusal among `effects`.
fn refusal(effects: &[Effect]) -> Option<Value> {
    lines_of(effects, "refused").first().map(|line| line["rule"].clone())
}

/// VS-212, VS-353: a state request needs the viewer's input within the
/// last 7,000 ms, and is refused without it; one in time challenges with
/// `create`, and the proof is `viewer_began`.
#[test]
fn a_state_request_needs_a_recent_gesture() {
    let mut channel = channel_at(ADDRESS, &secret());
    anonymous(&mut channel, CONNECTION, 0);
    channel.from_program(&request_state(CONNECTION), 1);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("no_gesture")), "no input yet");
    channel.received(CONNECTION, &press(&channel), 100);
    channel.from_program(&request_state(CONNECTION), 100 + STATE_GESTURE_MS + 1);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("no_gesture")), "7,001 ms after");
    channel.from_program(&request_state(CONNECTION), 100 + STATE_GESTURE_MS);
    let effects = channel.take_effects();
    assert_eq!(refusal(&effects), None);
    let sent = challenges(&effects, CONNECTION);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1, "it asks to create");
    channel.received(CONNECTION, &proof(&ALICE, Lifetime::Visit, &sent[0].0, ADDRESS), 7_200);
    let began = lines_of(&channel.take_effects(), "viewer_began");
    assert_eq!(began.len(), 1);
    assert!(is_handle(&began[0]["viewer"]));
    assert_eq!((&began[0]["connection"], &began[0]["remembered"]), (&json!(CONNECTION), &json!(false)));
    channel.received(CONNECTION, &press(&channel), 7_300);
    channel.from_program(&request_state(CONNECTION), 7_300);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("has_viewer")));
    channel.from_program(&request_state(9), 7_300);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("unknown_connection")));
}

/// VS-351 item 4: a state request answered without a key, or not at all,
/// is `state_refused` with the reason.
#[test]
fn a_state_request_not_granted_says_why() {
    let mut channel = channel_at(ADDRESS, &secret());
    anonymous(&mut channel, CONNECTION, 0);
    channel.received(CONNECTION, &press(&channel), 1);
    channel.from_program(&request_state(CONNECTION), 1);
    let _ = channel.take_effects();
    channel.received(CONNECTION, &none(NoneReason::Declined), 2);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused"), [json!({"type": "state_refused", "connection": CONNECTION, "reason": "declined"})]);
    channel.received(CONNECTION, &press(&channel), 3);
    channel.from_program(&request_state(CONNECTION), 3);
    let _ = channel.take_effects();
    channel.deadline(3 + STATE_ANSWER_MS);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused")[0]["reason"], json!("timeout"));
}

/// VS-205: at most one challenge outstanding, and at most six a minute on
/// a connection; past either, the request is `state_refused` `rate`.
#[test]
fn challenges_are_bounded() {
    let mut channel = channel_at(ADDRESS, &secret());
    anonymous(&mut channel, CONNECTION, 0);
    channel.received(CONNECTION, &press(&channel), 1);
    channel.from_program(&request_state(CONNECTION), 1);
    channel.from_program(&request_state(CONNECTION), 1);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused")[0]["reason"], json!("rate"), "one outstanding");
    // The opening challenge and this one are two of the minute's six.
    let mut sent = 2;
    let mut now = 1;
    while sent < MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE {
        channel.received(CONNECTION, &none(NoneReason::NoKey), now);
        now += 1;
        channel.received(CONNECTION, &press(&channel), now);
        channel.from_program(&request_state(CONNECTION), now);
        assert_eq!(challenges(&channel.take_effects(), CONNECTION).len(), 1);
        sent += 1;
    }
    channel.received(CONNECTION, &none(NoneReason::NoKey), now);
    channel.received(CONNECTION, &press(&channel), now);
    channel.from_program(&request_state(CONNECTION), now);
    let effects = channel.take_effects();
    assert_eq!(challenges(&effects, CONNECTION).len(), 0);
    assert_eq!(lines_of(&effects, "state_refused").last().unwrap()["reason"], json!("rate"));
}

/// VS-352 item 2, VS-353: `end_state` sends `STATE_END` naming the key to
/// every connection presenting the viewer, tells `viewer_left` for each,
/// and forgets the viewer; a handle the channel does not hold is refused.
#[test]
fn ending_state_reaches_every_connection_of_the_viewer() {
    let mut channel = channel_at(ADDRESS, &secret());
    let handle = proven(&mut channel, 1, &ALICE, 0);
    proven(&mut channel, 2, &ALICE, 0);
    proven(&mut channel, 3, &BOB, 0);
    let end = line(&json!({"type": "end_state", "viewer": handle}));
    channel.from_program(&end, 1);
    let effects = channel.take_effects();
    let key_id = viewer_state::key_id(Algorithm::Ed25519, &sign(&ALICE, Lifetime::Visit, &[0; CHALLENGE_BYTES], ADDRESS).0);
    for connection in [1, 2] {
        let records = state_records(&sent_to(&effects, connection));
        assert!(matches!(records.as_slice(), [ChannelRecord::StateEnd(end)] if end.key_id == key_id), "{connection}");
    }
    assert!(state_records(&sent_to(&effects, 3)).is_empty(), "Bob is not Alice");
    let left: Vec<Value> = lines_of(&effects, "viewer_left");
    assert_eq!(left, [json!({"type": "viewer_left", "connection": 1}), json!({"type": "viewer_left", "connection": 2})]);
    channel.from_program(&end, 2);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("unknown_viewer")));
}

fn offer_remember(connection: u64) -> Vec<u8> {
    line(&json!({"type": "offer_remember", "connection": connection}))
}

/// VS-352 item 3, VS-208: `STATE_KEEP_OFFER` once, to a visit key; refused
/// with no viewer, a second time, or for a remembered key.
#[test]
fn an_offer_to_remember_goes_once_to_a_visit_key() {
    let mut channel = channel_at(ADDRESS, &secret());
    anonymous(&mut channel, 1, 0);
    channel.from_program(&offer_remember(1), 1);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused")[0]["reason"], json!("no_key"));
    proven(&mut channel, 2, &ALICE, 0);
    channel.from_program(&offer_remember(2), 1);
    let effects = channel.take_effects();
    assert!(matches!(state_records(&sent_to(&effects, 2)).as_slice(), [ChannelRecord::StateKeepOffer]));
    channel.from_program(&offer_remember(2), 2);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused")[0]["reason"], json!("rate"));
    let challenge = open(&mut channel, 3, 0);
    channel.received(3, &proof(&BOB, Lifetime::Remembered, &challenge, ADDRESS), 0);
    let _ = channel.take_effects();
    channel.from_program(&offer_remember(3), 3);
    assert_eq!(lines_of(&channel.take_effects(), "state_refused")[0]["reason"], json!("declined"));
    channel.from_program(&offer_remember(9), 3);
    assert_eq!(refusal(&channel.take_effects()), Some(json!("unknown_connection")));
}

/// VS-113, VS-111: `STATE_NONE` `forgotten` unasked is `viewer_left`; a
/// `STATE_OFFER` is challenged, and its proof `viewer_began`.
#[test]
fn a_forgotten_key_leaves_and_an_offered_key_begins() {
    let mut channel = channel_at(ADDRESS, &secret());
    let handle = proven(&mut channel, CONNECTION, &ALICE, 0);
    channel.received(CONNECTION, &none(NoneReason::Forgotten), 1);
    assert_eq!(lines_of(&channel.take_effects(), "viewer_left"), [json!({"type": "viewer_left", "connection": CONNECTION})]);
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[Item::Event(Event::StateOffer)]), 1 + crate::viewer::STATE_OFFER_INTERVAL_MS);
    let sent = challenges(&channel.take_effects(), CONNECTION);
    assert_eq!(sent.len(), 1, "an offer is challenged");
    channel.received(CONNECTION, &proof(&BOB, Lifetime::Visit, &sent[0].0, ADDRESS), 20_000);
    let began = lines_of(&channel.take_effects(), "viewer_began");
    assert_eq!(began.len(), 1);
    assert_ne!(began[0]["viewer"], handle, "a new key after a reset is a new handle");
}

/// VS-354, VS-303: a closed connection is `gone`; its viewer is kept
/// `VIEWER_IDLE_MS` after its last connection closed, then forgotten.
#[test]
fn a_viewer_idles_after_its_last_connection_and_goes() {
    let mut channel = channel_at(ADDRESS, &secret());
    proven(&mut channel, CONNECTION, &ALICE, 0);
    assert_eq!(channel.viewers(), 1);
    channel.closed(CONNECTION, 10);
    assert_eq!(lines_of(&channel.take_effects(), "gone"), [json!({"type": "gone", "connection": CONNECTION})]);
    assert_eq!(channel.next_wake(), Some(10 + VIEWER_IDLE_MS));
    channel.deadline(9 + VIEWER_IDLE_MS);
    assert_eq!(channel.viewers(), 1);
    channel.deadline(10 + VIEWER_IDLE_MS);
    assert_eq!(channel.viewers(), 0);
}

/// VS-304: one network address brings at most 600 new viewers an hour;
/// the next proof of a new key is refused, a known key is not, and the
/// window starts again after the hour.
#[test]
fn new_viewers_from_one_address_are_bounded() {
    let address = Some(core::net::IpAddr::from([192, 0, 2, 1]));
    let mut channel = channel_at(ADDRESS, &secret());
    let mut connection = 0;
    let mut prove = |channel: &mut Channel, seed: [u8; 32], now: u64| {
        connection += 1;
        channel.opened(connection, address, now);
        channel.received(connection, &message(channel, connection, &[hello()]), now);
        let challenge = challenges(&channel.take_effects(), connection)[0].0;
        channel.received(connection, &proof(&seed, Lifetime::Visit, &challenge, ADDRESS), now);
        channel.received(connection, &message(channel, connection, &[select(0x100, 0)]), now);
        connecteds(&channel.take_effects())[0]["viewer"].clone()
    };
    let bound = usize::try_from(MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR).unwrap();
    for index in 0..bound {
        let mut seed = [0_u8; 32];
        seed[..8].copy_from_slice(&(index as u64).to_le_bytes());
        assert!(is_handle(&prove(&mut channel, seed, 0)), "{index}");
    }
    let seed = [0xEE; 32];
    assert_eq!(prove(&mut channel, seed, 0), Value::Null, "the 601st new viewer");
    assert!(is_handle(&prove(&mut channel, [0; 32], 1)), "a known viewer is not new");
    assert!(is_handle(&prove(&mut channel, seed, crate::viewer::NEW_VIEWER_WINDOW_MS)), "a new hour");
}

/// VS-306: rotation tells the program `viewers_reset`, then challenges
/// each connection that had a viewer, whose proof is a new handle.
#[test]
fn rotation_resets_every_viewer() {
    let mut channel = channel_at(ADDRESS, &secret());
    let before = proven(&mut channel, CONNECTION, &ALICE, 0);
    channel.rotate_viewers(&HandleSecret::new([9; 32]), 5);
    let effects = channel.take_effects();
    assert_eq!(told(&effects)[0], json!({"type": "viewers_reset"}), "told first");
    let sent = challenges(&effects, CONNECTION);
    assert_eq!(sent.len(), 1);
    channel.received(CONNECTION, &proof(&ALICE, Lifetime::Visit, &sent[0].0, ADDRESS), 6);
    let began = lines_of(&channel.take_effects(), "viewer_began");
    assert!(is_handle(&began[0]["viewer"]));
    assert_ne!(began[0]["viewer"], before);

    let mut stopped = Channel::new(config(ADDRESS), &secret(), random());
    stopped.rotate_viewers(&HandleSecret::new([9; 32]), 0);
    stopped.program_started(1);
    let order: Vec<String> = told(&stopped.take_effects()).iter().map(|line| line["type"].as_str().unwrap().into()).collect();
    assert_eq!(order, ["start", "viewers_reset"], "a program that was not running is told after start");
}

/// VS-209: nothing of the mechanism is in the operator's log, over a whole
/// session: challenges, proofs refused and accepted, requests refused.
#[test]
fn nothing_of_a_viewer_is_logged() {
    let mut channel = channel_at(ADDRESS, &secret());
    let handle = proven(&mut channel, 1, &ALICE, 0);
    let handle = handle.as_str().unwrap();
    let mut logged = String::new();
    let mut keep = |effects: Vec<Effect>| {
        for effect in effects {
            if let Effect::Log { text, .. } = effect {
                logged.push_str(&text);
            }
        }
    };
    channel.from_program(&line(&json!({"type": "end_state", "viewer": handle})), 1);
    keep(channel.take_effects());
    channel.from_program(&line(&json!({"type": "end_state", "viewer": handle})), 2);
    keep(channel.take_effects());
    channel.from_program(&line(&json!({"type": "end_state", "viewer": "0123456789ABCDEF0123456789ABCDEF"})), 3);
    keep(channel.take_effects());
    assert!(!logged.is_empty(), "the refusals are logged");
    assert!(!logged.contains(handle) && !logged.to_lowercase().contains("0123456789abcdef"), "{logged}");
}

/// A new viewer from `address`, seed `seed`, on a new `connection` at
/// `now`: its handle as `connected` gives it, null when it was not
/// admitted (VS-304). For the snapshot tests, which hold that snapshots
/// spend no viewer's budget (REQ-6882).
pub(super) fn new_viewer(channel: &mut Channel, connection: u64, address: core::net::IpAddr, seed: &[u8; 32], now: u64) -> Value {
    channel.opened(connection, Some(address), now);
    channel.received(connection, &message(channel, connection, &[hello()]), now);
    let challenge = challenges(&channel.take_effects(), connection)[0].0;
    channel.received(connection, &proof(seed, Lifetime::Visit, &challenge, ADDRESS), now);
    channel.received(connection, &message(channel, connection, &[select(0x100, 0)]), now);
    connecteds(&channel.take_effects())[0]["viewer"].clone()
}

/// Whether `value` is a viewer's handle.
pub(super) fn handle(value: &Value) -> bool {
    is_handle(value)
}

/// DEC-1755 option 1, TODO-4579: a connection given back to a new channel
/// after the channel's memory was lost, as the Cloudflare host's Durable
/// Object loses it at each hibernation, keeps what its receiver and its
/// program cannot ask for again. Its receiver is sent nothing (no WELCOME,
/// no challenge: it is not told a wake happened), and the program, started
/// again, is told it with its selection, query, number and the viewer it
/// proved, under the same handle.
#[test]
fn a_resumed_connection_keeps_its_viewer_selection_and_numbers() {
    let mut before = channel_at(ADDRESS, &secret());
    let challenge = open(&mut before, CONNECTION, 0);
    before.received(CONNECTION, &proof(&ALICE, Lifetime::Remembered, &challenge, ADDRESS), 1);
    before.received(CONNECTION, &message(&before, CONNECTION, &[select_with(0x302, 0, "q=1")]), 2);
    let first = connecteds(&before.take_effects());
    assert_eq!(first.len(), 1, "{first:?}");
    // Input acted on and acknowledged, so that the numbers it runs on are
    // not those of a new connection.
    let on = neoteletext_input::event::Stamp { page: 0x302, sub: 0 };
    before.received(CONNECTION, &message(&before, CONNECTION, &[key(on, Action::Press, false)]), 3);
    before.deadline(3 + ACKNOWLEDGEMENT_WAIT_MS);
    let _ = before.take_effects();
    let kept = before.resume_of(CONNECTION).expect("a viewer's connection is kept");
    let stored = serde_json::to_value(&kept).unwrap();
    assert!(stored["acked"] != json!(0) && stored["record_id"] != json!(0), "the numbers ran on: {stored}");

    let mut after = channel_at(ADDRESS, &secret());
    let _ = after.take_effects();
    after.resumed(CONNECTION, None, kept.clone(), 0);
    after.program_started(0);
    let effects = after.take_effects();
    assert_eq!(sent_to(&effects, CONNECTION), [] as [u8; 0], "the receiver is sent nothing");
    let lines = connecteds(&effects);
    assert_eq!(lines.len(), 1, "{lines:?}");
    for field in ["request", "selection", "path", "query", "viewer", "remembered"] {
        assert_eq!(lines[0][field], first[0][field], "{field}: {} against {}", lines[0], first[0]);
    }
    assert_eq!(after.resume_of(CONNECTION), Some(kept), "everything kept is as it was");
    assert_eq!(after.viewers(), 1, "the viewer is known again");
}

/// The stored state is read as untrusted: a query longer than SELECT
/// carries is not taken back.
#[test]
fn a_resumed_query_past_selects_bound_is_dropped() {
    let mut before = channel_at(ADDRESS, &secret());
    let _ = open(&mut before, CONNECTION, 0);
    before.received(CONNECTION, &message(&before, CONNECTION, &[select_with(0x302, ANY, "q=1")]), 1);
    let _ = before.take_effects();
    let kept = before.resume_of(CONNECTION).unwrap();
    let mut stored = serde_json::to_value(&kept).unwrap();
    stored["parameters"] = json!("q".repeat(neoteletext_input::MAX_PAGE_PARAMETERS + 1));
    let mut after = channel_at(ADDRESS, &secret());
    after.resumed(CONNECTION, None, serde_json::from_value(stored).unwrap(), 0);
    after.program_started(0);
    let lines = connecteds(&after.take_effects());
    assert_eq!(lines[0]["query"], json!(null), "{}", lines[0]);
}
