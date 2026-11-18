#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects, reason = "a test")]
//! REQ-1728's verification, over the runtime: what reaches the program, what
//! a program may answer, and what a page's address then shows.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Value, json};

use super::{Answer, DESCRIPTION_CACHE_MS, DESCRIPTION_WAIT_MS, DESCRIPTIONS_BURST, DESCRIPTIONS_PER_SECOND, MOST_CACHED_DESCRIPTIONS, MOST_WAITING_VISITORS};
use crate::landing::{CONTENT_SECURITY_POLICY, PUBLIC_CONTENT_SECURITY_POLICY, described};
use crate::protocol::tests::{line, subpage, with};
use crate::protocol::{DESCRIPTION_TEXT_CHARACTERS, PageNumber, TITLE_CHARACTERS};
use crate::{Channel, ChannelConfig, Effect, InitialPage};

const NAME: &str = "Test";
const ADDRESS: &str = "http://127.0.0.1/test/";
/// A page, and the same as the protocol writes it.
const PAGE: PageNumber = PageNumber(0x100);
const PAGE_TEXT: &str = "100";
/// A page the standard reserves (EN 300 706 Annex A.10).
const RESERVED: PageNumber = PageNumber(0x1FF);

/// A channel whose program runs, and has been told nothing that matters.
fn channel() -> Channel {
    let config = ChannelConfig {
        name: NAME.into(),
        address: ADDRESS.into(),
        initial_page: InitialPage::new(0x100).unwrap(),
        epoch_at_start_ms: 0,
        other_addresses: Vec::new(),
        wheel: false,
        default_rules: crate::channel::DefaultRules::None,
    };
    let mut channel = Channel::new(config, &crate::viewer::testing::secret(), crate::viewer::testing::random());
    channel.program_started(0);
    let _ = channel.take_effects();
    channel
}

/// The lines for the program among `effects`.
fn lines(effects: &[Effect]) -> Vec<Value> {
    effects.iter().filter_map(|effect| if let Effect::ToProgram { line, .. } = effect { Some(serde_json::from_slice(line).unwrap()) } else { None }).collect()
}

/// The answers among `effects`, with their visitors.
fn answers(effects: &[Effect]) -> Vec<(u64, Answer)> {
    effects.iter().filter_map(|effect| if let Effect::Described { visitor, answer } = effect { Some((*visitor, answer.clone())) } else { None }).collect()
}

fn public_description(title: &str, text: &str) -> Value {
    json!({"type": "description", "page": PAGE_TEXT, "public": true, "title": title, "text": text, "content": subpage()})
}

fn not_public() -> Value {
    json!({"type": "description", "page": PAGE_TEXT, "public": false})
}

/// The answer a program's `description` gives a visitor.
fn answered_with(description: &Value) -> Answer {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    let _ = channel.take_effects();
    channel.from_program(&line(description), 1);
    let found = answers(&channel.take_effects());
    assert_eq!(found.len(), 1, "{description}");
    found[0].1.clone()
}

/// What `answer` shows at `tail`.
fn shown(tail: &str, answer: &Answer) -> (String, &'static str) {
    described(NAME, ADDRESS, tail, PAGE, answer)
}

/// What lies between `open` and `close` in `page`, the first time.
fn between<'a>(page: &'a str, open: &str, close: &str) -> &'a str {
    page.split_once(open).unwrap().1.split_once(close).unwrap().0
}

/// The JSON-LD of a page.
fn structured(page: &str) -> Value {
    serde_json::from_str(between(page, "<script type=\"application/ld+json\">", "</script>")).unwrap()
}

/// REQ-1728 item 7: the request is the page number and nothing else.
/// REQ-1476: `describe`.
#[test]
fn the_request_is_a_page_number_and_nothing_else() {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    assert_eq!(lines(&channel.take_effects()), [json!({"type": "describe", "page": PAGE_TEXT})]);
}

