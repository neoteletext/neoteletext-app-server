use super::calendar::{DAY_MS, Date, hours_minutes, local};
use super::markets::{Instrument, STEP_CHOICES};
use super::sport::{Standing, fixtures};
use super::weather::{forecast, temperature};
use super::*;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::ToString;
use alloc::sync::Arc;
use teletext_runtime::{Channel, ChannelConfig, Effect};

/// 25 October 2026, 14:03:22 UTC.
const SOME_TIME: u64 = 1_792_886_400_000 + (14 * 3_600 + 3 * 60 + 22) * 1_000;

fn channel(epoch_at_start_ms: u64) -> Channel {
    let config = ChannelConfig { name: "Test".to_string(), push_address: "ws://127.0.0.1:1/test/push".to_string(), initial_page: 0x100, epoch_at_start_ms };
    let mut channel = Channel::new(config, applications(&[]), 7);
    channel.start(0);
    channel
}

/// The shared pages among `effects`, the last of each.
fn shared(effects: Vec<Effect>) -> BTreeMap<u16, Arc<[u8]>> {
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Shared { page, frame } => Some((page, frame)),
            _ => None,
        })
        .collect()
}

/// Runs the channel's time on to `until`, waking it whenever it asked.
fn run(channel: &mut Channel, until: u64) -> Vec<Effect> {
    let mut effects = Vec::new();
    while let Some(at) = channel.next_wake().filter(|&at| at <= until) {
        channel.deadline(at);
        effects.extend(channel.take_effects());
    }
    effects
}

#[test]
fn every_page_is_published_at_start_and_shared() {
    let mut channel = channel(SOME_TIME);
    let pages: BTreeSet<u16> = shared(channel.take_effects()).into_keys().collect();
    let expected: BTreeSet<u16> = SERVICES.iter().map(|&(page, _)| page).collect();
    assert_eq!(pages, expected);
    let served: BTreeSet<u16> = channel.entries().map(|entry| entry.number).collect();
    assert_eq!(served, expected);
}

#[test]
fn live_pages_change_and_fixed_pages_do_not() {
    let mut channel = channel(SOME_TIME);
    let first = shared(channel.take_effects());
    let later = shared(run(&mut channel, 61_000));
    // The clocks change each second, the weather and the guide at the
    // minute, the markets every 3 s and the sport every 15 s.
    for page in [WEATHER, WORLD_CLOCK, MARKETS, CLOCK, SPORT, GUIDE] {
        let (Some(before), Some(after)) = (first.get(&page), later.get(&page)) else { panic!("{page:X} not republished") };
        assert_ne!(before, after, "{page:X}");
    }
    // The carousels never change, so they are never sent again.
    for page in [NEWS, ABOUT, FACTS] {
        assert!(!later.contains_key(&page), "{page:X} republished");
    }
}

#[test]
fn the_clocks_change_after_their_deadline() {
    let mut channel = channel(SOME_TIME);
    channel.take_effects();
    let wake = channel.next_wake().unwrap();
    assert!(wake <= SECOND_MS, "{wake}");
    channel.deadline(wake);
    let pages: BTreeSet<u16> = shared(channel.take_effects()).into_keys().collect();
    assert_eq!(pages, BTreeSet::from([WORLD_CLOCK, CLOCK]));
}

#[test]
fn a_long_run_keeps_publishing() {
    // Three hours from two hours before midnight UTC: several sport
    // rounds, and the day changing for the guide and the weather.
    let start = 1_792_886_400_000 - 2 * 3_600 * 1_000;
    let mut channel = channel(start);
    channel.take_effects();
    let until = 3 * 3_600 * 1_000;
    let effects = run(&mut channel, until);
    let sport = effects.iter().filter(|effect| matches!(effect, Effect::Shared { page: SPORT, .. })).count();
    assert_eq!(sport, 3 * 3_600 / 15);
    assert!(channel.next_wake().is_some_and(|at| at > until));
}

