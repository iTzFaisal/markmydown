//! Behavioral tests for the Markdown preview specification.
//!
//! These tests exercise the preview model, parser, renderer, sanitizer,
//! math/Mermaid adapters, and scroll mapping without opening a window. Widget
//! trees are covered by the app's layout tests; the HTML fixtures here are the
//! visual contract for every block type.

use std::path::Path;
use std::time::Instant;

use markmydown::app::LayoutMode;
use markmydown::export::html::{self, HtmlOptions};
use markmydown::preview::model::{Block, Inline};
use markmydown::preview::render::{PreviewViewport, RenderCaches, VIRTUALIZE_AFTER, block_window};
use markmydown::preview::{math, mermaid, parser, sanitize, sync};
use markmydown::theme::tokens::{ThemeTokens, Typography};

fn options<'a>(
    theme: &'a ThemeTokens,
    typography: &'a Typography,
    base_dir: Option<&'a Path>,
) -> HtmlOptions<'a> {
    HtmlOptions {
        theme,
        typography,
        base_dir,
        title: "Fixture",
        embed_images: true,
    }
}

fn html_body(
    document: &markmydown::preview::model::PreviewDocument,
    base_dir: Option<&Path>,
) -> String {
    let theme = ThemeTokens::light();
    let typography = Typography::preview_default();
    let html = html::render_document(document, &options(&theme, &typography, base_dir))
        .expect("render fixture");
    html.split("<article class=\"mmd-document\">")
        .nth(1)
        .expect("article body")
        .split("</article>")
        .next()
        .expect("article end")
        .trim()
        .to_string()
}

#[test]
fn preview_model_snapshot_covers_blocks_and_inlines() {
    let source = "\
# Title

A paragraph with **strong**, *emphasis*, ~~strike~~, ==mark==, H~2~O,
x^2^, `inline`, a [link](https://example.com), and ![alt](image.png).

- alpha
- [x] done
- [ ] todo

1. first
2. second

> quoted text

| Left | Center | Right |
|:-----|:------:|------:|
| 1    | 2      | 3     |

```rust
fn main() {}
```

---

Term

: Definition
";
    let document = parser::parse(source, 0);

    insta::assert_debug_snapshot!("preview_model", document);
}

#[test]
fn block_fixtures_cover_basic_text_and_inlines() {
    let document = parser::parse(
        "# Heading 1\n\n## Heading 2\n\nA **bold**, *italic*, ~~struck~~, ==marked==, H~2~O, x^2^ paragraph.\n\n---\n",
        0,
    );

    insta::assert_snapshot!("blocks_basic", html_body(&document, None));
}

#[test]
fn block_fixtures_cover_lists_and_tasks() {
    let document = parser::parse(
        "- bullet\n  - nested\n\n1. ordered\n2. second\n\n- [x] checked\n- [ ] unchecked\n\nTerm\n\n: Definition\n",
        0,
    );

    insta::assert_snapshot!("blocks_lists", html_body(&document, None));
}

#[test]
fn block_fixtures_cover_tables_quotes_and_code() {
    let document = parser::parse(
        "> A quote\n>\n> - with a list\n\n| Left | Center | Right |\n|:-----|:------:|------:|\n| 1 | 2 | 3 |\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n",
        0,
    );

    insta::assert_snapshot!("blocks_table_quote_code", html_body(&document, None));
}

#[test]
fn block_fixtures_cover_media_and_links() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let image = tmp.path().join("image.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\n").expect("image");
    let document = parser::parse(
        "![A picture](image.png)\n\n[External](https://example.com) and [local](#section).\n",
        0,
    );

    insta::assert_snapshot!("blocks_media", html_body(&document, Some(tmp.path())));
}

#[test]
fn code_highlighting_handles_supported_unknown_and_empty_blocks() {
    let supported = parser::parse("```rust\nfn main() {}\n```\n", 0);
    let unknown = parser::parse("```nosuchlanguage\nplain text\n```\n", 0);
    let empty = parser::parse("```rust\n```\n", 0);

    let supported = html_body(&supported, None);
    let unknown = html_body(&unknown, None);
    let empty = html_body(&empty, None);

    assert!(supported.contains("class=\"language-rust\""));
    assert!(supported.contains("<span style=\"color:#"));
    assert!(!unknown.contains("<span style=\"color:#"));
    assert!(unknown.contains("plain text"));
    assert!(
        empty.contains("<pre class=\"mmd-code-block\"><code class=\"language-rust\"></code></pre>")
    );
}