/// REQ-1728 verification 1: the program that does not answer, the one that
/// answers not public, the page that cannot exist and the channel with no
/// program give one answer, byte for byte.
#[test]
fn not_public_unanswered_and_missing_are_one_answer() {
    let refusing = answered_with(&not_public());
    let mut silent = channel();
    silent.describe(1, PAGE, 0);
    assert_eq!(answers(&silent.take_effects()), []);
    silent.deadline(DESCRIPTION_WAIT_MS);
    let unanswered = answers(&silent.take_effects())[0].1.clone();
    let mut reserved = channel();
    reserved.describe(1, RESERVED, 0);
    let effects = reserved.take_effects();
    assert_eq!(lines(&effects), [] as [Value; 0], "a reserved page reaches no program");
    let missing = answers(&effects)[0].1.clone();
    let mut stopped = channel();
    stopped.program_ended();
    stopped.describe(1, PAGE, 0);
    let absent = answers(&stopped.take_effects())[0].1.clone();

    let bytes = shown("100", &refusing);
    for other in [unanswered, missing, absent] {
        assert_eq!(shown("100", &other), bytes);
    }
    assert!(bytes.0.contains("<meta name=\"robots\" content=\"noindex\">"));
    assert_eq!(bytes.1, CONTENT_SECURITY_POLICY);
    // Only the address asked for differs.
    assert_eq!(bytes.0.replace("/test/100", "/test/8FE/0002?a=1"), shown("8FE/0002?a=1", &None).0);
}

/// REQ-1728 item 3: a not-public answer holds nothing the program gave, even
/// a program that tries to give something.
#[test]
fn a_page_that_is_not_public_gives_nothing() {
    for field in ["title", "text"] {
        let mut channel = channel();
        channel.describe(1, PAGE, 0);
        let _ = channel.take_effects();
        channel.from_program(&line(&with(&not_public(), &format!("/{field}"), json!("secret"))), 1);
        let effects = channel.take_effects();
        assert_eq!(answers(&effects), [], "refused, so the answer waits for the bound");
        let told = lines(&effects);
        assert_eq!((&told[0]["type"], &told[0]["rule"]), (&json!("refused"), &json!("unknown_field")), "{told:?}");
    }
}

/// REQ-1728 verification 2: a public page's answer holds its title, an inline
/// picture, its text in the hidden element, and JSON-LD whose selector names
/// that element, which the stylesheet hides.
#[test]
fn a_public_page_shows_its_title_picture_and_hidden_text() {
    let answer = answered_with(&public_description("The title", "The words of the page."));
    let (page, policy) = shown("100", &answer);
    assert_eq!(between(&page, "<title>", "</title>"), "The title");
    assert_eq!(between(&page, "<h1>", "</h1>"), "The title");
    let picture = between(&page, "<img src=\"data:image/png;base64,", "\"");
    assert!(picture.starts_with("iVBOR"), "a PNG's signature in base64: {picture:.20}");
    assert!(page.contains("Open in NeoTeletext") && page.contains("Get NeoTeletext"));
    assert!(!page.contains("noindex"));
    assert_eq!(policy, PUBLIC_CONTENT_SECURITY_POLICY);

    let data = structured(&page);
    assert_eq!(data["isAccessibleForFree"], json!(false));
    assert_eq!(data["hasPart"]["isAccessibleForFree"], json!(false));
    assert_eq!(data["hasPart"]["@type"], json!("WebPageElement"));
    let selector = data["hasPart"]["cssSelector"].as_str().unwrap();
    let class = selector.strip_prefix('.').expect("Google accepts only a class selector");
    assert_eq!(between(&page, &format!("<div class=\"{class}\">"), "</div>"), "The words of the page.");
    assert_eq!(between(&page, "<style>", "</style>"), format!("{selector}{{display:none}}"));
    assert_eq!(data["headline"], json!("The title"));
    assert_eq!(data["url"], json!(format!("{ADDRESS}{PAGE_TEXT}")));
}

/// REQ-1728 item 1: a sub-page's address and a query show the page, and search
/// engines are told the page's own address.
#[test]
fn a_sub_page_and_a_query_show_the_page_under_its_own_address() {
    let answer = answered_with(&public_description("The title", "Words"));
    let (page, _) = shown("100/0002?a=1", &answer);
    assert!(page.contains("<link rel=\"canonical\" href=\"http://127.0.0.1/test/100\">"));
    assert_eq!(structured(&page)["url"], json!("http://127.0.0.1/test/100"));
    assert!(page.contains("href=\"neottx://http://127.0.0.1/test/100/0002?a=1\""), "the link is what was asked for");
}

