//! Create, rename, move, and trash operations for workspace entries.
//!
//! Every destination is validated against the workspace root before any disk
//! mutation. Names cannot contain path separators, `.`/`..`, NUL, or reserved
//! Windows device names.

use std::fs;
use std::path::{Path, PathBuf};

use super::tree::{self, FileKind};

/// Validates a single file or folder name.
pub fn validate_name(name: &str) -> Result<(), OpsError> {
    if name.is_empty() {
        return Err(OpsError::InvalidName(String::from("name cannot be empty")));
    }
    if name == "." || name == ".." {
        return Err(OpsError::InvalidName(String::from(
            "name cannot be . or ..",
        )));
    }
    if name.contains(['/', '\\', '\0']) {
        return Err(OpsError::InvalidName(String::from(
            "name cannot contain a path separator",
        )));
    }
    if name.trim() != name {
        return Err(OpsError::InvalidName(String::from(
            "name cannot start or end with whitespace",
        )));
    }
    if name.ends_with('.') {
        return Err(OpsError::InvalidName(String::from(
            "name cannot end with a dot",
        )));
    }

    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        stem.as_str(),
        "con" | "prn" | "aux" | "nul" | "com1" | "com2" | "com3" | "lpt1" | "lpt2" | "lpt3"
    ) {
        return Err(OpsError::InvalidName(format!(
            "{name} is reserved on Windows"
        )));
    }

    Ok(())
}

/// Creates an empty document. A missing extension becomes `.md`.
pub fn create_file(
    root: &Path,
    parent: &Path,
    name: &str,
    content: &str,
) -> Result<PathBuf, OpsError> {
    validate_name(name)?;
    let parent = canonical_directory(root, parent)?;

    let mut file_name = name.to_string();
    if Path::new(&file_name).extension().is_none() {
        file_name.push_str(".md");
    }

    if !tree::is_supported(Path::new(&file_name)) {
        return Err(OpsError::UnsupportedFile(file_name));
    }

    let path = parent.join(&file_name);
    ensure_target_inside(root, &path)?;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| map_exists(error, &path))?;

    use std::io::Write as _;
    file.write_all(content.as_bytes()).map_err(OpsError::Io)?;

    Ok(path)
}

/// Creates a folder inside the workspace.
pub fn create_folder(root: &Path, parent: &Path, name: &str) -> Result<PathBuf, OpsError> {
    validate_name(name)?;
    let parent = canonical_directory(root, parent)?;
    let path = parent.join(name);
    ensure_target_inside(root, &path)?;

    fs::create_dir(&path).map_err(|error| map_exists(error, &path))?;

    Ok(path)
}

/// Renames a file or folder without moving it to another directory.
pub fn rename(root: &Path, path: &Path, new_name: &str) -> Result<PathBuf, OpsError> {
    validate_name(new_name)?;
    ensure_mutable_entry(root, path)?;

    let parent = path
        .parent()
        .ok_or_else(|| OpsError::OutsideRoot(path.to_path_buf()))?;

    let new_path = parent.join(new_name);
    ensure_target_inside(root, &new_path)?;

    if new_path == path {
        return Ok(new_path);
    }

    if !path.is_dir() && !tree::is_supported(&new_path) {
        return Err(OpsError::UnsupportedFile(new_name.to_string()));
    }

    if new_path.exists() {
        return Err(OpsError::AlreadyExists(new_path));
    }

    fs::rename(path, &new_path).map_err(OpsError::Io)?;
    Ok(new_path)
}

/// Moves a file or folder into `destination_dir`.
pub fn move_entry(root: &Path, path: &Path, destination_dir: &Path) -> Result<PathBuf, OpsError> {
    ensure_mutable_entry(root, path)?;
    let destination_dir = canonical_directory(root, destination_dir)?;

    if path.is_dir() && destination_dir.starts_with(path) {
        return Err(OpsError::MoveIntoItself(path.to_path_buf()));
    }

    let name = path
        .file_name()
        .ok_or_else(|| OpsError::OutsideRoot(path.to_path_buf()))?;
    let destination = destination_dir.join(name);
    ensure_target_inside(root, &destination)?;

    if destination == path {
        return Ok(destination);
    }
    if destination.exists() {
        return Err(OpsError::AlreadyExists(destination));
    }

    fs::rename(path, &destination).map_err(|error| {
        if error.raw_os_error() == Some(libc_exdev()) {
            OpsError::CrossDevice(destination.clone())
        } else {
            OpsError::Io(error)
        }
    })?;

    Ok(destination)
}

/// Moves a file or folder to the operating system trash.
pub fn delete_to_trash(root: &Path, path: &Path) -> Result<(), OpsError> {
    ensure_mutable_entry(root, path)?;

    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }

    context
        .delete(path)
        .map_err(|error| OpsError::Trash(error.to_string()))
}

