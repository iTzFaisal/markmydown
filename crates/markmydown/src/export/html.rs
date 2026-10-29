//! Standalone HTML export.
//!
//! The exporter renders the shared preview model to semantic HTML with the
//! active theme's CSS, embedded local images, highlighted code, and inline SVG
//! for math and diagrams.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use base64::Engine as _;
use minijinja::{Environment, context};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use super::ExportError;
use crate::preview::math;
use crate::preview::mermaid;
use crate::preview::model::{Alignment as ModelAlignment, Block, Inline, PreviewDocument};
use crate::theme::css;
use crate::theme::tokens::{ThemeTokens, Typography};
use crate::workspace::atomic;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

static TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ title }}</title>
<style>{{ css }}</style>
</head>
<body>
<article class="mmd-document">
{{ body }}
</article>
</body>
</html>
"#;

/// Options for standalone HTML output.
pub struct HtmlOptions<'a> {
    pub theme: &'a ThemeTokens,
    pub typography: &'a Typography,
    pub base_dir: Option<&'a Path>,
    pub title: &'a str,
    /// Embed local images as data URIs.
    pub embed_images: bool,
}

/// Renders just the document body. Used by rich-text copy, where a full HTML
/// document (including `html` and `body` tags) is not appropriate.
pub fn render_fragment(document: &PreviewDocument, options: &HtmlOptions<'_>) -> String {
    let mut body = String::new();

    for block in &document.blocks {
        render_block(block, options, &mut body);
    }

    if !document.footnotes.is_empty() {
        render_footnotes(document, options, &mut body);
    }

    body
}

/// Renders a complete standalone HTML document.
pub fn render_document(
    document: &PreviewDocument,
    options: &HtmlOptions<'_>,
) -> Result<String, ExportError> {
    let body = render_fragment(document, options);
    let css = css::stylesheet(options.theme, options.typography);
    let mut env = Environment::new();
    env.add_template("page", TEMPLATE)
        .map_err(|error| ExportError::Template(error.to_string()))?;
    let template = env
        .get_template("page")
        .map_err(|error| ExportError::Template(error.to_string()))?;
    let title = if options.title.trim().is_empty() {
        "markmydown"
    } else {
        options.title
    };

    template
        .render(context! {
            title => title,
            css => css,
            body => body,
        })
        .map_err(|error| ExportError::Template(error.to_string()))
}

/// Writes a standalone HTML export and returns the destination path.
pub fn export(
    path: impl AsRef<Path>,
    document: &PreviewDocument,
    options: &HtmlOptions<'_>,
) -> Result<PathBuf, ExportError> {
    let path = ensure_html_extension(path.as_ref());
    let html = render_document(document, options)?;
    atomic::atomic_write_text(&path, &html).map_err(ExportError::Write)?;
    Ok(path)
}

fn ensure_html_extension(path: &Path) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension("html")
    } else {
        path.to_path_buf()
    }
}

