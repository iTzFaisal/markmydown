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

/// Renders the document to HTML and copies it with a plain-text fallback.
pub fn copy_document(
    document: &PreviewDocument,
    options: &HtmlOptions<'_>,
) -> Result<(), ExportError> {
    let html = html::render_document(document, options)?;
    let plain = document.plain_text();
    copy_html(&html, &plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_always_available_as_a_fallback() {
        let document = PreviewDocument::default();
        assert_eq!(document.plain_text(), "");
    }
}