#[test]
fn civil_dates_are_right() {
    let cases = [
        (0, (1970, 1, 1, "Thursday")),
        (951_782_400_000, (2000, 2, 29, "Tuesday")),
        (1_792_886_400_000, (2026, 10, 25, "Sunday")),
        (4_107_542_400_000, (2100, 3, 1, "Monday")),
        (94_608_000_000, (1972, 12, 31, "Sunday")),
        (13_574_563_200_000, (2400, 2, 29, "Tuesday")),
    ];
    for (epoch_ms, (year, month, day, weekday)) in cases {
        let date = Date::from_epoch_ms(epoch_ms);
        assert_eq!((date.year, date.month, date.day, date.weekday_name()), (year, month, day, weekday), "{epoch_ms}");
        if let Some(before) = epoch_ms.checked_sub(1) {
            assert_ne!(Date::from_epoch_ms(before).day, day, "{epoch_ms}");
        }
    }
    assert_eq!(Date::from_epoch_ms(SOME_TIME).long(), "Sunday 25 October 2026");
    assert_eq!(Date::from_epoch_ms(SOME_TIME).short(), "Sun 25 Oct");
    assert_eq!(calendar::hours_minutes_seconds(SOME_TIME), "14:03:22");
    assert_eq!(SOME_TIME / DAY_MS, 20_751);
}

#[test]
fn every_day_follows_the_one_before() {
    // 450 years of days, through the leap years and the century rules,
    // each one day on from the last.
    let mut last = Date::from_days(0);
    for days in 1..(450 * 366) {
        let date = Date::from_days(days);
        let next_day = date.day == last.day + 1 && date.month == last.month && date.year == last.year;
        let next_month = date.day == 1 && date.month == last.month + 1 && date.year == last.year;
        let next_year = date.day == 1 && date.month == 1 && last.month == 12 && last.day == 31 && date.year == last.year + 1;
        assert!(next_day || next_month || next_year, "{last:?} then {date:?}");
        assert_eq!(date.weekday, (last.weekday + 1) % 7);
        last = date;
    }
}

#[test]
fn local_times_move_by_their_offset() {
    assert_eq!(hours_minutes(local(SOME_TIME, 540)), "23:03");
    assert_eq!(hours_minutes(local(SOME_TIME, 330)), "19:33");
    assert_eq!(hours_minutes(local(SOME_TIME, -600)), "04:03");
    // Behind UTC at its start, the clock stops at the epoch.
    assert_eq!(local(0, -60), 0);
}

#[test]
fn the_world_clock_shows_each_city() {
    let screen = world_clock::screen(SOME_TIME);
    let rows: Vec<String> = (0..25).map(|row| screen.row_text(row)).collect();
    let text = rows.concat();
    for expected in ["Tokyo", "23:03", "Honolulu", "04:03", "Sunday 25 October 2026", "daylight saving"] {
        assert!(text.contains(expected), "{expected}");
    }
    // Sydney is past midnight: Monday.
    let sydney = rows.iter().find(|row| row.contains("Sydney")).unwrap();
    assert!(sydney.contains("00:03") && sydney.contains("Mon"), "{sydney}");
}

#[test]
fn market_walks_stay_bounded() {
    let mut instruments = [Instrument::new("up", 10_000), Instrument::new("down", 10_000), Instrument::new("random", 10_000)];
    let mut seed = 0x1234_5678_u64;
    for _ in 0..100_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let random = u32::try_from(seed % u64::from(STEP_CHOICES)).unwrap();
        for (instrument, choice) in instruments.iter_mut().zip([STEP_CHOICES - 1, 0, random]) {
            instrument.walk(choice);
            assert!((instrument.floor()..=instrument.ceiling()).contains(&instrument.value), "{instrument:?}");
        }
    }
    // The pull back stops even a steady rise or fall short of the bounds.
    assert!(instruments[0].value < instruments[0].ceiling() && instruments[0].value > instruments[0].base);
    assert!(instruments[1].value > instruments[1].floor() && instruments[1].value < instruments[1].base);
}

