//! Print using the PDF layout.
//!
//! Printing generates the same PDF layout and hands it to the operating system.
//! The document is never modified if the user cancels in the system dialog.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::ExportError;
use super::pdf::{self, PdfOptions};
use crate::preview::model::PreviewDocument;
use crate::theme::tokens::{ThemeTokens, Typography};

/// Generates a temporary PDF and opens the system print flow.
pub fn print_document(
    document: &PreviewDocument,
    options: &PdfOptions,
    theme: &ThemeTokens,
    typography: &Typography,
    base_dir: Option<&Path>,
) -> Result<PathBuf, ExportError> {
    let path = std::env::temp_dir().join(format!("markmydown-print-{}.pdf", std::process::id()));
    let bytes = pdf::export_bytes(document, options, theme, typography, base_dir)?;
    std::fs::write(&path, bytes).map_err(|error| ExportError::Print(error.to_string()))?;
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

#[cfg(target_os = "macos")]
fn print_command(path: &Path) -> Command {
    // Preview's print dialog is the standard macOS flow. `open -a Preview`
    // is used instead of a shell command so paths with spaces are safe.
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

    #[test]
    fn missing_print_file_is_an_error() {
        let error = print_pdf(Path::new("/definitely/not/here.pdf")).expect_err("missing");

        assert!(matches!(error, ExportError::Print(_)));
    }
}
