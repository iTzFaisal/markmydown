//! Workspace orchestration: tree, tabs, saves, watcher, session, and recovery.
//!
//! The lower-level modules own one concern each. This module is the stateful
//! layer the application drives: it opens a folder, keeps the tab list and
//! active tab, saves documents, applies external changes, and persists the
//! session and recovery buffers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::document::{Cursor, Document, DocumentError, ExternalChange};
use super::import::{self, ImportError, ImportResult};
use super::ops::{self, OpsError};
use super::recovery::{RecoveryError, RecoveryRecord, RecoveryStore};
use super::session::{Session, SessionError};
use super::tree::{TreeError, WorkspaceTree};
use super::watcher::{WatcherError, WorkspaceWatcher};

/// One document tab and the state needed to restore it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    pub dirty: bool,
    pub missing: bool,
    pub conflict: bool,
}

/// The result of an autosave pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    pub id: u64,
    pub path: PathBuf,
    pub saved: bool,
    pub error: Option<String>,
}

/// A watcher event that affected an open document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalChangeEvent {
    pub path: PathBuf,
    pub change: ExternalChange,
}

/// What happened while restoring the previous session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub workspace_opened: bool,
    pub workspace_missing: bool,
    pub tabs_opened: usize,
    pub tabs_missing: usize,
    pub active_path: Option<PathBuf>,
}

/// State shared by every workspace surface.
pub struct WorkspaceState {
    tree: Option<WorkspaceTree>,
    documents: Vec<Document>,
    active: Option<u64>,
    next_id: u64,
    watcher: Option<WorkspaceWatcher>,
    session_path: PathBuf,
    recovery: RecoveryStore,
    last_edit: HashMap<u64, Instant>,
}

impl WorkspaceState {
    /// Creates an empty workspace bound to a session file and recovery folder.
    pub fn new(session_path: impl Into<PathBuf>, recovery_dir: impl Into<PathBuf>) -> Self {
        Self {
            tree: None,
            documents: Vec::new(),
            active: None,
            next_id: 1,
            watcher: None,
            session_path: session_path.into(),
            recovery: RecoveryStore::new(recovery_dir),
            last_edit: HashMap::new(),
        }
    }

