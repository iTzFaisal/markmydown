//! End-to-end smoke coverage for the release workflow.
//!
//! The test exercises the same workspace, preview, export, session, and
//! recovery modules the desktop app uses, without opening a window.

use std::fs;

use markmydown::export::html::{self, HtmlOptions};
use markmydown::export::pdf::{self, PdfOptions};
use markmydown::preview::model::Block;
use markmydown::theme::tokens::{ThemeTokens, Typography};
use markmydown::workspace::document::Cursor;
use markmydown::workspace::session::Session;
use markmydown::workspace::state::WorkspaceState;

const DOCUMENT: &str = "\
---
title: Smoke test
---

# Smoke test

A paragraph with a [link](https://example.com) and **bold** text.

| Step | Result |
|:-----|-------:|
| Open | pass |

```rust
fn main() {
    println!(\"smoke\");
}
```

$$
E = mc^2
$$

```mermaid
flowchart LR
    Edit --> Preview
    Preview --> Export
```
";

#[test]
fn smoke_test_covers_open_edit_preview_export_and_restore() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("notes");
    fs::create_dir_all(&root).expect("notes");
    let path = root.join("smoke.md");
    fs::write(&path, DOCUMENT).expect("fixture");

    let session_path = tmp.path().join("session.json");
    let recovery_dir = tmp.path().join("recovery");
    let mut workspace = WorkspaceState::new(&session_path, &recovery_dir);
    workspace.open_folder(&root).expect("open folder");
    let id = workspace.open_document(&path).expect("open document");

    workspace.set_document_content(
        id,
        format!("{DOCUMENT}\nEdited during the smoke test.\n"),
        Cursor::new(10, 4),
        32.0,
    );
    workspace.save(id).expect("save edit");

    workspace
        .document_mut(id)
        .expect("document")
        .refresh_preview();
    let preview = workspace
        .document(id)
        .expect("document")
        .preview
        .clone()
        .expect("preview");

    assert!(
        preview
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Table { .. }))
    );
    assert!(
        preview
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Math { .. }))
    );
    assert!(
        preview
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Mermaid { .. }))
    );
    assert!(
        preview
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Code { .. }))
    );

    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html_path = tmp.path().join("smoke.html");
    html::export(
        &html_path,
        &preview,
        &HtmlOptions {
            theme: &theme,
            typography: &typography,
            base_dir: Some(&root),
            title: "Smoke test",
            embed_images: true,
        },
    )
    .expect("export HTML");
    let html_text = fs::read_to_string(&html_path).expect("read HTML");
    assert!(html_text.contains("Smoke test"));
    assert!(html_text.contains("<table>"));
    assert!(html_text.contains("mmd-math"));
    assert!(html_text.contains("mmd-mermaid"));

    let pdf_path = tmp.path().join("smoke.pdf");
    pdf::export(
        &pdf_path,
        &preview,
        &PdfOptions::default(),
        &theme,
        &typography,
        Some(&root),
    )
    .expect("export PDF");
    assert!(fs::read(&pdf_path).expect("read PDF").starts_with(b"%PDF"));

    workspace.save_session().expect("save session");
    let loaded = Session::load(&session_path);
    let mut restored = WorkspaceState::new(
        tmp.path().join("session-restored.json"),
        tmp.path().join("recovery-restored"),
    );
    let report = restored.restore_session(&loaded).expect("restore session");
    assert_eq!(report.tabs_opened, 1);
    let restored_document = restored.document(id).expect("restored document");
    assert_eq!(restored_document.cursor, Cursor::new(10, 4));
    assert_eq!(restored_document.scroll, 32.0);

    restored.set_document_content(id, "# Unsaved recovery\n", Cursor::default(), 0.0);
    restored.write_recovery().expect("write recovery");
    let next_launch = WorkspaceState::new(
        tmp.path().join("session-next.json"),
        tmp.path().join("recovery-restored"),
    );
    let pending = next_launch.pending_recovery();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].content.contains("Unsaved recovery"));
}