/// REQ-1728 verification 3: what a program gives comes out escaped, in HTML
/// and in the JSON-LD block, and the block still reads as what was given.
#[test]
fn a_title_or_text_holding_markup_comes_out_escaped() {
    let hostile = "</script><b>bold</b><script>alert(1)</script>&'\"<!--";
    let (page, _) = shown("100", &answered_with(&public_description(hostile, hostile)));
    assert!(!page.contains("<b>") && !page.contains("alert(1)</script>"));
    assert_eq!(page.matches("</script>").count(), 1, "the block's own end alone");
    assert_eq!(page.matches("<script").count(), 1, "the block's own start alone");
    assert_eq!(structured(&page)["headline"], json!(hostile), "the escapes read back as the title");
    assert!(page.contains("&lt;/script&gt;&lt;b&gt;bold&lt;/b&gt;"));
}

/// REQ-1728 verification 5: repeated requests for one page reach the program
/// once within the cache's life, public or not, and again after it.
#[test]
fn repeated_requests_reach_the_program_once_within_the_caches_life() {
    for description in [public_description("t", "x"), not_public()] {
        let mut channel = channel();
        channel.describe(1, PAGE, 0);
        assert_eq!(lines(&channel.take_effects()).len(), 1);
        channel.from_program(&line(&description), 1);
        let _ = channel.take_effects();
        for visitor in 2..50 {
            channel.describe(visitor, PAGE, DESCRIPTION_CACHE_MS);
            let effects = channel.take_effects();
            assert_eq!(lines(&effects), [] as [Value; 0], "{description}");
            assert_eq!(answers(&effects).len(), 1);
        }
        channel.describe(99, PAGE, DESCRIPTION_CACHE_MS + 1);
        assert_eq!(lines(&channel.take_effects()).len(), 1, "the cache's life is over");
    }
}

/// A program that does not answer is kept as not public too: a hung program
/// is not asked again for every request.
#[test]
fn an_unanswered_page_is_kept_as_not_public() {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    let _ = channel.take_effects();
    channel.deadline(DESCRIPTION_WAIT_MS);
    let _ = channel.take_effects();
    channel.describe(2, PAGE, DESCRIPTION_WAIT_MS + 1);
    let effects = channel.take_effects();
    assert_eq!(lines(&effects), [] as [Value; 0]);
    assert_eq!(answers(&effects), [(2, None)]);
}

/// Everyone waiting for one page is one request, and all get the one answer.
#[test]
fn those_waiting_for_a_page_are_one_request() {
    let mut channel = channel();
    for visitor in 1..=3 {
        channel.describe(visitor, PAGE, 0);
    }
    assert_eq!(lines(&channel.take_effects()).len(), 1);
    channel.from_program(&line(&not_public()), 1);
    assert_eq!(answers(&channel.take_effects()).iter().map(|(visitor, _)| *visitor).collect::<Vec<_>>(), [1, 2, 3]);
}

/// The deadline wakes the host: a waiting visitor sets the channel's next wake.
#[test]
fn a_waiting_visitor_sets_the_next_wake() {
    let mut channel = channel();
    assert_eq!(channel.next_wake(), None);
    channel.describe(1, PAGE, 10);
    assert_eq!(channel.next_wake(), Some(10 + DESCRIPTION_WAIT_MS));
}

/// The pages of a burst from a quiet time.
fn burst() -> impl Iterator<Item = PageNumber> {
    (0..=DESCRIPTIONS_BURST).map(|index| PageNumber(0x200 + u16::try_from(index).unwrap()))
}

/// REQ-1728 item 7: the program is asked at a named rate, whatever is asked
/// of the server, and what is over the rate is not kept.
#[test]
fn requests_to_the_program_are_limited() {
    let mut channel = channel();
    for (visitor, page) in burst().enumerate() {
        channel.describe(visitor as u64, page, 0);
    }
    let effects = channel.take_effects();
    assert_eq!(lines(&effects).len() as u64, DESCRIPTIONS_BURST);
    assert_eq!(answers(&effects).len(), 1, "the one over the limit is answered as not public at once");
    let over = burst().last().unwrap();
    // A request's worth of allowance comes back at the sustained rate.
    let gap = 1_000 / DESCRIPTIONS_PER_SECOND;
    channel.describe(1_000, over, gap);
    assert_eq!(lines(&channel.take_effects()).len(), 1, "it was not kept as not public");
}