fn render_block(block: &Block, options: &HtmlOptions<'_>, out: &mut String) {
    match block {
        Block::Heading { level, inlines, .. } => {
            out.push_str(&format!("<h{level}>"));
            render_inlines(inlines, options, out);
            out.push_str(&format!("</h{level}>\n"));
        }
        Block::Paragraph { inlines, .. } => {
            out.push_str("<p>");
            render_inlines(inlines, options, out);
            out.push_str("</p>\n");
        }
        Block::List {
            ordered,
            start,
            items,
            ..
        } => {
            if *ordered {
                out.push_str(&format!("<ol start=\"{}\">\n", (*start).max(1)));
            } else {
                out.push_str("<ul>\n");
            }
            for item in items {
                let class = if item.checked.is_some() {
                    " class=\"task-list-item\""
                } else {
                    ""
                };
                out.push_str(&format!("<li{class}>"));
                if let Some(checked) = item.checked {
                    out.push_str(if checked {
                        "<input type=\"checkbox\" checked disabled> "
                    } else {
                        "<input type=\"checkbox\" disabled> "
                    });
                }
                for block in &item.blocks {
                    render_block(block, options, out);
                }
                out.push_str("</li>\n");
            }
            out.push_str(if *ordered { "</ol>\n" } else { "</ul>\n" });
        }
        Block::DefinitionList { items, .. } => {
            out.push_str("<dl>\n");
            for item in items {
                out.push_str("<dt>");
                render_inlines(&item.term, options, out);
                out.push_str("</dt>\n");
                for definition in &item.definitions {
                    out.push_str("<dd>");
                    for block in definition {
                        render_block(block, options, out);
                    }
                    out.push_str("</dd>\n");
                }
            }
            out.push_str("</dl>\n");
        }
        Block::Code { language, code, .. } => render_code(language.as_deref(), code, options, out),
        Block::Quote { blocks, .. } => {
            out.push_str("<blockquote>\n");
            for block in blocks {
                render_block(block, options, out);
            }
            out.push_str("</blockquote>\n");
        }
        Block::Table {
            alignments,
            header,
            rows,
            ..
        } => render_table(alignments, header, rows, options, out),
        Block::Rule { .. } => out.push_str("<hr>\n"),
        Block::Math { source, .. } => {
            match math::render_svg_string(
                source,
                options.typography.font_size * 1.15,
                options.theme.text,
            ) {
                Ok(svg) => {
                    out.push_str("<div class=\"mmd-math\">");
                    out.push_str(&svg);
                    out.push_str("</div>\n");
                }
                Err(error) => {
                    out.push_str("<div class=\"mmd-math mmd-error\">");
                    push_escaped(out, source);
                    out.push_str(&format!("<div>{}</div>", escape_html(&error.to_string())));
                    out.push_str("</div>\n");
                }
            }
        }
        Block::Mermaid { source, .. } => match mermaid::render_svg(source) {
            Ok(svg) => {
                out.push_str("<div class=\"mmd-mermaid\">");
                out.push_str(&svg);
                out.push_str("</div>\n");
            }
            Err(error) => {
                out.push_str("<div class=\"mmd-mermaid mmd-error\">");
                push_escaped(out, source);
                out.push_str(&format!("<div>{}</div>", escape_html(&error.to_string())));
                out.push_str("</div>\n");
            }
        },
        Block::Html { html, .. } => {
            out.push_str("<div class=\"mmd-html\">");
            out.push_str(html);
            out.push_str("</div>\n");
        }
    }
}

fn render_inlines(inlines: &[Inline], options: &HtmlOptions<'_>, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => push_escaped(out, text),
            Inline::Emphasis(children) => {
                out.push_str("<em>");
                render_inlines(children, options, out);
                out.push_str("</em>");
            }
            Inline::Strong(children) => {
                out.push_str("<strong>");
                render_inlines(children, options, out);
                out.push_str("</strong>");
            }
            Inline::Strikethrough(children) => {
                out.push_str("<del>");
                render_inlines(children, options, out);
                out.push_str("</del>");
            }
            Inline::Highlight(children) => {
                out.push_str("<mark>");
                render_inlines(children, options, out);
                out.push_str("</mark>");
            }
            Inline::Underline(children) => {
                out.push_str("<u>");
                render_inlines(children, options, out);
                out.push_str("</u>");
            }
            Inline::Superscript(children) => {
                out.push_str("<sup>");
                render_inlines(children, options, out);
                out.push_str("</sup>");
            }
            Inline::Subscript(children) => {
                out.push_str("<sub>");
                render_inlines(children, options, out);
                out.push_str("</sub>");
            }
            Inline::Code(code) => {
                out.push_str("<code>");
                push_escaped(out, code);
                out.push_str("</code>");
            }
            Inline::Link { url, children, .. } => {
                out.push_str(&format!("<a href=\"{}\">", escape_html(&safe_link(url))));
                render_inlines(children, options, out);
                out.push_str("</a>");
            }
            Inline::Image { url, alt, .. } => render_image(url, alt, options, out),
            Inline::SoftBreak => out.push(' '),
            Inline::Break => out.push_str("<br>\n"),
            Inline::FootnoteReference { label, number } => {
                out.push_str(&format!(
                    "<sup id=\"fnref-{}\"><a href=\"#fn-{}\">[{}]</a></sup>",
                    escape_html(label),
                    escape_html(label),
                    number
                ));
            }
            Inline::Math(source) => {
                out.push_str("<code class=\"mmd-inline-math\">$");
                push_escaped(out, source);
                out.push_str("$</code>");
            }
            Inline::Html(html) => out.push_str(html),
        }
    }
}

