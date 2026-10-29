//! Print using the PDF layout.
//!
//! Printing generates the same PDF layout as PDF export and hands it to the
//! operating system's print flow. The document is never modified if the user
//! cancels in the system dialog.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::ExportError;
use super::pdf::{self, PdfOptions};
use crate::preview::model::PreviewDocument;
use crate::theme::tokens::{ThemeTokens, Typography};

/// Generates the PDF layout for printing without opening the system dialog.
///
/// The file is written to the temporary directory; it is safe to delete once
/// the print flow has consumed it.
pub fn prepare_print(
    document: &PreviewDocument,
    options: &PdfOptions,
    theme: &ThemeTokens,
    typography: &Typography,
    base_dir: Option<&Path>,
) -> Result<PathBuf, ExportError> {
    let path = std::env::temp_dir().join(format!(
        "markmydown-print-{}-{}.pdf",
        std::process::id(),
        unique_suffix()
    ));
    let bytes = pdf::export_bytes(document, options, theme, typography, base_dir)?;
    std::fs::write(&path, bytes).map_err(|error| ExportError::Print(error.to_string()))?;
    Ok(path)
}

/// Generates a temporary PDF and opens the system print flow.
pub fn print_document(
    document: &PreviewDocument,
    options: &PdfOptions,
    theme: &ThemeTokens,
    typography: &Typography,
    base_dir: Option<&Path>,
) -> Result<PathBuf, ExportError> {
    let path = prepare_print(document, options, theme, typography, base_dir)?;
    print_pdf(&path)?;
    Ok(path)
}

/// Opens the print dialog for an existing PDF.
pub fn print_pdf(path: &Path) -> Result<(), ExportError> {
    if !path.is_file() {
        return Err(ExportError::Print(format!(
            "print file does not exist: {}",
            path.display()
        )));
    }

    let mut command = print_command(path);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| ExportError::Print(error.to_string()))
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn print_command(path: &Path) -> Command {
    // Preview is the standard macOS PDF viewer and exposes the system print
    // panel for the document. Cancelling there leaves no print job behind.
    let mut command = Command::new("open");
    command.arg("-a").arg("Preview").arg(path);
    command
}

#[cfg(target_os = "windows")]
fn print_command(path: &Path) -> Command {
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-Command"]).arg(format!(
        "Start-Process -FilePath '{}' -Verb Print",
        path.display()
    ));
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn print_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::parser;

    #[test]
    fn missing_print_file_is_an_error() {
        let error = print_pdf(Path::new("/definitely/not/here.pdf")).expect_err("missing");

        assert!(matches!(error, ExportError::Print(_)));
    }

    #[test]
    fn the_print_layout_is_a_real_pdf() {
        let document = parser::parse("# Print me\n\nBody text.\n", 0);
        let path = prepare_print(
            &document,
            &PdfOptions::default(),
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("prepare");
        let bytes = std::fs::read(&path).expect("read");

        assert!(bytes.starts_with(b"%PDF"));
        assert!(path.to_string_lossy().contains("markmydown-print-"));
        let _ = std::fs::remove_file(path);
    }
}
