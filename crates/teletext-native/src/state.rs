//! What the server keeps through a restart, in its state directory
//! (requirements 5.2, 5.5 and 8.3): each channel's records under
//! `channels/<folder>/`, named by their keys; the session cookie's keys;
//! and the accumulated figures. Every file is written to a temporary file
//! and renamed into place, so a crash leaves the old file or the new one,
//! never half of either.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use teletext_runtime::session_cookie::{KEY_BYTES, Key};
use teletext_runtime::store::MOST_RECORD_BYTES;
use teletext_runtime::{StoreKey, json};

/// The channels' records, a folder each.
const CHANNELS: &str = "channels";
/// The session cookie's key, and the one before it (requirement 5.5).
const KEY_FILE: &str = "session.key";
const PREVIOUS_KEY_FILE: &str = "session.key.previous";
/// The accumulated figures (requirement 8.3).
pub const FIGURES_FILE: &str = "figures.json";
/// Where an unreadable figures file is kept aside.
const UNREADABLE_FIGURES: &str = "figures.json.unreadable";
/// Written first, then renamed into place.
const TEMPORARY_SUFFIX: &str = ".new";
/// The largest figures file read: a channel's figures are a few hundred
/// bytes and each page's opens a few more; 16 MiB is far beyond any.
const MOST_FIGURES_BYTES: u64 = 16 * 1024 * 1024;
/// Where the system's random bytes are read, on Linux and macOS.
#[cfg(unix)]
const RANDOM_SOURCE: &str = "/dev/urandom";
/// The key files are their owner's alone.
#[cfg(unix)]
const OWNER_ONLY_FILE: u32 = 0o600;
/// So are the folders that hold them.
#[cfg(unix)]
const OWNER_ONLY_FOLDER: u32 = 0o700;

/// A change to a channel's records: bytes to keep, or none to forget.
pub type Changes = BTreeMap<StoreKey, Option<Vec<u8>>>;

/// The state directory.
#[derive(Clone, Debug)]
pub struct StateDirectory {
    root: PathBuf,
}

impl StateDirectory {
    /// The directory at `root`, made, its owner's alone, if it does not exist.
    ///
    /// # Errors
    /// A directory that cannot be made.
    pub fn open(root: &Path) -> Result<Self, String> {
        make_folder(root).map_err(|error| format!("cannot make the state directory {}: {error}", root.display()))?;
        Ok(Self { root: root.to_owned() })
    }

