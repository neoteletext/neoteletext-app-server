//! What a channel's address answers over HTTP, on every host (the channel
//! URL map's REQ-1693 item 7, DEC-1467 as amended by DEC-1669): the small
//! page a web browser shows at `<channel>` and at each place in it, what a
//! page's address shows (REQ-1728), the redirects to one spelling,
//! `channel.ttx`, the WebSocket, and 404 for anything else. A page is never
//! served for reading: each one comes over the connection. The HTML itself
//! is the engine's, one builder with a static channel's (REQ-1725).

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;

use neoteletext_image::raster::picture_size;
use neoteletext_web::channel::SETTINGS_FILE;
use neoteletext_web::consts::PUSH_SUBPROTOCOL;
use neoteletext_web::map::Place;
use neoteletext_web::small_page::{self, PublicPage as Shown};
pub use neoteletext_web::small_page::{CONTENT_SECURITY_POLICY, PUBLIC_CONTENT_SECURITY_POLICY};
use neoteletext_web::ws::offers_subprotocol;

use crate::descriptions::Answer;
use crate::protocol::PageNumber;

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
/// The channel's one file. The name is the engine's, so that a server and a
/// reader cannot disagree on it (DEC-1726 item 5).
pub const DESCRIPTOR: &str = SETTINGS_FILE;

/// What a request under the viewers' address asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// `<channel>`, `<channel>contents` or `<channel>wall`: the small page.
    Page,
    /// `<channel>NNN` or `<channel>NNN/SSSS`: what the page shows the web
    /// (REQ-1728), which is the page's as a whole.
    Described(PageNumber),
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
        None => match Place::read(rest) {
            Some(Place::Page { page, .. }) => Route::Described(PageNumber(page.mpp())),
            Some(_) => Route::Page,
            None => Route::Missing,
        },
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

/// The small page of DEC-1669: the channel's name, a link that opens it in
/// the browser, and a link to get the browser. `address` is the address
/// asked for, which the page repeats escaped.
#[must_use]
pub fn page(name: &str, address: &str) -> String {
    small_page::channel_page(name, address)
}

/// What the address of `page` answers (REQ-1728), and the
/// `Content-Security-Policy` that goes with it. `tail` is what follows the
/// channel's folder in the request, which only the page's link repeats: a
/// sub-page or a query shows the same page, and the page's own address is what
/// search engines are told.
#[must_use]
pub fn described(name: &str, channel: &str, tail: &str, page: PageNumber, answer: &Answer) -> (String, &'static str) {
    let address = format!("{channel}{tail}");
    let Some(public) = answer else { return (small_page::private_page(name, &address), CONTENT_SECURITY_POLICY) };
    let canonical = format!("{channel}{:03X}", page.0);
    let (width, height) = picture_size();
    let shown = Shown { title: &public.title, text: &public.text, canonical: &canonical, picture: public.picture.as_deref().map(|png| (png, width, height)) };
    (small_page::public_page(name, &address, &shown), PUBLIC_CONTENT_SECURITY_POLICY)
}

#[cfg(test)]
mod tests;
