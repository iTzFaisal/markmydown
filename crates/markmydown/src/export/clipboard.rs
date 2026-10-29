//! Copy as HTML and plain text.

use super::{ExportError, html, html::HtmlOptions};
use crate::preview::model::PreviewDocument;

/// Copies rich HTML with a plain-text fallback.
pub fn copy_html(html: &str, plain_text: &str) -> Result<(), ExportError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| ExportError::Clipboard(error.to_string()))?;

    if clipboard
        .set()
        .html(html.to_string(), Some(plain_text.to_string()))
        .is_ok()
    {
        return Ok(());
    }

    // HTML clipboard data can be unavailable on some platforms. Fall back to
    // plain text rather than failing the copy action.
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| ExportError::Clipboard(error.to_string()))?;
    clipboard
        .set()
        .text(plain_text.to_string())
        .map_err(|error| ExportError::Clipboard(error.to_string()))
}

/// Copies only plain text.
pub fn copy_text(text: &str) -> Result<(), ExportError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| ExportError::Clipboard(error.to_string()))?;
    clipboard
        .set()
        .text(text.to_string())
        .map_err(|error| ExportError::Clipboard(error.to_string()))
}

/// Builds the rich HTML fragment and plain-text fallback for a document.
pub fn document_payload(document: &PreviewDocument, options: &HtmlOptions<'_>) -> (String, String) {
    (
        html::render_fragment(document, options),
        document.plain_text(),
    )
}

/// Renders the document to an HTML fragment and copies it with a plain-text
/// fallback.
pub fn copy_document(
    document: &PreviewDocument,
    options: &HtmlOptions<'_>,
) -> Result<(), ExportError> {
    let (html, plain) = document_payload(document, options);
    copy_html(&html, &plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::parser;
    use crate::theme::tokens::{ThemeTokens, Typography};

    #[test]
    fn plain_text_is_always_available_as_a_fallback() {
        let document = PreviewDocument::default();
        assert_eq!(document.plain_text(), "");
    }

    #[test]
    fn rich_payload_is_a_fragment_with_semantic_markup() {
        let document = parser::parse(
            "# Title\n\nA [link](https://example.com) and **bold** text.\n\n- one\n- two\n",
            0,
        );
        let (html, plain) = document_payload(
            &document,
            &HtmlOptions {
                theme: &ThemeTokens::light(),
                typography: &Typography::preview_default(),
                base_dir: None,
                title: "Title",
                embed_images: true,
            },
        );

        assert!(!html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<a href=\"https://example.com\">link</a>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<li>"));
        assert!(plain.contains("Title"));
        assert!(plain.contains("A link and bold text."));
    }
}