fn render_code(language: Option<&str>, code: &str, options: &HtmlOptions<'_>, out: &mut String) {
    let class = language
        .map(|language| format!(" class=\"language-{}\"", escape_html(language)))
        .unwrap_or_default();
    out.push_str(&format!("<pre class=\"mmd-code-block\"><code{class}>"));

    let highlighted =
        language.and_then(|language| highlight_code(code, language, options.theme.is_dark));
    match highlighted {
        Some(html) => out.push_str(&html),
        None => push_escaped(out, code),
    }

    out.push_str("</code></pre>\n");
}

fn highlight_code(code: &str, language: &str, is_dark: bool) -> Option<String> {
    let syntax = SYNTAXES
        .find_syntax_by_token(language)
        .or_else(|| SYNTAXES.find_syntax_by_name(language))?;
    let theme_name = if is_dark {
        "base16-ocean.dark"
    } else {
        "base16-ocean.light"
    };
    let theme = THEMES.themes.get(theme_name)?;
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = String::new();

    for line in code.split_inclusive('\n') {
        let ranges = highlighter.highlight_line(line, &SYNTAXES).ok()?;
        for (style, value) in ranges {
            if value.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>",
                style.foreground.r,
                style.foreground.g,
                style.foreground.b,
                escape_html(value)
            ));
        }
    }

    Some(out)
}

fn render_table(
    alignments: &[ModelAlignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    options: &HtmlOptions<'_>,
    out: &mut String,
) {
    out.push_str("<table>\n");
    if !header.is_empty() {
        out.push_str("<thead><tr>\n");
        for (index, cell) in header.iter().enumerate() {
            out.push_str(&format!(
                "<th style=\"text-align:{}\">",
                alignment_css(
                    alignments
                        .get(index)
                        .copied()
                        .unwrap_or(ModelAlignment::None)
                )
            ));
            render_inlines(cell, options, out);
            out.push_str("</th>\n");
        }
        out.push_str("</tr></thead>\n");
    }
    out.push_str("<tbody>\n");
    for row in rows {
        out.push_str("<tr>\n");
        for (index, cell) in row.iter().enumerate() {
            out.push_str(&format!(
                "<td style=\"text-align:{}\">",
                alignment_css(
                    alignments
                        .get(index)
                        .copied()
                        .unwrap_or(ModelAlignment::None)
                )
            ));
            render_inlines(cell, options, out);
            out.push_str("</td>\n");
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}

fn alignment_css(alignment: ModelAlignment) -> &'static str {
    match alignment {
        ModelAlignment::Left => "left",
        ModelAlignment::Center => "center",
        ModelAlignment::Right => "right",
        ModelAlignment::None => "left",
    }
}

fn render_image(url: &str, alt: &str, options: &HtmlOptions<'_>, out: &mut String) {
    if let Some(source) = embedded_image_source(url, options.base_dir, options.embed_images) {
        out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\">",
            source,
            escape_html(alt)
        ));
    } else if url.starts_with("http://") || url.starts_with("https://") {
        out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\">",
            escape_html(url),
            escape_html(alt)
        ));
    } else {
        out.push_str(&format!(
            "<span class=\"mmd-missing-image\">Image not found: {}</span>",
            escape_html(alt)
        ));
    }
}