    /// Opens or switches to `root`, scanning the tree and starting the watcher.
    ///
    /// Existing tabs are closed. Callers that switch folders should persist the
    /// previous session first if they need it.
    pub fn open_folder(&mut self, root: impl AsRef<Path>) -> Result<(), WorkspaceError> {
        let tree = WorkspaceTree::open(root)?;
        let watcher = match WorkspaceWatcher::new(tree.root()) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                log::warn!("workspace watcher disabled: {error}");
                None
            }
        };

        self.documents.clear();
        self.active = None;
        self.last_edit.clear();
        self.tree = Some(tree);
        self.watcher = watcher;
        Ok(())
    }

    /// Refreshes the tree from disk.
    pub fn refresh_tree(&mut self) -> Result<(), WorkspaceError> {
        self.tree
            .as_mut()
            .ok_or(WorkspaceError::NoWorkspace)?
            .refresh()?;
        Ok(())
    }

    pub fn root(&self) -> Option<&Path> {
        self.tree.as_ref().map(WorkspaceTree::root)
    }

    pub fn tree(&self) -> Option<&WorkspaceTree> {
        self.tree.as_ref()
    }

    pub fn is_open(&self) -> bool {
        self.tree.is_some()
    }

    /// Restores the workspace root, open tabs, active tab, cursor, and scroll.
    ///
    /// Missing files are skipped and counted rather than reopened as broken
    /// tabs. The caller can still show a recovery prompt for unsaved buffers.
    pub fn restore_session(&mut self, session: &Session) -> Result<RestoreReport, WorkspaceError> {
        let mut report = RestoreReport::default();
        let mut active_id = None;

        if let Some(root) = &session.workspace_root {
            if root.is_dir() {
                self.open_folder(root)?;
                report.workspace_opened = true;
            } else {
                report.workspace_missing = true;
            }
        }

        for tab in &session.tabs {
            if !tab.path.is_file() {
                report.tabs_missing += 1;
                continue;
            }

            let id = self.open_document_at(
                &tab.path,
                Cursor::new(tab.cursor_line, tab.cursor_column),
                tab.scroll,
            )?;
            report.tabs_opened += 1;

            if session.active_path.as_deref() == Some(tab.path.as_path()) {
                active_id = Some(id);
            }
        }

        if let Some(id) = active_id {
            self.active = Some(id);
            report.active_path = self.document(id).map(|document| document.path.clone());
        } else if let Some(first) = self.documents.first() {
            self.active = Some(first.id);
            report.active_path = Some(first.path.clone());
        }

        Ok(report)
    }

    /// Persists the current session.
    pub fn save_session(&self) -> Result<(), SessionError> {
        self.session().save(&self.session_path)
    }

    /// Captures the current session without writing it.
    pub fn session(&self) -> Session {
        Session::from_documents(self.root(), &self.documents, self.active)
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    pub fn tabs(&self) -> Vec<Tab> {
        self.documents
            .iter()
            .map(|document| Tab {
                id: document.id,
                path: document.path.clone(),
                title: document.display_title().to_string(),
                dirty: document.is_dirty(),
                missing: document.is_missing(),
                conflict: document.conflict.is_some(),
            })
            .collect()
    }

    pub fn document(&self, id: u64) -> Option<&Document> {
        self.documents.iter().find(|document| document.id == id)
    }

    pub fn document_mut(&mut self, id: u64) -> Option<&mut Document> {
        self.documents.iter_mut().find(|document| document.id == id)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.active
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.active.and_then(|id| self.document(id))
    }

    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(|id| self.document_mut(id))
    }

    pub fn activate(&mut self, id: u64) -> bool {
        if self.document(id).is_some() {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    /// Opens a document, reusing an existing tab when it is already open.
    pub fn open_document(&mut self, path: impl AsRef<Path>) -> Result<u64, WorkspaceError> {
        self.open_document_at(path, Cursor::default(), 0.0)
    }

    /// Opens a document at a restored cursor and scroll position.
    pub fn open_document_at(
        &mut self,
        path: impl AsRef<Path>,
        cursor: Cursor,
        scroll: f32,
    ) -> Result<u64, WorkspaceError> {
        let path = path.as_ref();
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let path = path.as_path();

        if let Some(document) = self
            .documents
            .iter_mut()
            .find(|document| document.path == path)
        {
            document.cursor = cursor;
            document.scroll = scroll;
            self.active = Some(document.id);
            return Ok(document.id);
        }

        let id = self.allocate_id();
        let mut document = Document::open(id, path)?;
        document.cursor = cursor;
        document.scroll = scroll;
        self.documents.push(document);
        self.active = Some(id);
        Ok(id)
    }

    /// Closes a tab and removes its recovery buffer.
    pub fn close_document(&mut self, id: u64) -> Option<Document> {
        let index = self
            .documents
            .iter()
            .position(|document| document.id == id)?;
        let document = self.documents.remove(index);
        let _ = self.recovery.clear(&document.path);
        self.last_edit.remove(&id);

        if self.active == Some(id) {
            self.active = self
                .documents
                .get(index)
                .or_else(|| self.documents.last())
                .map(|document| document.id);
        }

        Some(document)
    }

    /// Updates the text buffer, cursor, and scroll after an editor change.
    pub fn set_document_content(
        &mut self,
        id: u64,
        text: impl Into<String>,
        cursor: Cursor,
        scroll: f32,
    ) {
        let mut dirty = false;
        if let Some(document) = self.document_mut(id) {
            document.set_text(text);
            document.cursor = cursor;
            document.scroll = scroll;
            dirty = document.is_dirty();
        }

        if dirty {
            self.last_edit.insert(id, Instant::now());
        } else {
            self.last_edit.remove(&id);
        }
    }

    /// Updates only the cursor and scroll, avoiding a text comparison on every
    /// caret movement.
    pub fn set_document_position(&mut self, id: u64, cursor: Cursor, scroll: f32) {
        if let Some(document) = self.document_mut(id) {
            document.cursor = cursor;
            document.scroll = scroll;
        }
    }

    /// Saves one document immediately. Failures leave it dirty and open.
    pub fn save(&mut self, id: u64) -> Result<(), WorkspaceError> {
        let document = self
            .documents
            .iter_mut()
            .find(|document| document.id == id)
            .ok_or(WorkspaceError::NoDocument(id))?;

        let path = document.path.clone();
        document.save()?;
        self.last_edit.remove(&id);
        let _ = self.recovery.clear(&path);
        Ok(())
    }

    pub fn save_active(&mut self) -> Result<(), WorkspaceError> {
        let id = self.active.ok_or(WorkspaceError::NoActiveDocument)?;
        self.save(id)
    }

    /// Saves every dirty document.
    pub fn save_all(&mut self) -> Vec<SaveOutcome> {
        let ids: Vec<u64> = self
            .documents
            .iter()
            .filter(|document| document.is_dirty())
            .map(|document| document.id)
            .collect();

        ids.into_iter().map(|id| self.save_outcome(id)).collect()
    }

    /// Saves dirty documents whose last edit is at least `idle` ago.
    ///
    /// Documents in an unresolved conflict or marked missing are skipped so
    /// autosave never silently overwrites an external change or recreates a
    /// file the user deleted on purpose.
    pub fn autosave_due(&mut self, now: Instant, idle: Duration) -> Vec<SaveOutcome> {
        let due: Vec<u64> = self
            .documents
            .iter()
            .filter(|document| document.is_dirty())
            .filter(|document| document.conflict.is_none() && !document.is_missing())
            .filter(|document| {
                self.last_edit
                    .get(&document.id)
                    .is_none_or(|last| now.duration_since(*last) >= idle)
            })
            .map(|document| document.id)
            .collect();

        due.into_iter().map(|id| self.save_outcome(id)).collect()
    }

    /// Saves one document and reports the result without propagating errors.
    pub fn save_outcome(&mut self, id: u64) -> SaveOutcome {
        let Some(document) = self.document(id) else {
            return SaveOutcome {
                id,
                path: PathBuf::new(),
                saved: false,
                error: Some(String::from("document is no longer open")),
            };
        };

        let path = document.path.clone();
        match self.save(id) {
            Ok(()) => SaveOutcome {
                id,
                path,
                saved: true,
                error: None,
            },
            Err(error) => SaveOutcome {
                id,
                path,
                saved: false,
                error: Some(error.to_string()),
            },
        }
    }

    /// Writes recovery buffers for every dirty tab and clears clean ones.
    pub fn write_recovery(&self) -> Result<usize, RecoveryError> {
        self.recovery.write_all(&self.documents)
    }

    /// Lists recovery buffers left by an earlier run, newest first.
    pub fn pending_recovery(&self) -> Vec<RecoveryRecord> {
        self.recovery.list()
    }

    /// Opens a recovery record as a dirty tab and returns its id.
    pub fn accept_recovery(&mut self, record: RecoveryRecord) -> u64 {
        let id = self.allocate_id();
        let document = record.into_document(id);
        self.documents.push(document);
        self.active = Some(id);
        self.last_edit.insert(id, Instant::now());
        id
    }

    /// Discards one recovery record.
    pub fn discard_recovery(&self, record: &RecoveryRecord) -> Result<(), RecoveryError> {
        self.recovery.clear(&record.path)
    }

    /// Discards every recovery record.
    pub fn discard_all_recovery(&self) -> Result<(), RecoveryError> {
        self.recovery.clear_all()
    }

    /// Polls the watcher, reloads clean documents, and marks conflicts and
    /// missing files. The tree is refreshed when its structure changed.
    pub fn poll_external_changes(&mut self) -> Vec<ExternalChangeEvent> {
        let Some(watcher) = &self.watcher else {
            return Vec::new();
        };
        let events = watcher.drain();
        if events.is_empty() {
            return Vec::new();
        }

        let mut structural = false;
        let mut changes: HashMap<PathBuf, ExternalChange> = HashMap::new();

        for event in events {
            let path = event.path;
            let open = self.documents.iter().any(|document| document.path == path);

            if !open || path.is_dir() || !path.exists() {
                structural = true;
            }

            for document in &mut self.documents {
                let affected =
                    document.path == path || (!path.exists() && document.path.starts_with(&path));
                if !affected {
                    continue;
                }

                let change = document.check_external_change();
                if change != ExternalChange::None {
                    changes.insert(document.path.clone(), change);
                }
            }
        }

        if structural {
            let _ = self.refresh_tree();
        }

        changes
            .into_iter()
            .map(|(path, change)| ExternalChangeEvent { path, change })
            .collect()
    }

    /// Creates a document, refreshes the tree, and opens it in a tab.
    pub fn create_document(
        &mut self,
        parent: impl AsRef<Path>,
        name: &str,
        content: &str,
    ) -> Result<u64, WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let path = ops::create_file(&root, parent.as_ref(), name, content)?;
        self.refresh_tree()?;
        self.open_document(path)
    }

    /// Creates a folder and refreshes the tree.
    pub fn create_folder(
        &mut self,
        parent: impl AsRef<Path>,
        name: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let path = ops::create_folder(&root, parent.as_ref(), name)?;
        self.refresh_tree()?;
        Ok(path)
    }

    /// Renames an entry and retargets every open document below it.
    pub fn rename_entry(
        &mut self,
        path: impl AsRef<Path>,
        new_name: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let path = path.as_ref();
        let new_path = ops::rename(&root, path, new_name)?;
        self.retarget_documents(path, &new_path);
        self.refresh_tree()?;
        Ok(new_path)
    }

    /// Moves an entry into another folder and retargets open documents.
    pub fn move_entry(
        &mut self,
        path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf, WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let path = path.as_ref();
        let new_path = ops::move_entry(&root, path, destination.as_ref())?;
        self.retarget_documents(path, &new_path);
        self.refresh_tree()?;
        Ok(new_path)
    }

    /// Moves an entry to the operating system trash and closes its tabs.
    pub fn delete_entry(&mut self, path: impl AsRef<Path>) -> Result<(), WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let path = path.as_ref();
        ops::delete_to_trash(&root, path)?;

        let removed: Vec<u64> = self
            .documents
            .iter()
            .filter(|document| document.path == path || document.path.starts_with(path))
            .map(|document| document.id)
            .collect();

        for id in &removed {
            if let Some(document) = self.document(*id) {
                let _ = self.recovery.clear(&document.path);
            }
        }

        self.documents
            .retain(|document| !removed.contains(&document.id));
        self.last_edit.retain(|id, _| !removed.contains(id));

        if self.active.is_some_and(|id| removed.contains(&id)) {
            self.active = self.documents.last().map(|document| document.id);
        }

        self.refresh_tree()?;
        Ok(())
    }

    /// Imports a Markdown or HTML file, refreshes the tree, and opens it.
    pub fn import_file(
        &mut self,
        source: impl AsRef<Path>,
        destination_dir: impl AsRef<Path>,
    ) -> Result<ImportResult, WorkspaceError> {
        let root = self
            .root()
            .ok_or(WorkspaceError::NoWorkspace)?
            .to_path_buf();
        let result = import::import(source.as_ref(), &root, destination_dir.as_ref())?;
        self.refresh_tree()?;
        self.open_document(&result.path)?;
        Ok(result)
    }

    /// Saves the file of a tab whose file was deleted externally.
    pub fn recreate_missing_file(&mut self, id: u64) -> Result<(), WorkspaceError> {
        let document = self
            .documents
            .iter_mut()
            .find(|document| document.id == id)
            .ok_or(WorkspaceError::NoDocument(id))?;
        document.recreate_missing_file()?;
        let _ = self.recovery.clear(&document.path);
        Ok(())
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn retarget_documents(&mut self, old: &Path, new: &Path) {
        for document in &mut self.documents {
            if document.path == old {
                document.relocate(new.to_path_buf());
            } else if let Ok(relative) = document.path.strip_prefix(old) {
                document.relocate(new.join(relative));
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("no workspace folder is open")]
    NoWorkspace,
    #[error("document {0} is not open")]
    NoDocument(u64),
    #[error("no document tab is active")]
    NoActiveDocument,
    #[error(transparent)]
    Tree(#[from] TreeError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Ops(#[from] OpsError),
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Recovery(#[from] RecoveryError),
    #[error(transparent)]
    Watcher(#[from] WatcherError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Instant;

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("one.md"), "# One\n").expect("one");
        fs::write(tmp.path().join("two.markdown"), "# Two\n").expect("two");
        fs::write(tmp.path().join("image.png"), b"png").expect("image");
        fs::create_dir_all(tmp.path().join("notes")).expect("notes");
        fs::write(tmp.path().join("notes/three.md"), "# Three\n").expect("three");
        tmp
    }

    fn state(root: &Path) -> WorkspaceState {
        let mut state = WorkspaceState::new(root.join("session.json"), root.join("recovery"));
        state.open_folder(root).expect("open folder");
        state
    }

    #[test]
    fn opens_a_folder_and_lists_supported_documents() {
        let tmp = fixture();
        let state = state(tmp.path());
        let names: Vec<_> = state
            .tree()
            .expect("tree")
            .files()
            .into_iter()
            .map(|node| node.name.as_str())
            .collect();

        assert_eq!(names, vec!["three.md", "one.md", "two.markdown"]);
        assert!(state.tabs().is_empty());
    }

    #[test]
    fn creates_renames_moves_and_deletes_with_open_tabs() {
        let tmp = fixture();
        let mut state = state(tmp.path());

        let root = state.root().expect("root").to_path_buf();
        let id = state
            .create_document(tmp.path(), "new", "# New\n")
            .expect("create");
        assert_eq!(
            state.document(id).expect("doc").path.file_name().unwrap(),
            "new.md"
        );
        assert!(
            state
                .tree()
                .expect("tree")
                .find(&root.join("new.md"))
                .is_some()
        );

        let renamed = state
            .rename_entry(root.join("new.md"), "renamed.md")
            .expect("rename");
        assert_eq!(state.document(id).expect("doc").path, renamed);
        assert_eq!(state.document(id).expect("doc").display_title(), "renamed");

        let moved = state
            .move_entry(&renamed, root.join("notes"))
            .expect("move");
        assert_eq!(state.document(id).expect("doc").path, moved);
        assert!(state.tree().expect("tree").find(&moved).is_some());

        state.delete_entry(&moved).expect("delete");
        assert!(state.document(id).is_none());
        assert!(state.tree().expect("tree").find(&moved).is_none());
    }

    #[test]
    fn tabs_report_dirty_state_and_restore_cursor_and_scroll() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let one = state.open_document(tmp.path().join("one.md")).expect("one");
        let two = state
            .open_document(tmp.path().join("two.markdown"))
            .expect("two");
        state.activate(one);

        state.set_document_content(one, "# One\n\nEdited\n", Cursor::new(2, 6), 120.0);
        assert!(state.tabs().iter().any(|tab| tab.id == one && tab.dirty));
        assert!(!state.tabs().iter().any(|tab| tab.id == two && tab.dirty));

        state.activate(two);
        let session = state.session();
        let mut restored = WorkspaceState::new(
            tmp.path().join("session-two.json"),
            tmp.path().join("recovery-two"),
        );
        let report = restored.restore_session(&session).expect("restore");

        assert_eq!(report.tabs_opened, 2);
        assert_eq!(restored.active_id(), Some(2));
        let restored_one = restored.document(1).expect("one");
        assert_eq!(restored_one.cursor, Cursor::new(2, 6));
        assert_eq!(restored_one.scroll, 120.0);
    }

    #[test]
    fn explicit_save_and_autosave_write_after_idle() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let id = state
            .open_document(tmp.path().join("one.md"))
            .expect("open");

        state.set_document_content(id, "# Changed\n", Cursor::default(), 0.0);
        state.save(id).expect("save");
        assert_eq!(
            fs::read_to_string(tmp.path().join("one.md")).expect("read"),
            "# Changed\n"
        );
        assert!(!state.document(id).expect("doc").is_dirty());

        state.set_document_content(id, "# Autosaved\n", Cursor::default(), 0.0);
        let outcomes = state.autosave_due(Instant::now(), Duration::ZERO);
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].saved);
        assert_eq!(
            fs::read_to_string(tmp.path().join("one.md")).expect("read"),
            "# Autosaved\n"
        );
    }

    #[test]
    fn save_failure_keeps_the_document_dirty_and_open() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let path = tmp.path().join("locked.md");
        fs::write(&path, "# Locked\n").expect("fixture");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).expect("readonly");

        let id = state.open_document(&path).expect("open");
        state.set_document_content(id, "# Edited\n", Cursor::default(), 0.0);
        let outcome = state.save_outcome(id);

        assert!(!outcome.saved);
        assert!(outcome.error.is_some());
        assert!(state.document(id).expect("doc").is_dirty());
        assert!(state.document(id).expect("doc").save_error.is_some());
    }

    #[test]
    fn writes_recovery_buffers_and_reopens_them_dirty() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let id = state
            .open_document(tmp.path().join("one.md"))
            .expect("open");
        state.set_document_content(id, "# Unsaved\n", Cursor::new(1, 2), 44.0);
        state.write_recovery().expect("write recovery");

        let records = state.pending_recovery();
        assert_eq!(records.len(), 1);
        let recovered = state.accept_recovery(records.into_iter().next().expect("record"));

        let document = state.document(recovered).expect("recovered");
        assert_eq!(document.text, "# Unsaved\n");
        assert!(document.is_dirty());
        assert_eq!(document.cursor, Cursor::new(1, 2));
        assert_eq!(document.scroll, 44.0);
    }

    #[test]
    fn external_changes_reload_clean_documents_and_conflict_with_dirty_ones() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let clean = state
            .open_document(tmp.path().join("one.md"))
            .expect("clean");
        let dirty = state
            .open_document(tmp.path().join("two.markdown"))
            .expect("dirty");
        state.set_document_content(dirty, "# Mine\n", Cursor::default(), 0.0);

        fs::write(tmp.path().join("one.md"), "# External\n").expect("external clean");
        fs::write(tmp.path().join("two.markdown"), "# Theirs\n").expect("external dirty");
        wait_for_changes(&mut state, 2);

        assert_eq!(state.document(clean).expect("clean").text, "# External\n");
        assert!(state.document(dirty).expect("dirty").conflict.is_some());
    }

    #[test]
    fn deleted_files_are_marked_missing_and_can_be_recreated() {
        let tmp = fixture();
        let mut state = state(tmp.path());
        let id = state
            .open_document(tmp.path().join("one.md"))
            .expect("open");
        state.set_document_content(id, "# Keep\n", Cursor::default(), 0.0);

        fs::remove_file(tmp.path().join("one.md")).expect("delete");
        wait_for_changes(&mut state, 1);

        assert!(state.document(id).expect("doc").is_missing());
        state.recreate_missing_file(id).expect("recreate");
        assert!(!state.document(id).expect("doc").is_missing());
        assert!(tmp.path().join("one.md").is_file());
    }

    fn wait_for_changes(state: &mut WorkspaceState, minimum: usize) -> Vec<ExternalChangeEvent> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut changes = Vec::new();
        while Instant::now() < deadline {
            changes.extend(state.poll_external_changes());
            if changes.len() >= minimum {
                return changes;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out waiting for {minimum} changes, got {changes:?}");
    }

    #[test]
    fn imports_markdown_and_html_into_the_tree() {
        let tmp = fixture();
        let source = tempfile::tempdir().expect("source");
        let mut state = state(tmp.path());

        let markdown = source.path().join("imported.md");
        fs::write(&markdown, "# Imported\n").expect("markdown");
        let root = state.root().expect("root").to_path_buf();
        let result = state
            .import_file(&markdown, root.join("notes"))
            .expect("import markdown");
        assert_eq!(result.path, root.join("notes/imported.md"));
        assert!(state.tree().expect("tree").find(&result.path).is_some());

        let html = source.path().join("page.html");
        fs::write(&html, "<h1>Page</h1><p>Hello</p>").expect("html");
        let result = state.import_file(&html, &root).expect("import html");
        assert!(result.converted_from_html);
        assert_eq!(result.path.extension().unwrap(), "md");
        assert!(state.tree().expect("tree").find(&result.path).is_some());
    }
}
