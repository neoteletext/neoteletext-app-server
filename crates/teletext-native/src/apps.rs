//! Applications in other processes (DOC-1965;
//! DOC-1964): each channel named in the channels file has
//! one program, in any language, spoken to one JSON object a line. Three
//! transports carry the same lines through the same relay (requirement
//! 6.4):
//! - **standard input and output** (requirement 6.1): the server starts the
//!   program, and starts it again after a pause that doubles each time, up
//!   to [`LONGEST_PAUSE`], when it ends or breaks the protocol;
//! - **a Unix domain socket** (requirement 6.2): the program runs as a
//!   service of its own and connects, and connects again after it or the
//!   server restarts;
//! - **a WebSocket** (REQ-1044): the program runs anywhere, on another
//!   machine too, and connects to `<channel>program` with the channel's
//!   secret; the module `remote` serves it.
//!
//! Each time a program starts its channel is told, and sends it `start`
//! and `connected` for each viewer connected (REQ-1481); while it is away
//! its channel has no pages to show (DEC-1482).

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use neoteletext_web::map::Place;
use teletext_runtime::InitialPage;
use teletext_runtime::channel::DefaultRules;
use teletext_runtime::json::{self, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

mod remote;
mod waiting;

pub use remote::{Claim, Door, LONGEST_MESSAGE as LONGEST_PROGRAM_MESSAGE, Reading, Writing};
pub use waiting::{Outgoing, ToProgram};

/// The longest line either side may send, which `start` tells a program:
/// a page of TTI is under 4 KB, a two-minute sound's samples in base64
/// about 15 MB.
pub use teletext_runtime::protocol::LONGEST_LINE;
/// The longest line of a program's standard error logged whole; the rest
/// of a longer one is cut (requirement 7.5).
pub const LONGEST_ERROR_LINE: usize = 64 * 1024;
/// Lines that may wait for the program: beyond this the server drops the
/// newest and says so, since the network loop never waits for an
/// application (DOC-1965). A pointer's moves are not among them
/// (the module `waiting` holds them apart).
pub const WAITING_LINES: usize = 4_096;
/// Lines from the program that may wait for the channel.
const ARRIVING_LINES: usize = 1_024;
/// The first pause before a restart.
const FIRST_PAUSE: Duration = Duration::from_secs(1);
/// The longest pause before a restart.
pub const LONGEST_PAUSE: Duration = Duration::from_secs(30);
/// A program that ran this long after declaring its pages was ended, not
/// failing: it starts again at once, as when its author saves a change.
/// One that keeps ending sooner starts at most this often or more slowly.
const STEADY: Duration = Duration::from_secs(5);
/// The socket's folder is its owner's alone: its permissions are the
/// authentication (requirement 6.2).
#[cfg(unix)]
const OWNER_ONLY_FOLDER: u32 = 0o700;
/// So is the socket.
#[cfg(unix)]
const OWNER_ONLY_SOCKET: u32 = 0o600;

/// The longest secret read from a file: a secret is a token of a few dozen
/// bytes, and the bound refuses a file named by mistake, such as a log,
/// rather than reading it whole.
pub const LONGEST_SECRET: usize = 4_096;

/// A channel's secret, which its program presents to connect over a
/// WebSocket (REQ-1044). It is never written out: its `Debug` names none of
/// it, so no log can carry it.
#[derive(Clone)]
pub struct Secret(Arc<[u8]>);

impl Secret {
    /// A secret of these bytes.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Self {
        Self(bytes.into())
    }

    /// Its bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Secret(..)")
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        teletext_runtime::landing::same_secret(self.bytes(), other.bytes())
    }
}

impl Eq for Secret {}

/// How a channel's program reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// The server starts it, over its standard input and output.
    Command {
        /// The program and its arguments.
        command: Vec<String>,
        /// The folder it runs in.
        directory: PathBuf,
    },
    /// It connects to this Unix domain socket.
    Socket(PathBuf),
    /// It connects to `<channel>program` over a WebSocket, presenting this
    /// secret (REQ-1044).
    WebSocket(Secret),
}