/// The cache and the waiters have bounds of their own.
#[test]
fn the_cache_and_the_waiters_are_bounded() {
    let mut channel = channel();
    for visitor in 0..=MOST_WAITING_VISITORS as u64 {
        channel.describe(visitor, PAGE, 0);
    }
    assert_eq!(answers(&channel.take_effects()).len(), 1, "the one past the bound does not wait");
    channel.from_program(&line(&not_public()), 1);
    let _ = channel.take_effects();

    let mut cache = super::Descriptions::default();
    for page in 0..=u16::try_from(MOST_CACHED_DESCRIPTIONS).unwrap() {
        cache.keep(page, None, false, u64::from(page));
    }
    assert_eq!(cache.cached.len(), MOST_CACHED_DESCRIPTIONS);
    assert!(!cache.cached.contains_key(&0), "the oldest went");
}

/// A program that goes answers nobody: those waiting are told, and what it
/// said goes with it.
#[test]
fn a_program_that_goes_leaves_nobody_waiting() {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    let _ = channel.take_effects();
    channel.program_ended();
    assert_eq!(answers(&channel.take_effects()), [(1, None)]);
    assert_eq!(channel.next_wake(), None);
    channel.program_started(1);
    let _ = channel.take_effects();
    channel.describe(2, PAGE, 1);
    assert_eq!(lines(&channel.take_effects()).len(), 1);
}

/// What the program sends is checked whole: each of these is refused, and
/// none reaches the cache.
/// REQ-1476: `description`.
#[test]
fn a_description_is_checked_whole() {
    let public = public_description("t", "x");
    let refused = |description: &Value, rule: &str| {
        let mut channel = channel();
        channel.describe(1, PAGE, 0);
        let _ = channel.take_effects();
        channel.from_program(&line(description), 1);
        let effects = channel.take_effects();
        assert_eq!(answers(&effects), [], "{description}");
        assert_eq!(lines(&effects)[0]["rule"], json!(rule), "{description}");
    };
    let long_title: String = "t".repeat(TITLE_CHARACTERS + 1);
    let long_text: String = "x".repeat(DESCRIPTION_TEXT_CHARACTERS + 1);
    refused(&with(&public, "/title", json!(long_title)), "too_long");
    refused(&with(&public, "/text", json!(long_text)), "too_long");
    refused(&with(&public, "/text", json!("two\nlines")), "forbidden_character");
    refused(&with(&public, "/title", json!("right\u{202e}left")), "forbidden_character");
    refused(&with(&public, "/title", json!("")), "too_short");
    refused(&with(&public, "/title", Value::Null), "missing_field");
    refused(&with(&public, "/content", Value::Null), "missing_field");
    refused(&with(&public, "/content/page", json!("200")), "wrong_page");
    refused(&with(&public, "/connection", json!(1)), "unknown_field");
}

/// A description nobody asked for is refused, so a program cannot fill the
/// server's memory with pictures.
/// REQ-1476: `description`.
#[test]
fn a_description_nobody_asked_for_is_refused() {
    let mut channel = channel();
    channel.from_program(&line(&public_description("t", "x")), 1);
    let told = lines(&channel.take_effects());
    assert_eq!((&told[0]["type"], &told[0]["rule"]), (&json!("refused"), &json!("not_awaited")), "{told:?}");
    channel.describe(1, PAGE, 2);
    assert_eq!(lines(&channel.take_effects()).len(), 1, "nothing was kept");
}

