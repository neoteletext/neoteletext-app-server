//! Viewer state's cryptography and bounds, the server's part
//! (neoteletext-common's `docs/viewer-state.md`, adopted by DEC-6471):
//! verifying a viewer's proof (VS-301), the key identifier the server keeps
//! (VS-302) and the per-channel handle a program is given (VS-305).
//!
//! The bytes a proof signs and the key identifier are the engine's
//! ([`neoteletext_web::viewer_state`]), so that the browser and the server
//! cannot disagree; the signature and the keyed hash are `ring`'s, approved
//! by name for the server (DEC-6471, open question Q7), both constant-time.
//!
//! Nothing here implements `Display`, and every type that holds a secret,
//! a key identifier or a handle prints a placeholder for `Debug`, so that
//! none of them can reach a log line (VS-209).

use alloc::vec::Vec;

use neoteletext_input::state::{Algorithm, Lifetime, PUBLIC_KEY_BYTES, SIGNATURE_BYTES, StateProof};
use neoteletext_web::viewer_state::{self, ChannelIdentity, KeyId, MAX_SIGNED_BYTES};
use ring::hmac;
use ring::signature::{ED25519, UnparsedPublicKey};

use crate::channel::PROGRAM_REPLY_LIMIT_MS;
use crate::protocol::Handle;

pub use neoteletext_input::state::CHALLENGE_BYTES;
pub use neoteletext_web::viewer_state::KEY_ID_BYTES;

/// How long the server waits for the answer to a challenge (VS-202,
/// VS-205; open question Q12, answered by DEC-6471 with its recommended
/// value): 10 seconds.
pub const STATE_ANSWER_MS: u64 = 10_000;
/// How recent the viewer's input must be for a program's state request to
/// be honoured (VS-212): the time a program already has to answer an
/// event, 7,000 ms.
pub const STATE_GESTURE_MS: u64 = PROGRAM_REPLY_LIMIT_MS;
/// The challenges one connection is sent in [`CHALLENGE_WINDOW_MS`] at
/// most (VS-205; Q12): 6.
pub const MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE: usize = 6;
/// The window [`MOST_CHALLENGES_PER_CONNECTION_PER_MINUTE`] counts in: a
/// minute.
pub const CHALLENGE_WINDOW_MS: u64 = 60 * 1_000;
/// How often a receiver may send `STATE_NONE` `forgotten` or `STATE_OFFER`
/// unprompted (VS-204; Q12): once in 10 seconds. One sooner is ignored.
pub const STATE_OFFER_INTERVAL_MS: u64 = 10_000;
/// The viewers one channel's table holds at most (VS-303; Q12): 262,144.
/// At the bound the viewer seen longest ago is removed, who loses nothing:
/// their next proof gives the same handle.
pub const MOST_VIEWERS_PER_CHANNEL: usize = 262_144;
/// The new key identifiers a channel takes from one network address in
/// [`NEW_VIEWER_WINDOW_MS`] (VS-304; Q12): 600, high enough for many
/// viewers behind one carrier-grade address.
pub const MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR: u32 = 600;
/// The window of [`MOST_NEW_VIEWERS_PER_ADDRESS_PER_HOUR`]: an hour.
pub const NEW_VIEWER_WINDOW_MS: u64 = 60 * 60 * 1_000;
/// The network addresses whose new viewers a channel counts at once: one
/// for each viewer its table holds, since an address is counted only when
/// it brought a new viewer. At the bound the address counted longest ago
/// is forgotten.
pub const MOST_COUNTED_ADDRESSES: usize = MOST_VIEWERS_PER_CHANNEL;
/// How long a viewer's entry stays after its last connection closed
/// (VS-303; open question Q2): 5 minutes, the grace sessions had.
pub const VIEWER_IDLE_MS: u64 = 5 * 60 * 1_000;
/// The public addresses a channel is verified under at most (VS-301): 4,
/// so that one channel served under `http` and `https`, or under two host
/// names, is verified under each, each its own channel with its own keys.
pub const MOST_CHANNEL_ADDRESSES: usize = 4;
/// The bytes of the server's handle secret (VS-305): 256 bits.
pub const HANDLE_SECRET_BYTES: usize = 32;
/// The bytes of a handle: the first 16 of the HMAC (VS-305), 128 bits.
pub const HANDLE_BYTES: usize = 16;