#[test]
fn every_team_plays_each_round_and_meets_each_other_home_and_away() {
    let mut meetings = BTreeMap::new();
    for round in 0..14 {
        let matches = fixtures(round);
        let teams: BTreeSet<usize> = matches.iter().flat_map(|game| [game.home, game.away]).collect();
        assert_eq!(teams, (0..8).collect::<BTreeSet<_>>(), "round {round}");
        for game in matches {
            *meetings.entry((game.home, game.away)).or_insert(0) += 1;
        }
    }
    let expected: BTreeMap<(usize, usize), i32> = (0..8).flat_map(|home| (0..8).filter(move |&away| away != home).map(move |away| ((home, away), 1))).collect();
    assert_eq!(meetings, expected);
}

#[test]
fn standings_count_results() {
    let standing = Standing::default().with(3, 1).with(0, 0).with(1, 2);
    assert_eq!((standing.played, standing.won, standing.drawn, standing.lost), (3, 1, 1, 1));
    assert_eq!((standing.points(), standing.difference()), (4, 1));
}

#[test]
fn the_weather_follows_the_day() {
    let day = forecast(20_000, 0);
    assert!(day.low < day.high);
    assert_eq!(temperature(day, 300), day.low);
    assert_eq!(temperature(day, 900), day.high);
    let mut last = day.low;
    for minute in 300..=900 {
        let now = temperature(day, minute);
        assert!(now >= last && now <= day.high, "{minute}");
        last = now;
    }
    for minute in 901..(1_440 + 300) {
        let now = temperature(day, minute % 1_440);
        assert!(now <= last && now >= day.low, "{minute}");
        last = now;
    }
    // Every viewer, and every restart, sees the same day.
    assert_eq!(forecast(20_000, 3), forecast(20_000, 3));
}

#[test]
fn the_guide_finds_the_programme_on_now() {
    assert_eq!(guide::on_now(0), 0);
    assert_eq!(guide::on_now(359), 0);
    assert_eq!(guide::on_now(360), 1);
    assert_eq!(guide::on_now(1_439), 19);
    let screen = guide::screen(SOME_TIME);
    // 14:03 is in the afternoon film, from 13:30 to 15:30.
    assert!(screen.row_text(5).contains("13:30") && screen.row_text(5).contains("The Lost Sub-page"), "{}", screen.row_text(5));
    assert!(screen.row_text(7).contains("Ends 15:30, 1 h 27 min to go"), "{}", screen.row_text(7));
}

#[test]
fn navigation_links_three_other_services_and_the_index() {
    for &(page, _) in &SERVICES {
        let links = navigation(page);
        let pages: BTreeSet<u16> = links.iter().filter_map(|&(_, link)| link).collect();
        assert_eq!(pages.len(), 4, "{page:X}");
        assert!(!pages.contains(&page), "{page:X}");
        assert!(pages.contains(&INDEX));
        assert!(links.iter().all(|&(label, _)| !label.is_empty() && label.len() <= 9));
    }
}

#[test]
fn text_wraps_at_spaces() {
    assert_eq!(draw::wrap("one two three four", 9), ["one two", "three", "four"]);
    assert_eq!(draw::wrap("abcdefghijk", 4), ["abcd"]);
    assert_eq!(draw::wrap("", 10), Vec::<String>::new());
}

#[test]
fn carousels_number_their_sub_pages() {
    for (screens, count) in [(news::screens(), 7), (about::about_screens(), 4), (about::facts_screens(), 9)] {
        assert_eq!(screens.len(), count);
        for (screen, number) in screens.iter().zip(1..) {
            assert!(screen.row_text(1).trim_end().ends_with(&alloc::format!("{number}/{count}")), "{}", screen.row_text(1));
        }
    }
}

mod long_read_page {
    use super::long_read::{BAR_ATTRIBUTE_COLUMN, BAR_COLUMN, POSITION_ROW, TEXT_WIDTH, TRACK_SIXELS, VIEW_FIRST_ROW, VIEW_ROWS, lines, thumb};
    use super::{LONG_READ, applications, navigation};
    use crate::common::{FASTEXT_ROW, label_columns};
    use crate::testing::{Harness, Rows, pages_in, sent, text};
    use alloc::format;
    use alloc::string::String;
    use neoteletext_input::modes::MOUSE_BUTTONS;
    use teletext_runtime::Colour;
    use teletext_runtime::SessionId;

