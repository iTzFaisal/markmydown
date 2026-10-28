//! Markdown and HTML import.
//!
//! Markdown is copied byte-for-byte into the workspace. HTML is converted to
//! Markdown with `html2md` and only written after a successful conversion, so
//! a failed import leaves the workspace untouched.

use std::path::{Path, PathBuf};

use super::atomic;
use super::encoding;
use super::ops;
use super::tree;

/// The result of an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub path: PathBuf,
    pub converted_from_html: bool,
    pub copied: bool,
}

/// Imports a Markdown or HTML file into the workspace.
pub fn import(
    source: &Path,
    workspace_root: &Path,
    destination_dir: &Path,
) -> Result<ImportResult, ImportError> {
    let source_metadata = std::fs::metadata(source).map_err(ImportError::Io)?;
    if !source_metadata.is_file() {
        return Err(ImportError::NotAFile(source.to_path_buf()));
    }

    let extension = tree::extension(source).unwrap_or_default();
    match extension.as_str() {
        "md" | "markdown" | "mdown" => import_markdown(source, workspace_root, destination_dir),
        "html" | "htm" => import_html(source, workspace_root, destination_dir),
        _ => Err(ImportError::Unsupported(source.to_path_buf())),
    }
}

/// Copies a Markdown document into the workspace, or returns it when it is
/// already inside.
pub fn import_markdown(
    source: &Path,
    workspace_root: &Path,
    destination_dir: &Path,
) -> Result<ImportResult, ImportError> {
    let root = workspace_root.canonicalize().map_err(ImportError::Io)?;
    let source_canonical = source.canonicalize().map_err(ImportError::Io)?;

    if source_canonical.starts_with(&root) {
        return Ok(ImportResult {
            path: source_canonical,
            converted_from_html: false,
            copied: false,
        });
    }

    let bytes = std::fs::read(&source_canonical).map_err(ImportError::Io)?;
    encoding::decode(&bytes)?;

    let destination = unique_destination(&root, destination_dir, source, false)?;
    atomic::atomic_write(&destination, &bytes).map_err(ImportError::Write)?;

    Ok(ImportResult {
        path: destination,
        converted_from_html: false,
        copied: true,
    })
}

/// Converts an HTML document to Markdown and writes it into the workspace.
pub fn import_html(
    source: &Path,
    workspace_root: &Path,
    destination_dir: &Path,
) -> Result<ImportResult, ImportError> {
    let root = workspace_root.canonicalize().map_err(ImportError::Io)?;
    let source_canonical = source.canonicalize().map_err(ImportError::Io)?;

    let bytes = std::fs::read(&source_canonical).map_err(ImportError::Io)?;
    let decoded = encoding::decode(&bytes)?;

    if decoded.text.trim().is_empty() {
        return Err(ImportError::InvalidHtml(
            "the HTML document is empty".to_string(),
        ));
    }

    let markdown = html2md::parse_html(&decoded.text);
    if markdown.trim().is_empty() {
        return Err(ImportError::InvalidHtml(
            "no readable content was found".to_string(),
        ));
    }

    let destination = unique_destination(&root, destination_dir, source, true)?;
    atomic::atomic_write_text(&destination, &markdown).map_err(ImportError::Write)?;

    Ok(ImportResult {
        path: destination,
        converted_from_html: true,
        copied: true,
    })
}

