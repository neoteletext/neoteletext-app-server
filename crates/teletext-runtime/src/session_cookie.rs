//! The session cookie (`SERVER.md` section 8.2, requirement 5.5): a
//! session's number and expiry, signed with HMAC-SHA-256 (RFC 2104 over
//! FIPS 180-4, the engine's SHA-256, as decision 5 planned) under the
//! channel's name, so that a viewer coming back continues their session
//! and nobody can claim another's. The key, the time and the random
//! bytes are the host's: this does no input or output.

use alloc::string::String;
use alloc::vec::Vec;

use neoteletext_web::consts::{SHA256_BLOCK_BYTES, SHA256_DIGEST_BYTES};
use neoteletext_web::sha256::{Digest, Sha256, parse_hex, to_hex};

use crate::channel::SessionId;

/// The cookie's name: names beginning `_` are the server's
/// (`SERVER.md` section 8.2).
pub const NAME: &str = "_session";
/// A key's length: as long as the digest (RFC 2104 section 3 advises no
/// shorter).
pub const KEY_BYTES: usize = SHA256_DIGEST_BYTES;
/// How long a cookie lasts, in seconds: 30 days, renewed at each
/// connection.
pub const LIFETIME_S: u64 = 30 * 24 * 60 * 60;
/// Every session number is below this, 2^53: the largest integer a
/// TypeScript program reads exactly from JSON (ECMA-262 section 21.1.2.6).
pub const SESSION_LIMIT: u64 = 9_007_199_254_740_992;
/// Session numbers start from the start time, in milliseconds since the
/// Unix epoch, times this (requirement 5.4): a server may start 1,024
/// sessions a millisecond before its numbers reach those of a later start.
pub const SESSIONS_PER_MILLISECOND: u64 = 1_024;
/// HMAC's inner and outer pads (RFC 2104 section 2).
const INNER_PAD: u8 = 0x36;
const OUTER_PAD: u8 = 0x5C;
/// The most decimal digits a number in the cookie has: `u64::MAX` has 20.
const MOST_DIGITS: usize = 20;

// RFC 2104 section 2: a key no longer than the block is padded with zeros.
const _: () = assert!(KEY_BYTES <= SHA256_BLOCK_BYTES);
const _: () = assert!(SESSION_LIMIT == 1 << 53);

/// A key the server signs with.
pub type Key = [u8; KEY_BYTES];

/// The first session number of a server that started at `epoch_ms`
/// (requirement 5.4).
#[must_use]
pub fn first_session(epoch_ms: u64) -> SessionId {
    epoch_ms.saturating_mul(SESSIONS_PER_MILLISECOND).min(SESSION_LIMIT.saturating_sub(1))
}

/// HMAC-SHA-256 under `key` of the channel, the session and the expiry,
/// each followed by a zero byte so no two inputs read alike.
fn signature(key: &Key, channel: &str, session: SessionId, expires_s: u64) -> Digest {
    let mut block = [0_u8; SHA256_BLOCK_BYTES];
    for (byte, key_byte) in block.iter_mut().zip(key) {
        *byte = *key_byte;
    }
    let padded = |pad: u8| block.map(|byte| byte ^ pad);
    let mut inner = Sha256::default();
    inner.update(&padded(INNER_PAD));
    for part in [channel.as_bytes(), alloc::format!("{session}").as_bytes(), alloc::format!("{expires_s}").as_bytes()] {
        inner.update(part);
        inner.update(&[0]);
    }
    let mut outer = Sha256::default();
    outer.update(&padded(OUTER_PAD));
    outer.update(&inner.finish());
    outer.finish()
}

/// The cookie's value: `<session>.<expiry>.<signature in hexadecimal>`.
#[must_use]
pub fn value(key: &Key, channel: &str, session: SessionId, expires_s: u64) -> String {
    let signed = to_hex(&signature(key, channel, session, expires_s));
    alloc::format!("{session}.{expires_s}.{}", String::from_utf8_lossy(&signed))
}

/// The `Set-Cookie` header's value for `session` of `channel` at `now_s`,
/// seconds since the Unix epoch; `secure` when the origin is `https`.
#[must_use]
pub fn set_cookie(key: &Key, channel: &str, session: SessionId, now_s: u64, secure: bool) -> String {
    let value = value(key, channel, session, now_s.saturating_add(LIFETIME_S));
    let secure = if secure { "; Secure" } else { "" };
    alloc::format!("{NAME}={value}; Path=/{channel}/; Max-Age={LIFETIME_S}; HttpOnly; SameSite=Strict{secure}")
}

/// The session a `Cookie` header's value names for `channel`, if one of
/// its `_session` cookies verifies under one of `keys` and has not expired
/// at `now_s`.
#[must_use]
pub fn session_of(header: &[u8], channel: &str, now_s: u64, keys: &[&Key]) -> Option<SessionId> {
    header
        .split(|&byte| byte == b';')
        .filter_map(|pair| pair.trim_ascii().strip_prefix(NAME.as_bytes())?.strip_prefix(b"="))
        .find_map(|value| verified(value, channel, now_s, keys))
}

fn verified(value: &[u8], channel: &str, now_s: u64, keys: &[&Key]) -> Option<SessionId> {
    let parts: Vec<&[u8]> = value.split(|&byte| byte == b'.').collect();
    let [session, expires, signed] = parts.as_slice() else { return None };
    let session = number(session).filter(|&session| session < SESSION_LIMIT)?;
    let expires_s = number(expires).filter(|&expires| expires > now_s)?;
    let signed = parse_hex(signed)?;
    keys.iter().any(|key| same(&signature(key, channel, session, expires_s), &signed)).then_some(session)
}

/// A number written in decimal digits alone.
fn number(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() || digits.len() > MOST_DIGITS || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    core::str::from_utf8(digits).ok()?.parse().ok()
}

/// Whether two digests are the same, in time that does not depend on
/// where they differ, so a forger learns nothing from how long a refusal
/// takes.
fn same(left: &Digest, right: &Digest) -> bool {
    left.iter().zip(right).fold(0_u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

#[cfg(test)]
mod tests;