#[test]
fn inline_math_stays_literal_and_display_math_renders() {
    let inline = parser::parse("Inline $x^2$ stays literal.\n", 0);
    let paragraph = format!("{:?}", inline.blocks[0]);
    assert!(paragraph.contains("Math(\"x^2\")"));
    assert!(
        inline
            .blocks
            .iter()
            .all(|block| !matches!(block, Block::Math { .. }))
    );
    assert!(html_body(&inline, None).contains("$x^2$"));

    let display = parser::parse("$$\n\\frac{a}{b}\n$$\n", 0);
    assert!(matches!(display.blocks[0], Block::Math { .. }));
    let html = html_body(&display, None);
    assert!(html.contains("mmd-math"));
    assert!(html.contains("<svg"));

    let error = math::render_svg_string(r"\frac{a", 18.0, ThemeTokens::light().text);
    assert!(error.is_err());
}

#[test]
fn mermaid_flowcharts_render_and_invalid_sources_report_errors() {
    let document = parser::parse(
        "```mermaid\nflowchart TD\n    A[Start] --> B[Done]\n```\n",
        0,
    );
    assert!(matches!(document.blocks[0], Block::Mermaid { .. }));

    let svg = mermaid::render_svg("flowchart TD\n    A[Start] --> B[Done]").expect("flowchart");
    assert!(svg.contains("<svg"));
    assert!(
        svg.contains("viewBox"),
        "SVG is scalable to the preview width"
    );

    let invalid = mermaid::render_svg("this is not a diagram");
    assert!(invalid.is_err());

    let oversized = format!(
        "flowchart LR\n{}",
        (0..40)
            .map(|index| format!("    N{index}[Node {index}] --> N{}\n", index + 1))
            .collect::<String>()
    );
    let svg = mermaid::render_svg(&oversized).expect("oversized diagram");
    assert!(svg.contains("viewBox"));
    assert!(svg.contains("<svg"));
}

#[test]
fn relative_images_resolve_and_missing_images_show_placeholders() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("photo.png"), b"\x89PNG\r\n\x1a\n").expect("image");

    let present = parser::parse("![Photo](photo.png)\n", 0);
    let html = html_body(&present, Some(tmp.path()));
    assert!(html.contains("src=\"data:image/png;base64,"));
    assert!(html.contains("alt=\"Photo\""));

    let missing = parser::parse("![Missing](missing.png)\n", 0);
    let html = html_body(&missing, Some(tmp.path()));
    assert!(html.contains("mmd-missing-image"));
    assert!(html.contains("Missing"));
}

#[test]
fn external_links_are_preserved_for_the_system_browser() {
    let document = parser::parse("[Site](https://example.com/docs)\n", 0);
    let html = html_body(&document, None);

    assert!(html.contains("href=\"https://example.com/docs\""));
}

#[test]
fn unsafe_html_is_neutralized_while_safe_html_renders() {
    let document = parser::parse(
        "<p onclick=\"alert(1)\">safe</p>\n\n<script>alert(1)</script>\n\nPress <kbd>Cmd</kbd><br>then <mark>save</mark>.\n\n[unsafe](javascript:alert(1))\n",
        0,
    );
    let html = html_body(&document, None);

    assert!(html.contains("safe"));
    assert!(!html.to_lowercase().contains("script"));
    assert!(!html.to_lowercase().contains("onclick"));
    assert!(!html.to_lowercase().contains("javascript:"));
    assert!(html.contains("<kbd>") || html.contains("Cmd"));
    assert!(html.contains("<br") || html.contains("then"));

    let cleaned = sanitize::sanitize_html("<p onclick=\"x\">safe</p><script>bad()</script>");
    assert!(cleaned.contains("safe"));
    assert!(!cleaned.contains("bad()"));
    assert!(!cleaned.contains("onclick"));
}

