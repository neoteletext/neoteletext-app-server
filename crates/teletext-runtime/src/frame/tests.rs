use super::*;
use crate::screen::Screen;
use neoteletext_core::packet::{Decoded, Packet, decode};

/// A page of 64 sub-pages, every row written, as a gallery's is: more than
/// one push message holds.
fn big_frame() -> Vec<u8> {
    let subpages: Vec<PageFile> = (1..=64_u8)
        .map(|subcode| {
            let mut screen = Screen::new();
            for row in 1..=23 {
                screen.text(row, 0, &"x".repeat(40));
            }
            screen.page(0x501, subcode)
        })
        .collect();
    page_frame(&subpages)
}

fn is_header(packet: &[u8]) -> bool {
    packet.try_into().is_ok_and(|bytes| matches!(decode(&Packet { bytes }), Ok(Decoded::Header(_))))
}

/// A frame over the push limit goes as several messages, each within it,
/// each starting at a sub-page's header, together the frame unchanged.
/// Before, it went as one message the browser refused, which closed its
/// connection every second and so lost every key (28 October 2026).
#[test]
fn a_frame_over_the_limit_goes_in_messages_cut_between_sub_pages() {
    let frame = big_frame();
    assert!(frame.len() > MOST_MESSAGE_BYTES, "the test's frame is {} bytes", frame.len());
    let parts = messages(&frame, MOST_MESSAGE_BYTES);
    assert!(parts.len() >= 2);
    for part in &parts {
        assert!(part.len() <= MOST_MESSAGE_BYTES, "{} bytes", part.len());
        assert!(part.len().is_multiple_of(PACKET_BYTES));
        assert!(is_header(&part[..PACKET_BYTES]), "a message starts with a header");
    }
    assert_eq!(parts.concat(), frame);
}

/// What fits, or is not whole packets, is one message; a sub-page longer
/// than the limit by itself is cut at whole packets.
#[test]
fn small_frames_stay_whole_and_a_huge_sub_page_is_cut_at_packets() {
    let frame = big_frame();
    let first_subpage = &frame[..frame.iter().len() / 64];
    assert_eq!(messages(first_subpage, MOST_MESSAGE_BYTES), [first_subpage]);
    assert_eq!(messages(&[1, 2, 3], 2), [&[1_u8, 2, 3][..]]);
    let tiny = messages(&frame, 3 * PACKET_BYTES + 5);
    assert!(tiny.iter().all(|part| part.len() <= 3 * PACKET_BYTES && part.len().is_multiple_of(PACKET_BYTES)));
    assert_eq!(tiny.concat(), frame);
}
