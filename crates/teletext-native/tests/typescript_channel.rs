//! The server with a channel whose program is TypeScript on the reference
//! kit (`docs/applications.md`): the real binary, started with a channels
//! file naming `kits/typescript/test/fixtures/hello.ts`, serves the pages
//! the program publishes, read back over HTTP as the engine's packets. It
//! checks the kit's TTI against the server's reader, which only the server
//! has: control codes, the English set's signs, mosaics, Fastext links,
//! the cursor and input modes, sub-pages, links to a sub-page, and TTI
//! written elsewhere. It also serves the repository's own `channels.json`,
//! whose showcase is TypeScript.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neoteletext_core::consts::{COLUMNS, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, Packet, decode};

/// How long the server and the program have to start: a first `node` run
/// and the declaration's own 30 s limit fit well inside it.
const STARTING: Duration = Duration::from_secs(60);
/// Between tries while it starts.
const RETRY: Duration = Duration::from_millis(100);
/// The seven bits of a code; the eighth is odd parity (EN 300 706 clause 8.2).
const CODE_MASK: u8 = 0x7F;

/// The server, killed when the test ends however it ends; its program
/// then reads the end of its input and exits.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// A port nothing listens on now.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Starts the server with the channels file `channels` and answers its address.
fn serve(channels: &Path) -> (Server, String) {
    let address = format!("127.0.0.1:{}", free_port());
    let child = Command::new(env!("CARGO_BIN_EXE_teletext-server"))
        .args(["--listen", &address, "--origin", &format!("http://{address}"), "--channels"])
        .arg(channels)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    (Server(child), address)
}

/// GET `path`: the status and the body, or `None` while nothing answers.
fn get(address: &str, path: &str) -> Option<(u16, Vec<u8>)> {
    let mut stream = TcpStream::connect(address).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let end = response.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&response[..end]).into_owned();
    let status = head.split(' ').nth(1)?.parse().ok()?;
    Some((status, response[end + 4..].to_vec()))
}

/// GET `path` once the server answers it with 200.
fn fetch(address: &str, path: &str) -> Vec<u8> {
    let started = Instant::now();
    loop {
        if let Some((200, body)) = get(address, path) {
            return body;
        }
        assert!(started.elapsed() < STARTING, "{path} was not served within {STARTING:?}");
        std::thread::sleep(RETRY);
    }
}

/// One sub-page of a page's frame: its sub-code, its rows' codes without
/// parity, and its links.
#[derive(Default)]
struct Received {
    subcode: u16,
    rows: Vec<(u8, Vec<u8>)>,
    links: Vec<Option<u16>>,
    /// The sub-code each link names.
    link_subcodes: Vec<Option<u16>>,
    row_24: bool,
}

impl Received {
    fn row(&self, number: u8) -> Vec<u8> {
        self.rows.iter().find(|(row, _)| *row == number).map_or_else(|| vec![b' '; COLUMNS], |(_, codes)| codes.clone())
    }
}

/// The sub-pages a frame holds, in order.
fn received(frame: &[u8]) -> Vec<Received> {
    let mut pages: Vec<Received> = Vec::new();
    for chunk in frame.chunks(PACKET_BYTES) {
        let packet = Packet { bytes: chunk.try_into().unwrap() };
        match decode(&packet) {
            Ok(Decoded::Header(header)) => pages.push(Received { subcode: header.subcode.unwrap().value(), ..Received::default() }),
            Ok(Decoded::Row(row)) => {
                let page = pages.last_mut().unwrap();
                page.rows.push((row.row, row.bytes.iter().map(|byte| byte & CODE_MASK).collect()));
            }
            Ok(Decoded::Links(links)) => {
                let page = pages.last_mut().unwrap();
                page.links = links.links.iter().map(|link| link.map(|link| (u16::from(link.magazine.number()) << 8) | u16::from(link.page.0))).collect();
                page.link_subcodes = links.links.iter().map(|link| link.map(|link| link.subcode.value())).collect();
                page.row_24 = links.display_row_24;
            }
            _ => {}
        }
    }
    pages
}

fn padded(codes: &[u8]) -> Vec<u8> {
    let mut row = codes.to_vec();
    row.resize(COLUMNS, b' ');
    row
}

#[test]
fn the_server_serves_what_a_typescript_program_publishes() {
    let repository = repository();
    let folder = std::env::temp_dir().join(format!("teletext-typescript-channel-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let channels = folder.join("channels.json");
    let fixture = repository.join("kits/typescript/test/fixtures/hello.ts");
    std::fs::write(
        &channels,
        format!(
            r#"{{"channels":[{{"folder":"kit","name":"Kit","command":["node",{fixture:?}],"directory":{repository:?}}}]}}"#,
            fixture = fixture.display().to_string(),
            repository = repository.display().to_string()
        ),
    )
    .unwrap();
    let (_server, address) = serve(&channels);

    let index = received(&fetch(&address, "/kit/100.t42"));
    assert_eq!(index.len(), 1);
    let page = &index[0];
    // Red, then the English set's # at 0x5F and £ at 0x23 (EN 300 706 Table 36).
    assert_eq!(page.row(1), padded(b"\x01Red \x5F1 \x232"));
    // Double Height, then Yellow.
    assert_eq!(page.row(2), padded(b"\x0D\x03Tall"));
    // Green mosaics, a cell of every sixel, then a cell of none (Table 47).
    assert_eq!(page.row(4), padded(b"\x12\x7F\x20"));
    // Red to 101 and cyan to 100; link 5 none; the index link sixth.
    assert_eq!(page.links, [Some(0x101), None, None, Some(0x100), None, Some(0x100)]);
    assert!(page.row_24, "a page with links shows row 24");

    let carousel = received(&fetch(&address, "/kit/101.t42"));
    assert_eq!(carousel.iter().map(|page| page.subcode).collect::<Vec<_>>(), [1, 2], "two sub-pages, numbered as published");
    assert_eq!(carousel[1].row(1), padded(b"\x07Sub-page 2"));

    let raw = received(&fetch(&address, "/kit/102.t42"));
    assert_eq!(raw[0].row(1), padded(b"\x02Raw"), "TTI published as it was written");

    let linked = received(&fetch(&address, "/kit/103.t42"));
    assert_eq!(linked[0].links, [Some(0x101), None, None, None, None, Some(0x100)]);
    // Sub-page 2's sub-code, and 3F7F, any sub-page (EN 300 706 clause 9.6.1).
    assert_eq!(linked[0].link_subcodes, [Some(0x0002), None, None, None, None, Some(0x3F7F)], "the red link names sub-page 2");
    assert!(linked[0].row_24, "the packed line shows row 24");

    let contents = String::from_utf8(fetch(&address, "/kit/contents.txt")).unwrap();
    assert!(contents.contains("Kit index") && contents.contains("Kit carousel"), "{contents}");
}

/// The repository's own channels file serves the TypeScript showcase, as
/// `cargo run -p teletext-native -- --channels channels.json` does.
#[test]
fn the_channels_file_serves_the_typescript_showcase() {
    let (_server, address) = serve(&repository().join("channels.json"));
    let index = received(&fetch(&address, "/showcase-ts/100.t42"));
    let title = String::from_utf8_lossy(&index[0].row(1)).into_owned();
    assert!(title.contains("NeoTeletext showcase"), "{title:?}");
}
