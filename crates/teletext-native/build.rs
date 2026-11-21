//! Gives every compile of the server a version of its own (REQ-7050): the
//! workspace's major and minor numbers, then a third number one higher
//! than the previous compile's in this checkout.
//!
//! The count is kept in `build-number`, at the root of the checkout, which
//! Git ignores: no tracked file changes when the server is built, and each
//! machine and each checkout counts its own compiles. The version reaches
//! the crate's code as the compile-time variable named by `VERSION_VARIABLE`.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The count's file, at the root of the checkout; `.gitignore` names it.
const COUNT_FILE: &str = "build-number";
/// From this crate's folder, `crates/teletext-native`, to the checkout's root.
const ROOT_FROM_CRATE: &str = "../..";
/// The compile-time variable the crate reads its version from.
const VERSION_VARIABLE: &str = "TELETEXT_SERVER_VERSION";
/// The most the count's file is read: the 20 digits of `u64::MAX`, and as
/// many bytes again for a line ending and spaces a person's editor left.
const MAX_COUNT_FILE_BYTES: u64 = 40;
/// A file Cargo is told to watch and that is never made: Cargo runs a
/// build script again when a file it watches is missing, so the script
/// runs on every build (the Cargo Book, "Build Scripts",
/// `rerun-if-changed`).
const NEVER_MADE: &str = "run-on-every-build";

fn main() -> ExitCode {
    match version() {
        Ok(version) => {
            println!("cargo::rustc-env={VERSION_VARIABLE}={version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("cargo::error={error}");
            ExitCode::FAILURE
        }
    }
}

/// This compile's version, after counting it.
fn version() -> Result<String, String> {
    let major = variable("CARGO_PKG_VERSION_MAJOR")?;
    let minor = variable("CARGO_PKG_VERSION_MINOR")?;
    let never_made = Path::new(&variable("OUT_DIR")?).join(NEVER_MADE);
    println!("cargo::rerun-if-changed={}", never_made.display());
    let file = PathBuf::from(variable("CARGO_MANIFEST_DIR")?).join(ROOT_FROM_CRATE).join(COUNT_FILE);
    let count = counted(&file).map_err(|error| format!("the build number in {}: {error}", file.display()))?;
    Ok(format!("{major}.{minor}.{count}"))
}

/// A variable Cargo sets for every build script.
fn variable(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|error| format!("Cargo did not set {name}: {error}"))
}

/// Raises the count in `path` by one and returns the new count. A file
/// that is missing or empty counts from zero, so the first compile is 1.
///
/// The file is locked from the read to the write: two builds at once, a
/// deploy's beside a development build, each take a number of their own.
fn counted(path: &Path) -> Result<u64, String> {
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(|error| format!("cannot open it: {error}"))?;
    file.lock().map_err(|error| format!("cannot lock it: {error}"))?;
    let mut text = String::new();
    Read::by_ref(&mut file).take(MAX_COUNT_FILE_BYTES.saturating_add(1)).read_to_string(&mut text).map_err(|error| format!("cannot read it: {error}"))?;
    let previous = match text.trim() {
        "" => 0,
        digits if u64::try_from(text.len()).is_ok_and(|length| length <= MAX_COUNT_FILE_BYTES) => {
            digits.parse::<u64>().map_err(|error| format!("{digits:?} is not a count ({error}); correct the file, or delete it to count again from 1"))?
        }
        _ => return Err(format!("it holds more than {MAX_COUNT_FILE_BYTES} bytes; correct the file, or delete it to count again from 1")),
    };
    let count = previous.checked_add(1).ok_or("the count has reached the largest number it can hold")?;
    write(&mut file, count).map_err(|error| format!("cannot write it: {error}"))?;
    Ok(count)
}

/// Writes the count over the old one, then drops what the old one left
/// beyond it: the file is never empty between the two, so a build stopped
/// there leaves a count or a file the next build refuses, never a count
/// that starts again.
fn write(file: &mut File, count: u64) -> std::io::Result<()> {
    let line = format!("{count}\n");
    file.seek(SeekFrom::Start(0))?;
    file.write_all(line.as_bytes())?;
    file.set_len(u64::try_from(line.len()).map_err(std::io::Error::other)?)?;
    file.sync_all()
}
