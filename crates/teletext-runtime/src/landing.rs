//! What a channel's address answers over HTTP, on every host (the channel
//! URL map's REQ-1693 item 7, DEC-1467 as amended by DEC-1669): the small
//! page a web browser shows at `<channel>` and at each place in it, the
//! redirects to one spelling, `channel.ttx`, the WebSocket, and 404 for anything
//! else. A page is never served: each one comes over the connection.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;

use neoteletext_core::consts::LINK_SCHEME;
use neoteletext_web::channel::SETTINGS_FILE;
use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use neoteletext_web::map::Place;
use neoteletext_web::ws::offers_subprotocol;

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
/// The channel's one file. The name is the engine's, so that a server and a
/// reader cannot disagree on it (DEC-1726 item 5).
pub const DESCRIPTOR: &str = SETTINGS_FILE;
/// Where someone without the browser gets it.
const GET_NEOTELETEXT: &str = "https://www.neoteletext.com/";

/// The page's `Content-Security-Policy`: it loads and runs nothing. The server
/// sees the whole address, so the page's link needs no script (DEC-1694).
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// What a request under the viewers' address asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// `<channel>`, or a place of the channel: the small page.
    Page,
    /// The folder without its final `/`: a redirect to `<channel>`.
    ToFolder,
    /// A place with a final `/`: a redirect to its one spelling.
    ToPlace,
    /// `<channel>channel.ttx`.
    Descriptor,
    /// `<channel>push`.
    Push,
    /// Anything else under the channel: 404.
    Missing,
}

/// The folder a path names and what it asks of it. Whether the folder is a
/// channel is the host's to say; anything else is 404 either way.
#[must_use]
pub fn route(path: &str) -> (&str, Route) {
    let path = path.strip_prefix('/').unwrap_or(path);
    match path.split_once('/') {
        None => (path, Route::ToFolder),
        Some((folder, "")) => (folder, Route::Page),
        Some((folder, DESCRIPTOR)) => (folder, Route::Descriptor),
        Some((folder, PUSH)) => (folder, Route::Push),
        Some((folder, rest)) => (folder, place_route(rest)),
    }
}

/// What the end of a path asks for: a place of the URL map, read by the
/// engine's one reader (REQ-1693 item 1), or nothing.
fn place_route(rest: &str) -> Route {
    match rest.strip_suffix('/') {
        Some(place) if Place::read(place).is_some() => Route::ToPlace,
        None if Place::read(rest).is_some() => Route::Page,
        _ => Route::Missing,
    }
}

/// What follows the channel's folder in a request's path, and its query:
/// the part of the address the small page's link repeats.
#[must_use]
pub fn tail(path: &str, query: Option<&str>) -> String {
    let path = path.strip_prefix('/').unwrap_or(path);
    let place = path.split_once('/').map_or("", |(_, rest)| rest);
    query.filter(|query| !query.is_empty()).map_or_else(|| place.to_owned(), |query| format!("{place}?{query}"))
}

/// Why a push upgrade is refused (`SERVER.md` section 5.1), each a status a
/// host answers with. A receiver stops on either.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PushRefusal {
    /// It carries `Origin`: a web page's script always sends one and a
    /// receiver never does, so this stops a web page reaching a channel with
    /// a viewer's cookies (cross-site WebSocket hijacking, DEC-1759 lesson
    /// 18): 403.
    Origin,
    /// It does not offer the input protocol's subprotocol: another program
    /// or another version (DEC-1759 lesson 14): 426, naming the one spoken.
    Subprotocol,
}

/// Whether a push upgrade may go ahead, from whether it carried `Origin`
/// and each `Sec-WebSocket-Protocol` value it carried.
///
/// # Errors
/// The reason it is refused.
pub fn push_upgrade<'a>(origin: bool, mut offered: impl Iterator<Item = &'a [u8]>) -> Result<(), PushRefusal> {
    if origin {
        return Err(PushRefusal::Origin);
    }
    if offered.any(|value| offers_subprotocol(value, PUSH_SUBPROTOCOL)) { Ok(()) } else { Err(PushRefusal::Subprotocol) }
}

/// Where a place with a final `/` redirects to: a path on the same origin,
/// so that it cannot lead away.
#[must_use]
pub fn place_location(path: &str, query: Option<&str>) -> String {
    let path = path.strip_suffix('/').unwrap_or(path);
    query.map_or_else(|| path.to_owned(), |query| format!("{path}?{query}"))
}

/// Where the folder without its final `/` redirects to: a path on the same
/// origin, of a folder the host has just found, so that it cannot lead away.
#[must_use]
pub fn folder_location(folder: &str, query: Option<&str>) -> String {
    query.map_or_else(|| format!("/{folder}/"), |query| format!("/{folder}/?{query}"))
}

/// `text` safe in HTML, in a tag's text or in an attribute in double quotes.
fn escaped(text: &str) -> String {
    let mut written = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => written.push_str("&amp;"),
            '<' => written.push_str("&lt;"),
            '>' => written.push_str("&gt;"),
            '"' => written.push_str("&quot;"),
            '\'' => written.push_str("&#39;"),
            other => written.push(other),
        }
    }
    written
}

/// The small page of DEC-1669: the channel's name, a link that opens it in
/// the browser, and a link to get the browser. `address` is the address
/// asked for, a page's or the channel's, which the page repeats escaped.
#[must_use]
pub fn page(name: &str, address: &str) -> String {
    let (name, link) = (escaped(name), escaped(&format!("{LINK_SCHEME}{address}")));
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{name}</title>\n</head>\n\
         <body>\n<h1>{name}</h1>\n<p>This is a teletext channel, which opens in the NeoTeletext browser.</p>\n\
         <p><a id=\"open\" href=\"{link}\">Open in NeoTeletext</a></p>\n\
         <p><a href=\"{GET_NEOTELETEXT}\">Get NeoTeletext</a></p>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests;
