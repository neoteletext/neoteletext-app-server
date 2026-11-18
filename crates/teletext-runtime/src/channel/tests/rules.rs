//! The page rule table on a channel, second form (page-rules.md RULE-40,
//! RULE-53, RULE-111, RULE-112; REQ-6532, REQ-6615).

use super::*;

/// The default table of `page` and `sub` as the engine builds it, with the
/// `wheel` rule the tests' channel has.
fn default_image(page: u16, sub: u16) -> Vec<u8> {
    crate::rules::Compiled::send_all(true, page, sub).unwrap().image().to_vec()
}

/// RULE-40, RULE-53 (REQ-6532): a selection is sent the channel's default
/// table for its page before anything else, byte for byte the engine's
/// `default_send_all`, at generation 1.
#[test]
fn a_selection_is_sent_the_default_table_first() {
    let mut channel = channel();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 0)]), 0);
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 0, 1, Some(default_image(0x100, 0)))]);
}

/// RULE-112, DEC-6599 (REQ-6615): another sub-code of the same page number
/// is another page: it is sent its own default table, at the next
/// generation of the connection.
#[test]
fn another_sub_code_is_another_page() {
    let mut channel = selected();
    channel.received(CONNECTION, &message(&channel, CONNECTION, &[select(0x100, 1)]), 0);
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 1, 2, Some(default_image(0x100, 1)))]);
}

/// RULE-111 case 3 (REQ-6615): when the program's connection ends, each
/// connection is sent its page's default table again, which drops what
/// the receiver held of the program's table.
#[test]
fn the_programs_end_sends_the_default_table_again() {
    let mut channel = selected();
    channel.program_ended();
    let effects = channel.take_effects();
    assert_eq!(rules_sent(&effects, CONNECTION), [(0x100, 0, 2, Some(default_image(0x100, 0)))]);
}

/// The names of the default table's rules are the server's, in the
/// engine's order (RULE-53).
#[test]
fn the_default_tables_rules_have_their_names() {
    let table = crate::rules::Compiled::send_all(true, 0x100, 0).unwrap();
    let names: Vec<String> = (0..DEFAULT_RULE_NAMES.len()).filter_map(|rule| table.rule(rule)).map(|name| name.0).collect();
    assert_eq!(names, DEFAULT_RULE_NAMES);
}