/// VS-305's domain of a channel's handle key, before its zero byte.
const CHANNEL_KEY_DOMAIN: &[u8] = b"NeoTeletext channel handle key v1";
/// VS-305's domain of a handle, before its zero byte.
const HANDLE_DOMAIN: &[u8] = b"NeoTeletext viewer handle v1";
/// What ends each domain (VS-203 item 1, VS-305).
const DOMAIN_END: u8 = 0;

// VS-305 gives the domains' lengths.
const _: () = assert!(CHANNEL_KEY_DOMAIN.len() == 33 && HANDLE_DOMAIN.len() == 28);
// A handle is a prefix of an HMAC-SHA-256 output.
const _: () = assert!(HANDLE_BYTES <= KEY_ID_BYTES);
// No challenge outlasts the window that counts it.
const _: () = assert!(STATE_ANSWER_MS < CHALLENGE_WINDOW_MS);

/// The order of the base point of edwards25519, ℓ = 2²⁵² +
/// 27742317777372353535851937790883648493 (RFC 8032 section 5.1), little
/// endian: a signature's `S` must be below it (section 5.1.7, step 1).
const GROUP_ORDER: [u8; 32] = [
    0xED, 0xD3, 0xF5, 0x5C, 0x1A, 0x63, 0x12, 0x58, 0xD6, 0x9C, 0xF7, 0xA2, 0xDE, 0xF9, 0xDE, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];
/// The field's prime, p = 2²⁵⁵ − 19 (RFC 8032 section 5.1), little endian:
/// a public key's `y` must be below it (section 5.1.3, step 1).
const FIELD_PRIME: [u8; 32] = [
    0xED, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F,
];
/// The sign of `x`, the top bit of a point's last byte (RFC 8032 section
/// 5.1.2).
const SIGN_BIT: u8 = 0x80;
/// The eight points of small order of edwards25519, its torsion subgroup,
/// in their canonical encodings: a public key that is one of them verifies
/// signatures that no private key made, so VS-301 refuses it. Checked on
/// 18 November 2026 by decoding each and multiplying it by the cofactor 8,
/// which gave the neutral point for every one, and by generating the
/// subgroup from the first point of order 8, which gave these eight and no
/// others. Non-canonical encodings of them are refused by the checks of
/// `y` and of the sign of `x = 0`.
const SMALL_ORDER_POINTS: [[u8; 32]; 8] = [
    // The neutral point, (0, 1).
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    // Order 2, (0, −1).
    [
        0xEC, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F,
    ],
    // Order 4, (±√−1, 0).
    [0; 32],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80],
    // Order 8.
    [
        0x26, 0xE8, 0x95, 0x8F, 0xC2, 0xB2, 0x27, 0xB0, 0x45, 0xC3, 0xF4, 0x89, 0xF2, 0xEF, 0x98, 0xF0, 0xD5, 0xDF, 0xAC, 0x05, 0xD3, 0xC6, 0x33, 0x39, 0xB1,
        0x38, 0x02, 0x88, 0x6D, 0x53, 0xFC, 0x05,
    ],
    [
        0x26, 0xE8, 0x95, 0x8F, 0xC2, 0xB2, 0x27, 0xB0, 0x45, 0xC3, 0xF4, 0x89, 0xF2, 0xEF, 0x98, 0xF0, 0xD5, 0xDF, 0xAC, 0x05, 0xD3, 0xC6, 0x33, 0x39, 0xB1,
        0x38, 0x02, 0x88, 0x6D, 0x53, 0xFC, 0x85,
    ],
    [
        0xC7, 0x17, 0x6A, 0x70, 0x3D, 0x4D, 0xD8, 0x4F, 0xBA, 0x3C, 0x0B, 0x76, 0x0D, 0x10, 0x67, 0x0F, 0x2A, 0x20, 0x53, 0xFA, 0x2C, 0x39, 0xCC, 0xC6, 0x4E,
        0xC7, 0xFD, 0x77, 0x92, 0xAC, 0x03, 0x7A,
    ],
    [
        0xC7, 0x17, 0x6A, 0x70, 0x3D, 0x4D, 0xD8, 0x4F, 0xBA, 0x3C, 0x0B, 0x76, 0x0D, 0x10, 0x67, 0x0F, 0x2A, 0x20, 0x53, 0xFA, 0x2C, 0x39, 0xCC, 0xC6, 0x4E,
        0xC7, 0xFD, 0x77, 0x92, 0xAC, 0x03, 0xFA,
    ],
];
const _: () = assert!(PUBLIC_KEY_BYTES == 32 && SIGNATURE_BYTES == 64);

