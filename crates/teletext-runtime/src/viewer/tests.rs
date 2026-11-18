//! Verification, the key identifier and the handle (DEC-6471: VS-301,
//! VS-302, VS-305, VS-211).

extern crate std;

use alloc::vec::Vec;

use neoteletext_input::state::{Algorithm, Lifetime, StateProof};
use neoteletext_web::viewer_state::{self, ChannelIdentity, MAX_SIGNED_BYTES};
use ring::signature::{Ed25519KeyPair, KeyPair};

use super::*;

/// A viewer's seed.
const SEED: [u8; 32] = [42; 32];
/// A challenge.
const CHALLENGE: [u8; CHALLENGE_BYTES] = [9; CHALLENGE_BYTES];
/// The channel of the tests.
const CHANNEL: &str = "https://example.org/news/";

/// A browser's proof for `channel` of `challenge`, signed with `seed`:
/// its public key and signature.
pub(crate) fn sign(seed: &[u8; 32], lifetime: Lifetime, challenge: &[u8; CHALLENGE_BYTES], channel: &str) -> ([u8; 32], [u8; 64]) {
    let pair = Ed25519KeyPair::from_seed_unchecked(seed).unwrap();
    let identity = ChannelIdentity::read(channel).unwrap();
    let mut signed = [0_u8; MAX_SIGNED_BYTES];
    let length = viewer_state::signed_bytes(Algorithm::Ed25519, lifetime, challenge, &identity, &mut signed).unwrap();
    let signature: [u8; 64] = pair.sign(&signed[..length]).as_ref().try_into().unwrap();
    let public_key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
    (public_key, signature)
}

fn identities_of(addresses: &[&str]) -> Vec<ChannelIdentity> {
    identities(addresses.iter().copied()).0
}

fn proof<'a>(public_key: &'a [u8; 32], signature: &'a [u8; 64]) -> StateProof<'a> {
    StateProof { algorithm: Algorithm::Ed25519, lifetime: Lifetime::Visit, public_key, signature }
}

/// VS-301, VS-302: a proof verifies under its channel, gives the key
/// identifier of its public key and the lifetime it claimed.
#[test]
fn a_proof_verifies_and_gives_its_key_identifier() {
    let (public_key, signature) = sign(&SEED, Lifetime::Visit, &CHALLENGE, CHANNEL);
    let verified = verify(&proof(&public_key, &signature), &CHALLENGE, &identities_of(&[CHANNEL]));
    assert_eq!(verified, Ok((viewer_state::key_id(Algorithm::Ed25519, &public_key), Lifetime::Visit)));
}

/// VS-203, VS-301: changing any one byte of anything signed or sent makes
/// the proof fail: the challenge, the channel, the lifetime, the public key
/// and the signature.
#[test]
fn any_byte_changed_fails_the_proof() {
    let (public_key, signature) = sign(&SEED, Lifetime::Visit, &CHALLENGE, CHANNEL);
    let channel = identities_of(&[CHANNEL]);
    for index in 0..CHALLENGE_BYTES {
        let mut other = CHALLENGE;
        other[index] ^= 1;
        assert_eq!(verify(&proof(&public_key, &signature), &other, &channel), Err(Invalid), "challenge byte {index}");
    }
    for index in 0..public_key.len() {
        let mut other = public_key;
        other[index] ^= 1;
        assert_eq!(verify(&proof(&other, &signature), &CHALLENGE, &channel), Err(Invalid), "public key byte {index}");
    }
    for index in 0..signature.len() {
        let mut other = signature;
        other[index] ^= 1;
        assert_eq!(verify(&proof(&public_key, &other), &CHALLENGE, &channel), Err(Invalid), "signature byte {index}");
    }
    let remembered = StateProof { lifetime: Lifetime::Remembered, ..proof(&public_key, &signature) };
    assert_eq!(verify(&remembered, &CHALLENGE, &channel), Err(Invalid), "the lifetime is signed");
    for other in ["http://example.org/news/", "https://example.org/news/sport/", "https://example.org/", "https://example.org:8443/news/"] {
        assert_eq!(verify(&proof(&public_key, &signature), &CHALLENGE, &identities_of(&[other])), Err(Invalid), "{other}");
    }
}

/// VS-301: a channel served at several addresses verifies a proof made for
/// any of them, and only those; at most four are taken.
#[test]
fn a_channel_verifies_under_each_of_its_addresses() {
    let addresses = ["http://example.org/news/", CHANNEL];
    let (public_key, signature) = sign(&SEED, Lifetime::Visit, &CHALLENGE, CHANNEL);
    assert!(verify(&proof(&public_key, &signature), &CHALLENGE, &identities_of(&addresses)).is_ok());
    let five = ["http://a.example/x/", "http://b.example/x/", "http://c.example/x/", "http://d.example/x/", "http://e.example/x/"];
    let (read, refused) = identities(five.iter().copied());
    assert_eq!((read.len(), refused), (MOST_CHANNEL_ADDRESSES, Vec::from(["http://e.example/x/"])));
    let (_, refused) = identities(["https://Example.org/news/", "https://example.org/news"].into_iter());
    assert_eq!(refused.len(), 2, "an address not in its canonical form is no identity");
}