fn embedded_image_source(url: &str, base_dir: Option<&Path>, embed: bool) -> Option<String> {
    if !embed
        || url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("data:")
    {
        return None;
    }

    let path = Path::new(url);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    let bytes = std::fs::read(&path).ok()?;
    let mime = match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

fn render_footnotes(document: &PreviewDocument, options: &HtmlOptions<'_>, out: &mut String) {
    out.push_str("<section class=\"mmd-footnotes\">\n<hr>\n<ol>\n");
    for footnote in &document.footnotes {
        out.push_str(&format!("<li id=\"fn-{}\">", escape_html(&footnote.label)));
        for block in &footnote.blocks {
            render_block(block, options, out);
        }
        out.push_str(&format!(
            " <a href=\"#fnref-{}\">↩</a></li>\n",
            escape_html(&footnote.label)
        ));
    }
    out.push_str("</ol>\n</section>\n");
}

fn safe_link(url: &str) -> String {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:")
    {
        String::from("#")
    } else {
        url.to_string()
    }
}

fn push_escaped(out: &mut String, text: &str) {
    out.push_str(&escape_html(text));
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::parser;
    use crate::theme::tokens::{ThemeTokens, Typography};

    fn options<'a>(base_dir: Option<&'a Path>) -> HtmlOptions<'a> {
        HtmlOptions {
            theme: &MOCK_THEME,
            typography: &MOCK_TYPOGRAPHY,
            base_dir,
            title: "Test",
            embed_images: true,
        }
    }

    static MOCK_THEME: LazyLock<ThemeTokens> = LazyLock::new(ThemeTokens::dark);
    static MOCK_TYPOGRAPHY: LazyLock<Typography> = LazyLock::new(Typography::preview_default);

    #[test]
    fn renders_semantic_blocks_and_theme_css() {
        let document = parser::parse(
            "# Title\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n",
            0,
        );
        let html = render_document(&document, &options(None)).expect("render");

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("language-rust"));
        assert!(html.contains("--mmd-bg"));
        assert!(html.contains(&css::css_color(MOCK_THEME.background)));
    }

    #[test]
    fn unsafe_links_are_neutralized() {
        let document = parser::parse("[click](javascript:alert(1))\n", 0);
        let html = render_document(&document, &options(None)).expect("render");

        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn local_images_are_embedded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let image = tmp.path().join("image.png");
        std::fs::write(&image, b"\x89PNG\r\n\x1a\n").expect("image");
        let document = parser::parse("![Alt](image.png)\n", 0);
        let html = render_document(&document, &options(Some(tmp.path()))).expect("render");

        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("alt=\"Alt\""));
    }

    #[test]
    fn math_and_mermaid_are_embedded_as_svg() {
        let document = parser::parse("$$\nx^2\n$$\n\n```mermaid\nflowchart TD\nA --> B\n```\n", 0);
        let html = render_document(&document, &options(None)).expect("render");

        assert!(html.contains("mmd-math"));
        assert!(html.contains("mmd-mermaid"));
        assert!(html.matches("<svg").count() >= 2);
    }

    #[test]
    fn footnotes_are_rendered() {
        let document = parser::parse("Text[^a].\n\n[^a]: Note.\n", 0);
        let html = render_document(&document, &options(None)).expect("render");

        assert!(html.contains("mmd-footnotes"));
        assert!(html.contains("id=\"fn-a\""));
    }

    #[test]
    fn output_is_standalone_and_has_no_remote_dependencies() {
        let document = parser::parse(
            "# Title\n\nText with a [link](https://example.com).\n\n```mermaid\nflowchart TD\nA --> B\n```\n",
            0,
        );
        let html = render_document(&document, &options(None)).expect("render");

        assert!(html.contains("<style>"));
        assert!(!html.contains("<link rel="));
        assert!(!html.contains("<script"));
        assert!(!html.contains("src=\"http://"));
        assert!(!html.contains("src=\"https://"));
        assert!(html.contains("<svg"));
    }

    #[test]
    fn active_theme_and_typography_drive_the_exported_css() {
        let document = parser::parse("# Title\n", 0);
        let mut typography = Typography::preview_default();
        typography.font_size = 21.0;
        typography.max_width = 640.0;
        let html = render_document(
            &document,
            &HtmlOptions {
                theme: &MOCK_THEME,
                typography: &typography,
                base_dir: None,
                title: "Title",
                embed_images: true,
            },
        )
        .expect("render");

        assert!(html.contains(&css::css_color(MOCK_THEME.background)));
        assert!(html.contains("21px"));
        assert!(html.contains("640px"));
    }
}
