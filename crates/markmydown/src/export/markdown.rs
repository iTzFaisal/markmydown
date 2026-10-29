//! Markdown export.
//!
//! The source is exported unchanged. The only transformation is appending
//! `.md` when the destination has no extension.

use std::path::{Path, PathBuf};

use crate::workspace::atomic;

/// Appends `.md` when the path has no extension.
pub fn ensure_extension(path: impl Into<PathBuf>) -> PathBuf {
    let path = path.into();
    if path.extension().is_none() {
        path.with_extension("md")
    } else {
        path
    }
}

/// Writes the current source unchanged and returns the final path.
pub fn export(path: impl AsRef<Path>, source: &str) -> Result<PathBuf, MarkdownExportError> {
    let path = ensure_extension(path.as_ref().to_path_buf());
    atomic::atomic_write_text(&path, source).map_err(MarkdownExportError::Write)?;
    Ok(path)
}

/// Suggested Markdown file name for the save dialog.
pub fn suggested_name(title: &str) -> String {
    suggested_name_with_extension(title, "md")
}

/// Suggested file name for the save dialog with the requested extension.
pub fn suggested_name_with_extension(title: &str, extension: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('-');
    if cleaned.is_empty() {
        format!("document.{extension}")
    } else {
        format!("{cleaned}.{extension}")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MarkdownExportError {
    #[error("could not write Markdown export: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn writes_the_source_unchanged() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("export.md");
        let source = "# Title\n\nText with **markers**.\n";

        let written = export(&path, source).expect("export");

        assert_eq!(written, path);
        assert_eq!(fs::read_to_string(path).expect("read"), source);
    }

    #[test]
    fn missing_extension_becomes_md() {
        assert_eq!(
            ensure_extension("/tmp/document"),
            PathBuf::from("/tmp/document.md")
        );
        assert_eq!(
            ensure_extension("/tmp/document.markdown"),
            PathBuf::from("/tmp/document.markdown")
        );
    }

    #[test]
    fn suggested_names_are_safe() {
        assert_eq!(suggested_name("My Post"), "My Post.md");
        assert_eq!(suggested_name("a/b:c"), "a-b-c.md");
        assert_eq!(suggested_name(""), "document.md");
    }
}