/// One channel of the channels file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Its folder in the server's address space.
    pub folder: String,
    /// Its name, in `channel.ttx`.
    pub name: String,
    /// Its program.
    pub program: Program,
    /// The page its viewers start on.
    pub initial_page: InitialPage,
    /// It uses the mouse wheel over its pages (`"wheel": true`): its
    /// default rule table then has the `wheel` rule (page-rules.md
    /// RULE-53).
    pub wheel: bool,
    /// What a page has for a connection its program gave no rule table of
    /// its own (`"default_rules"`, RULE-53).
    pub default_rules: DefaultRules,
}

/// The page a channel's viewers start on when its entry names none: the
/// service's home page (EN 300 706 Annex E.15).
pub const DEFAULT_INITIAL_PAGE: u16 = neoteletext_core::limits::PAGE_HOME;

/// The digits of a page number, as a channels file writes one.
const PAGE_DIGITS: usize = 3;
/// A page number's digits are read as the hexadecimal they also are: page
/// 100 is `0x100`.
const HEXADECIMAL: u32 = 16;

/// Reads the channels file: `{"channels":[{"folder","name","command":[…],
/// "directory","initial_page","wheel","default_rules"}]}`, or `"socket"` in place of `"command"` and
/// `"directory"`, or, for a program that connects over a WebSocket,
/// `"secret_variable"` or `"secret_file"`, the environment variable or file
/// that holds the channel's secret, never the secret itself (REQ-1044). A
/// relative directory, socket or secret file is taken from the file's own
/// folder. `initial_page`, three decimal digits such as `"100"`, is the
/// page the channel's viewers start on, [`DEFAULT_INITIAL_PAGE`] when left
/// out (TODO-2135 item 11: `channel.ttx` reports it, so it can vary).
/// `wheel`, true or false and false when left out, says the channel uses
/// the mouse wheel over its pages. `default_rules`, `"none"` or
/// `"send_all"` and `"none"` when left out, says what a page has for a
/// connection its program gave no rule table: none, so that the page sends
/// its channel no input, or the default table, which sends every kind of
/// input as push protocol 8 did, with the wheel when `wheel` is true
/// (page-rules.md RULE-53, REQ-6523).
///
/// # Errors
/// A file that cannot be read or is not a valid channels file.
pub fn read_channels(path: &Path) -> Result<Vec<ChannelSpec>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let value = json::parse(&bytes).map_err(|fault| format!("{}: not JSON: {fault:?}", path.display()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let items = value.get("channels").and_then(Value::items).ok_or_else(|| format!("{}: channels must be an array", path.display()))?;
    let mut specs: Vec<ChannelSpec> = Vec::new();
    for item in items {
        let text = |name: &str| item.get(name).and_then(Value::text).map(str::to_owned);
        let folder =
            text("folder").filter(|folder| !folder.is_empty() && folder.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        let folder = folder.ok_or_else(|| format!("{}: a folder is letters, digits, - and _", path.display()))?;
        // Its address would read as a place in the channel above it, so no
        // viewer could reach it (DEC-1694 rule 2).
        if Place::read(&folder).is_some() {
            return Err(format!("{}: {folder} belongs to the channel above it: a channel cannot be named like a page, contents or wall", path.display()));
        }
        let command: Vec<String> = item.get("command").and_then(Value::items).unwrap_or(&[]).iter().filter_map(Value::text).map(str::to_owned).collect();
        let socket = text("socket").filter(|socket| !socket.is_empty());
        let variable = text("secret_variable").filter(|variable| !variable.is_empty());
        let secret_file = text("secret_file").filter(|file| !file.is_empty());
        let named = [!command.is_empty(), socket.is_some(), variable.is_some() || secret_file.is_some()];
        if named.iter().filter(|&&named| named).count() != 1 {
            return Err(format!("{}: {folder} needs one of a command, a socket, or a secret for a program over a WebSocket", path.display()));
        }
        let program = if let Some(socket) = socket {
            Program::Socket(base.join(socket))
        } else if command.is_empty() {
            let secret = match (variable, secret_file) {
                (Some(variable), None) => secret_from_variable(&variable),
                (None, Some(file)) => secret_from_file(&base.join(file)),
                _ => Err("names a secret_variable and a secret_file; it needs one".to_owned()),
            };
            Program::WebSocket(secret.map_err(|fault| format!("{}: {folder}'s secret: {fault}", path.display()))?)
        } else {
            Program::Command { command, directory: base.join(text("directory").unwrap_or_default()) }
        };
        if specs.iter().any(|spec| spec.folder == folder) {
            return Err(format!("{}: {folder} twice", path.display()));
        }
        let initial_page = match text("initial_page") {
            None => InitialPage::new(DEFAULT_INITIAL_PAGE),
            // Three digits, read as the page number they spell (DOC-1865);
            // InitialPage refuses a hexadecimal digit or one outside 100 to 899.
            Some(digits) if digits.len() == PAGE_DIGITS && digits.bytes().all(|byte| byte.is_ascii_digit()) => {
                u16::from_str_radix(&digits, HEXADECIMAL).map_err(|error| error.to_string()).and_then(InitialPage::new)
            }
            Some(digits) => Err(format!("{digits:?} is not three decimal digits")),
        }
        .map_err(|fault| format!("{}: {folder}'s initial_page: {fault}", path.display()))?;
        let wheel = match item.get("wheel") {
            None => false,
            Some(value) => value.boolean().ok_or_else(|| format!("{}: {folder}'s wheel is true or false", path.display()))?,
        };
        let default_rules = match item.get("default_rules").map(Value::text) {
            None | Some(Some("none")) => DefaultRules::None,
            Some(Some("send_all")) => DefaultRules::SendAll,
            Some(_) => return Err(format!("{}: {folder}'s default_rules is \"none\" or \"send_all\"", path.display())),
        };
        specs.push(ChannelSpec { name: text("name").unwrap_or_else(|| folder.clone()), folder, program, initial_page, wheel, default_rules });
    }
    Ok(specs)
}

/// The secret in the environment variable `name`, its surrounding white
/// space dropped.
fn secret_from_variable(name: &str) -> Result<Secret, String> {
    let value = std::env::var_os(name).ok_or_else(|| format!("the environment variable {name} is not set"))?;
    let value = value.into_string().map_err(|_| format!("the environment variable {name} is not UTF-8"))?;
    nonempty_secret(value.as_bytes()).ok_or_else(|| format!("the environment variable {name} is empty"))
}

/// The secret in the file at `path`, of at most [`LONGEST_SECRET`] bytes,
/// its surrounding white space dropped, so that the newline an editor ends
/// it with is not part of it.
fn secret_from_file(path: &Path) -> Result<Secret, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    let limit = u64::try_from(LONGEST_SECRET).unwrap_or(u64::MAX).saturating_add(1);
    file.take(limit).read_to_end(&mut bytes).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.len() > LONGEST_SECRET {
        return Err(format!("{} is over {LONGEST_SECRET} bytes", path.display()));
    }
    nonempty_secret(&bytes).ok_or_else(|| format!("{} is empty", path.display()))
}

/// `bytes` without surrounding white space as a secret, unless nothing is
/// left: an empty secret would admit no program anyway.
fn nonempty_secret(bytes: &[u8]) -> Option<Secret> {
    Some(bytes.trim_ascii()).filter(|secret| !secret.is_empty()).map(Secret::new)
}

/// What the program sends the channel.
#[derive(Debug)]
pub enum FromApplication {
    /// The program started, or connected: it is to be sent `start`.
    Started,
    /// A line, without its newline.
    Line(Vec<u8>),
    /// The program ended or went: lines for it are dropped until it
    /// starts again.
    Ended,
}

/// The two directions of a channel's program, as its channel's task holds
/// them, and, for a program that connects over a WebSocket, the door the
/// HTTP side lets it in by.
#[derive(Debug)]
pub struct Link {
    /// What the program sends.
    pub from: mpsc::Receiver<FromApplication>,
    /// What is sent to the program.
    pub to: ToProgram,
    /// Where an upgraded connection of the program's is handed over
    /// (REQ-1044); none for a program over standard input and output or a
    /// Unix domain socket.
    pub door: Option<Door>,
}

/// Starts serving `spec`'s program, which is kept running or awaited, and
/// started again or awaited again whenever it ends, until the server
/// stops. Its declarations and lines arrive on the link; a program over a
/// WebSocket comes in by the link's door.
///
/// # Errors
/// A socket that cannot be made, or a socket on a system without them.
pub fn start(spec: ChannelSpec) -> Result<Link, String> {
    let (to, outgoing) = waiting::waiting();
    let (arrived, from) = mpsc::channel::<FromApplication>(ARRIVING_LINES);
    let mut door = None;
    match spec.program {
        Program::Command { command, directory } => {
            tokio::spawn(supervise(spec.folder, command, directory, arrived, outgoing));
        }
        Program::Socket(path) => {
            #[cfg(unix)]
            {
                let listener = bind(&path).map_err(|error| format!("{}: cannot listen on {}: {error}", spec.folder, path.display()))?;
                tracing::info!(target: "program", channel = spec.folder, socket = %path.display(), "waiting for the program to connect");
                tokio::spawn(await_connections(spec.folder, listener, arrived, outgoing));
            }
            #[cfg(not(unix))]
            {
                drop((arrived, outgoing));
                return Err(format!("{}: a socket ({}) needs Linux or macOS", spec.folder, path.display()));
            }
        }
        Program::WebSocket(secret) => {
            let (opened, arrivals) = remote::door(secret);
            tracing::info!(target: "program", channel = spec.folder, "waiting for the program to connect over a WebSocket");
            tokio::spawn(remote::await_programs(spec.folder, arrivals, arrived, outgoing));
            door = Some(opened);
        }
    }
    Ok(Link { from, to, door })
}

/// Keeps a program of the server's own running: starts it, relays its
/// lines, and starts it again when it ends.
async fn supervise(folder: String, command: Vec<String>, directory: PathBuf, arrived: mpsc::Sender<FromApplication>, mut outgoing: Outgoing) {
    let mut pause = FIRST_PAUSE;
    loop {
        let (ended, ran) = match launch(&folder, &command, &directory) {
            Ok((mut child, stdin, stdout, stderr)) => {
                tracing::info!(target: "program", channel = folder, program = command.first().map(String::as_str), "the program started");
                tokio::spawn(log_errors(folder.clone(), stderr));
                if !started(&arrived, &mut outgoing).await {
                    return;
                }
                let began = tokio::time::Instant::now();
                let reason = relay(stdout, stdin, &arrived, &mut outgoing).await;
                let _ = child.kill().await;
                if arrived.send(FromApplication::Ended).await.is_err() {
                    return;
                }
                (reason, began.elapsed())
            }
            Err(error) => (error, Duration::ZERO),
        };
        if ran >= STEADY {
            pause = FIRST_PAUSE;
        }
        let wait = pause_after(ran, pause);
        tracing::info!(target: "program", channel = folder, reason = ended, restart_in_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX), "the program ended");
        tokio::time::sleep(wait).await;
        if !wait.is_zero() {
            pause = pause.saturating_mul(2).min(LONGEST_PAUSE);
        }
        if arrived.is_closed() {
            return;
        }
    }
}

