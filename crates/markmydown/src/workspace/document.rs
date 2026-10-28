//! A single open document and its file-safety metadata.
//!
//! `Document` is the domain object; the editor widget owns the live text
//! buffer while a tab is focused. The document records the normalized text,
//! the last on-disk fingerprint, line endings, front matter, cursor, scroll,
//! and any external-change conflict.

use std::path::{Path, PathBuf};

use super::atomic::{self, FileFingerprint};
use super::encoding::{self, DecodeError, LineEnding};
use super::frontmatter::FrontMatter;
use crate::editor::outline::{self, Heading};
use crate::editor::stats::{self, TextStats};

/// A zero-based cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

impl Cursor {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// One-based position for the status bar.
    pub fn display(&self) -> (usize, usize) {
        (self.line + 1, self.column + 1)
    }
}

/// An external edit that conflicts with local unsaved changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Fingerprint of the version currently on disk.
    pub disk: FileFingerprint,
}

/// What happened when the watcher reported a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChange {
    None,
    Reloaded,
    Conflict,
    Missing,
}

/// A document open in a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    /// Normalized text (`\n` line endings).
    pub text: String,
    saved_text_hash: String,
    disk: FileFingerprint,
    line_ending: LineEnding,
    had_bom: bool,
    dirty: bool,
    missing: bool,
    pub cursor: Cursor,
    pub scroll: f32,
    front_matter: FrontMatter,
    pub conflict: Option<Conflict>,
    pub save_error: Option<String>,
    /// Derived preview model, refreshed after the preview debounce.
    pub preview: Option<crate::preview::model::PreviewDocument>,
    /// True when the text changed since the preview was built.
    pub preview_stale: bool,
}

