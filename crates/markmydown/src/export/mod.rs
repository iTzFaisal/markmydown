//! Markdown, standalone HTML, PDF, clipboard, and print export.

pub mod clipboard;
pub mod html;
pub mod markdown;
pub mod pdf;
pub mod print;

use crate::workspace::atomic;

/// Shared export failure type. Every failure leaves the document and app open.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("could not write export: {0}")]
    Write(#[source] atomic::AtomicError),
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