/// A slow program's answer arrives after the visitor's wait: it fills the
/// cache, so a public page is not shown as not public for the rest of the
/// minute, and being one answer to one request it comes once.
#[test]
fn a_late_description_fills_the_cache_once() {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    channel.deadline(DESCRIPTION_WAIT_MS);
    assert_eq!(answers(&channel.take_effects()), [(1, None)], "the visitor has waited long enough");
    channel.from_program(&line(&public_description("t", "x")), DESCRIPTION_WAIT_MS + 1);
    let effects = channel.take_effects();
    assert_eq!(lines(&effects), Vec::<Value>::new(), "it is not refused");
    assert_eq!(answers(&effects), [], "nobody waits for it now");
    channel.describe(2, PAGE, DESCRIPTION_WAIT_MS + 2);
    let effects = channel.take_effects();
    assert_eq!(lines(&effects), Vec::<Value>::new(), "the program is not asked again");
    assert_eq!(answers(&effects)[0].1.as_ref().map(|page| page.title.as_str()), Some("t"));
    channel.from_program(&line(&not_public()), DESCRIPTION_WAIT_MS + 3);
    assert_eq!(lines(&channel.take_effects())[0]["rule"], json!("not_awaited"), "a second answer is one nobody asked for");
    channel.describe(3, PAGE, DESCRIPTION_WAIT_MS + 4);
    assert!(answers(&channel.take_effects())[0].1.is_some(), "and changes nothing");
}

/// A late answer has as long as the entry that stands for it: once that has
/// gone nothing is owed, and it is refused as any other.
#[test]
fn a_description_later_than_the_caches_life_is_refused() {
    let mut channel = channel();
    channel.describe(1, PAGE, 0);
    channel.deadline(DESCRIPTION_WAIT_MS);
    let _ = channel.take_effects();
    channel.from_program(&line(&public_description("t", "x")), DESCRIPTION_WAIT_MS + DESCRIPTION_CACHE_MS);
    assert_eq!(lines(&channel.take_effects())[0]["rule"], json!("not_awaited"));
}

/// A picture is the page: two pages draw differently.
#[test]
fn the_picture_is_of_the_page_the_program_gave() {
    let picture_of = |rows: Value| {
        let description = with(&public_description("t", "x"), "/content/rows/1", rows);
        answered_with(&description).unwrap().picture.clone().unwrap().0
    };
    let blank = picture_of(Value::Null);
    let filled = picture_of(json!("QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVpBQkNERUZHSElKS0xNTg=="));
    assert_ne!(blank, filled);
    assert!(filled.len() <= super::MOST_PICTURE_BYTES);
    assert_eq!(filled[..4].to_vec(), [0x89, b'P', b'N', b'G']);
}

/// Requirement 8.4: a program is asked once, for no visitor, to learn that it
/// answers. A channel is ready from its answer, which is not refused and is
/// kept for the first visitor, and not before.
#[test]
fn a_program_is_ready_once_it_answers() {
    let mut channel = channel();
    channel.ask_whether_ready();
    assert_eq!(lines(&channel.take_effects()), [json!({"type": "describe", "page": PAGE_TEXT})]);
    assert!(!channel.is_ready(), "it started, and has said nothing");
    channel.from_program(&line(&json!({"type": "log", "level": "info", "event": "loading", "text": "loading"})), 1);
    assert!(!channel.is_ready(), "a log line answers nothing");
    channel.from_program(&line(&not_public()), 2);
    assert!(channel.is_ready());
    assert_eq!(channel.stats().refused, 0, "it was asked");
    assert_eq!(lines(&channel.take_effects()), [] as [Value; 0]);
    channel.describe(1, PAGE, 3);
    assert_eq!(lines(&channel.take_effects()), [] as [Value; 0], "the answer is kept");
    channel.from_program(&line(&not_public()), 4);
    assert_eq!(channel.stats().refused, 1, "it was asked once");
    channel.program_ended();
    channel.program_started(5);
    assert!(!channel.is_ready(), "a program started again has yet to answer");
}

/// A viewer's answer shows a program answers as well as the question does.
#[test]
fn any_answer_makes_a_program_ready() {
    let mut channel = channel();
    channel.ask_whether_ready();
    channel.from_program(&line(&json!({"type": "denied", "connection": 1, "request": 1, "status": 503})), 1);
    assert_eq!(channel.stats().refused, 0);
    assert!(channel.is_ready());
}

/// A visitor at the page already asked about waits for that answer: a second
/// `describe` would bring a second answer, and that would be refused.
#[test]
fn a_visitor_shares_the_question_a_program_is_asked_on_starting() {
    let mut channel = channel();
    channel.ask_whether_ready();
    let _ = channel.take_effects();
    channel.describe(1, PAGE, 0);
    assert_eq!(lines(&channel.take_effects()), [] as [Value; 0]);
    channel.from_program(&line(&not_public()), 1);
    assert_eq!(answers(&channel.take_effects()), [(1, None)]);
    assert_eq!(channel.stats().refused, 0);
}

