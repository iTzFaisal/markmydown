//! PDF export.
//!
//! The preview model is mapped onto `pdfrs` elements. Block math uses pdfrs's
//! display-math layout; Mermaid diagrams are rasterized from their SVG to PNG
//! and embedded as images. Page size and margins come from settings.

use std::path::{Path, PathBuf};

use pdfrs::elements::{Element, TableAlignment as PdfAlignment, TextSegment};
use pdfrs::pdf_generator::{PageLayout, generate_pdf_bytes, generate_pdf_bytes_with_image_base};

use super::ExportError;
use crate::preview::mermaid;
use crate::preview::model::{Alignment as TableAlignment, Block, Inline, PreviewDocument};
use crate::theme::tokens::{ThemeTokens, Typography};
use crate::workspace::atomic;

/// Paper size for PDF export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    #[default]
    Letter,
    A4,
}

impl PageSize {
    /// Parses the value stored in settings. Unknown values fall back to
    /// Letter, matching the settings validator.
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "a4" => Self::A4,
            _ => Self::Letter,
        }
    }

    pub fn as_settings(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::A4 => "a4",
        }
    }
}

/// PDF layout options.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfOptions {
    pub page_size: PageSize,
    pub margin: f32,
    pub font_family: String,
    pub font_size: f32,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize::Letter,
            margin: 72.0,
            font_family: String::from("Helvetica"),
            font_size: 12.0,
        }
    }
}

/// Generates PDF bytes for the document.
pub fn export_bytes(
    document: &PreviewDocument,
    options: &PdfOptions,
    _theme: &ThemeTokens,
    _typography: &Typography,
    base_dir: Option<&Path>,
) -> Result<Vec<u8>, ExportError> {
    let temp = tempfile::tempdir().map_err(|error| ExportError::Pdf(error.to_string()))?;
    let elements = build_elements(document, temp.path());

    let mut layout = match options.page_size {
        PageSize::Letter => PageLayout::portrait(),
        PageSize::A4 => PageLayout {
            width: 595.0,
            height: 842.0,
            ..PageLayout::portrait()
        },
    };
    let margin = options.margin.clamp(18.0, 144.0);
    layout.margin_left = margin;
    layout.margin_right = margin;
    layout.margin_top = margin;
    layout.margin_bottom = margin;

    let font_size = options.font_size.clamp(7.0, 24.0);
    let result = if let Some(base_dir) = base_dir {
        generate_pdf_bytes_with_image_base(
            &elements,
            &options.font_family,
            font_size,
            layout,
            base_dir,
        )
    } else {
        generate_pdf_bytes(&elements, &options.font_family, font_size, layout)
    };

    result.map_err(|error| ExportError::Pdf(error.to_string()))
}

/// Writes a PDF export and returns the destination path.
pub fn export(
    path: impl AsRef<Path>,
    document: &PreviewDocument,
    options: &PdfOptions,
    theme: &ThemeTokens,
    typography: &Typography,
    base_dir: Option<&Path>,
) -> Result<PathBuf, ExportError> {
    let path = ensure_pdf_extension(path.as_ref());
    let bytes = export_bytes(document, options, theme, typography, base_dir)?;
    atomic::atomic_write(&path, &bytes).map_err(ExportError::Write)?;
    Ok(path)
}

fn ensure_pdf_extension(path: &Path) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension("pdf")
    } else {
        path.to_path_buf()
    }
}

pub(crate) fn build_elements(document: &PreviewDocument, temp_dir: &Path) -> Vec<Element> {
    let mut elements = Vec::new();
    for block in &document.blocks {
        append_block(block, &mut elements, temp_dir);
    }

    for footnote in &document.footnotes {
        elements.push(Element::Footnote {
            label: footnote.label.clone(),
            text: footnote
                .blocks
                .iter()
                .map(Block::plain_text)
                .collect::<Vec<_>>()
                .join(" "),
        });
    }

    elements
}