/// Where a host's random bytes come from: its operating system's
/// cryptographic source (VS-205, VS-305). The runtime does no input or
/// output itself, so the host gives it this.
pub trait RandomSource: Send {
    /// Fills `bytes` with random bytes.
    ///
    /// # Errors
    /// When the source cannot give them; nothing is then sent that needs
    /// them.
    fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable>;
}

/// The host's random source failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RandomUnavailable;

/// The server's handle secret (VS-305): 32 random bytes the host keeps as
/// `viewer-handle.key` and never sends anywhere.
#[derive(Clone)]
pub struct HandleSecret([u8; HANDLE_SECRET_BYTES]);

impl HandleSecret {
    /// The secret of `bytes`, as the host read or made them.
    #[must_use]
    pub const fn new(bytes: [u8; HANDLE_SECRET_BYTES]) -> Self {
        Self(bytes)
    }

    /// The secret's bytes, for the host to keep.
    #[must_use]
    pub const fn bytes(&self) -> &[u8; HANDLE_SECRET_BYTES] {
        &self.0
    }
}

impl core::fmt::Debug for HandleSecret {
    /// Never the secret (VS-209).
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("HandleSecret(..)")
    }
}

/// One channel's handle key (VS-305): HMAC-SHA-256 under the server's
/// handle secret of its domain and the channel's folder path, so that two
/// channels' handles of one viewer are unrelated (VS-211).
pub(crate) struct HandleKey(hmac::Key);

impl HandleKey {
    /// The key of the channel whose folder path is `folder`, such as
    /// `/news/`.
    pub(crate) fn derive(secret: &HandleSecret, folder: &str) -> Self {
        let secret = hmac::Key::new(hmac::HMAC_SHA256, secret.bytes());
        let mut context = hmac::Context::with_key(&secret);
        context.update(CHANNEL_KEY_DOMAIN);
        context.update(&[DOMAIN_END]);
        context.update(folder.as_bytes());
        Self(hmac::Key::new(hmac::HMAC_SHA256, context.sign().as_ref()))
    }

    /// The handle of `key_id` on this channel: the first [`HANDLE_BYTES`]
    /// of HMAC-SHA-256 of the handle's domain and the key identifier.
    pub(crate) fn handle(&self, key_id: &KeyId) -> Handle {
        let mut context = hmac::Context::with_key(&self.0);
        context.update(HANDLE_DOMAIN);
        context.update(&[DOMAIN_END]);
        context.update(key_id);
        let tag = context.sign();
        let mut handle = [0_u8; HANDLE_BYTES];
        for (byte, tag_byte) in handle.iter_mut().zip(tag.as_ref()) {
            *byte = *tag_byte;
        }
        Handle::of_bytes(handle)
    }
}

/// The folder path of a channel identity, from the `/` after its host and
/// port to its end (DOC-1863: it begins and ends with `/`).
pub(crate) fn folder_of(identity: &ChannelIdentity) -> &str {
    let text = identity.as_str();
    let after_scheme = text.find("://").map_or(0, |at| at.saturating_add("://".len()));
    text.get(after_scheme..).and_then(|rest| rest.find('/').and_then(|slash| rest.get(slash..))).unwrap_or("/")
}

/// The channel identities a proof is verified under (VS-301): the
/// channel's address and the others it is served at, each read as an
/// identity must already be, at most [`MOST_CHANNEL_ADDRESSES`]. An
/// address that is not one is left out and named in the error.
///
/// # Errors
/// The addresses that are not channel identities, or that are past the
/// bound: the channel is then verified under the others.
pub(crate) fn identities<'a>(addresses: impl Iterator<Item = &'a str>) -> (Vec<ChannelIdentity>, Vec<&'a str>) {
    let mut read = Vec::new();
    let mut refused = Vec::new();
    for address in addresses {
        match ChannelIdentity::read(address) {
            Ok(identity) if read.len() < MOST_CHANNEL_ADDRESSES => read.push(identity),
            _ => refused.push(address),
        }
    }
    (read, refused)
}

