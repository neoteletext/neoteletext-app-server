//! What a channel's address answers over HTTP (the channel URL map's
//! REQ-1693 item 7, DEC-1467 as amended by DEC-1669): the small page a web
//! browser shows at `<channel>` and at each place in it, what a page's
//! address shows (REQ-1728), the redirects to one spelling, `channel.ttx`,
//! the viewers' WebSocket, the program's WebSocket and how its secret is
//! checked (REQ-1044), and 404 for anything else. A page is never served
//! for reading: each one comes over the connection. The HTML itself is the
//! engine's, one builder with a static channel's (REQ-1725).

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;

use neoteletext_web::channel::SETTINGS_FILE;
pub use neoteletext_web::consts::{PUSH_SUBPROTOCOL, PUSH_SUBPROTOCOL_OLDEST, WEBSOCKET_VERSION};
use neoteletext_web::map::Place;
use neoteletext_web::small_page::{self, PublicPage as Shown};
pub use neoteletext_web::small_page::{CONTENT_SECURITY_POLICY, PUBLIC_CONTENT_SECURITY_POLICY};
use neoteletext_web::ws::offers_subprotocol;

use crate::descriptions::Answer;
use crate::protocol::PageNumber;

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
/// The program's WebSocket's path within a channel's folder (REQ-1044).
pub const PROGRAM: &str = "program";
/// What comes before the channel's secret in the program's
/// `Authorization` header (RFC 6750 section 2.1).
pub const BEARER: &[u8] = b"Bearer ";
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
    /// `<channel>program`: the channel's program, with the channel's
    /// secret (REQ-1044).
    Program,
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
        Some((folder, PROGRAM)) => (folder, Route::Program),
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

/// Why a push upgrade is refused (DOC-1917), each a status a
/// host answers with. A receiver stops on either.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PushRefusal {
    /// It carries `Origin`: a web page's script always sends one and a
    /// receiver never does, so this stops a web page reaching a channel as
    /// the viewer (cross-site WebSocket hijacking, DEC-1759 lesson 18): 403.
    Origin,
    /// It does not offer the input protocol's subprotocol: another program
    /// or another version (DEC-1759 lesson 14): 426, naming the one spoken.
    Subprotocol,
}

/// 403 Forbidden (RFC 9110 section 15.5.4).
const FORBIDDEN: u16 = 403;
/// 426 Upgrade Required (RFC 9110 section 15.5.22).
const UPGRADE_REQUIRED: u16 = 426;

impl PushRefusal {
    /// The status every host answers it with: 403 for `Origin`, and 426
    /// for the subprotocol, whose answer also names [`PUSH_SUBPROTOCOL`] in
    /// `Sec-WebSocket-Protocol` (and, over HTTP/1.1, `Upgrade: websocket`).
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Self::Origin => FORBIDDEN,
            Self::Subprotocol => UPGRADE_REQUIRED,
        }
    }
}

/// Whether a push upgrade may go ahead, from whether it carried `Origin`
/// and each `Sec-WebSocket-Protocol` value it carried.
///
/// # Errors
/// The reason it is refused.
pub fn push_upgrade<'a>(origin: bool, offered: impl Iterator<Item = &'a [u8]>) -> Result<&'static str, PushRefusal> {
    if origin {
        return Err(PushRefusal::Origin);
    }
    // The newest version spoken that any value offers (REQ-6927 item 2,
    // DEC-6953 item 1): a receiver of version 11 offers 11 and 10, and one
    // of version 10 offers 10 alone.
    let mut oldest = false;
    for value in offered {
        if offers_subprotocol(value, PUSH_SUBPROTOCOL) {
            return Ok(PUSH_SUBPROTOCOL);
        }
        oldest |= offers_subprotocol(value, PUSH_SUBPROTOCOL_OLDEST);
    }
    if oldest { Ok(PUSH_SUBPROTOCOL_OLDEST) } else { Err(PushRefusal::Subprotocol) }
}

/// Whether two secrets are the same, in time that does not depend on
/// where they differ, so that the time an answer takes tells nothing of a
/// secret guessed in part. Their lengths are not hidden.
#[must_use]
pub fn same_secret(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0_u8, |difference, (a, b)| difference | (a ^ b)) == 0
}

/// Whether a program's upgrade may go ahead: its `Authorization` header
/// is `Bearer ` and the channel's secret (REQ-1044). A channel
/// whose secret is empty admits no program.
#[must_use]
pub fn program_admitted(authorization: Option<&[u8]>, secret: &[u8]) -> bool {
    let presented = authorization.and_then(|value| value.strip_prefix(BEARER)).unwrap_or_default();
    !secret.is_empty() && same_secret(presented, secret)
}

/// The lines of one text message from a program: one line a message, as the
/// bridge and the kits send them (REQ-1044), though a message of several
/// lines is read as those lines, as standard input would read them. Each
/// line's trailing white space goes, and empty lines are skipped.
pub fn program_lines(message: &[u8]) -> impl Iterator<Item = &[u8]> {
    message.split(|&byte| byte == b'\n').map(<[u8]>::trim_ascii_end).filter(|line| !line.is_empty())
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
    let picture = public.picture.as_ref().map(|(png, width, height)| (&png[..], *width, *height));
    let shown = Shown { title: &public.title, text: &public.text, canonical: &canonical, picture, alt: public.alt.as_deref() };
    (small_page::public_page(name, &address, &shown), PUBLIC_CONTENT_SECURITY_POLICY)
}

#[cfg(test)]
mod tests;
