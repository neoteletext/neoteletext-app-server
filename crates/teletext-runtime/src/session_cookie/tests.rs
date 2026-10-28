use super::*;

const KEY: Key = [7; KEY_BYTES];
const OLD_KEY: Key = [9; KEY_BYTES];
const NOW_S: u64 = 1_800_000_000;
const SESSION: SessionId = 1_843_000_000_000_123;

/// The `Cookie` header a browser sends back for the `Set-Cookie` given.
fn header_for(set_cookie: &str) -> Vec<u8> {
    let pair = set_cookie.split(';').next().unwrap();
    alloc::format!("theme=dark; {pair}; other=1").into_bytes()
}

/// A cookie the server set names its session to that channel while it
/// lasts, under the current key or the previous one, and to nobody after,
/// elsewhere or under another key.
#[test]
fn a_cookie_names_its_session_while_it_lasts() {
    let header = header_for(&set_cookie(&KEY, "hackaday", SESSION, NOW_S, false));
    assert_eq!(session_of(&header, "hackaday", NOW_S, &[&KEY]), Some(SESSION));
    assert_eq!(session_of(&header, "hackaday", NOW_S + LIFETIME_S - 1, &[&OLD_KEY, &KEY]), Some(SESSION), "the previous key's place");
    assert_eq!(session_of(&header, "hackaday", NOW_S + LIFETIME_S, &[&KEY]), None, "expired");
    assert_eq!(session_of(&header, "demosite", NOW_S, &[&KEY]), None, "another channel's");
    assert_eq!(session_of(&header, "hackaday", NOW_S, &[&OLD_KEY]), None, "another key");
}

/// A cookie altered anywhere, or not of the server's form, names nothing.
#[test]
fn an_altered_cookie_names_nothing() {
    let value = value(&KEY, "hackaday", SESSION, NOW_S + LIFETIME_S);
    let forged_session = value.replacen("123.", "124.", 1);
    let forged_expiry = value.replacen(&alloc::format!(".{}.", NOW_S + LIFETIME_S), &alloc::format!(".{}.", NOW_S + 2 * LIFETIME_S), 1);
    let mut flipped = value.clone().into_bytes();
    let last = flipped.len() - 1;
    flipped[last] = if flipped[last] == b'0' { b'1' } else { b'0' };
    for cookie in [
        forged_session,
        forged_expiry,
        String::from_utf8(flipped).unwrap(),
        alloc::format!("{value}.1"),
        alloc::format!("{SESSION}"),
        String::from("..."),
        alloc::format!("{}.{}.{}", SESSION_LIMIT, NOW_S + LIFETIME_S, "0".repeat(64)),
        alloc::format!("-1.{}", value.split_once('.').unwrap().1),
    ] {
        let header = alloc::format!("{NAME}={cookie}").into_bytes();
        assert_eq!(session_of(&header, "hackaday", NOW_S, &[&KEY]), None, "{cookie}");
    }
}

/// The cookie's attributes: its channel's folder alone, never to scripts,
/// never from another site, 30 days, and `Secure` over `https`.
#[test]
fn the_cookie_is_scoped_to_its_channel() {
    let plain = set_cookie(&KEY, "hackaday", SESSION, NOW_S, false);
    assert!(plain.starts_with("_session=1843000000000123.1802592000."), "{plain}");
    assert!(plain.ends_with("; Path=/hackaday/; Max-Age=2592000; HttpOnly; SameSite=Strict"), "{plain}");
    assert!(set_cookie(&KEY, "hackaday", SESSION, NOW_S, true).ends_with("; SameSite=Strict; Secure"));
}

/// Session numbers start from the start time times 1,024, below 2^53, so a
/// restarted server never gives a number given before (requirement 5.4).
#[test]
fn session_numbers_start_from_the_start_time() {
    let started_ms = 1_761_600_000_000;
    assert_eq!(first_session(started_ms), started_ms * 1_024);
    assert!(first_session(started_ms) < SESSION_LIMIT);
    assert!(first_session(started_ms + 1) > first_session(started_ms) + 1_000);
    assert_eq!(first_session(u64::MAX), SESSION_LIMIT - 1);
}