/// Whether `left` is below `right`, both little endian, in time that does
/// not depend on their values.
fn below(left: &[u8; 32], right: &[u8; 32]) -> bool {
    // From the most significant byte down: the first that differs decides.
    let mut decided = false;
    let mut less = false;
    for (left, right) in left.iter().rev().zip(right.iter().rev()) {
        let differs = left != right;
        less |= !decided && differs && left < right;
        decided |= differs;
    }
    less
}

/// Whether `public_key` is a canonical encoding of a point not of small
/// order (RFC 8032 section 5.1.3; VS-301): `y` below the prime, `x = 0`
/// never with its sign set, and none of [`SMALL_ORDER_POINTS`]. Whether
/// `y` has an `x` at all is left to the verification, which refuses it.
fn acceptable_key(public_key: &[u8; PUBLIC_KEY_BYTES]) -> bool {
    let mut y = *public_key;
    if let Some(last) = y.last_mut() {
        *last &= !SIGN_BIT;
    }
    // y = 1 and y = p − 1 are the points with x = 0, the neutral point and
    // the point of order 2: with the sign set they are no encoding.
    let negative_zero = public_key.last().is_some_and(|last| last & SIGN_BIT != 0) && (y == SMALL_ORDER_POINTS[0] || y == SMALL_ORDER_POINTS[1]);
    below(&y, &FIELD_PRIME) && !negative_zero && !SMALL_ORDER_POINTS.contains(public_key)
}

/// Verifies `proof` against `challenge` under each identity of the channel
/// (VS-301): strict Ed25519 (RFC 8032 section 5.1.7) over the bytes of
/// VS-203. On success, the key identifier (VS-302); the public key and
/// the signature are then dropped with `proof`.
///
/// # Errors
/// [`Invalid`] for an unknown algorithm, a public key that is no
/// acceptable point, a signature whose `S` is not below the group's order,
/// or a signature that verifies under no identity of the channel.
pub(crate) fn verify(
    version: u32,
    proof: &StateProof<'_>,
    challenge: &[u8; CHALLENGE_BYTES],
    identities: &[ChannelIdentity],
) -> Result<(KeyId, Lifetime), Invalid> {
    let Algorithm::Ed25519 = proof.algorithm;
    let mut s = [0_u8; 32];
    for (byte, signature_byte) in s.iter_mut().zip(proof.signature.iter().skip(SIGNATURE_BYTES / 2)) {
        *byte = *signature_byte;
    }
    if !acceptable_key(proof.public_key) || !below(&s, &GROUP_ORDER) {
        return Err(Invalid);
    }
    let key = UnparsedPublicKey::new(&ED25519, proof.public_key);
    let mut signed = [0_u8; MAX_SIGNED_BYTES];
    for identity in identities {
        let Ok(length) = viewer_state::signed_bytes(version, proof.algorithm, proof.lifetime, challenge, identity, &mut signed) else { continue };
        let Some(bytes) = signed.get(..length) else { continue };
        if key.verify(bytes, proof.signature).is_ok() {
            return Ok((viewer_state::key_id(proof.algorithm, proof.public_key), proof.lifetime));
        }
    }
    Err(Invalid)
}

/// A proof that does not verify (VS-206).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Invalid;

#[cfg(test)]
pub(crate) mod tests;

/// What the runtime's tests give a channel: a fixed secret, and a source
/// that counts, so that every challenge differs and a run repeats.
#[cfg(test)]
pub(crate) mod testing {
    use alloc::boxed::Box;

    use super::{HANDLE_SECRET_BYTES, HandleSecret, RandomSource, RandomUnavailable};

    /// The tests' handle secret.
    pub(crate) const fn secret() -> HandleSecret {
        HandleSecret::new([7; HANDLE_SECRET_BYTES])
    }

    /// Bytes that count up from 1, never the same twice in a run.
    pub(crate) struct Counting(u8);

    impl RandomSource for Counting {
        fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable> {
            for byte in bytes {
                self.0 = self.0.wrapping_add(1);
                *byte = self.0;
            }
            Ok(())
        }
    }

    /// The tests' random source.
    pub(crate) fn random() -> Box<dyn RandomSource> {
        Box::new(Counting(0))
    }
}