impl Document {
    /// Opens a supported file and records the metadata needed for safe saves.
    pub fn open(id: u64, path: impl Into<PathBuf>) -> Result<Self, DocumentError> {
        let path = path.into();

        if !super::tree::is_supported(&path) {
            return Err(DocumentError::Unsupported(path));
        }

        let bytes = std::fs::read(&path).map_err(DocumentError::Io)?;
        let metadata = std::fs::metadata(&path).map_err(DocumentError::Io)?;

        if !metadata.is_file() {
            return Err(DocumentError::NotAFile(path));
        }

        let decoded = encoding::decode(&bytes)?;
        let front_matter = FrontMatter::parse(&decoded.text);
        let title = file_title(&path);

        Ok(Self {
            id,
            path,
            title,
            saved_text_hash: atomic::hash_bytes(decoded.text.as_bytes()),
            text: decoded.text,
            disk: FileFingerprint {
                len: metadata.len(),
                modified: metadata.modified().ok(),
                hash: atomic::hash_bytes(&bytes),
            },
            line_ending: decoded.line_ending,
            had_bom: decoded.had_bom,
            dirty: false,
            missing: false,
            cursor: Cursor::default(),
            scroll: 0.0,
            front_matter,
            conflict: None,
            save_error: None,
            preview: None,
            preview_stale: true,
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Rebuilds a document from a recovery buffer. The tab is always dirty:
    /// recovery content has not been written to the file yet.
    pub fn from_recovery(
        id: u64,
        path: impl Into<PathBuf>,
        content: impl Into<String>,
        cursor: Cursor,
        scroll: f32,
    ) -> Self {
        let path = path.into();
        let text = LineEnding::normalize(&content.into());
        let title = file_title(&path);
        let front_matter = FrontMatter::parse(&text);
        let missing = !path.exists();

        Self {
            id,
            path,
            title,
            saved_text_hash: String::new(),
            text,
            disk: FileFingerprint {
                len: 0,
                modified: None,
                hash: String::new(),
            },
            line_ending: LineEnding::Lf,
            had_bom: false,
            dirty: true,
            missing,
            cursor,
            scroll,
            front_matter,
            conflict: None,
            save_error: None,
            preview: None,
            preview_stale: true,
        }
    }

    pub fn is_missing(&self) -> bool {
        self.missing
    }

    /// Updates the document's path after a rename or move.
    pub fn relocate(&mut self, path: impl Into<PathBuf>) {
        self.path = path.into();
        self.title = file_title(&self.path);
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn front_matter(&self) -> &FrontMatter {
        &self.front_matter
    }

    pub fn disk_fingerprint(&self) -> &FileFingerprint {
        &self.disk
    }

    /// Title from front matter when present, otherwise the file stem.
    pub fn display_title(&self) -> &str {
        self.front_matter.title().unwrap_or(&self.title)
    }

    /// Replaces the text buffer after an edit and refreshes dirty state.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = LineEnding::normalize(&text.into());
        if text == self.text {
            return;
        }

        self.text = text;
        self.front_matter = FrontMatter::parse(&self.text);
        self.dirty = atomic::hash_bytes(self.text.as_bytes()) != self.saved_text_hash;
        self.preview_stale = true;
        self.save_error = None;
    }

    /// Rebuilds the derived preview model from the current text.
    pub fn refresh_preview(&mut self) {
        let offset = if self.front_matter.is_valid() {
            crate::preview::parser::line_offset_for(&self.text, self.front_matter.body_offset())
        } else {
            0
        };
        self.preview = Some(crate::preview::parser::parse(
            self.front_matter.body(),
            offset,
        ));
        self.preview_stale = false;
    }

    /// Recomputes the dirty flag from the current text.
    pub fn refresh_dirty(&mut self) {
        self.dirty = atomic::hash_bytes(self.text.as_bytes()) != self.saved_text_hash;
    }

    /// Writes the document immediately and records the new fingerprint.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        self.save_inner(None)
    }

    /// Saves to a new path, updating the document's identity.
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), DocumentError> {
        let path = path.into();
        if !super::tree::is_supported(&path) {
            return Err(DocumentError::Unsupported(path));
        }
        self.save_inner(Some(path))
    }

    fn save_inner(&mut self, new_path: Option<PathBuf>) -> Result<(), DocumentError> {
        let path = new_path.as_deref().unwrap_or(&self.path);

        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.permissions().readonly()
        {
            let error = DocumentError::ReadOnly(path.to_path_buf());
            self.save_error = Some(error.to_string());
            return Err(error);
        }

        let decoded = encoding::DecodedText {
            text: self.text.clone(),
            line_ending: self.line_ending,
            had_bom: self.had_bom,
        };
        let bytes = decoded.encode(&self.text);

        let fingerprint = atomic::atomic_write(path, &bytes).map_err(|error| {
            let error = DocumentError::Write(error);
            self.save_error = Some(error.to_string());
            error
        })?;

        if let Some(new_path) = new_path {
            self.path = new_path;
            self.title = file_title(&self.path);
        }

        self.disk = fingerprint;
        self.saved_text_hash = atomic::hash_bytes(self.text.as_bytes());
        self.dirty = false;
        self.missing = false;
        self.save_error = None;
        Ok(())
    }

    /// Re-reads the document from disk, discarding local edits.
    pub fn reload(&mut self) -> Result<(), DocumentError> {
        let bytes = std::fs::read(&self.path).map_err(DocumentError::Io)?;
        let metadata = std::fs::metadata(&self.path).map_err(DocumentError::Io)?;
        let decoded = encoding::decode(&bytes)?;

        self.text = decoded.text;
        self.front_matter = FrontMatter::parse(&self.text);
        self.line_ending = decoded.line_ending;
        self.had_bom = decoded.had_bom;
        self.saved_text_hash = atomic::hash_bytes(self.text.as_bytes());
        self.disk = FileFingerprint {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            hash: atomic::hash_bytes(&bytes),
        };
        self.dirty = false;
        self.missing = false;
        self.conflict = None;
        self.save_error = None;
        self.preview_stale = true;
        Ok(())
    }

    /// Reads the on-disk text for a compare view without changing the document.
    pub fn disk_text(&self) -> Result<String, DocumentError> {
        let bytes = std::fs::read(&self.path).map_err(DocumentError::Io)?;
        Ok(encoding::decode(&bytes)?.text)
    }

    /// Inspects the current state of the file after a watcher event.
    pub fn check_external_change(&mut self) -> ExternalChange {
        if !self.path.exists() {
            self.missing = true;
            self.conflict = None;
            return ExternalChange::Missing;
        }

        let Ok(fingerprint) = atomic::read_fingerprint(&self.path) else {
            self.missing = true;
            return ExternalChange::Missing;
        };

        if fingerprint.same_content(&self.disk) {
            return ExternalChange::None;
        }

        if self.dirty {
            self.conflict = Some(Conflict { disk: fingerprint });
            return ExternalChange::Conflict;
        }

        match self.reload() {
            Ok(()) => ExternalChange::Reloaded,
            Err(error) => {
                self.save_error = Some(error.to_string());
                ExternalChange::None
            }
        }
    }

    /// Accepts the external version and clears the conflict.
    pub fn accept_disk_version(&mut self) -> Result<(), DocumentError> {
        self.reload()
    }

    /// Keeps local edits and suppresses another conflict for this disk version.
    pub fn keep_local_version(&mut self) {
        if let Ok(fingerprint) = atomic::read_fingerprint(&self.path) {
            self.disk = fingerprint;
        }
        self.conflict = None;
        self.refresh_dirty();
    }

    /// Creates a new empty file for a document whose file was deleted.
    pub fn recreate_missing_file(&mut self) -> Result<(), DocumentError> {
        self.save()
    }

    pub fn stats(&self) -> TextStats {
        TextStats::from_text(self.front_matter.body())
    }

    pub fn selection_stats(&self, start: (usize, usize), end: (usize, usize)) -> Option<TextStats> {
        stats::selection_stats(&self.text, start, end)
    }

    pub fn outline(&self) -> Vec<Heading> {
        outline::headings(&self.text)
    }

    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }
}

