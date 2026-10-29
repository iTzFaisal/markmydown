//! One-megabyte performance benchmark.
//!
//! Run with:
//!
//! ```text
//! cargo test -p markmydown --release --test performance_spec -- --ignored --nocapture
//! ```

use std::fs;
use std::time::Instant;

use iced_code_editor::bench_support::IncrementalEditBenchmark;
use markmydown::export::html::{self, HtmlOptions};
use markmydown::export::pdf::{self, PdfOptions};
use markmydown::theme::tokens::{ThemeTokens, Typography};
use markmydown::workspace::state::WorkspaceState;

fn one_megabyte_source() -> String {
    let mut source = String::with_capacity(1_100_000);
    let mut index = 0;
    while source.len() < 1_000_000 {
        source.push_str(&format!(
            "# Section {index}\n\n\
             A paragraph with **bold**, *italic*, and a [link](https://example.com).\n\n\
             | Column A | Column B |\n|---|---|\n| one | two |\n\n\
             ```rust\nfn section_{index}() -> usize {{ {index} }}\n```\n\n"
        ));
        index += 1;
    }
    source
}

#[test]
#[ignore = "performance benchmark; run with --release --ignored"]
fn benchmark_one_megabyte_document() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("large");
    fs::create_dir_all(&root).expect("root");
    let path = root.join("large.md");
    let source = one_megabyte_source();
    fs::write(&path, &source).expect("write fixture");

    let start = Instant::now();
    let mut workspace =
        WorkspaceState::new(tmp.path().join("session.json"), tmp.path().join("recovery"));
    workspace.open_folder(&root).expect("open folder");
    let id = workspace.open_document(&path).expect("open document");
    let open_elapsed = start.elapsed();
    println!("open 1 MB document: {open_elapsed:?}");

    let start = Instant::now();
    workspace
        .document_mut(id)
        .expect("document")
        .refresh_preview();
    let preview_elapsed = start.elapsed();
    println!("preview update: {preview_elapsed:?}");

    let mut editor = IncrementalEditBenchmark::new(&source, 100, 0);
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        editor.insert_and_backspace();
    }
    let keystroke_elapsed = start.elapsed() / (iterations * 2);
    println!("keystroke: {keystroke_elapsed:?}");

    let preview = workspace
        .document(id)
        .expect("document")
        .preview
        .clone()
        .expect("preview");
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();

    let start = Instant::now();
    html::export(
        tmp.path().join("large.html"),
        &preview,
        &HtmlOptions {
            theme: &theme,
            typography: &typography,
            base_dir: Some(&root),
            title: "Large",
            embed_images: true,
        },
    )
    .expect("html");
    println!("HTML export: {:?}", start.elapsed());

    let start = Instant::now();
    pdf::export(
        tmp.path().join("large.pdf"),
        &preview,
        &PdfOptions::default(),
        &theme,
        &typography,
        Some(&root),
    )
    .expect("pdf");
    println!("PDF export: {:?}", start.elapsed());

    assert!(
        open_elapsed < std::time::Duration::from_millis(200),
        "opening a 1 MB document took {open_elapsed:?}"
    );
    assert!(
        preview_elapsed < std::time::Duration::from_millis(80),
        "the preview update took {preview_elapsed:?}"
    );
    assert!(
        keystroke_elapsed < std::time::Duration::from_millis(16),
        "a keystroke took {keystroke_elapsed:?}"
    );
}