    const READER: SessionId = 1;
    const OTHER: SessionId = 2;
    /// A mosaic cell with every sixel set (EN 300 706 Table 47).
    const FULL: u8 = 0x7F;
    /// A mosaic cell with its left column of sixels set: the thin track.
    const TRACK: u8 = 0x20 | 0x01 | 0x04 | 0x10;

    /// The first line of the article shown on `rows`: the position row's
    /// first number, checked against the text in the view's first row.
    fn top_of(rows: &Rows) -> usize {
        let position = text(rows, POSITION_ROW);
        let after = position.split("Lines ").nth(1).unwrap();
        let first: usize = after.split('-').next().unwrap().parse().unwrap();
        let top = first.checked_sub(1).unwrap();
        let line = &lines()[top];
        assert!(text(rows, VIEW_FIRST_ROW).contains(line.text.as_str()), "line {top} is at the top");
        top
    }

    fn opened() -> Harness {
        let mut harness = Harness::new(applications(&[]));
        harness.join(READER, LONG_READ);
        harness
    }

    fn now_at(harness: &Harness) -> usize {
        top_of(&harness.own(READER, LONG_READ))
    }

    /// How many copies of the page the reader has been sent.
    fn copies(harness: &Harness) -> usize {
        let bytes: alloc::vec::Vec<u8> = sent(&harness.effects, READER).iter().flat_map(|packet| packet.bytes).collect();
        pages_in(&bytes).iter().filter(|(number, _)| *number == LONG_READ).count()
    }

    /// The article is long: several screens, every line within the width
    /// left beside the bar, in characters the English set shows as
    /// themselves (EN 300 706 Table 36 replaces `#`, brackets and others).
    #[test]
    fn the_article_is_long_and_fits_beside_the_bar() {
        let all = lines();
        assert!(all.len() > 5 * VIEW_ROWS, "{} lines", all.len());
        for line in &all {
            assert!(line.text.chars().count() <= TEXT_WIDTH, "{:?}", line.text);
            assert!(line.text.chars().all(|c| c.is_ascii_alphanumeric() || " .,:;'()-!?".contains(c)), "{:?}", line.text);
        }
    }

    /// Before scrolling a reader sees the shared start; each key moves only
    /// their own page, by a screen less one line, a line, or to an end.
    #[test]
    fn the_keys_scroll_each_readers_own_page() {
        let mut harness = opened();
        let shared = harness.shared(LONG_READ);
        assert_eq!(top_of(&shared), 0);
        assert!(text(&shared, POSITION_ROW).contains(&format!("Lines 1-{VIEW_ROWS} of {}", lines().len())));
        assert_eq!(harness.page_modes(READER), Some((LONG_READ, MOUSE_BUTTONS)), "the page takes clicks and the wheel");

        let screen = VIEW_ROWS - 1;
        harness.press(READER, "PageDown");
        assert_eq!(now_at(&harness), screen);
        harness.type_text(READER, " ");
        assert_eq!(now_at(&harness), 2 * screen, "Space pages down too");
        harness.press(READER, "PageUp");
        assert_eq!(now_at(&harness), screen);
        harness.press(READER, "ArrowDown");
        assert_eq!(now_at(&harness), screen + 1);
        harness.press(READER, "ArrowUp");
        assert_eq!(now_at(&harness), screen);

        let last = lines().len() - VIEW_ROWS;
        harness.press(READER, "End");
        let end = harness.own(READER, LONG_READ);
        assert_eq!(top_of(&end), last);
        assert!(text(&end, POSITION_ROW).contains("100%"));
        let before = copies(&harness);
        harness.press(READER, "PageDown");
        assert_eq!(copies(&harness), before, "no further than the end, so nothing sent");
        assert_eq!(now_at(&harness), last);
        harness.press(READER, "Home");
        assert_eq!(now_at(&harness), 0);

        // Another reader still starts at the top, and the shared page never moved.
        harness.press(READER, "PageDown");
        harness.join(OTHER, LONG_READ);
        assert_eq!(top_of(&harness.shared(LONG_READ)), 0);
        assert_eq!(now_at(&harness), screen);
    }