fn unique_destination(
    root: &Path,
    destination_dir: &Path,
    source: &Path,
    force_markdown: bool,
) -> Result<PathBuf, ImportError> {
    let destination_dir = destination_dir.canonicalize().map_err(ImportError::Io)?;

    if !destination_dir.starts_with(root) || !destination_dir.is_dir() {
        return Err(ImportError::OutsideWorkspace(destination_dir));
    }

    let stem = source.file_stem().map_or_else(
        || String::from("imported"),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let stem = sanitize_name(&stem);
    let extension = if force_markdown {
        "md".to_string()
    } else {
        tree::extension(source).unwrap_or_else(|| String::from("md"))
    };

    let mut candidate = destination_dir.join(format!("{stem}.{extension}"));
    let mut suffix = 1;
    while candidate.exists() {
        candidate = destination_dir.join(format!("{stem}-{suffix}.{extension}"));
        suffix += 1;
    }

    // Reuse the same name validation as file operations.
    ops::validate_name(
        candidate
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
    )
    .map_err(|error| ImportError::InvalidName(error.to_string()))?;

    Ok(candidate)
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '-'
            } else {
                ch
            }
        })
        .collect();

    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        String::from("imported")
    } else {
        cleaned.to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("import failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("not a file: {0}")]
    NotAFile(PathBuf),
    #[error("unsupported import type: {0}")]
    Unsupported(PathBuf),
    #[error("destination is outside the workspace: {0}")]
    OutsideWorkspace(PathBuf),
    #[error("invalid HTML: {0}")]
    InvalidHtml(String),
    #[error("invalid import name: {0}")]
    InvalidName(String),
    #[error("document cannot be decoded: {0}")]
    Decode(#[from] encoding::DecodeError),
    #[error("import write failed: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn imports_markdown_into_the_workspace() {
        let source_dir = tempfile::tempdir().expect("source");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = source_dir.path().join("chapter.md");
        fs::write(&source, "# Chapter\r\n\r\nText\r\n").expect("fixture");

        let result = import(&source, workspace.path(), workspace.path()).expect("import");

        assert!(result.copied);
        assert!(!result.converted_from_html);
        assert_eq!(result.path.file_name().unwrap(), "chapter.md");
        assert_eq!(
            fs::read(result.path).expect("read"),
            b"# Chapter\r\n\r\nText\r\n"
        );
    }

    #[test]
    fn importing_a_workspace_document_does_not_duplicate_it() {
        let workspace = tempfile::tempdir().expect("workspace");
        let source = workspace.path().join("note.md");
        fs::write(&source, "# Note").expect("fixture");

        let result = import(&source, workspace.path(), workspace.path()).expect("import");

        assert!(!result.copied);
        assert_eq!(result.path, source.canonicalize().expect("canonical"));
    }

    #[test]
    fn duplicate_names_get_a_suffix() {
        let source_dir = tempfile::tempdir().expect("source");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = source_dir.path().join("note.md");
        fs::write(&source, "one").expect("first");
        fs::write(workspace.path().join("note.md"), "existing").expect("existing");

        let result = import(&source, workspace.path(), workspace.path()).expect("import");

        assert_eq!(result.path.file_name().unwrap(), "note-1.md");
    }

    #[test]
    fn converts_html_to_markdown() {
        let source_dir = tempfile::tempdir().expect("source");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = source_dir.path().join("page.html");
        fs::write(
            &source,
            "<html><body><h1>Title</h1><p>Hello <strong>world</strong></p></body></html>",
        )
        .expect("fixture");

        let result = import(&source, workspace.path(), workspace.path()).expect("import");
        let markdown = fs::read_to_string(&result.path).expect("read");

        assert!(result.converted_from_html);
        assert_eq!(result.path.extension().unwrap(), "md");
        assert!(markdown.contains("Title"));
        assert!(markdown.contains("**world**"));
    }

    #[test]
    fn failed_html_import_leaves_the_workspace_unchanged() {
        let source_dir = tempfile::tempdir().expect("source");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = source_dir.path().join("empty.html");
        fs::write(&source, "   \n").expect("fixture");

        let error = import(&source, workspace.path(), workspace.path()).expect_err("must fail");

        assert!(matches!(error, ImportError::InvalidHtml(_)));
        assert_eq!(fs::read_dir(workspace.path()).expect("read dir").count(), 0);
    }

    #[test]
    fn rejects_unsupported_imports() {
        let source_dir = tempfile::tempdir().expect("source");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = source_dir.path().join("image.png");
        fs::write(&source, b"png").expect("fixture");

        assert!(matches!(
            import(&source, workspace.path(), workspace.path()),
            Err(ImportError::Unsupported(_))
        ));
    }
}