fn append_block(block: &Block, elements: &mut Vec<Element>, temp_dir: &Path) {
    match block {
        Block::Heading { level, inlines, .. } => elements.push(Element::Heading {
            level: *level,
            text: Inline::plain_text(inlines),
        }),
        Block::Paragraph { inlines, .. } => {
            let segments = inline_segments(inlines);
            if segments.is_empty() {
                elements.push(Element::EmptyLine);
            } else {
                elements.push(Element::RichParagraph { segments });
            }
        }
        Block::List {
            ordered,
            start,
            items,
            ..
        } => {
            for (index, item) in items.iter().enumerate() {
                let text = item
                    .blocks
                    .iter()
                    .map(Block::plain_text)
                    .collect::<Vec<_>>()
                    .join(" ");
                if let Some(checked) = item.checked {
                    elements.push(Element::TaskListItem { checked, text });
                } else if *ordered {
                    elements.push(Element::OrderedListItem {
                        number: (*start + index) as u32,
                        text,
                        depth: 0,
                    });
                } else {
                    elements.push(Element::UnorderedListItem { text, depth: 0 });
                }
            }
        }
        Block::DefinitionList { items, .. } => {
            for item in items {
                let term = Inline::plain_text(&item.term);
                for definition in &item.definitions {
                    let text = definition
                        .iter()
                        .map(Block::plain_text)
                        .collect::<Vec<_>>()
                        .join(" ");
                    elements.push(Element::DefinitionItem {
                        term: term.clone(),
                        definition: text,
                    });
                }
            }
        }
        Block::Code { language, code, .. } => {
            elements.push(Element::CodeBlock {
                language: language.clone().unwrap_or_default(),
                code: code.clone(),
            });
        }
        Block::Quote { blocks, .. } => {
            for block in blocks {
                let text = block.plain_text();
                if !text.is_empty() {
                    elements.push(Element::BlockQuote { text, depth: 1 });
                }
            }
        }
        Block::Table {
            alignments,
            header,
            rows,
            ..
        } => {
            let pdf_alignments: Vec<_> = alignments.iter().map(map_alignment).collect();
            if !header.is_empty() {
                elements.push(table_row(header, false, &pdf_alignments));
                elements.push(Element::TableRow {
                    cells: header.iter().map(|_| String::new()).collect(),
                    is_separator: true,
                    alignments: pdf_alignments.clone(),
                    colspans: vec![1; header.len()],
                    rowspans: vec![1; header.len()],
                });
            }
            for row in rows {
                elements.push(table_row(row, false, &pdf_alignments));
            }
        }
        Block::Rule { .. } => elements.push(Element::HorizontalRule),
        Block::Math { source, .. } => {
            elements.push(Element::MathBlock {
                expression: source.clone(),
            });
        }
        Block::Mermaid { source, .. } => match render_mermaid_png(source, temp_dir) {
            Ok(path) => elements.push(Element::Image {
                alt: String::from("Mermaid diagram"),
                path: path.to_string_lossy().into_owned(),
            }),
            Err(error) => {
                elements.push(Element::Paragraph {
                    text: format!("[Mermaid diagram could not be rendered: {error}]\n{source}"),
                });
            }
        },
        Block::Html { html, .. } => {
            elements.push(Element::Paragraph {
                text: strip_html(html),
            });
        }
    }
}

fn table_row(cells: &[Vec<Inline>], is_separator: bool, alignments: &[PdfAlignment]) -> Element {
    Element::TableRow {
        cells: cells.iter().map(|cell| Inline::plain_text(cell)).collect(),
        is_separator,
        alignments: alignments.to_vec(),
        colspans: vec![1; cells.len()],
        rowspans: vec![1; cells.len()],
    }
}

fn inline_segments(inlines: &[Inline]) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => segments.push(TextSegment::Plain(text.clone())),
            Inline::Emphasis(children) | Inline::Highlight(children) => {
                segments.push(TextSegment::Italic(Inline::plain_text(children)));
            }
            Inline::Strong(children) => {
                segments.push(TextSegment::Bold(Inline::plain_text(children)));
            }
            Inline::Strikethrough(children) => {
                segments.push(TextSegment::Strikethrough(Inline::plain_text(children)));
            }
            Inline::Underline(children)
            | Inline::Superscript(children)
            | Inline::Subscript(children) => {
                segments.push(TextSegment::Plain(Inline::plain_text(children)));
            }
            Inline::Code(code) => segments.push(TextSegment::Code(code.clone())),
            Inline::Link { url, children, .. } => segments.push(TextSegment::Link {
                text: Inline::plain_text(children),
                url: url.clone(),
            }),
            Inline::Image { alt, .. } => {
                segments.push(TextSegment::Plain(format!("[Image: {alt}]")));
            }
            Inline::SoftBreak => segments.push(TextSegment::Plain(String::from(" "))),
            Inline::Break => segments.push(TextSegment::Plain(String::from(" "))),
            Inline::FootnoteReference { number, .. } => {
                segments.push(TextSegment::Plain(format!("[{number}]")));
            }
            Inline::Math(source) => {
                segments.push(TextSegment::MathInline(source.clone()));
            }
            Inline::Html(html) => segments.push(TextSegment::Plain(strip_html(html))),
        }
    }
    segments
}

fn map_alignment(alignment: &TableAlignment) -> PdfAlignment {
    match alignment {
        TableAlignment::Left | TableAlignment::None => PdfAlignment::Left,
        TableAlignment::Center => PdfAlignment::Center,
        TableAlignment::Right => PdfAlignment::Right,
    }
}