    /// Where it is.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn channel(&self, folder: &str) -> PathBuf {
        self.root.join(CHANNELS).join(folder)
    }

    /// Every record kept for the channel `folder`, by key; a file whose
    /// name is no key, or which is over the largest record, is named in
    /// the second list, for the log, and left alone.
    #[must_use]
    pub fn records(&self, folder: &str) -> (Vec<(StoreKey, Vec<u8>)>, Vec<String>) {
        let mut records = Vec::new();
        let mut ignored = Vec::new();
        let Ok(entries) = fs::read_dir(self.channel(folder)) else { return (records, ignored) };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(key) = StoreKey::parse(&name) else {
                ignored.push(name);
                continue;
            };
            match read_bounded(&entry.path(), u64::try_from(MOST_RECORD_BYTES).unwrap_or(u64::MAX)) {
                Ok(bytes) => records.push((key, bytes)),
                Err(error) => ignored.push(format!("{name}: {error}")),
            }
        }
        records.sort_by_key(|(key, _)| *key);
        (records, ignored)
    }

    /// Keeps and forgets the channel `folder`'s records as `changes` say.
    ///
    /// # Errors
    /// The first file that could not be written or removed.
    pub fn apply(&self, folder: &str, changes: &Changes) -> Result<(), String> {
        let folder = self.channel(folder);
        make_folder(&folder).map_err(|error| format!("cannot make {}: {error}", folder.display()))?;
        for (key, change) in changes {
            let path = folder.join(key.name());
            match change {
                Some(bytes) => write_whole(&path, bytes, false)?,
                None => match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("cannot remove {}: {error}", path.display())),
                },
            }
        }
        Ok(())
    }

    /// The session cookie's keys: the current, made and kept if there is
    /// none, and the previous, if one is kept.
    ///
    /// # Errors
    /// A key that cannot be read, made or kept.
    pub fn session_keys(&self) -> Result<(Key, Option<Key>), String> {
        let current = self.root.join(KEY_FILE);
        let key = if let Some(key) = read_key(&current)? {
            key
        } else {
            let key = random_key()?;
            write_whole(&current, &key, true)?;
            key
        };
        Ok((key, read_key(&self.root.join(PREVIOUS_KEY_FILE))?))
    }

    /// The accumulated figures, or none when there are none yet. A file
    /// that does not read is kept aside and named in the error, and the
    /// figures start again (requirement 8.3).
    ///
    /// # Errors
    /// What was wrong with the file, which is now kept aside.
    pub fn figures(&self) -> Result<Option<json::Value>, String> {
        let path = self.root.join(FIGURES_FILE);
        let bytes = match read_bounded(&path, MOST_FIGURES_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.keep_aside(&path, &error.to_string())),
        };
        json::parse(&bytes).map(Some).map_err(|fault| self.keep_aside(&path, &format!("not JSON: {fault:?}")))
    }

    fn keep_aside(&self, path: &Path, why: &str) -> String {
        let aside = self.root.join(UNREADABLE_FIGURES);
        match fs::rename(path, &aside) {
            Ok(()) => format!("{} did not read ({why}); kept as {}", path.display(), aside.display()),
            Err(error) => format!("{} did not read ({why}) and could not be kept aside: {error}", path.display()),
        }
    }

    /// Keeps the accumulated figures.
    ///
    /// # Errors
    /// A file that cannot be written.
    pub fn keep_figures(&self, bytes: &[u8]) -> Result<(), String> {
        write_whole(&self.root.join(FIGURES_FILE), bytes, false)
    }
}

/// Reads a file of at most `most` bytes; a longer one is an error.
fn read_bounded(path: &Path, most: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(most.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > most {
        return Err(std::io::Error::other(format!("over {most} bytes")));
    }
    Ok(bytes)
}

/// Writes `bytes` to a temporary file beside `path`, flushed to the disk,
/// then renames it into place; `secret` makes it its owner's alone.
fn write_whole(path: &Path, bytes: &[u8], secret: bool) -> Result<(), String> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(TEMPORARY_SUFFIX);
    let temporary = PathBuf::from(temporary);
    let written = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if secret {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(OWNER_ONLY_FILE);
        }
        #[cfg(not(unix))]
        let _ = secret;
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    written.map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Makes `folder` and those above it, the new ones their owner's alone.
fn make_folder(folder: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(OWNER_ONLY_FOLDER);
    }
    builder.create(folder)
}

/// A key file's key, or none when there is no file.
fn read_key(path: &Path) -> Result<Option<Key>, String> {
    match read_bounded(path, u64::try_from(KEY_BYTES).unwrap_or(u64::MAX)) {
        Ok(bytes) => Key::try_from(bytes.as_slice()).map(Some).map_err(|_| format!("{} is not a key of {KEY_BYTES} bytes", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// A key of the system's random bytes.
///
/// # Errors
/// No source of random bytes on this system.
pub fn random_key() -> Result<Key, String> {
    #[cfg(unix)]
    {
        let mut key = [0_u8; KEY_BYTES];
        fs::File::open(RANDOM_SOURCE).and_then(|mut source| source.read_exact(&mut key)).map_err(|error| format!("cannot read {RANDOM_SOURCE}: {error}"))?;
        Ok(key)
    }
    #[cfg(not(unix))]
    {
        // No random source in the standard library elsewhere; `getrandom`
        // awaits the owner's approval (decision 5).
        Err("no source of random bytes on this system".to_owned())
    }
}

#[cfg(test)]
mod tests;
