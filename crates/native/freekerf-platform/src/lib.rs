//! Operating system services: where FreeKerf keeps its files, and writing them
//! safely (a crash never leaves a half-written settings file).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Errors of the platform layer.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// No home/config directory could be determined.
    #[error("cannot determine the configuration directory")]
    NoConfigDir,
    /// I/O error on a path.
    #[error("{path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Cause.
        source: std::io::Error,
    },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> PlatformError + '_ {
    move |source| PlatformError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Directories of the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// Settings (`settings.toml`).
    pub config: PathBuf,
    /// Data kept between runs (laser life counters, machine cache).
    pub data: PathBuf,
}

impl Dirs {
    /// The per-user directories (`$FREEKERF_HOME` overrides both, for tests and
    /// portable installs).
    ///
    /// # Errors
    /// When no home directory is known.
    pub fn user() -> Result<Self, PlatformError> {
        if let Some(home) = std::env::var_os("FREEKERF_HOME") {
            let home = PathBuf::from(home);
            return Ok(Self {
                config: home.clone(),
                data: home,
            });
        }
        let p = directories::ProjectDirs::from("org", "FreeKerf", "freekerf")
            .ok_or(PlatformError::NoConfigDir)?;
        Ok(Self {
            config: p.config_dir().to_path_buf(),
            data: p.data_dir().to_path_buf(),
        })
    }

    /// Both directories under `root` (tests, portable installs).
    #[must_use]
    pub fn under(root: &Path) -> Self {
        Self {
            config: root.to_path_buf(),
            data: root.to_path_buf(),
        }
    }

    /// `settings.toml`.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    /// File in the data directory.
    #[must_use]
    pub fn data_file(&self, name: &str) -> PathBuf {
        self.data.join(name)
    }
}

/// Writes `contents` to `path` atomically: a temporary file in the same directory
/// is written, flushed and renamed over the target.
///
/// # Errors
/// On any I/O error (the target is then unchanged).
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), PlatformError> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(io(dir))?;
    let name = path
        .file_name()
        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(".{name}.tmp{}", std::process::id()));
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io(path))
}

/// Reads a file, `None` when it does not exist.
///
/// # Errors
/// On I/O errors other than "not found".
pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, PlatformError> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(path)(e)),
    }
}

/// Reads a whole file.
///
/// # Errors
/// On I/O errors.
pub fn read(path: &Path) -> Result<Vec<u8>, PlatformError> {
    fs::read(path).map_err(io(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/settings.toml");
        assert_eq!(read_optional(&p).unwrap(), None);
        write_atomic(&p, b"a = 1\n").unwrap();
        write_atomic(&p, b"a = 2\n").unwrap();
        assert_eq!(read(&p).unwrap(), b"a = 2\n");
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
        assert!(read(&dir.path().join("missing")).is_err());
        // a directory in the way: the write fails and leaves nothing behind
        let blocked = dir.path().join("blocked");
        fs::create_dir_all(blocked.join("x")).unwrap();
        assert!(write_atomic(&blocked, b"x").is_err());
        assert!(read_optional(dir.path()).is_err());
    }

    #[test]
    fn directories() {
        let d = Dirs::under(Path::new("/tmp/fk"));
        assert_eq!(d.settings_file(), PathBuf::from("/tmp/fk/settings.toml"));
        assert_eq!(d.data_file("x"), PathBuf::from("/tmp/fk/x"));
        assert!(Dirs::user().is_ok());
        let e = PlatformError::NoConfigDir;
        assert!(e.to_string().contains("configuration"));
    }
}