fn render_mermaid_png(source: &str, temp_dir: &Path) -> Result<PathBuf, String> {
    let svg = mermaid::render_svg(source).map_err(|error| error.to_string())?;
    let png = svg_to_png(&svg)?;
    let path = temp_dir.join(format!(
        "mermaid-{}.png",
        crate::workspace::atomic::hash_bytes(source.as_bytes())
    ));
    std::fs::write(&path, png).map_err(|error| error.to_string())?;
    Ok(path)
}

fn svg_to_png(svg: &str) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|error| error.to_string())?;
    let size = tree.size();
    let max_width = 1200.0;
    let scale = if size.width() > max_width {
        max_width / size.width()
    } else {
        1.0
    };
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| String::from("could not allocate diagram image"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    encode_rgb_png(&pixmap)
}

/// Encodes a pixmap as an RGB PNG with the transparent background composited
/// over white.
///
/// `pdfrs` embeds PNG images with three color components, while `tiny-skia`
/// produces RGBA data. Flattening here keeps diagrams and equations
/// embeddable without adding an image-format dependency.
fn encode_rgb_png(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, String> {
    let width = pixmap.width();
    let height = pixmap.height();
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);

    for pixel in pixmap.data().as_chunks::<4>().0 {
        let alpha = pixel[3] as u32;
        let white = 255 - alpha;
        rgb.push((pixel[0] as u32 + white).min(255) as u8);
        rgb.push((pixel[1] as u32 + white).min(255) as u8);
        rgb.push((pixel[2] as u32 + white).min(255) as u8);
    }

    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&rgb)
            .map_err(|error| error.to_string())?;
    }

    Ok(encoded)
}

fn strip_html(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::parser;

    #[test]
    fn generates_a_pdf_with_content() {
        let document = parser::parse(
            "# Title\n\nText with **bold** and a [link](https://example.com).\n\n- one\n- two\n\n```rust\nfn main() {}\n```\n\n$$\nx^2\n$$\n",
            0,
        );
        let options = PdfOptions::default();
        let bytes = export_bytes(
            &document,
            &options,
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("pdf");

        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 500);
    }

    #[test]
    fn respects_a4_page_size() {
        let document = parser::parse("# A4\n", 0);
        let options = PdfOptions {
            page_size: PageSize::A4,
            margin: 36.0,
            ..PdfOptions::default()
        };
        let bytes = export_bytes(
            &document,
            &options,
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("pdf");

        assert!(bytes.starts_with(b"%PDF"));
        assert!(layout_contains_a4(&bytes));
    }

    fn layout_contains_a4(bytes: &[u8]) -> bool {
        String::from_utf8_lossy(bytes).contains("595")
    }

    #[test]
    fn svg_converts_to_png() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>"#;
        let png = svg_to_png(svg).expect("png");

        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn page_sizes_parse_from_settings() {
        assert_eq!(PageSize::parse("letter"), PageSize::Letter);
        assert_eq!(PageSize::parse("A4"), PageSize::A4);
        assert_eq!(PageSize::parse("a4"), PageSize::A4);
        assert_eq!(PageSize::parse("legal"), PageSize::Letter);
        assert_eq!(PageSize::A4.as_settings(), "a4");
        assert_eq!(PageSize::Letter.as_settings(), "letter");
    }

    #[test]
    fn long_documents_are_not_truncated() {
        let mut source = String::from("# Long document\n\n");
        for index in 0..150 {
            source.push_str(&format!(
                "## Section {index}\n\nParagraph {index} with enough text to wrap across the page.\n\n"
            ));
        }
        let document = parser::parse(&source, 0);
        let bytes = export_bytes(
            &document,
            &PdfOptions::default(),
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("pdf");

        assert!(bytes.starts_with(b"%PDF"));
        assert!(
            bytes.len() > 20_000,
            "a long document should produce a substantial PDF, got {} bytes",
            bytes.len()
        );
    }

    #[test]
    fn margins_change_the_layout() {
        let document = parser::parse(
            "# Margins\n\nA paragraph long enough to wrap when the page margin is large.\n",
            0,
        );
        let narrow = export_bytes(
            &document,
            &PdfOptions {
                margin: 18.0,
                ..PdfOptions::default()
            },
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("pdf");
        let wide = export_bytes(
            &document,
            &PdfOptions {
                margin: 144.0,
                ..PdfOptions::default()
            },
            &ThemeTokens::light(),
            &Typography::preview_default(),
            None,
        )
        .expect("pdf");

        assert_ne!(narrow, wide);
    }

    #[test]
    fn mermaid_diagrams_are_embedded_as_images() {
        let document = parser::parse(
            "```mermaid\nflowchart TD\n    A[Start] --> B[Done]\n```\n",
            0,
        );
        let bytes = export_bytes(
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
            "the diagram should be embedded as an image"
        );
    }
}