    /// The wheel scrolls three lines a notch, and small turns add up.
    #[test]
    fn the_wheel_scrolls_and_small_turns_add_up() {
        let mut harness = opened();
        harness.wheel(READER, 120, 10, 10);
        assert_eq!(now_at(&harness), 3);
        for _ in 0..3 {
            harness.wheel(READER, 40, 10, 10);
        }
        assert_eq!(now_at(&harness), 6, "three thirds of a notch are a notch");
        harness.wheel(READER, -480, 10, 10);
        assert_eq!(now_at(&harness), 0, "never above the start");
    }

    /// The bar's block is the share shown, at the top at the start and at
    /// the bottom at the end, drawn in white over a thin blue track.
    #[test]
    fn the_scroll_bar_shows_where_the_reader_is() {
        let total = lines().len();
        let (start, length) = thumb(0, total);
        assert_eq!(start, 0);
        assert_eq!(length, (TRACK_SIXELS * VIEW_ROWS / total).max(3));
        let (start, length) = thumb(total - VIEW_ROWS, total);
        assert_eq!(start + length, TRACK_SIXELS, "the block ends at the bottom");
        let middle = thumb((total - VIEW_ROWS) / 2, total).0;
        assert!(middle > 0 && middle + length < TRACK_SIXELS, "halfway is between");

        let mut harness = opened();
        let at_top = harness.shared(LONG_READ);
        let bottom = VIEW_FIRST_ROW + VIEW_ROWS - 1;
        assert_eq!(at_top[VIEW_FIRST_ROW][BAR_COLUMN], FULL);
        assert_eq!(at_top[VIEW_FIRST_ROW][BAR_ATTRIBUTE_COLUMN], Colour::White.mosaic());
        assert_eq!(at_top[bottom][BAR_COLUMN], TRACK);
        assert_eq!(at_top[bottom][BAR_ATTRIBUTE_COLUMN], Colour::Blue.mosaic());
        harness.press(READER, "End");
        let at_end = harness.own(READER, LONG_READ);
        assert_eq!(at_end[VIEW_FIRST_ROW][BAR_COLUMN], TRACK);
        assert_eq!(at_end[bottom][BAR_COLUMN], FULL);
        assert_eq!(at_end[bottom][BAR_ATTRIBUTE_COLUMN], Colour::White.mosaic());
    }

    /// A click on the track below the block pages down, above it pages up,
    /// and on the block does nothing.
    #[test]
    fn a_click_on_the_bar_moves_a_screen() {
        let mut harness = opened();
        let bottom = VIEW_FIRST_ROW + VIEW_ROWS - 1;
        harness.click(READER, 0, BAR_COLUMN, bottom);
        assert_eq!(now_at(&harness), VIEW_ROWS - 1);
        harness.click(READER, 0, BAR_ATTRIBUTE_COLUMN, VIEW_FIRST_ROW);
        assert_eq!(now_at(&harness), 0, "the colour cell counts as the bar");
        let before = copies(&harness);
        harness.click(READER, 0, BAR_COLUMN, VIEW_FIRST_ROW);
        assert_eq!(copies(&harness), before, "a click on the block does nothing");
        harness.click(READER, 0, 10, bottom);
        assert_eq!(copies(&harness), before, "a click on the text does nothing");
    }

    /// The page takes the mouse, so it follows a click on a Fastext prompt
    /// itself.
    #[test]
    fn a_click_on_a_prompt_follows_its_link() {
        let mut harness = opened();
        let links = navigation(LONG_READ);
        let (start, _) = label_columns(links.map(|(label, _)| label))[1].unwrap();
        harness.click(READER, 0, start + 1, FASTEXT_ROW);
        assert_eq!(harness.cursor_pages(READER).last().copied(), links[1].1, "sent to {:?}", String::from(links[1].0));
    }
}