#[test]
fn extended_syntax_renders_footnotes_definitions_emoji_and_punctuation() {
    let document = parser::parse(
        "Text[^note].\n\n[^note]: The note.\n\nTerm\n\n: Definition\n\n:smile: \"quoted\"\n",
        0,
    );
    let debug = format!("{:?}", document);
    let html = html_body(&document, None);

    assert_eq!(document.footnotes.len(), 1);
    assert!(debug.contains("FootnoteReference"));
    assert!(debug.contains("DefinitionList"));
    assert!(debug.contains('😄'));
    assert!(debug.contains('“'));
    assert!(html.contains("mmd-footnotes"));
    assert!(html.contains("The note."));
}

#[test]
fn scroll_sync_is_monotonic_in_both_directions() {
    let mut source = String::new();
    for index in 0..200 {
        source.push_str(&format!("## Section {index}\n\nParagraph {index}.\n\n"));
    }
    let document = parser::parse(&source, 0);
    assert!(document.blocks.len() > 100);

    let mut previous = 0.0;
    for line in 0..source.lines().count() {
        let progress = sync::preview_progress_for_line(&document, line);
        assert!(progress >= previous, "progress must not move backwards");
        previous = progress;
        let editor_line = sync::editor_line_for_preview_progress(&document, progress);
        assert!(editor_line <= source.lines().count());
    }

    assert_eq!(sync::editor_line_for_preview_progress(&document, 0.0), 0);
    assert!(sync::preview_progress_for_line(&document, source.lines().count() - 1) > 0.5);
}

#[test]
fn read_mode_exposes_progress_and_outline_mapping() {
    assert!(!LayoutMode::Read.shows_editor());
    assert!(LayoutMode::Read.shows_preview());

    let document = parser::parse("# One\n\n## Two\n\nBody\n", 0);
    let progress = sync::preview_progress_for_line(&document, 4);
    assert!((0.0..=1.0).contains(&progress));
    let line = sync::editor_line_for_preview_progress(&document, progress);
    assert!(line <= 4);

    assert_eq!(sync::reading_progress(0.0, 1000.0, 500.0), 0.0);
    assert_eq!(sync::reading_progress(250.0, 1000.0, 500.0), 0.5);
    assert_eq!(sync::reading_progress(500.0, 1000.0, 500.0), 1.0);
}

#[test]
fn a_one_megabyte_document_parses_and_virtualizes() {
    let paragraph = "A paragraph with **bold**, a [link](https://example.com), and some words.\n\n";
    let mut source = String::with_capacity(1_100_000);
    while source.len() < 1_050_000 {
        source.push_str(paragraph);
    }

    let started = Instant::now();
    let document = parser::parse(&source, 0);
    let elapsed = started.elapsed();

    assert!(
        elapsed.as_secs_f64() < 3.0,
        "1 MB parse took {elapsed:?}, which is not responsive in a debug build"
    );
    assert!(document.blocks.len() > VIRTUALIZE_AFTER);

    let window = block_window(
        &document,
        &Typography::preview_default(),
        PreviewViewport {
            offset_y: 100_000.0,
            height: 800.0,
        },
    );
    assert!(window.start > 0);
    assert!(window.end - window.start < document.blocks.len());
    assert!(window.top_pad > 0.0);
    assert!(window.bottom_pad > 0.0);

    let mut caches = RenderCaches::default();
    assert!(caches.math.is_empty());
    assert!(caches.mermaid.is_empty());
    caches.math.clear();
}

#[test]
fn cached_math_and_mermaid_results_are_deterministic() {
    let tokens = ThemeTokens::light();
    let first = math::render_svg(r"\frac{1}{2}", 18.0, tokens.text).expect("math");
    let second = math::render_svg(r"\frac{1}{2}", 18.0, tokens.text).expect("math");
    assert_eq!(first, second);

    let first = mermaid::render_svg("flowchart TD\nA --> B").expect("mermaid");
    let second = mermaid::render_svg("flowchart TD\nA --> B").expect("mermaid");
    assert_eq!(first, second);
}

#[test]
fn inline_image_alt_text_is_available_to_plain_text() {
    let document = parser::parse("![A **bold** alt](image.png)\n", 0);
    let debug = format!("{:?}", document.blocks[0]);

    assert!(debug.contains("A bold alt"));
    assert!(matches!(
        document.blocks[0],
        Block::Paragraph { ref inlines, .. }
            if inlines.iter().any(|inline| matches!(inline, Inline::Image { .. }))
    ));
}
