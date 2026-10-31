//! What a channel's address answers over HTTP, on every host (the channel
//! URL map's REQ-1693 item 7, DEC-1467 as amended by DEC-1669): the small
//! page a web browser shows at `<channel>`, the redirect from the folder
//! with no final slash, `channel.json`, the WebSocket, and 404 for anything
//! else. A page is never served: each one comes over the connection.

use alloc::format;
use alloc::string::String;

/// The push WebSocket's path within a channel's folder.
pub const PUSH: &str = "push";
/// The channel's one file.
pub const DESCRIPTOR: &str = "channel.json";
/// Where someone without the browser gets it.
const GET_NEOTELETEXT: &str = "https://www.neoteletext.com/";
/// What makes a channel's address a link that opens the browser (DEC-1666).
const LINK_SCHEME: &str = "neoteletext://";

/// The one line of script (REQ-1693 item 7). The server never sees a
/// fragment, so the page carries it, and the query, into its link here. A
/// fragment the map does not list is left out, so the page never offers a
/// link the browser would refuse. It only changes that link's `href`: it
/// navigates nowhere and starts no timer, so the page cannot be used as a
/// redirector (DEC-1669 item 3).
const CARRY_FRAGMENT: &str = r##"var f=location.hash.slice(1);document.getElementById("open").href="neoteletext://"+location.href.split("#")[0]+(/^([1-8][0-9A-Fa-f]{2}(\/[0-9A-Fa-f]{4})?(\?[!-~]*)?|contents|wall)$/.test(f)?"#"+f:"");"##;

/// The page's `Content-Security-Policy`: it runs [`CARRY_FRAGMENT`], which
/// the hash names, and loads nothing. A test keeps the hash true.
pub const CONTENT_SECURITY_POLICY: &str =
    "default-src 'none'; script-src 'sha256-lq5g4WjtHVTsX5bHsky9SoqgMzKqF7a0DKk9e5zDQok='; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// What a request under the viewers' address asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// `<channel>`: the small page.
    Page,
    /// The folder without its final `/`: a redirect to `<channel>`.
    ToFolder,
    /// `<channel>channel.json`.
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
        Some((folder, _)) => (folder, Route::Missing),
    }
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
/// the browser, and a link to get the browser. `address` is `<channel>`.
#[must_use]
pub fn page(name: &str, address: &str) -> String {
    let (name, link) = (escaped(name), escaped(&format!("{LINK_SCHEME}{address}")));
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{name}</title>\n</head>\n\
         <body>\n<h1>{name}</h1>\n<p>This is a teletext channel, which opens in the NeoTeletext browser.</p>\n\
         <p><a id=\"open\" href=\"{link}\">Open in NeoTeletext</a></p>\n\
         <p><a href=\"{GET_NEOTELETEXT}\">Get NeoTeletext</a></p>\n\
         <script>{CARRY_FRAGMENT}</script>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests;
