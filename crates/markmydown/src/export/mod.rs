//! Markdown, standalone HTML, PDF, clipboard, and print export.

pub mod clipboard;
pub mod html;
pub mod markdown;
pub mod pdf;
pub mod print;

use std::path::{Path, PathBuf};

use crate::preview::model::PreviewDocument;
use crate::theme::tokens::{ThemeTokens, Typography};
use crate::workspace::atomic;

/// The export formats offered by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Html,
    Pdf,
}

impl ExportFormat {
    pub const ALL: [Self; 3] = [Self::Markdown, Self::Html, Self::Pdf];

    /// Human-readable format name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::Pdf => "PDF",
        }
    }

    /// Label for the toolbar action.
    pub fn action_label(self) -> &'static str {
        match self {
            Self::Markdown => "Export .md",
            Self::Html => "Export HTML",
            Self::Pdf => "Export PDF",
        }
    }

    /// File extension without the leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }

    pub fn filter_name(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::Pdf => "PDF",
        }
    }
}

/// Everything needed to render and write one export.
///
/// The request owns its inputs so it can be moved into an asynchronous save
/// dialog without holding a borrow of the application state.
#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub format: ExportFormat,
    /// The Markdown source exactly as it is in the editor buffer.
    pub source: String,
    /// The preview model built from `source`.
    pub preview: PreviewDocument,
    /// Directory used to resolve relative image paths.
    pub base_dir: Option<PathBuf>,
    pub title: String,
    pub theme: ThemeTokens,
    pub typography: Typography,
    pub pdf: pdf::PdfOptions,
}

impl ExportRequest {
    /// File name suggested in the save dialog.
    pub fn suggested_name(&self) -> String {
        markdown::suggested_name_with_extension(&self.title, self.format.extension())
    }

    /// HTML options borrowed from this request.
    pub fn html_options(&self) -> html::HtmlOptions<'_> {
        html::HtmlOptions {
            theme: &self.theme,
            typography: &self.typography,
            base_dir: self.base_dir.as_deref(),
            title: &self.title,
            embed_images: true,
        }
    }

    /// Writes the export to `path` and returns the final path.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<PathBuf, ExportError> {
        match self.format {
            ExportFormat::Markdown => {
                markdown::export(path, &self.source).map_err(ExportError::Markdown)
            }
            ExportFormat::Html => {
                let options = self.html_options();
                html::export(path, &self.preview, &options)
            }
            ExportFormat::Pdf => pdf::export(
                path,
                &self.preview,
                &self.pdf,
                &self.theme,
                &self.typography,
                self.base_dir.as_deref(),
            ),
        }
    }
}

/// Result of a completed export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    pub format: ExportFormat,
    pub path: PathBuf,
}

/// A failed export, including the format so the UI can offer a retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFailure {
    pub format: ExportFormat,
    pub message: String,
}

/// Applies a save-dialog choice to a request.
///
/// A `None` choice means the user cancelled; no file is touched. This is kept
/// separate from the dialog itself so the cancel and failure paths are
/// testable without a GUI.
pub fn export_to_choice(
    request: &ExportRequest,
    path: Option<PathBuf>,
) -> Result<Option<ExportOutcome>, ExportFailure> {
    let Some(path) = path else {
        return Ok(None);
    };

    request
        .write_to(&path)
        .map(|path| {
            Some(ExportOutcome {
                format: request.format,
                path,
            })
        })
        .map_err(|error| ExportFailure {
            format: request.format,
            message: error.to_string(),
        })
}

