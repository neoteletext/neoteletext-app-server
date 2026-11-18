//! What the server keeps through a restart, in its state directory
//! (requirement 8.3, DEC-6471): the viewer-handle secret (VS-305) and the
//! accumulated figures. No channel's content is kept (REQ-1474). Every file is written to a temporary file
//! and renamed into place, so a crash leaves the old file or the new one,
//! never half of either.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ring::rand::{SecureRandom, SystemRandom};
use teletext_runtime::json;
use teletext_runtime::viewer::{HANDLE_SECRET_BYTES, HandleSecret, RandomSource, RandomUnavailable};

/// The viewer-handle secret (VS-305).
pub const HANDLE_SECRET_FILE: &str = "viewer-handle.key";
/// Left by `rotate-viewer-secret`, and removed at the next start once every
/// channel's program has been told `viewers_reset` (VS-306).
pub const ROTATED_FILE: &str = "viewer-handle.rotated";
/// The session cookie's keys before viewer state replaced them, deleted
/// at start and never read (VS-603).
const RETIRED_FILES: [&str; 2] = ["session.key", "session.key.previous"];
/// The accumulated figures (requirement 8.3).
pub const FIGURES_FILE: &str = "figures.json";
/// Where an unreadable figures file is kept aside.
const UNREADABLE_FIGURES: &str = "figures.json.unreadable";
/// Written first, then renamed into place.
const TEMPORARY_SUFFIX: &str = ".new";
/// The largest figures file read: a channel's figures are a few hundred
/// bytes and each page's opens a few more; 16 MiB is far beyond any.
const MOST_FIGURES_BYTES: u64 = 16 * 1024 * 1024;
/// The key files are their owner's alone.
#[cfg(unix)]
const OWNER_ONLY_FILE: u32 = 0o600;
/// So are the folders that hold them.
#[cfg(unix)]
const OWNER_ONLY_FOLDER: u32 = 0o700;

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

    /// The viewer-handle secret (VS-305), made once and kept for the
    /// owner alone; and whether it was rotated since the last start, so
    /// that every program is told `viewers_reset` (VS-306). The session
    /// cookie's old keys are deleted (VS-603).
    ///
    /// # Errors
    /// A secret that cannot be read, made or kept.
    pub fn handle_secret(&self) -> Result<(HandleSecret, bool), String> {
        for retired in RETIRED_FILES {
            match fs::remove_file(self.root.join(retired)) {
                Ok(()) => tracing::info!(target: "state", file = retired, "the session cookie's key was deleted: viewer state replaced it"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cannot delete {retired}: {error}")),
            }
        }
        let path = self.root.join(HANDLE_SECRET_FILE);
        let secret = if let Some(secret) = read_secret(&path)? {
            secret
        } else {
            let secret = random_secret()?;
            write_whole(&path, secret.bytes(), true)?;
            secret
        };
        Ok((secret, self.root.join(ROTATED_FILE).exists()))
    }

    /// Makes a new viewer-handle secret in place of the old (VS-306), and
    /// marks it rotated, for `teletext-server rotate-viewer-secret`.
    ///
    /// # Errors
    /// A secret that cannot be made or kept.
    pub fn rotate_handle_secret(&self) -> Result<(), String> {
        write_whole(&self.root.join(ROTATED_FILE), b"", false)?;
        write_whole(&self.root.join(HANDLE_SECRET_FILE), random_secret()?.bytes(), true)
    }

    /// Every program has been told of a rotation (VS-306).
    ///
    /// # Errors
    /// A mark that cannot be removed.
    pub fn rotation_told(&self) -> Result<(), String> {
        match fs::remove_file(self.root.join(ROTATED_FILE)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("cannot remove {ROTATED_FILE}: {error}")),
        }
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

/// A secret file's secret, or none when there is no file.
fn read_secret(path: &Path) -> Result<Option<HandleSecret>, String> {
    match read_bounded(path, u64::try_from(HANDLE_SECRET_BYTES).unwrap_or(u64::MAX)) {
        Ok(bytes) => <[u8; HANDLE_SECRET_BYTES]>::try_from(bytes.as_slice())
            .map(|bytes| Some(HandleSecret::new(bytes)))
            .map_err(|_| format!("{} is not a secret of {HANDLE_SECRET_BYTES} bytes", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// A viewer-handle secret of the system's random bytes (VS-305).
///
/// # Errors
/// No source of random bytes on this system.
pub fn random_secret() -> Result<HandleSecret, String> {
    let mut bytes = [0_u8; HANDLE_SECRET_BYTES];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no source of random bytes on this system".to_owned())?;
    Ok(HandleSecret::new(bytes))
}

/// The operating system's cryptographic random source, as `ring` reads it
/// (DEC-6471), for the runtime's challenges (VS-205).
#[derive(Debug)]
pub struct SystemRandomSource(SystemRandom);

impl SystemRandomSource {
    /// The system's source.
    #[must_use]
    pub fn new() -> Self {
        Self(SystemRandom::new())
    }
}

impl Default for SystemRandomSource {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomSource for SystemRandomSource {
    fn fill(&mut self, bytes: &mut [u8]) -> Result<(), RandomUnavailable> {
        self.0.fill(bytes).map_err(|_| RandomUnavailable)
    }
}

#[cfg(test)]
mod tests;
