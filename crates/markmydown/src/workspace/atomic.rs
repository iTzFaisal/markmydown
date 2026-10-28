//! Atomic file writes and change fingerprints.
//!
//! Every save writes a temporary file in the destination directory, syncs it,
//! and renames it over the original. A failed write therefore leaves the
//! original byte-for-byte intact and no partial destination file. The
//! fingerprint (content hash, length, mtime) is used to tell our own writes
//! apart from external edits and avoid watcher feedback loops.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A snapshot of a file's content and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    pub modified: Option<SystemTime>,
    /// Lowercase hex SHA-256 of the bytes on disk.
    pub hash: String,
}

impl FileFingerprint {
    /// Whether another snapshot describes the same content. The modified time
    /// is intentionally excluded: cloud drives and editors may touch it
    /// without changing the bytes.
    pub fn same_content(&self, other: &Self) -> bool {
        self.len == other.len && self.hash == other.hash
    }
}

/// Hashes bytes with SHA-256.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Reads a file and returns its fingerprint.
pub fn read_fingerprint(path: &Path) -> Result<FileFingerprint, AtomicError> {
    let mut file = File::open(path).map_err(AtomicError::Io)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(AtomicError::Io)?;
    let metadata = file.metadata().map_err(AtomicError::Io)?;

    Ok(FileFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        hash: hash_bytes(&bytes),
    })
}

/// Atomically replaces `path` with `bytes`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<FileFingerprint, AtomicError> {
    atomic_write_inner(path, bytes, None)
}

/// Atomically writes UTF-8 text.
pub fn atomic_write_text(path: &Path, text: &str) -> Result<FileFingerprint, AtomicError> {
    atomic_write(path, text.as_bytes())
}

fn atomic_write_inner(
    path: &Path,
    bytes: &[u8],
    fail_after: Option<usize>,
) -> Result<FileFingerprint, AtomicError> {
    let parent = path
        .parent()
        .ok_or_else(|| AtomicError::InvalidPath(path.to_path_buf()))?;

    if !parent.is_dir() {
        return Err(AtomicError::InvalidPath(parent.to_path_buf()));
    }

    let original_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let temp = temporary_path(path);

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(AtomicError::Io)?;

        let to_write = fail_after.map_or(bytes.len(), |limit| limit.min(bytes.len()));
        file.write_all(&bytes[..to_write])
            .map_err(AtomicError::Io)?;

        if fail_after.is_some() {
            file.flush().map_err(AtomicError::Io)?;
            return Err(AtomicError::InjectedFailure);
        }

        file.sync_all().map_err(AtomicError::Io)?;

        if let Some(permissions) = &original_permissions {
            fs::set_permissions(&temp, permissions.clone()).map_err(AtomicError::Io)?;
        }

        fs::rename(&temp, path).map_err(AtomicError::Io)?;
        sync_parent(parent);

        Ok(FileFingerprint {
            len: bytes.len() as u64,
            modified: fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok()),
            hash: hash_bytes(bytes),
        })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }

    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().map_or_else(
        || String::from("document"),
        |name| name.to_string_lossy().into_owned(),
    );

    path.with_file_name(format!(
        ".{file_name}.markmydown-tmp-{}-{counter}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn sync_parent(parent: &Path) {
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) {}

/// Simulates a failure after a partial temporary write. Test-only.
#[cfg(test)]
pub fn atomic_write_failing(
    path: &Path,
    bytes: &[u8],
    fail_after: usize,
) -> Result<FileFingerprint, AtomicError> {
    atomic_write_inner(path, bytes, Some(fail_after))
}

#[derive(Debug, thiserror::Error)]
pub enum AtomicError {
    #[error("file operation failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("cannot write to invalid destination: {0}")]
    InvalidPath(PathBuf),
    #[error("simulated write failure")]
    InjectedFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content_and_returns_a_fingerprint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("doc.md");

        let fingerprint = atomic_write(&path, b"hello").expect("write");

        assert_eq!(fs::read(&path).expect("read"), b"hello");
        assert_eq!(fingerprint.len, 5);
        assert_eq!(fingerprint.hash, hash_bytes(b"hello"));
        assert_eq!(read_fingerprint(&path).expect("fingerprint"), fingerprint);
    }

    #[test]
    fn overwrites_without_leaving_temporary_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("doc.md");
        fs::write(&path, b"first").expect("seed");

        atomic_write(&path, b"second").expect("overwrite");

        assert_eq!(fs::read(&path).expect("read"), b"second");
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("markmydown-tmp")
            })
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn failed_write_keeps_the_original_and_removes_the_temp_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("doc.md");
        fs::write(&path, b"original").expect("seed");

        let error = atomic_write_failing(&path, b"replacement", 4).expect_err("fail");

        assert!(matches!(error, AtomicError::InjectedFailure));
        assert_eq!(fs::read(&path).expect("read"), b"original");
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("markmydown-tmp")
            })
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn fingerprints_detect_content_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("doc.md");
        fs::write(&path, b"one").expect("seed");
        let first = read_fingerprint(&path).expect("first");

        fs::write(&path, b"two").expect("change");
        let second = read_fingerprint(&path).expect("second");

        assert!(!first.same_content(&second));
    }

    #[cfg(unix)]
    #[test]
    fn permissions_are_preserved() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("doc.md");
        fs::write(&path, b"one").expect("seed");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");

        atomic_write(&path, b"two").expect("overwrite");

        assert_eq!(fs::metadata(&path).expect("metadata").mode() & 0o777, 0o640);
    }
}
