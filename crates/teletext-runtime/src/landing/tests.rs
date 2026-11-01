#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, reason = "a test")]
//! REQ-1693 item 7 and DEC-1669: the routes and the small page.

use alloc::vec::Vec;

use neoteletext_web::map::CONFORMANCE;

use super::*;

const CHANNEL: &str = "home";

/// The status each route answers on a channel, the WebSocket's asked for
/// without an upgrade, as the engine's conformance table states them.
fn status(route: Route) -> &'static str {
    match route {
        Route::Page | Route::Descriptor => "200",
        Route::ToFolder | Route::ToPlace => "308",
        Route::Push => "426",
        Route::Missing => "404",
    }
}

/// REQ-1693's Verification: the routes against the one table.
#[test]
fn every_route_answers_as_the_conformance_table_says() {
    let mut rows = 0;
    for line in CONFORMANCE.lines().filter(|line| line.starts_with("answer\t")) {
        let columns: Vec<&str> = line.split('\t').collect();
        let [_, target, expected, location] = columns[..] else { panic!("{line}") };
        let (path, query) = target.split_once('?').map_or((target, None), |(path, query)| (path, Some(query)));
        let (folder, route) = route(path);
        let answered = if folder == CHANNEL { status(route) } else { "404" };
        assert_eq!(answered, expected, "{line}");
        let redirected = (answered == "308").then(|| match route {
            Route::ToPlace => place_location(path, query),
            _ => folder_location(folder, query),
        });
        assert_eq!(redirected.as_deref(), Some(location).filter(|location| *location != "-"), "{line}");
        rows += 1;
    }
    assert!(rows >= 18, "{rows} rows");
}

#[test]
fn the_page_offers_the_channel_and_the_browser_and_goes_nowhere() {
    let page = page("Home", "https://app.neottx.com/home/");
    assert!(page.contains(&format!(r#"<a id="open" href="{LINK_SCHEME}https://app.neottx.com/home/">Open in NeoTeletext</a>"#)));
    assert!(!page.contains("neoteletext://"), "DEC-1726: the old scheme is gone: {page}");
    assert!(page.contains(r#"<a href="https://www.neoteletext.com/">Get NeoTeletext</a>"#));
    assert!(page.contains("<title>Home</title>") && page.contains("<h1>Home</h1>"));
    // DEC-1669 item 3: no timer, no automatic launch, no redirect.
    for forbidden in ["setTimeout", "setInterval", "http-equiv", "location.href=", "location=", "location.assign", "location.replace", ".click(", "window.open"]
    {
        assert!(!page.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn a_name_and_an_address_cannot_become_markup() {
    let page = page(r#"<script>"'&"#, r#"https://a.example/"><b>/"#);
    assert!(page.contains("<h1>&lt;script&gt;&quot;&#39;&amp;</h1>"));
    assert!(page.contains(&format!("{LINK_SCHEME}https://a.example/&quot;&gt;&lt;b&gt;/")));
    assert!(!page.contains("<b>"));
}

/// DEC-1694: the page runs no script, and its link is built on the server
/// from the address asked for.
#[test]
fn the_page_runs_nothing_and_its_link_names_the_page_asked_for() {
    let page = page("Home", &format!("https://a.example/home/{}", tail("/home/302/0002", Some("a=1"))));
    assert!(page.contains(&format!(r#"href="{LINK_SCHEME}https://a.example/home/302/0002?a=1""#)), "{page}");
    assert!(!page.contains("<script"), "{page}");
    assert!(!CONTENT_SECURITY_POLICY.contains("script-src"));
}