/// A program that takes longer than a visitor waits is ready all the same.
#[test]
fn a_late_answer_makes_a_program_ready() {
    let mut channel = channel();
    channel.ask_whether_ready();
    channel.describe(1, PAGE, 0);
    channel.deadline(DESCRIPTION_WAIT_MS);
    let _ = channel.take_effects();
    channel.from_program(&line(&not_public()), DESCRIPTION_WAIT_MS + DESCRIPTION_CACHE_MS * 2);
    assert_eq!(channel.stats().refused, 0, "long after the cache's life");
    assert!(channel.is_ready());
}

/// The ReGIS of a public picture: the left half red (the ReGIS manual's
/// chapter 2, HLS hue 120; chapter 11, a filled polygon).
const RED_LEFT: &[u8] = b"S(M1(H120L50S100))W(I1)P[0,0]F(V[399,0][399,479][0,479][0,0])";

/// The PNG the server's renderer draws for `language` and `bytes`.
fn drawn_png(language: crate::kept::PictureLanguage, bytes: &[u8]) -> (Vec<u8>, usize, usize) {
    crate::fallback::png(&crate::fallback::draw(language, bytes, None).unwrap()).unwrap()
}

/// Renderer parity PAR-26 verification 1: a described ReGIS picture gives
/// the PNG the server's renderer draws, at 800 by 480, in place of the
/// teletext page's.
#[test]
fn a_described_regis_picture_is_the_png_the_servers_renderer_draws() {
    let picture = json!({"language": "regis", "data": crate::protocol::tests::base64(RED_LEFT)});
    let answer = answered_with(&with(&public_description("Red", "x"), "/picture", picture)).unwrap();
    let (png, width, height) = answer.picture.clone().unwrap();
    assert_eq!((width, height), (800, 480));
    assert_eq!((png.to_vec(), width, height), drawn_png(crate::kept::PictureLanguage::Regis, RED_LEFT));
    let (page, _) = shown("100", &Some(answer));
    assert!(page.contains("width=\"800\" height=\"480\" alt=\"Red\""), "{page}");
}

/// PAR-26 and TODO-6813: a public page's ReGIS v2 picture is drawn with
/// its committed numbered drawings, as the browser composes them: a kept
/// picture whose red half lies in drawing 1 gives the PNG of the same red
/// half drawn on the base.
#[test]
fn a_described_kept_regis_pictures_numbered_drawings_are_in_its_png() {
    let mut channel = channel();
    let in_drawing = crate::fallback::tests::RED_LEFT_IN_A_DRAWING;
    let kept =
        json!({"type": "regis", "connections": "kept", "page": PAGE_TEXT, "sub": "0001", "restart": true, "data": crate::protocol::tests::base64(in_drawing)});
    channel.from_program(&line(&kept), 0);
    channel.describe(1, PAGE, 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&with(&public_description("t", "x"), "/picture", json!({"kept": "0001"}))), 2);
    let found = answers(&channel.take_effects());
    let (png, width, height) = found[0].1.as_ref().unwrap().picture.clone().unwrap();
    assert_eq!((png.to_vec(), width, height), drawn_png(crate::kept::PictureLanguage::Regis, RED_LEFT));
    assert_ne!((png.to_vec(), width, height), drawn_png(crate::kept::PictureLanguage::Regis, b""), "not the bare base");
}

/// PAR-26 verification 2: a kept NAPLPS picture likewise, on the 480 by
/// 500 raster.
#[test]
fn a_described_kept_naplps_picture_is_drawn_on_the_raster() {
    let mut channel = channel();
    let stream = [0x0E, 0x3E, 0x5C, 0x24, 0x40, 0x40, 0x40, 0x31, 0x52, 0x47, 0x47];
    let kept =
        json!({"type": "naplps", "connections": "kept", "page": PAGE_TEXT, "sub": "0001", "restart": true, "data": crate::protocol::tests::base64(&stream)});
    channel.from_program(&line(&kept), 0);
    channel.describe(1, PAGE, 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&with(&public_description("t", "x"), "/picture", json!({"kept": "0001"}))), 2);
    let found = answers(&channel.take_effects());
    let (png, width, height) = found[0].1.as_ref().unwrap().picture.clone().unwrap();
    assert_eq!((width, height), (480, 500));
    assert_eq!((png.to_vec(), width, height), drawn_png(crate::kept::PictureLanguage::Naplps, &stream));
}

