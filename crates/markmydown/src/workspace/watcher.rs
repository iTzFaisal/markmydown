//! Debounced filesystem watcher for open workspace paths.
//!
//! Events are coalesced by `notify-debouncer-mini`. The app polls the channel
//! on its own timer, compares fingerprints, and ignores our own atomic writes,
//! hidden entries, and ignored build directories.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

use super::tree;

/// A coalesced change inside the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    pub path: PathBuf,
}

/// A running workspace watcher.
pub struct WorkspaceWatcher {
    root: PathBuf,
    _debouncer: Debouncer<RecommendedWatcher>,
    receiver: Receiver<DebounceEventResult>,
}

impl WorkspaceWatcher {
    /// Starts watching `root` recursively with a 250 ms debounce.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, WatcherError> {
        let root = root.as_ref().canonicalize().map_err(WatcherError::Io)?;
        let (sender, receiver) = channel();

        let mut debouncer = new_debouncer(Duration::from_millis(250), sender)
            .map_err(|error| WatcherError::Notify(error.to_string()))?;
        debouncer
            .watcher()
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| WatcherError::Notify(error.to_string()))?;

        Ok(Self {
            root,
            _debouncer: debouncer,
            receiver,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Drains pending events without blocking. Duplicate paths are collapsed.
    pub fn drain(&self) -> Vec<WatchEvent> {
        let mut paths = BTreeSet::new();

        while let Ok(result) = self.receiver.try_recv() {
            match result {
                Ok(events) => {
                    for event in events {
                        if should_report(&self.root, &event.path) {
                            paths.insert(event.path);
                        }
                    }
                }
                Err(error) => {
                    log::warn!("workspace watcher error: {error}");
                }
            }
        }

        paths.into_iter().map(|path| WatchEvent { path }).collect()
    }
}

fn should_report(root: &Path, path: &Path) -> bool {
    if !path.starts_with(root) {
        return false;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    let ignored_component = relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), ".git" | "node_modules" | "target")
            || name.starts_with('.')
            || name.contains(".markmydown-tmp-")
    });

    if ignored_component {
        return false;
    }

    if path.is_dir() {
        return true;
    }

    match tree::extension(path) {
        Some(extension) => tree::SUPPORTED_EXTENSIONS.contains(&extension.as_str()),
        // A path without an extension may be a directory that was deleted.
        None => true,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("cannot watch workspace: {0}")]
    Io(#[source] std::io::Error),
    #[error("filesystem watcher failed: {0}")]
    Notify(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    fn wait_for_event(watcher: &WorkspaceWatcher, needle: &Path) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            for event in watcher.drain() {
                if event.path == needle {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn reports_a_changed_document() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().canonicalize().expect("canonical root");
        let watcher = WorkspaceWatcher::new(&root).expect("watcher");
        let path = root.join("note.md");
        fs::write(&path, "# hello").expect("write");

        assert!(wait_for_event(&watcher, &path));
    }

    #[test]
    fn ignores_temporary_and_unsupported_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let watcher = WorkspaceWatcher::new(tmp.path()).expect("watcher");
        let ignored = tmp.path().join(".note.md.markmydown-tmp-1-0");
        let unsupported = tmp.path().join("image.png");

        fs::write(&ignored, b"temp").expect("write");
        fs::write(&unsupported, b"png").expect("write");
        std::thread::sleep(Duration::from_millis(500));

        assert!(watcher.drain().is_empty());
    }
}
