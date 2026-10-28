//! Applications in other processes (`SERVER.md` section 7.1;
//! `docs/applications.md`): each channel named in the channels file has
//! one program, in any language, spoken to one JSON object a line. Two
//! transports carry the same lines through the same relay (requirement
//! 6.4):
//! - **standard input and output** (requirement 6.1): the server starts the
//!   program, and starts it again after a pause that doubles each time, up
//!   to [`LONGEST_PAUSE`], when it ends or breaks the protocol;
//! - **a Unix domain socket** (requirement 6.2): the program runs as a
//!   service of its own and connects, and connects again after it or the
//!   server restarts.
//!
//! Each time a program starts it declares its pages, and its channel is
//! told (requirement 5.3); its channel pages stay as last published while
//! it is away (section 7.5).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use teletext_runtime::PageEntry;
use teletext_runtime::json::{self, Value};
use teletext_runtime::remote::declared_pages;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

/// The longest line either side may send: a page of TTI is under 4 KB,
/// a two-minute sound's samples in base64 about 15 MB.
pub const LONGEST_LINE: usize = 16 * 1024 * 1024;
/// The longest line of a program's standard error logged whole; the rest
/// of a longer one is cut (requirement 7.5).
pub const LONGEST_ERROR_LINE: usize = 64 * 1024;
/// Lines that may wait for the program: beyond this the server drops the
/// newest and says so, since the network loop never waits for an
/// application (section 7.5).
pub const WAITING_LINES: usize = 4_096;
/// Lines from the program that may wait for the channel.
const ARRIVING_LINES: usize = 1_024;
/// How long a program has to declare its pages.
const DECLARE_WITHIN: Duration = Duration::from_secs(30);
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
}

/// One channel of the channels file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Its folder in the server's address space.
    pub folder: String,
    /// Its name, in `channel.json`.
    pub name: String,
    /// Its program.
    pub program: Program,
}

/// Reads the channels file: `{"channels":[{"folder","name","command":[…],
/// "directory"}]}`, or `"socket"` in place of `"command"` and
/// `"directory"`. A relative directory or socket is taken from the file's
/// own folder.
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
        let command: Vec<String> = item.get("command").and_then(Value::items).unwrap_or(&[]).iter().filter_map(Value::text).map(str::to_owned).collect();
        let program = match (command.is_empty(), text("socket").filter(|socket| !socket.is_empty())) {
            (false, None) => Program::Command { command, directory: base.join(text("directory").unwrap_or_default()) },
            (true, Some(socket)) => Program::Socket(base.join(socket)),
            (false, Some(_)) => return Err(format!("{}: {folder} has a command and a socket; it needs one", path.display())),
            (true, None) => return Err(format!("{}: {folder} needs a command or a socket", path.display())),
        };
        if specs.iter().any(|spec| spec.folder == folder) {
            return Err(format!("{}: {folder} twice", path.display()));
        }
        specs.push(ChannelSpec { name: text("name").unwrap_or_else(|| folder.clone()), folder, program });
    }
    Ok(specs)
}

/// What the program sends the channel.
#[derive(Debug)]
pub enum FromApplication {
    /// The program started and declared these pages: its first line, each
    /// time it starts (requirement 5.3).
    Declared(Vec<PageEntry>),
    /// A line, without its newline.
    Line(Vec<u8>),
    /// The program ended or went: lines for it are dropped until it
    /// declares its pages again.
    Ended,
}

/// The two directions of a channel's program, as its channel's task holds
/// them.
#[derive(Debug)]
pub struct Link {
    /// What the program sends.
    pub from: mpsc::Receiver<FromApplication>,
    /// What is sent to the program.
    pub to: mpsc::Sender<Vec<u8>>,
}

/// Starts serving `spec`'s program, which is kept running or awaited, and
/// started again or awaited again whenever it ends, until the server
/// stops. Its declarations and lines arrive on the link.
///
/// # Errors
/// A socket that cannot be made, or a socket on a system without them.
pub fn start(spec: ChannelSpec) -> Result<Link, String> {
    let (to, outgoing) = mpsc::channel::<Vec<u8>>(WAITING_LINES);
    let (arrived, from) = mpsc::channel::<FromApplication>(ARRIVING_LINES);
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
    }
    Ok(Link { from, to })
}

