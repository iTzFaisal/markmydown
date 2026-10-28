//! Recovery buffers for unsaved work after an unexpected exit.
//!
//! Dirty buffers are written every few seconds to the app data directory.
//! Each record is keyed by the hash of its document path, so multiple tabs
//! cannot overwrite one another. A clean save or explicit close removes the
//! record.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::atomic;
use super::document::{Cursor, Document};

/// One unsaved buffer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub schema_version: u32,
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    pub saved_at: u64,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub scroll: f32,
}

impl RecoveryRecord {
    pub const CURRENT_SCHEMA: u32 = 1;

    pub fn from_document(document: &Document) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA,
            path: document.path.clone(),
            title: document.display_title().to_string(),
            content: document.text.clone(),
            saved_at: now(),
            cursor_line: document.cursor.line,
            cursor_column: document.cursor.column,
            scroll: document.scroll,
        }
    }

    /// Converts a recovery record back into a dirty document.
    pub fn into_document(self, id: u64) -> Document {
        Document::from_recovery(
            id,
            self.path,
            self.content,
            Cursor::new(self.cursor_line, self.cursor_column),
            self.scroll,
        )
    }
}

/// Recovery storage rooted at a directory.
#[derive(Debug, Clone)]
pub struct RecoveryStore {
    dir: PathBuf,
}

impl RecoveryStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Writes a dirty document and returns the record path. Clean documents
    /// have their old record removed.
    pub fn write(&self, document: &Document) -> Result<Option<PathBuf>, RecoveryError> {
        if !document.is_dirty() {
            self.clear(&document.path)?;
            return Ok(None);
        }

        std::fs::create_dir_all(&self.dir).map_err(RecoveryError::Io)?;
        let record = RecoveryRecord::from_document(document);
        let path = self.record_path(&document.path);
        let json = serde_json::to_string_pretty(&record).map_err(RecoveryError::Serialize)?;
        atomic::atomic_write(&path, json.as_bytes()).map_err(RecoveryError::Write)?;
        Ok(Some(path))
    }

    /// Writes every dirty document and clears records for clean ones.
    pub fn write_all<'a>(
        &self,
        documents: impl IntoIterator<Item = &'a Document>,
    ) -> Result<usize, RecoveryError> {
        let mut written = 0;
        for document in documents {
            if self.write(document)?.is_some() {
                written += 1;
            }
        }
        Ok(written)
    }

    /// Lists recoverable buffers, newest first. Corrupt records are skipped.
    pub fn list(&self) -> Vec<RecoveryRecord> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };

        let mut records: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            })
            .filter_map(|entry| {
                let bytes = std::fs::read(entry.path()).ok()?;
                let record = serde_json::from_slice::<RecoveryRecord>(&bytes).ok()?;
                (record.schema_version <= RecoveryRecord::CURRENT_SCHEMA).then_some(record)
            })
            .collect();

        records.sort_by_key(|record| std::cmp::Reverse(record.saved_at));
        records
    }

    /// Removes the record for one document.
    pub fn clear(&self, document_path: &Path) -> Result<(), RecoveryError> {
        let path = self.record_path(document_path);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RecoveryError::Io(error)),
        }
    }

    /// Removes every recovery record.
    pub fn clear_all(&self) -> Result<(), RecoveryError> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Ok(());
        };

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                std::fs::remove_file(path).map_err(RecoveryError::Io)?;
            }
        }

        Ok(())
    }

    fn record_path(&self, document_path: &Path) -> PathBuf {
        self.dir.join(format!(
            "{}.json",
            atomic::hash_bytes(document_path.as_os_str().as_encoded_bytes())
        ))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("recovery I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("recovery serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("recovery write failed: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dirty_document(tmp: &Path) -> Document {
        let path = tmp.join("draft.md");
        fs::write(&path, "# Saved\n").expect("fixture");
        let mut document = Document::open(1, &path).expect("open");
        document.set_text("# Unsaved edit\n\nWith more text.\n");
        document.cursor = Cursor::new(2, 4);
        document.scroll = 42.0;
        document
    }

    #[test]
    fn writes_and_lists_a_recovery_buffer() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        let document = dirty_document(tmp.path());

        let path = store.write(&document).expect("write").expect("dirty");

        assert!(path.exists());
        let records = store.list();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, document.path);
        assert_eq!(records[0].content, document.text);
        assert_eq!((records[0].cursor_line, records[0].cursor_column), (2, 4));
        assert_eq!(records[0].scroll, 42.0);
    }

    #[test]
    fn clean_documents_clear_their_record() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        let mut document = dirty_document(tmp.path());
        store.write(&document).expect("write dirty");

        document.save().expect("save");
        let written = store.write(&document).expect("write clean");

        assert!(written.is_none());
        assert!(store.list().is_empty());
    }

    #[test]
    fn rejects_corrupt_records_but_keeps_good_ones() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        let document = dirty_document(tmp.path());
        store.write(&document).expect("write");

        fs::create_dir_all(&store.dir).expect("dir");
        fs::write(store.dir.join("corrupt.json"), b"{ nope").expect("corrupt");

        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn accepted_recovery_opens_as_a_dirty_document() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        let document = dirty_document(tmp.path());
        store.write(&document).expect("write");

        let record = store.list().remove(0);
        let recovered = record.into_document(9);

        assert_eq!(recovered.id, 9);
        assert_eq!(recovered.text, document.text);
        assert!(recovered.is_dirty());
        assert_eq!(recovered.cursor, Cursor::new(2, 4));
    }

    #[test]
    fn declined_recovery_is_cleared() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = RecoveryStore::new(tmp.path().join("recovery"));
        let document = dirty_document(tmp.path());
        store.write(&document).expect("write");

        store.clear_all().expect("clear");

        assert!(store.list().is_empty());
    }
}