fn ensure_mutable_entry(root: &Path, path: &Path) -> Result<(), OpsError> {
    let root = root.canonicalize().map_err(OpsError::Io)?;
    let path = path.canonicalize().map_err(OpsError::Io)?;

    if path == root || !path.starts_with(&root) {
        return Err(OpsError::OutsideRoot(path));
    }
    if path.parent().is_none() {
        return Err(OpsError::OutsideRoot(path));
    }

    Ok(())
}

fn canonical_directory(root: &Path, directory: &Path) -> Result<PathBuf, OpsError> {
    let root = root.canonicalize().map_err(OpsError::Io)?;
    let directory = directory.canonicalize().map_err(OpsError::Io)?;

    if !directory.starts_with(&root) || !directory.is_dir() {
        return Err(OpsError::OutsideRoot(directory));
    }

    Ok(directory)
}

fn ensure_target_inside(root: &Path, target: &Path) -> Result<(), OpsError> {
    let parent = target
        .parent()
        .ok_or_else(|| OpsError::OutsideRoot(target.to_path_buf()))?
        .canonicalize()
        .map_err(OpsError::Io)?;
    let root = root.canonicalize().map_err(OpsError::Io)?;

    if !parent.starts_with(&root) {
        return Err(OpsError::OutsideRoot(target.to_path_buf()));
    }

    Ok(())
}

fn map_exists(error: std::io::Error, path: &Path) -> OpsError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        OpsError::AlreadyExists(path.to_path_buf())
    } else {
        OpsError::Io(error)
    }
}

#[cfg(unix)]
fn libc_exdev() -> i32 {
    18
}

#[cfg(not(unix))]
fn libc_exdev() -> i32 {
    17
}

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("file operation failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("unsupported file type: {0}")]
    UnsupportedFile(String),
    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("outside the workspace: {0}")]
    OutsideRoot(PathBuf),
    #[error("cannot move a folder into itself: {0}")]
    MoveIntoItself(PathBuf),
    #[error("cross-device move is not supported for {0}")]
    CrossDevice(PathBuf),
    #[error("could not move to trash: {0}")]
    Trash(String),
}

/// Best-effort MIME-ish category for a path, used by import and export.
pub fn kind_for(path: &Path) -> Option<FileKind> {
    FileKind::from_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_names() {
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            " name",
            "name.",
            "con",
        ] {
            assert!(validate_name(name).is_err(), "{name} should be rejected");
        }

        for name in ["note.md", "My Note.markdown", "draft-2", "a.b.c"] {
            assert!(validate_name(name).is_ok(), "{name} should be accepted");
        }
    }

    #[test]
    fn creates_a_document_and_appends_md() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let path = create_file(root, root, "note", "# hello").expect("create");

        assert_eq!(path.file_name().unwrap(), "note.md");
        assert_eq!(fs::read_to_string(path).expect("read"), "# hello");
    }

    #[test]
    fn rejects_duplicate_and_unsupported_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        create_file(root, root, "note.md", "").expect("first");

        assert!(matches!(
            create_file(root, root, "note.md", ""),
            Err(OpsError::AlreadyExists(_))
        ));
        assert!(matches!(
            create_file(root, root, "image.png", ""),
            Err(OpsError::UnsupportedFile(_))
        ));
    }

    #[test]
    fn creates_folders_and_files_inside_them() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let folder = create_folder(root, root, "notes").expect("folder");
        let file = create_file(root, &folder, "topic", "# topic").expect("file");

        assert!(folder.is_dir());
        assert_eq!(file.parent().unwrap(), folder);
    }

    #[test]
    fn renames_files_and_folders() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let file = create_file(root, root, "old.md", "").expect("file");
        let folder = create_folder(root, root, "old-folder").expect("folder");

        let renamed = rename(root, &file, "new.md").expect("rename file");
        let renamed_folder = rename(root, &folder, "new-folder").expect("rename folder");

        assert!(!file.exists());
        assert!(renamed.exists());
        assert!(!folder.exists());
        assert!(renamed_folder.is_dir());
    }

    #[test]
    fn moves_a_document_between_folders() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let file = create_file(root, root, "note.md", "# note").expect("file");
        let folder = create_folder(root, root, "archive").expect("folder");

        let moved = move_entry(root, &file, &folder).expect("move");

        assert!(!file.exists());
        assert_eq!(moved.parent().unwrap(), folder);
        assert_eq!(fs::read_to_string(moved).expect("read"), "# note");
    }

    #[test]
    fn refuses_to_move_a_folder_into_itself() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let folder = create_folder(root, root, "notes").expect("folder");

        assert!(matches!(
            move_entry(root, &folder, &folder),
            Err(OpsError::MoveIntoItself(_))
        ));
    }

    #[test]
    fn refuses_to_delete_the_workspace_root() {
        let tmp = tempfile::tempdir().expect("tempdir");

        assert!(matches!(
            delete_to_trash(tmp.path(), tmp.path()),
            Err(OpsError::OutsideRoot(_))
        ));
    }

    #[test]
    fn deletes_a_document_to_the_operating_system_trash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let file = create_file(root, root, "trash-me.md", "").expect("file");

        delete_to_trash(root, &file).expect("trash");

        assert!(!file.exists());
    }
}
