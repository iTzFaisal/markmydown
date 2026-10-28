//! Session persistence: open tabs, active tab, cursor, and scroll.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::atomic;
use super::document::Document;

/// One restored tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionTab {
    pub path: PathBuf,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub scroll: f32,
}

/// The last session, persisted as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub schema_version: u32,
    pub workspace_root: Option<PathBuf>,
    pub active_path: Option<PathBuf>,
    pub tabs: Vec<SessionTab>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            schema_version: 1,
            workspace_root: None,
            active_path: None,
            tabs: Vec::new(),
        }
    }
}

impl Session {
    pub const CURRENT_SCHEMA: u32 = 1;

    /// Captures the open documents and the active tab.
    pub fn from_documents(
        workspace_root: Option<&Path>,
        documents: &[Document],
        active_id: Option<u64>,
    ) -> Self {
        let tabs: Vec<_> = documents
            .iter()
            .map(|document| SessionTab {
                path: document.path.clone(),
                cursor_line: document.cursor.line,
                cursor_column: document.cursor.column,
                scroll: document.scroll,
            })
            .collect();

        let active_path = active_id
            .and_then(|id| documents.iter().find(|document| document.id == id))
            .map(|document| document.path.clone());

        Self {
            schema_version: Self::CURRENT_SCHEMA,
            workspace_root: workspace_root.map(Path::to_path_buf),
            active_path,
            tabs,
        }
    }

    /// Loads a session. Missing or unreadable sessions produce defaults.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };

        match serde_json::from_slice::<Self>(&bytes) {
            Ok(session) if session.schema_version <= Self::CURRENT_SCHEMA => session,
            Ok(_) | Err(_) => {
                preserve_corrupt(path);
                Self::default()
            }
        }
    }

    /// Persists the session atomically.
    pub fn save(&self, path: &Path) -> Result<(), SessionError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SessionError::Io)?;
        }

        let json = serde_json::to_string_pretty(self).map_err(SessionError::Serialize)?;
        atomic::atomic_write(path, json.as_bytes()).map_err(SessionError::Write)?;
        Ok(())
    }
}

fn preserve_corrupt(path: &Path) {
    let backup = path.with_extension(format!("corrupt-{}", std::process::id()));
    let _ = std::fs::rename(path, backup);
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("session serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("session write failed: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_open_tabs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let session_path = tmp.path().join("session.json");
        let session = Session {
            schema_version: Session::CURRENT_SCHEMA,
            workspace_root: Some(PathBuf::from("/tmp/workspace")),
            active_path: Some(PathBuf::from("/tmp/workspace/two.md")),
            tabs: vec![
                SessionTab {
                    path: PathBuf::from("/tmp/workspace/one.md"),
                    cursor_line: 4,
                    cursor_column: 2,
                    scroll: 12.5,
                },
                SessionTab {
                    path: PathBuf::from("/tmp/workspace/two.md"),
                    cursor_line: 0,
                    cursor_column: 0,
                    scroll: 0.0,
                },
            ],
        };

        session.save(&session_path).expect("save");
        let loaded = Session::load(&session_path);

        assert_eq!(loaded, session);
    }

    #[test]
    fn corrupt_session_falls_back_to_defaults_and_is_preserved() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let session_path = tmp.path().join("session.json");
        std::fs::write(&session_path, b"{ not json").expect("fixture");

        let loaded = Session::load(&session_path);

        assert_eq!(loaded, Session::default());
        assert!(!session_path.exists());
        let backups: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().starts_with("corrupt-"))
            })
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn missing_session_is_default() {
        let tmp = tempfile::tempdir().expect("tempdir");

        assert_eq!(
            Session::load(&tmp.path().join("nothing.json")),
            Session::default()
        );
    }
}
