//! The server with a channel's program (`docs/applications.md`): the real
//! binary, started with a channels file naming a program that replays
//! `fixtures/kit-channel.jsonl`, serves the pages the program publishes,
//! read back over HTTP as the engine's packets. The lines are those the
//! TypeScript kit's fixture channel writes (neoteletext-server-channels'
//! `kits/typescript/test/fixtures/hello.jsonl`, which that repository's
//! test checks the kit still writes; the two files must stay the same), so
//! this checks the kit's TTI against the server's reader, which only the
//! server has: control codes, the English set's signs, mosaics, Fastext
//! links, the cursor, sub-pages, links to a sub-page, TTI written
//! elsewhere, and a section and a new title in `contents.txt`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neoteletext_core::consts::{COLUMNS, PACKET_BYTES};
use neoteletext_core::packet::{Decoded, Packet, decode};

/// How long the server and the program have to start: the declaration's
/// own 30 s limit fits well inside it.
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

/// Replays the fixture as a program does: the first line, its pages, then,
/// once the server sends `start`, the rest; then it reads to the end of its
/// input, as a program does until the server ends.
const REPLAY: &str = r#"head -n 1 "$0"; read -r _; tail -n +2 "$0"; exec cat >/dev/null"#;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kit-channel.jsonl").canonicalize().unwrap()
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
fn the_server_serves_what_a_program_publishes() {
    let folder = std::env::temp_dir().join(format!("teletext-channel-program-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let channels = folder.join("channels.json");
    std::fs::write(
        &channels,
        format!(
            r#"{{"channels":[{{"folder":"kit","name":"Kit","command":["sh","-c",{REPLAY:?},{fixture:?}],"directory":{folder:?}}}]}}"#,
            fixture = fixture().display().to_string(),
            folder = folder.display().to_string()
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

    // A page of several sub-pages is files of its own (the engine's
    // REQ-WEB-019): its first in NNN.t42, each in NNN-SSSS.t42, listed by
    // NNN.sub, numbered by place.
    let carousel = received(&fetch(&address, "/kit/101.t42"));
    assert_eq!(carousel.iter().map(|page| page.subcode).collect::<Vec<_>>(), [1], "the first sub-page alone");
    let second = received(&fetch(&address, "/kit/101-0002.t42"));
    assert_eq!(second.iter().map(|page| page.subcode).collect::<Vec<_>>(), [2]);
    assert_eq!(second[0].row(1), padded(b"\x07Sub-page 2"));
    let catalogue = String::from_utf8(fetch(&address, "/kit/101.sub")).unwrap();
    assert_eq!(catalogue.lines().map(|line| line.split('\t').next().unwrap_or("")).collect::<Vec<_>>(), ["0001", "0002"], "{catalogue}");
    let bundle = received(&fetch(&address, "/kit/index.t42"));
    assert_eq!(bundle.iter().filter(|page| page.row(1) == padded(b"\x07Sub-page 2")).count(), 0, "the bundle carries first sub-pages only");

    let raw = received(&fetch(&address, "/kit/102.t42"));
    assert_eq!(raw[0].row(1), padded(b"\x02Raw"), "TTI published as it was written");

    let linked = received(&fetch(&address, "/kit/103.t42"));
    assert_eq!(linked[0].links, [Some(0x101), None, None, None, None, Some(0x100)]);
    // Sub-page 2's sub-code, and 3F7F, any sub-page (EN 300 706 clause 9.6.1).
    assert_eq!(linked[0].link_subcodes, [Some(0x0002), None, None, None, None, Some(0x3F7F)], "the red link names sub-page 2");
    assert!(linked[0].row_24, "the packed line shows row 24");

    let contents = String::from_utf8(fetch(&address, "/kit/contents.txt")).unwrap();
    assert!(contents.contains("Kit index") && contents.contains("Kit carousel"), "{contents}");
    assert!(contents.contains("p\t102\tKit raw, retitled\n") && !contents.contains("\tKit raw\n"), "the page's new title: {contents}");
    assert!(contents.contains("s\tKit links\np\t103\tKit sub-page link\n"), "the section page 103 starts: {contents}");
}
