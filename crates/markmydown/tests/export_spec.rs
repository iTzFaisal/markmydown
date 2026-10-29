//! Behavioral tests for the document export specification.
//!
//! These tests exercise the shared preview model, the standalone HTML writer,
//! PDF generation, Markdown passthrough, and export failure handling without
//! opening a window.

use std::path::Path;

use insta::assert_snapshot;
use markmydown::export::pdf::{self, PageSize, PdfOptions};
use markmydown::export::{ExportFormat, ExportRequest, export_to_choice, html};
use markmydown::preview::parser;
use markmydown::theme::tokens::{ThemeTokens, Typography};

const SOURCE: &str = "\
# Field notes

A paragraph with **strong**, *emphasis*, and a [link](https://example.com).

## Checklist

- [x] one
- [ ] two

| Name | Value |
|:-----|------:|
| Alpha | 1 |

> A quote.

```rust
fn main() {}
```
";

fn options<'a>(theme: &'a ThemeTokens, typography: &'a Typography) -> html::HtmlOptions<'a> {
    html::HtmlOptions {
        theme,
        typography,
        base_dir: None,
        title: "Field notes",
        embed_images: true,
    }
}

fn request(format: ExportFormat) -> ExportRequest {
    let document = parser::parse(SOURCE, 0);
    ExportRequest {
        format,
        source: SOURCE.to_string(),
        preview: document,
        base_dir: None,
        title: String::from("Field notes"),
        theme: ThemeTokens::light(),
        typography: Typography::preview_default(),
        pdf: PdfOptions {
            page_size: PageSize::A4,
            margin: 48.0,
            font_family: String::from("Helvetica"),
            font_size: 11.0,
        },
    }
}

#[test]
fn html_export_matches_snapshot() {
    let document = parser::parse(SOURCE, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_fragment(&document, &options(&theme, &typography));

    assert_snapshot!("html_export", html);
}

#[test]
fn standalone_html_contains_styles_and_semantic_content() {
    let document = parser::parse(SOURCE, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html =
        html::render_document(&document, &options(&theme, &typography)).expect("standalone HTML");

    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(html.contains("<h1>Field notes</h1>"));
    assert!(html.contains("<h2>Checklist</h2>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("task-list-item"));
    assert!(html.contains("<blockquote>"));
    assert!(html.contains("language-rust"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("<link rel="));
}

#[test]
fn html_uses_the_active_theme() {
    let document = parser::parse("# Theme\n", 0);

    for tokens in [
        ThemeTokens::light(),
        ThemeTokens::dark(),
        ThemeTokens::sepia(),
    ] {
        let typography = Typography::preview_default();
        let html = html::render_document(&document, &options(&tokens, &typography))
            .expect("standalone HTML");
        assert!(html.contains(&markmydown::theme::css::css_color(tokens.background)));
        assert!(html.contains(&markmydown::theme::css::css_color(tokens.text)));
    }
}

#[test]
fn markdown_export_is_unchanged_source() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("field-notes");

    let outcome = export_to_choice(&request(ExportFormat::Markdown), Some(target))
        .expect("export")
        .expect("outcome");

    assert_eq!(outcome.path, tmp.path().join("field-notes.md"));
    assert_eq!(std::fs::read_to_string(outcome.path).expect("read"), SOURCE);
}

#[test]
fn cancelling_an_export_writes_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("field-notes.html");

    let outcome = export_to_choice(&request(ExportFormat::Html), None).expect("cancel");

    assert!(outcome.is_none());
    assert!(!target.exists());
}

#[test]
fn an_unwritable_destination_reports_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("missing").join("field-notes.pdf");

    let failure = export_to_choice(&request(ExportFormat::Pdf), Some(target)).expect_err("fails");

    assert_eq!(failure.format, ExportFormat::Pdf);
    assert!(!failure.message.is_empty());
}

#[test]
fn pdf_uses_the_configured_page_size_and_margins() {
    let document = parser::parse(SOURCE, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let options = PdfOptions {
        page_size: PageSize::A4,
        margin: 36.0,
        ..PdfOptions::default()
    };

    let bytes = pdf::export_bytes(&document, &options, &theme, &typography, None).expect("pdf");
    let text = String::from_utf8_lossy(&bytes);

    assert!(bytes.starts_with(b"%PDF"));
    assert!(text.contains("595"), "A4 width should be present");
    assert!(bytes.len() > 1_000);
}

#[test]
fn pdf_export_covers_math_and_diagrams() {
    let source = "# Science\n\n$$\nE = mc^2\n$$\n\n```mermaid\nflowchart TD\n    A --> B\n```\n";
    let document = parser::parse(source, 0);
    let bytes = pdf::export_bytes(
        &document,
        &PdfOptions::default(),
        &ThemeTokens::light(),
        &Typography::preview_default(),
        None,
    )
    .expect("pdf");
    let text = String::from_utf8_lossy(&bytes);

    assert!(bytes.starts_with(b"%PDF"));
    assert!(
        text.contains("/Subtype /Image") || text.contains("/Image"),
        "the Mermaid diagram should be embedded as an image"
    );
}

#[test]
fn html_and_pdf_share_the_same_document_text() {
    let document = parser::parse(SOURCE, 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_fragment(&document, &options(&theme, &typography));
    let pdf = pdf::export_bytes(&document, &PdfOptions::default(), &theme, &typography, None)
        .expect("pdf");
    let pdf_text = String::from_utf8_lossy(&pdf);

    for text in ["Field notes", "Checklist", "Alpha", "A quote"] {
        assert!(html.contains(text), "HTML is missing {text}");
        assert!(pdf_text.contains(text), "PDF is missing {text}");
    }
}

#[test]
fn preview_model_and_exports_cover_the_same_block_types() {
    use markmydown::preview::model::Block;

    let source = "# Heading\n\n- item one\n- item two\n\nA [link](https://example.com).\n\n```rust\nfn main() {}\n```\n\n$$\nx^2\n$$\n\n```mermaid\nflowchart TD\n    A --> B\n```\n";
    let document = parser::parse(source, 0);

    for block in ["heading", "list", "link", "code", "math", "mermaid"] {
        let present = match block {
            "heading" => document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Heading { .. })),
            "list" => document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::List { .. })),
            "link" => document.blocks.iter().any(|block| match block {
                Block::Paragraph { inlines, .. } => {
                    format!("{inlines:?}").contains("https://example.com")
                }
                _ => false,
            }),
            "code" => document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Code { .. })),
            "math" => document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Math { .. })),
            "mermaid" => document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Mermaid { .. })),
            _ => false,
        };
        assert!(present, "preview model is missing {block}");
    }

    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_fragment(&document, &options(&theme, &typography));
    assert!(html.contains("<h1>Heading</h1>"));
    assert!(html.contains("<li>"));
    assert!(html.contains("<a href=\"https://example.com\">link</a>"));
    assert!(html.contains("language-rust"));
    assert!(html.contains("mmd-math"));
    assert!(html.contains("mmd-mermaid"));

    let pdf = pdf::export_bytes(&document, &PdfOptions::default(), &theme, &typography, None)
        .expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn images_are_resolved_relative_to_the_document() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let image = tmp.path().join("diagram.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\n").expect("image");
    let document = parser::parse("![Diagram](diagram.png)\n", 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_document(
        &document,
        &html::HtmlOptions {
            theme: &theme,
            typography: &typography,
            base_dir: Some(tmp.path()),
            title: "Diagram",
            embed_images: true,
        },
    )
    .expect("html");

    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("alt=\"Diagram\""));
}

#[test]
fn export_requests_use_a_safe_suggested_name() {
    assert_eq!(
        request(ExportFormat::Markdown).suggested_name(),
        "Field notes.md"
    );
    assert_eq!(
        request(ExportFormat::Html).suggested_name(),
        "Field notes.html"
    );
    assert_eq!(
        request(ExportFormat::Pdf).suggested_name(),
        "Field notes.pdf"
    );
}

#[test]
fn a_missing_image_does_not_break_the_export() {
    let document = parser::parse("![Missing](nope.png)\n", 0);
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_document(
        &document,
        &html::HtmlOptions {
            theme: &theme,
            typography: &typography,
            base_dir: Some(Path::new("/definitely/not/here")),
            title: "Missing",
            embed_images: true,
        },
    )
    .expect("html");

    assert!(html.contains("mmd-missing-image"));
    assert!(html.contains("Missing"));
}
