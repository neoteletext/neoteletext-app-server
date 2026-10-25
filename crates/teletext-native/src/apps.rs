//! Applications in other processes (`SERVER.md` section 7.1;
//! `docs/applications.md`): each channel named in the channels file runs
//! one program, in any language, which the server starts and talks to over
//! its standard input and output, one JSON object a line. A program that
//! ends, or breaks the protocol, is started again after a pause that
//! doubles each time, up to [`LONGEST_PAUSE`]; its channel pages stay as
//! last published meanwhile (section 7.5).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use teletext_runtime::PageEntry;
use teletext_runtime::json::{self, Value};
use teletext_runtime::remote::declared_pages;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

/// The longest line either side may send: a page of TTI is under 4 KB,
/// a two-minute sound's samples in base64 about 15 MB.
pub const LONGEST_LINE: usize = 16 * 1024 * 1024;
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

/// One channel of the channels file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelSpec {
    /// Its folder in the server's address space.
    pub folder: String,
    /// Its name, in `channel.json`.
    pub name: String,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// The folder it runs in.
    pub directory: PathBuf,
}

/// Reads the channels file: `{"channels":[{"folder","name","command":[…],
/// "directory"}]}`. A relative directory is taken from the file's own
/// folder.
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
        if command.is_empty() {
            return Err(format!("{}: {folder} needs a command", path.display()));
        }
        if specs.iter().any(|spec| spec.folder == folder) {
            return Err(format!("{}: {folder} twice", path.display()));
        }
        let directory = base.join(text("directory").unwrap_or_default());
        specs.push(ChannelSpec { name: text("name").unwrap_or_else(|| folder.clone()), folder, command, directory });
    }
    Ok(specs)
}

/// What the program sends the channel.
#[derive(Debug)]
pub enum FromApplication {
    /// A line, without its newline.
    Line(Vec<u8>),
    /// The program started again after it ended: its pages are to be
    /// published afresh.
    Restarted,
}

/// The two directions of a running program, as its channel's task holds
/// them.
#[derive(Debug)]
pub struct Link {
    /// What the program sends.
    pub from: mpsc::Receiver<FromApplication>,
    /// What is sent to the program.
    pub to: mpsc::Sender<Vec<u8>>,
}

/// Starts `spec`'s program and waits for it to declare its pages; it is
/// kept running, and started again whenever it ends, until the server
/// stops.
///
/// # Errors
/// A program that cannot be started or does not declare its pages.
pub async fn start(spec: ChannelSpec) -> Result<(Vec<PageEntry>, Link), String> {
    let (to, mut outgoing) = mpsc::channel::<Vec<u8>>(WAITING_LINES);
    let (arrived, from) = mpsc::channel::<FromApplication>(ARRIVING_LINES);
    let (declared, declaration) = oneshot::channel::<Result<Vec<PageEntry>, String>>();
    let folder = spec.folder.clone();
    tokio::spawn(async move {
        let mut declared = Some(declared);
        let mut pause = FIRST_PAUSE;
        loop {
            let ended = match launch(&spec) {
                Ok((mut child, stdin, mut stdout)) => match declare(&spec, &mut stdout).await {
                    Ok(pages) => {
                        match declared.take() {
                            Some(declared) => {
                                let _ = declared.send(Ok(pages));
                            }
                            None => {
                                if arrived.send(FromApplication::Restarted).await.is_err() {
                                    return;
                                }
                            }
                        }
                        pause = FIRST_PAUSE;
                        let reason = relay(stdin, stdout, &arrived, &mut outgoing).await;
                        let _ = child.kill().await;
                        reason
                    }
                    Err(error) => {
                        let _ = child.kill().await;
                        error
                    }
                },
                Err(error) => error,
            };
            if let Some(declared) = declared.take() {
                let _ = declared.send(Err(ended));
                return;
            }
            eprintln!("teletext-server: {}: the application ended ({ended}); starting it again in {} s", spec.folder, pause.as_secs());
            tokio::time::sleep(pause).await;
            pause = pause.saturating_mul(2).min(LONGEST_PAUSE);
            if arrived.is_closed() {
                return;
            }
        }
    });
    let pages = declaration.await.map_err(|_| format!("{folder}: the application's supervisor ended"))?.map_err(|error| format!("{folder}: {error}"))?;
    Ok((pages, Link { from, to }))
}

fn launch(spec: &ChannelSpec) -> Result<(Child, ChildStdin, BufReader<ChildStdout>), String> {
    let (program, arguments) = spec.command.split_first().ok_or("no command")?;
    let mut child = Command::new(program)
        .args(arguments)
        .current_dir(&spec.directory)
        .env("NEOTELETEXT_CHANNEL", &spec.folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("cannot start {program} in {}: {error}", spec.directory.display()))?;
    let stdin = child.stdin.take().ok_or("no standard input")?;
    let stdout = child.stdout.take().ok_or("no standard output")?;
    Ok((child, stdin, BufReader::new(stdout)))
}

/// Reads the program's first line, its pages.
async fn declare(spec: &ChannelSpec, stdout: &mut BufReader<ChildStdout>) -> Result<Vec<PageEntry>, String> {
    let line = tokio::time::timeout(DECLARE_WITHIN, read_line(stdout))
        .await
        .map_err(|_| format!("declared no pages within {} s", DECLARE_WITHIN.as_secs()))??
        .ok_or("ended before declaring its pages")?;
    declared_pages(&line).map_err(|error| format!("{}: {error}", spec.folder))
}

/// One line of at most [`LONGEST_LINE`] bytes, without its newline; `None`
/// at the end of the output.
async fn read_line(stdout: &mut BufReader<ChildStdout>) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    let limit = u64::try_from(LONGEST_LINE).unwrap_or(u64::MAX).saturating_add(1);
    let read = (&mut *stdout).take(limit).read_until(b'\n', &mut line).await.map_err(|error| format!("reading its output: {error}"))?;
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

/// Carries lines both ways until the program ends; answers why it did.
async fn relay(
    mut stdin: ChildStdin,
    mut stdout: BufReader<ChildStdout>,
    arrived: &mpsc::Sender<FromApplication>,
    outgoing: &mut mpsc::Receiver<Vec<u8>>,
) -> String {
    loop {
        tokio::select! {
            line = read_line(&mut stdout) => match line {
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
                    if let Err(error) = stdin.write_all(&line).await {
                        return format!("writing to it: {error}");
                    }
                }
                None => return "the channel stopped".to_owned(),
            },
        }
    }
}

#[cfg(test)]
mod tests;