/// VS-301 (RFC 8032 section 5.1.7): a signature whose `S` is not below the
/// group's order is refused, though `S + ℓ` verifies the same equation.
#[test]
fn a_non_canonical_signature_is_refused() {
    let (public_key, mut signature) = sign(&SEED, Lifetime::Visit, &CHALLENGE, CHANNEL);
    let mut carry = 0_u16;
    for (byte, order) in signature[32..].iter_mut().zip(GROUP_ORDER) {
        let sum = u16::from(*byte) + u16::from(order) + carry;
        *byte = (sum & 0xFF) as u8;
        carry = sum >> 8;
    }
    assert_eq!(carry, 0, "S + ℓ fits in 256 bits");
    assert_eq!(verify(&proof(&public_key, &signature), &CHALLENGE, &identities_of(&[CHANNEL])), Err(Invalid));
}

/// VS-301: a public key of small order, or not a canonical encoding, is
/// refused whatever the signature, and the check refuses no ordinary key.
#[test]
fn a_small_order_or_non_canonical_key_is_refused() {
    let channel = identities_of(&[CHANNEL]);
    let signature = [0_u8; 64];
    for key in &SMALL_ORDER_POINTS {
        assert!(!acceptable_key(key));
        assert_eq!(verify(&proof(key, &signature), &CHALLENGE, &channel), Err(Invalid));
    }
    let mut negative_neutral = SMALL_ORDER_POINTS[0];
    negative_neutral[31] |= SIGN_BIT;
    assert!(!acceptable_key(&negative_neutral), "x = 0 with its sign set");
    let mut above_prime = FIELD_PRIME;
    above_prime[0] += 1;
    assert!(!acceptable_key(&FIELD_PRIME) && !acceptable_key(&above_prime), "y not below p");
    for seed in 0..=u8::MAX {
        let (public_key, _) = sign(&[seed; 32], Lifetime::Visit, &CHALLENGE, CHANNEL);
        assert!(acceptable_key(&public_key), "seed {seed}");
    }
}

/// `below` decides by the most significant byte that differs.
#[test]
fn below_compares_little_endian_numbers() {
    let mut small = [0_u8; 32];
    let mut large = [0_u8; 32];
    small[0] = 0xFF;
    large[1] = 1;
    assert!(below(&small, &large) && !below(&large, &small) && !below(&small, &small));
}

/// VS-305, VS-211: a handle depends on the key, the channel's folder and
/// the secret, each; and a key's handle is the same every time.
#[test]
fn a_handle_is_per_key_per_channel_per_secret() {
    let key = viewer_state::key_id(Algorithm::Ed25519, &sign(&SEED, Lifetime::Visit, &CHALLENGE, CHANNEL).0);
    let other_key = viewer_state::key_id(Algorithm::Ed25519, &sign(&[1; 32], Lifetime::Visit, &CHALLENGE, CHANNEL).0);
    let secret = HandleSecret::new([3; HANDLE_SECRET_BYTES]);
    let news = HandleKey::derive(&secret, "/news/");
    assert_eq!(news.handle(&key), HandleKey::derive(&secret, "/news/").handle(&key), "the same after a restart");
    assert_ne!(news.handle(&key), news.handle(&other_key));
    assert_ne!(news.handle(&key), HandleKey::derive(&secret, "/sport/").handle(&key));
    assert_ne!(news.handle(&key), HandleKey::derive(&HandleSecret::new([4; HANDLE_SECRET_BYTES]), "/news/").handle(&key));
}

/// VS-305: the handle is the first 16 bytes of HMAC-SHA-256 under the
/// channel's key of the domain, a zero byte and the key identifier, the
/// channel's key HMAC-SHA-256 under the secret of its own domain, a zero
/// byte and the folder: checked against `ring`'s one-shot HMAC.
#[test]
fn the_handle_is_the_hmac_vs_305_names() {
    let secret = HandleSecret::new([5; HANDLE_SECRET_BYTES]);
    let key_id = [6_u8; KEY_ID_BYTES];
    let channel_key = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, secret.bytes()), b"NeoTeletext channel handle key v1\0/news/");
    let mut message = b"NeoTeletext viewer handle v1\0".to_vec();
    message.extend_from_slice(&key_id);
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, channel_key.as_ref()), &message);
    let expected: [u8; HANDLE_BYTES] = tag.as_ref()[..HANDLE_BYTES].try_into().unwrap();
    assert_eq!(HandleKey::derive(&secret, "/news/").handle(&key_id), Handle::of_bytes(expected));
}

/// VS-209: neither the secret nor a handle prints.
#[test]
fn no_secret_or_handle_prints() {
    let secret = HandleSecret::new([0xAB; HANDLE_SECRET_BYTES]);
    let handle = HandleKey::derive(&secret, "/news/").handle(&[1; KEY_ID_BYTES]);
    let printed = std::format!("{secret:?} {handle:?}");
    assert!(!printed.contains("171") && !printed.contains("ab") && !printed.contains(&handle.text()), "{printed}");
}

/// The folder of an identity is its path.
#[test]
fn the_folder_is_the_identitys_path() {
    for (address, folder) in [(CHANNEL, "/news/"), ("http://127.0.0.1:8090/ch/", "/ch/"), ("http://a/", "/")] {
        assert_eq!(folder_of(&ChannelIdentity::read(address).unwrap()), folder);
    }
}