/// Keeps a program of the server's own running: starts it, relays its
/// lines, and starts it again when it ends.
async fn supervise(folder: String, command: Vec<String>, directory: PathBuf, arrived: mpsc::Sender<FromApplication>, mut outgoing: mpsc::Receiver<Vec<u8>>) {
    let mut pause = FIRST_PAUSE;
    loop {
        let (ended, ran) = match launch(&folder, &command, &directory) {
            Ok((mut child, stdin, mut stdout, stderr)) => {
                tracing::info!(target: "program", channel = folder, program = command.first().map(String::as_str), "the program started");
                tokio::spawn(log_errors(folder.clone(), stderr));
                match declare(&mut stdout).await {
                    Ok(pages) => {
                        tracing::info!(target: "program", channel = folder, pages = pages.len(), "the program declared its pages");
                        if !started(pages, &arrived, &mut outgoing).await {
                            return;
                        }
                        pause = FIRST_PAUSE;
                        let began = tokio::time::Instant::now();
                        let reason = relay(stdout, stdin, &arrived, &mut outgoing).await;
                        let _ = child.kill().await;
                        if arrived.send(FromApplication::Ended).await.is_err() {
                            return;
                        }
                        (reason, began.elapsed())
                    }
                    Err(error) => {
                        let _ = child.kill().await;
                        (error, Duration::ZERO)
                    }
                }
            }
            Err(error) => (error, Duration::ZERO),
        };
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

/// The program declared `pages`: lines left from before it are dropped,
/// since it is told of every viewer afresh, and its channel is told.
/// False when the channel has stopped.
async fn started(pages: Vec<PageEntry>, arrived: &mpsc::Sender<FromApplication>, outgoing: &mut mpsc::Receiver<Vec<u8>>) -> bool {
    while outgoing.try_recv().is_ok() {}
    arrived.send(FromApplication::Declared(pages)).await.is_ok()
}

/// The pause before starting a program again that ran `ran` after
/// declaring its pages: none after a steady run, else the back-off's.
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

/// Reads the program's first line, its pages, within [`DECLARE_WITHIN`].
async fn declare<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Vec<PageEntry>, String> {
    let line = tokio::time::timeout(DECLARE_WITHIN, read_line(reader))
        .await
        .map_err(|_| format!("declared no pages within {} s", DECLARE_WITHIN.as_secs()))??
        .ok_or("ended before declaring its pages")?;
    declared_pages(&line)
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
/// The same for every transport (requirement 6.4).
async fn relay<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
    arrived: &mpsc::Sender<FromApplication>,
    outgoing: &mut mpsc::Receiver<Vec<u8>>,
) -> String {
    loop {
        tokio::select! {
            line = read_line(&mut reader) => match line {
                Ok(Some(line)) if line.is_empty() => {}
                Ok(Some(line)) => {
                    if arrived.send(FromApplication::Line(line)).await.is_err() {
                        return "the channel stopped".to_owned();
                    }
                }
                Ok(None) => return "its output closed".to_owned(),
                Err(error) => return error,
            },
            line = outgoing.recv() => match line {
                Some(line) => {
                    if let Err(error) = writer.write_all(&line).await {
                        return format!("writing to it: {error}");
                    }
                }
                None => return "the channel stopped".to_owned(),
            },
        }
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
async fn await_connections(folder: String, listener: tokio::net::UnixListener, arrived: mpsc::Sender<FromApplication>, mut outgoing: mpsc::Receiver<Vec<u8>>) {
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
        let mut reader = BufReader::new(reading);
        let pages = match declare(&mut reader).await {
            Ok(pages) => pages,
            Err(error) => {
                tracing::warn!(target: "program", channel = folder, reason = error, "the program connected and did not declare its pages");
                continue;
            }
        };
        tracing::info!(target: "program", channel = folder, pages = pages.len(), "the program connected and declared its pages");
        if !started(pages, &arrived, &mut outgoing).await {
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
