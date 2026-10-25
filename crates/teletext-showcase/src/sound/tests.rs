use super::*;
use neoteletext_audio::serial_newer;

/// Two applications' sounds number their commands as one count, so a
/// receiver that heard one never takes the other's for old. Before, each
/// counted from 0, and the second application's effects were ignored as
/// older than the first's.
#[test]
fn every_application_s_commands_are_newer_than_the_last() {
    let mut first = Sounds::new();
    let mut second = Sounds::new();
    let numbers = [first.command(Effect::Laser).sequence, first.command(Effect::Hit).sequence, second.command(Effect::Coin).sequence];
    assert!(numbers.windows(2).all(|pair| serial_newer(pair[1], pair[0])), "{numbers:?}");
}