/// The program started: lines left from before it are dropped, since it
/// is told of every viewer afresh, and its channel is told. False when the
/// channel has stopped.
async fn started(arrived: &mpsc::Sender<FromApplication>, outgoing: &mut Outgoing) -> bool {
    outgoing.clear();
    arrived.send(FromApplication::Started).await.is_ok()
}

/// The pause before starting a program again that ran `ran`: none after a
/// steady run, else the back-off's.
fn pause_after(ran: Duration, pause: Duration) -> Duration {
    if ran >= STEADY { Duration::ZERO } else { pause }
}

fn launch(folder: &str, command: &[String], directory: &Path) -> Result<(Child, ChildStdin, BufReader<ChildStdout>, ChildStderr), String> {
    let (program, arguments) = command.split_first().ok_or("no command")?;
    let mut child = Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .env("NEOTELETEXT_CHANNEL", folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("cannot start {program} in {}: {error}", directory.display()))?;
    let stdin = child.stdin.take().ok_or("no standard input")?;
    let stdout = child.stdout.take().ok_or("no standard output")?;
    let stderr = child.stderr.take().ok_or("no standard error")?;
    Ok((child, stdin, BufReader::new(stdout), stderr))
}

/// Logs each line of a program's standard error (requirement 7.5).
async fn log_errors(folder: String, stderr: ChildStderr) {
    let mut reader = BufReader::new(stderr);
    while let Ok(Some((line, cut))) = read_line_cut(&mut reader, LONGEST_ERROR_LINE).await {
        tracing::info!(target: "program", channel = folder, cut, "{}", String::from_utf8_lossy(&line));
    }
}