/// PAR-26 verification 4: the landing page names the picture's own title
/// as `alt`, the ReGIS picture's `D(T'...')` committed (`docs/regis-v2.md`, "Title"),
/// in place of the page's title.
#[test]
fn a_regis_pictures_own_title_is_the_landing_pages_alt() {
    let titled = [RED_LEFT, b"D(T'Harbour',C)".as_slice()].concat();
    let picture = json!({"language": "regis", "data": crate::protocol::tests::base64(&titled)});
    let answer = answered_with(&with(&public_description("Red", "x"), "/picture", picture)).unwrap();
    assert_eq!(answer.alt.as_deref(), Some("Harbour"));
    let (page, _) = shown("100", &Some(answer));
    assert!(page.contains("width=\"800\" height=\"480\" alt=\"Harbour\""), "{page}");
}

/// PAR-26 verification 4: a kept NAPLPS picture's title comes with its
/// objects (ANIM-11, DEC-6638 item 3) and is the landing page's `alt`.
#[test]
fn a_kept_naplps_pictures_title_is_the_landing_pages_alt() {
    let mut channel = channel();
    let stream = [0x0E, 0x3E, 0x5C, 0x24, 0x40, 0x40, 0x40, 0x31, 0x52, 0x47, 0x47];
    let kept =
        json!({"type": "naplps", "connections": "kept", "page": PAGE_TEXT, "sub": "0001", "restart": true, "data": crate::protocol::tests::base64(&stream)});
    let objects = json!({"type": "naplps_objects", "connections": "kept", "page": PAGE_TEXT, "sub": "0001", "objects": [], "title": "Jukebox"});
    channel.from_program(&line(&kept), 0);
    channel.from_program(&line(&objects), 0);
    channel.describe(1, PAGE, 1);
    let _ = channel.take_effects();
    channel.from_program(&line(&with(&public_description("t", "x"), "/picture", json!({"kept": "0001"}))), 2);
    let found = answers(&channel.take_effects());
    let answer = found[0].1.clone().unwrap();
    assert_eq!(answer.alt.as_deref(), Some("Jukebox"));
    let (page, _) = shown("100", &Some(answer));
    assert!(page.contains("alt=\"Jukebox\""), "{page}");
}

/// PAR-26 verification 3: each refusal.
#[test]
fn a_description_picture_with_nothing_kept_or_refused_bytes_is_refused() {
    let refusal = |picture: Value| {
        let mut channel = channel();
        channel.describe(1, PAGE, 0);
        let _ = channel.take_effects();
        channel.from_program(&line(&with(&public_description("t", "x"), "/picture", picture)), 1);
        let refused: Vec<Value> = lines(&channel.take_effects()).into_iter().filter(|line| line["type"] == "refused").collect();
        assert_eq!(refused.len(), 1);
        (refused[0]["path"].clone(), refused[0]["rule"].clone())
    };
    assert_eq!(refusal(json!({"kept": "0001"})), (json!("picture.kept"), json!("not_kept")));
    let failing = crate::protocol::tests::base64(b"V[1,1]D(S2(P9),C)");
    assert_eq!(refusal(json!({"language": "regis", "data": failing})), (json!("picture.data"), json!("invalid_regis")));
    let mut past = alloc::vec![0x1B, 0x2E, 0x20, 0x7A];
    for number in 0x21..0x31_u8 {
        past.extend_from_slice(&[0x80, number]);
        for _ in 0..60 {
            past.extend_from_slice(&[0x19, number.saturating_add(1)]);
        }
    }
    past.extend_from_slice(&[0x80, 0x31, b'x', 0x85, 0x19, 0x21, b'a']);
    let past = crate::protocol::tests::base64(&past);
    assert_eq!(refusal(json!({"language": "naplps", "data": past})), (json!("picture.data"), json!("invalid_naplps")));
}
