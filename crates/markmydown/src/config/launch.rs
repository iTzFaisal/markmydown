//! Command-line and file-manager launch handling.
//!
//! The operating system passes document paths as ordinary arguments for
//! open-with and command-line launches. Finder may also pass `-psn_…` process
//! serial numbers, so flags are ignored rather than treated as paths.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::workspace::tree;

/// Extracts document and folder paths from process arguments.
pub fn document_paths(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for arg in args.into_iter().skip(1) {
        let text = arg.to_string_lossy();
        if text.starts_with('-') {
            continue;
        }

        let path = PathBuf::from(arg);
        if path.is_dir() || (path.is_file() && tree::is_supported(&path)) {
            paths.push(canonical(path));
        }
    }

    paths
}

/// Chooses the workspace root for a launch.
///
/// A folder argument is used directly. A file argument opens its parent folder
/// so the sidebar and watcher have a workspace to work with.
pub fn launch_root(paths: &[PathBuf]) -> Option<PathBuf> {
    for path in paths {
        if path.is_dir() {
            return Some(canonical(path.clone()));
        }
        if path.is_file()
            && let Some(parent) = path.parent()
        {
            return Some(canonical(parent.to_path_buf()));
        }
    }

    None
}

/// Whether a path is a folder argument (as opposed to a document).
pub fn is_folder(path: &Path) -> bool {
    path.is_dir()
}

fn canonical(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn flags_are_not_treated_as_paths() {
        let paths = document_paths(args(&["markmydown", "-psn_0_12345", "--flag"]));
        assert!(paths.is_empty());
    }

    #[test]
    fn supported_files_and_folders_are_kept() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("root");
        let document = root.join("note.md");
        let ignored = root.join("image.png");
        std::fs::write(&document, "# Note\n").expect("note");
        std::fs::write(&ignored, b"png").expect("image");

        let paths = document_paths(args(&[
            "markmydown",
            document.to_str().expect("path"),
            ignored.to_str().expect("path"),
            root.to_str().expect("path"),
        ]));

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&document.canonicalize().expect("canonical")));
        assert!(paths.contains(&root.canonicalize().expect("canonical")));
    }

    #[test]
    fn a_file_launch_uses_its_parent_as_the_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let document = tmp.path().join("note.markdown");
        std::fs::write(&document, "# Note\n").expect("note");

        assert_eq!(
            launch_root(std::slice::from_ref(&document)),
            Some(tmp.path().canonicalize().expect("canonical"))
        );
    }

    #[test]
    fn a_folder_launch_uses_the_folder_as_the_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let folder = tmp.path().join("notes");
        std::fs::create_dir_all(&folder).expect("folder");

        assert_eq!(
            launch_root(std::slice::from_ref(&folder)),
            Some(folder.canonicalize().expect("canonical"))
        );
    }

    #[test]
    fn no_usable_paths_means_no_launch_root() {
        assert_eq!(launch_root(&[]), None);
        assert_eq!(
            launch_root(&[PathBuf::from("/definitely/not/here.md")]),
            None
        );
    }
}