/// One line of at most [`LONGEST_LINE`] bytes, without its newline; `None`
/// at the end of the stream.
async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    let limit = u64::try_from(LONGEST_LINE).unwrap_or(u64::MAX).saturating_add(1);
    let read = (&mut *reader).take(limit).read_until(b'\n', &mut line).await.map_err(|error| format!("reading its output: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    } else if line.len() > LONGEST_LINE {
        return Err(format!("sent a line over {LONGEST_LINE} bytes"));
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

/// One line, its first `most` bytes and whether the rest was cut; `None`
/// at the end of the stream.
async fn read_line_cut<R: AsyncBufRead + Unpin>(reader: &mut R, most: usize) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let limit = u64::try_from(most).unwrap_or(u64::MAX);
    let mut line = Vec::new();
    if (&mut *reader).take(limit).read_until(b'\n', &mut line).await? == 0 {
        return Ok(None);
    }
    let mut cut = false;
    if line.last() != Some(&b'\n') && line.len() >= most {
        cut = true;
        let mut rest = Vec::new();
        loop {
            rest.clear();
            if (&mut *reader).take(limit).read_until(b'\n', &mut rest).await? == 0 || rest.last() == Some(&b'\n') {
                break;
            }
        }
    }
    while matches!(line.last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(Some((line, cut)))
}

/// Carries lines both ways until the program ends; answers why it did.
/// The same for every transport (requirement 6.4). Each direction runs to
/// the end of the relay, so a line partly read is never dropped when a line
/// for the program comes first (REQ-1345), and neither direction waits for
/// the other: a program stuck writing a long line while the server writes
/// to it would stop them both.
async fn relay<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
    arrived: &mpsc::Sender<FromApplication>,
    outgoing: &mut Outgoing,
) -> String {
    let hearing = async {
        loop {
            match read_line(&mut reader).await {
                Ok(Some(line)) if line.is_empty() => {}
                Ok(Some(line)) => {
                    if arrived.send(FromApplication::Line(line)).await.is_err() {
                        return "the channel stopped".to_owned();
                    }
                }
                Ok(None) => return "its output closed".to_owned(),
                Err(error) => return error,
            }
        }
    };
    let telling = async {
        while let Some(line) = outgoing.next().await {
            if let Err(error) = writer.write_all(&line).await {
                return format!("writing to it: {error}");
            }
        }
        "the channel stopped".to_owned()
    };
    tokio::select! {
        reason = hearing => reason,
        reason = telling => reason,
    }
}

/// Makes the program's socket at `path` (requirement 6.2): its folder made
/// its owner's alone if it does not exist, a socket left there removed,
/// the socket its owner's alone.
#[cfg(unix)]
fn bind(path: &Path) -> Result<tokio::net::UnixListener, String> {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty() && !folder.exists()) {
        std::fs::DirBuilder::new().recursive(true).mode(OWNER_ONLY_FOLDER).create(folder).map_err(|error| error.to_string())?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(found) if found.file_type().is_socket() => std::fs::remove_file(path).map_err(|error| format!("removing the old socket: {error}"))?,
        Ok(_) => return Err("something other than a socket is there".to_owned()),
        Err(_) => {}
    }
    let listener = tokio::net::UnixListener::bind(path).map_err(|error| error.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(OWNER_ONLY_SOCKET)).map_err(|error| error.to_string())?;
    Ok(listener)
}

/// Serves the program that connects to the socket, one at a time: a second
/// while one is connected is closed.
#[cfg(unix)]
async fn await_connections(folder: String, listener: tokio::net::UnixListener, arrived: mpsc::Sender<FromApplication>, mut outgoing: Outgoing) {
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(error) => {
                tracing::warn!(target: "program", channel = folder, %error, "accepting the program's connection failed");
                tokio::time::sleep(FIRST_PAUSE).await;
                continue;
            }
        };
        let (reading, writing) = stream.into_split();
        let reader = BufReader::new(reading);
        tracing::info!(target: "program", channel = folder, "the program connected");
        if !started(&arrived, &mut outgoing).await {
            return;
        }
        let relaying = relay(reader, writing, &arrived, &mut outgoing);
        tokio::pin!(relaying);
        let reason = loop {
            tokio::select! {
                reason = &mut relaying => break reason,
                extra = listener.accept() => {
                    if extra.is_ok() {
                        tracing::warn!(target: "program", channel = folder, "a second program connected and was closed: one at a time");
                    }
                }
            }
        };
        tracing::info!(target: "program", channel = folder, reason, "the program went");
        if arrived.send(FromApplication::Ended).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