/// Shared export failure type. Every failure leaves the document and app open.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not write export: {0}")]
    Write(#[source] atomic::AtomicError),
    #[error("{0}")]
    Markdown(#[from] markdown::MarkdownExportError),
    #[error("HTML template failed: {0}")]
    Template(String),
    #[error("PDF export failed: {0}")]
    Pdf(String),
    #[error("clipboard operation failed: {0}")]
    Clipboard(String),
    #[error("print failed: {0}")]
    Print(String),
    #[error("export destination is not writable: {0}")]
    Destination(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::parser;
    use crate::theme::tokens::{ThemeTokens, Typography};

    fn request(format: ExportFormat) -> ExportRequest {
        ExportRequest {
            format,
            source: String::from("# Title\n\nBody with **bold** text.\n"),
            preview: parser::parse("# Title\n\nBody with **bold** text.\n", 0),
            base_dir: None,
            title: String::from("My Post"),
            theme: ThemeTokens::light(),
            typography: Typography::preview_default(),
            pdf: pdf::PdfOptions::default(),
        }
    }

    #[test]
    fn suggested_names_use_the_selected_extension() {
        assert_eq!(
            request(ExportFormat::Markdown).suggested_name(),
            "My Post.md"
        );
        assert_eq!(request(ExportFormat::Html).suggested_name(), "My Post.html");
        assert_eq!(request(ExportFormat::Pdf).suggested_name(), "My Post.pdf");
    }

    #[test]
    fn cancelling_writes_no_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("document.html");

        let outcome = export_to_choice(&request(ExportFormat::Html), None).expect("cancelled");

        assert!(outcome.is_none());
        assert!(!target.exists());
    }

    #[test]
    fn each_format_writes_its_own_file() {
        let tmp = tempfile::tempdir().expect("tempdir");

        for (format, expected_extension) in [
            (ExportFormat::Markdown, "md"),
            (ExportFormat::Html, "html"),
            (ExportFormat::Pdf, "pdf"),
        ] {
            let target = tmp.path().join(format!("document.{}", format.extension()));
            let outcome = export_to_choice(&request(format), Some(target.clone()))
                .expect("export")
                .expect("outcome");

            assert_eq!(outcome.format, format);
            assert_eq!(outcome.path, target);
            assert_eq!(
                outcome.path.extension().and_then(|ext| ext.to_str()),
                Some(expected_extension)
            );
            assert!(target.is_file());
        }
    }

    #[test]
    fn a_missing_extension_is_appended() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("document");

        let outcome = export_to_choice(&request(ExportFormat::Markdown), Some(target))
            .expect("export")
            .expect("outcome");

        assert_eq!(outcome.path, tmp.path().join("document.md"));
        assert_eq!(
            std::fs::read_to_string(&outcome.path).expect("read"),
            "# Title\n\nBody with **bold** text.\n"
        );
    }

    #[test]
    fn an_unwritable_destination_is_reported_without_losing_the_request() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("missing-folder").join("document.html");
        let request = request(ExportFormat::Html);

        let failure = export_to_choice(&request, Some(target)).expect_err("failure");

        assert_eq!(failure.format, ExportFormat::Html);
        assert!(!failure.message.is_empty());
        assert_eq!(request.source, "# Title\n\nBody with **bold** text.\n");
    }

    #[test]
    fn preview_html_and_pdf_share_the_same_content() {
        use pdfrs::elements::Element;

        let source = "# Title\n\nParagraph with a [link](https://example.com) and **bold** text.\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n$$\nx^2 + y^2\n$$\n\n```mermaid\nflowchart TD\n    A --> B\n```\n";
        let document = parser::parse(source, 0);
        let export = ExportRequest {
            preview: document.clone(),
            ..request(ExportFormat::Html)
        };
        let html = html::render_fragment(&document, &export.html_options());
        let tmp = tempfile::tempdir().expect("tempdir");
        let elements = pdf::build_elements(&document, tmp.path());
        let pdf_structure = format!("{elements:?}");

        for expected in [
            "Title",
            "Paragraph with a",
            "link",
            "https://example.com",
            "one",
            "two",
        ] {
            assert!(html.contains(expected), "HTML is missing {expected}");
            assert!(
                pdf_structure.contains(expected),
                "PDF elements are missing {expected}"
            );
        }

        assert!(html.contains("language-rust"));
        assert!(html.contains("mmd-math"));
        assert!(pdf_structure.contains("fn main() {}"));
        assert!(pdf_structure.contains("x^2 + y^2"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(
            elements
                .iter()
                .any(|element| matches!(element, Element::MathBlock { .. }))
        );
        assert!(
            elements
                .iter()
                .any(|element| matches!(element, Element::Image { .. }))
        );
    }
}