fn file_title(path: &Path) -> String {
    path.file_stem().map_or_else(
        || path.display().to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// Encoding wrapper used by saves.

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("file operation failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("unsupported document type: {0}")]
    Unsupported(PathBuf),
    #[error("not a regular file: {0}")]
    NotAFile(PathBuf),
    #[error("document cannot be decoded: {0}")]
    Decode(#[from] DecodeError),
    #[error("document is read-only: {0}")]
    ReadOnly(PathBuf),
    #[error("document write failed: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn document(bytes: &[u8], name: &str) -> (tempfile::TempDir, Document) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(name);
        fs::write(&path, bytes).expect("write fixture");
        let document = Document::open(1, &path).expect("open");
        (tmp, document)
    }

    #[test]
    fn opens_utf8_and_exposes_front_matter() {
        let (_tmp, document) = document(
            b"---\ntitle: My Post\ntags: [one, two]\n---\n# Body\n",
            "post.md",
        );

        assert_eq!(
            document.text,
            "---\ntitle: My Post\ntags: [one, two]\n---\n# Body\n"
        );
        assert_eq!(document.display_title(), "My Post");
        assert_eq!(document.front_matter().tags(), vec!["one", "two"]);
        assert!(!document.is_dirty());
        assert_eq!(document.stats().words, 1);
    }

    #[test]
    fn editing_marks_dirty_and_saving_clears_it() {
        let (_tmp, mut document) = document(b"# One\n", "note.md");

        document.set_text("# Two\n");
        assert!(document.is_dirty());
        document.save().expect("save");
        assert!(!document.is_dirty());
        assert_eq!(fs::read_to_string(&document.path).expect("read"), "# Two\n");
    }

    #[test]
    fn saving_preserves_crlf() {
        let (_tmp, mut document) = document(b"one\r\ntwo\r\n", "crlf.md");

        document.set_text("one\ntwo\nthree\n");
        document.save().expect("save");

        assert_eq!(
            fs::read(&document.path).expect("read"),
            b"one\r\ntwo\r\nthree\r\n"
        );
    }

    #[test]
    fn unreadable_encoding_is_reported_without_touching_the_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("bad.md");
        fs::write(&path, [0xFF, 0xFE, 0x00]).expect("fixture");

        let error = Document::open(1, &path).expect_err("decode error");

        assert!(matches!(error, DocumentError::Decode(_)));
        assert_eq!(fs::read(&path).expect("read"), [0xFF, 0xFE, 0x00]);
    }

    #[test]
    fn read_only_file_reports_error_and_stays_dirty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("readonly.md");
        fs::write(&path, b"original").expect("fixture");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).expect("chmod");

        let mut document = Document::open(1, &path).expect("open");
        document.set_text("changed");
        let error = document.save().expect_err("must fail");

        assert!(matches!(error, DocumentError::ReadOnly(_)));
        assert!(document.is_dirty());
        assert!(document.save_error.is_some());
        assert_eq!(fs::read(&path).expect("read"), b"original");
    }

    #[test]
    fn clean_external_change_reloads() {
        let (_tmp, mut document) = document(b"# Original\n", "note.md");
        fs::write(&document.path, b"# Changed elsewhere\n").expect("external");

        let change = document.check_external_change();

        assert_eq!(change, ExternalChange::Reloaded);
        assert_eq!(document.text, "# Changed elsewhere\n");
        assert!(!document.is_dirty());
    }

    #[test]
    fn dirty_external_change_becomes_a_conflict() {
        let (_tmp, mut document) = document(b"# Original\n", "note.md");
        document.set_text("# Mine\n");
        fs::write(&document.path, b"# Theirs\n").expect("external");

        let change = document.check_external_change();

        assert_eq!(change, ExternalChange::Conflict);
        assert!(document.conflict.is_some());
        assert_eq!(document.text, "# Mine\n");
        assert_eq!(document.disk_text().expect("disk text"), "# Theirs\n");
    }

    #[test]
    fn conflict_can_keep_local_or_accept_disk() {
        let (_tmp, mut document) = document(b"# Original\n", "note.md");
        document.set_text("# Mine\n");
        fs::write(&document.path, b"# Theirs\n").expect("external");
        document.check_external_change();

        document.accept_disk_version().expect("accept");
        assert_eq!(document.text, "# Theirs\n");
        assert!(!document.is_dirty());

        document.set_text("# Mine again\n");
        fs::write(&document.path, b"# Theirs again\n").expect("external");
        document.check_external_change();
        document.keep_local_version();
        assert_eq!(document.text, "# Mine again\n");
        assert!(document.is_dirty());
        assert!(document.conflict.is_none());
    }

    #[test]
    fn deleted_file_is_marked_missing_and_can_be_saved_again() {
        let (_tmp, mut document) = document(b"# Keep me\n", "note.md");
        document.set_text("# Keep me\n\nEdited\n");
        fs::remove_file(&document.path).expect("delete");

        let change = document.check_external_change();
        assert_eq!(change, ExternalChange::Missing);
        assert!(document.is_missing());

        document.recreate_missing_file().expect("recreate");
        assert!(!document.is_missing());
        assert!(document.path.exists());
    }

    #[test]
    fn outline_and_selection_stats_use_the_document_text() {
        let (_tmp, document) = document(b"# One\n\n## Two\n\nSome text here.\n", "note.md");

        assert_eq!(document.outline().len(), 2);
        let selected = document.selection_stats((4, 0), (4, 4)).expect("stats");
        assert_eq!(selected.words, 1);
    }
}
