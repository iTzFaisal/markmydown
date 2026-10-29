//! Iced widget rendering for preview blocks.
//!
//! The renderer is deliberately a pure function of the preview model, theme
//! tokens, typography, and a render cache. The same model and tokens feed HTML
//! and PDF export, which keeps the three surfaces consistent.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use iced::advanced::text::Highlight as SpanHighlight;
use iced::border::Border;
use iced::font::{Style as FontStyle, Weight};
use iced::widget::Id as ScrollableId;
use iced::widget::text::Rich;
use iced::widget::{Space, button, column, container, image, row, rule, scrollable, svg, text};
use iced::{Color, ContentFit, Element, Font, Length, Padding};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use super::cache::ContentCache;
use super::math;
use super::mermaid;
use super::model::{Alignment as TableAlignment, Block, Inline, PreviewDocument};
use super::sanitize;
use crate::theme::tokens::{ThemeTokens, Typography, font_for};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Messages emitted by preview interactions.
#[derive(Debug, Clone)]
pub enum Message {
    /// A link or footnote reference was activated.
    LinkClicked(String),
    /// A code block's copy action was activated.
    CopyCode(String),
    /// The preview viewport scrolled.
    Scrolled(iced::widget::scrollable::Viewport),
}

/// Theme, typography, and base directory for resolving images.
///
/// This is a small `Copy` bundle so it can be constructed for a single view
/// call without extending the lifetime of the returned widgets.
#[derive(Debug, Clone, Copy)]
pub struct PreviewStyles<'a> {
    pub tokens: &'a ThemeTokens,
    pub typography: &'a Typography,
    pub base_dir: Option<&'a Path>,
}

/// Bounded caches for rendered math and Mermaid SVG.
pub struct RenderCaches {
    pub math: ContentCache<Result<Vec<u8>, String>>,
    pub mermaid: ContentCache<Result<String, String>>,
}

/// The last known preview viewport, used to virtualize long documents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewViewport {
    pub offset_y: f32,
    pub height: f32,
}

impl Default for PreviewViewport {
    fn default() -> Self {
        Self {
            offset_y: 0.0,
            height: 800.0,
        }
    }
}

/// A window of blocks to render plus spacers that preserve the scroll height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockWindow {
    pub start: usize,
    pub end: usize,
    pub top_pad: f32,
    pub bottom_pad: f32,
}

/// Documents shorter than this render all blocks at once.
pub const VIRTUALIZE_AFTER: usize = 80;
/// Extra blocks kept above and below the viewport.
pub const OVERSCAN_BLOCKS: usize = 8;
const BLOCK_SPACING: f32 = 12.0;

impl Default for RenderCaches {
    fn default() -> Self {
        Self {
            math: ContentCache::new(128),
            mermaid: ContentCache::new(64),
        }
    }
}

/// Renders the document into a scrollable preview.
///
/// Long documents are virtualized: only the blocks around the viewport are
/// built, while fixed spacers preserve the estimated scroll height.
pub fn view<'a>(
    document: &'a PreviewDocument,
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
    scroll_id: &'a ScrollableId,
    viewport: PreviewViewport,
) -> Element<'a, Message> {
    let window = block_window(document, styles.typography, viewport);

    let mut content = column![].spacing(BLOCK_SPACING).width(Length::Fill);
    if window.top_pad > 0.0 {
        content = content.push(Space::new().height(Length::Fixed(window.top_pad)));
    }

    for block in document
        .blocks
        .get(window.start..window.end)
        .unwrap_or_default()
    {
        content = content.push(render_block(block, styles, caches));
    }

    if window.bottom_pad > 0.0 {
        content = content.push(Space::new().height(Length::Fixed(window.bottom_pad)));
    }

    if window.end == document.blocks.len() && !document.footnotes.is_empty() {
        content = content.push(render_footnotes(document, styles, caches));
    }

    let max_width = if styles.typography.max_width > 0.0 {
        Length::Fixed(styles.typography.max_width)
    } else {
        Length::Fill
    };

    let inner = container(content)
        .max_width(if let Length::Fixed(width) = max_width {
            width
        } else {
            f32::MAX
        })
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .padding([32, 48]);

    scrollable(inner)
        .id(scroll_id.clone())
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(Message::Scrolled)
        .into()
}

/// Chooses the blocks to render for a viewport and estimates the spacers that
/// keep the scrollbar proportional to the whole document.
pub fn block_window(
    document: &PreviewDocument,
    typography: &Typography,
    viewport: PreviewViewport,
) -> BlockWindow {
    let len = document.blocks.len();
    if len <= VIRTUALIZE_AFTER {
        return BlockWindow {
            start: 0,
            end: len,
            top_pad: 0.0,
            bottom_pad: 0.0,
        };
    }

    let heights: Vec<f32> = document
        .blocks
        .iter()
        .map(|block| estimated_block_height(block, typography) + BLOCK_SPACING)
        .collect();
    let total: f32 = heights.iter().sum();
    let offset = viewport.offset_y.clamp(0.0, total.max(0.0));
    let viewport_height = viewport.height.max(1.0);

    let mut cursor = 0.0;
    let mut start = 0;
    while start < len && cursor + heights[start] < offset {
        cursor += heights[start];
        start += 1;
    }

    let mut end = start;
    let mut visible_end = cursor;
    while end < len && visible_end < offset + viewport_height {
        visible_end += heights[end];
        end += 1;
    }

    start = start.saturating_sub(OVERSCAN_BLOCKS);
    end = end.saturating_add(OVERSCAN_BLOCKS).min(len);

    let top_pad: f32 = heights[..start].iter().sum();
    let bottom_pad: f32 = heights[end..].iter().sum();

    BlockWindow {
        start,
        end,
        top_pad,
        bottom_pad,
    }
}

/// Rough height of one block, used only for virtualized scroll geometry.
pub fn estimated_block_height(block: &Block, typography: &Typography) -> f32 {
    let font_size = typography.font_size.max(8.0);
    let line_height = font_size * typography.line_height.max(1.0);
    let width = if typography.max_width > 0.0 {
        typography.max_width
    } else {
        760.0
    };
    let chars_per_line = (width / (font_size * 0.52)).max(20.0);

    match block {
        Block::Heading { level, inlines, .. } => {
            let scale = match level {
                1 => 2.0,
                2 => 1.6,
                3 => 1.3,
                4 => 1.12,
                5 => 1.0,
                _ => 0.95,
            };
            let lines = estimate_lines(&Inline::plain_text(inlines), chars_per_line / scale);
            lines as f32 * font_size * scale * 1.35 + 22.0
        }
        Block::Paragraph { inlines, .. } => {
            let lines = estimate_lines(&Inline::plain_text(inlines), chars_per_line);
            lines as f32 * line_height + BLOCK_SPACING
        }
        Block::List { items, .. } => {
            let heights: f32 = items
                .iter()
                .map(|item| {
                    let inner: f32 = item
                        .blocks
                        .iter()
                        .map(|block| estimated_block_height(block, typography))
                        .sum();
                    inner.max(line_height) + 6.0
                })
                .sum();
            heights + BLOCK_SPACING
        }
        Block::DefinitionList { items, .. } => {
            let heights: f32 = items
                .iter()
                .map(|item| {
                    let definitions: f32 = item
                        .definitions
                        .iter()
                        .flat_map(|blocks| blocks.iter())
                        .map(|block| estimated_block_height(block, typography))
                        .sum();
                    line_height + definitions + 8.0
                })
                .sum();
            heights + BLOCK_SPACING
        }
        Block::Code { code, .. } => {
            let lines = code.lines().count().max(1) as f32;
            lines * font_size * 0.86 * 1.45 + 64.0
        }
        Block::Quote { blocks, .. } => {
            let inner: f32 = blocks
                .iter()
                .map(|block| estimated_block_height(block, typography))
                .sum();
            inner + 24.0
        }
        Block::Table { rows, .. } => {
            (rows.len() + 1) as f32 * (font_size * 1.5 + 16.0) + BLOCK_SPACING
        }
        Block::Rule { .. } => BLOCK_SPACING,
        Block::Math { .. } => font_size * 2.0 + 32.0,
        Block::Mermaid { .. } => 220.0,
        Block::Html { html, .. } => {
            let lines = estimate_lines(html, chars_per_line);
            lines as f32 * line_height + BLOCK_SPACING
        }
    }
}

fn estimate_lines(text: &str, chars_per_line: f32) -> usize {
    let chars_per_line = chars_per_line.max(1.0);
    let mut lines = 0usize;
    for line in text.split('\n') {
        let wrapped = (line.chars().count() as f32 / chars_per_line).ceil() as usize;
        lines += wrapped.max(1);
    }
    lines.max(1)
}

fn render_block<'a>(
    block: &'a Block,
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
) -> Element<'a, Message> {
    match block {
        Block::Heading { level, inlines, .. } => {
            let scale = match level {
                1 => 2.0,
                2 => 1.6,
                3 => 1.3,
                4 => 1.12,
                5 => 1.0,
                _ => 0.95,
            };
            let size = styles.typography.font_size * scale;
            container(rich_text(
                inlines,
                SpanContext::heading(styles, size),
                styles,
            ))
            .padding(Padding {
                top: if *level == 1 { 18.0 } else { 10.0 },
                bottom: 2.0,
                ..Padding::default()
            })
            .width(Length::Fill)
            .into()
        }
        Block::Paragraph { inlines, .. } => {
            if inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Image { .. }))
            {
                render_paragraph_with_images(inlines, styles)
            } else {
                rich_text(inlines, SpanContext::body(styles), styles)
            }
        }
        Block::List {
            ordered,
            start,
            items,
            ..
        } => render_list(*ordered, *start, items, styles, caches),
        Block::DefinitionList { items, .. } => {
            let mut col = column![].spacing(8).width(Length::Fill);
            for item in items {
                col = col.push(rich_text(
                    &item.term,
                    SpanContext {
                        font: Font {
                            weight: Weight::Bold,
                            ..SpanContext::body(styles).font
                        },
                        ..SpanContext::body(styles)
                    },
                    styles,
                ));
                for definition in &item.definitions {
                    let mut def = column![].spacing(6).width(Length::Fill);
                    for block in definition {
                        def = def.push(render_block(block, styles, caches));
                    }
                    col = col.push(container(def).padding(Padding {
                        left: 24.0,
                        ..Padding::default()
                    }));
                }
            }
            col.into()
        }
        Block::Code { language, code, .. } => render_code(language.as_deref(), code, styles),
        Block::Quote { blocks, .. } => {
            let mut col = column![].spacing(8).width(Length::Fill);
            for block in blocks {
                col = col.push(render_block(block, styles, caches));
            }
            container(col)
                .padding(12)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(styles.tokens.surface.into()),
                    border: Border {
                        color: styles.tokens.accent,
                        width: 3.0,
                        radius: 4.0.into(),
                    },
                    ..container::Style::default()
                })
                .into()
        }
        Block::Table {
            alignments,
            header,
            rows,
            ..
        } => render_table(alignments, header, rows, styles),
        Block::Rule { .. } => rule::horizontal(1)
            .style(move |_| rule::Style {
                color: styles.tokens.border,
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            })
            .into(),
        Block::Math { source, .. } => render_math(source, styles, caches),
        Block::Mermaid { source, .. } => render_mermaid(source, styles, caches),
        Block::Html { html, .. } => render_html_block(html, styles),
    }
}

#[derive(Clone)]
struct SpanContext<'a> {
    styles: PreviewStyles<'a>,
    color: Color,
    font: Font,
    size: f32,
    underline: bool,
    strikethrough: bool,
    link: Option<String>,
    highlight: Option<Color>,
}

impl<'a> SpanContext<'a> {
    fn body(styles: PreviewStyles<'a>) -> Self {
        Self {
            styles,
            color: styles.tokens.text,
            font: font_for(&styles.typography.font_family),
            size: styles.typography.font_size,
            underline: false,
            strikethrough: false,
            link: None,
            highlight: None,
        }
    }

    fn heading(styles: PreviewStyles<'a>, size: f32) -> Self {
        Self {
            color: styles.tokens.heading,
            font: Font {
                weight: Weight::Bold,
                ..font_for(&styles.typography.font_family)
            },
            size,
            ..Self::body(styles)
        }
    }

    fn with_italic(self) -> Self {
        Self {
            font: Font {
                style: FontStyle::Italic,
                ..self.font
            },
            ..self
        }
    }

    fn with_bold(self) -> Self {
        Self {
            font: Font {
                weight: Weight::Bold,
                ..self.font
            },
            ..self
        }
    }

    fn with_size(self, factor: f32) -> Self {
        Self {
            size: self.size * factor,
            ..self
        }
    }
}

fn rich_text<'a>(
    inlines: &'a [Inline],
    context: SpanContext<'a>,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    let mut spans = Vec::new();
    push_inlines(inlines, &context, &mut spans);

    rich_text_from_spans(spans, &context, styles)
}

fn rich_text_from_spans<'a>(
    spans: Vec<iced::widget::text::Span<'a, String, Font>>,
    context: &SpanContext<'a>,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    Rich::<String, Message>::with_spans(spans)
        .size(context.size)
        .font(context.font)
        .line_height(styles.typography.line_height)
        .width(Length::Fill)
        .on_link_click(Message::LinkClicked)
        .into()
}

fn render_html_block<'a>(html: &'a str, styles: PreviewStyles<'a>) -> Element<'a, Message> {
    let context = SpanContext::body(styles);
    let mut spans = Vec::new();
    let inline = Inline::Html(html.to_string());
    push_inlines(std::slice::from_ref(&inline), &context, &mut spans);
    rich_text_from_spans(spans, &context, styles)
}

fn push_inlines<'a>(
    inlines: &[Inline],
    context: &SpanContext<'a>,
    spans: &mut Vec<iced::widget::text::Span<'a, String, Font>>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(value) => spans.push(span(value.clone(), context)),
            Inline::Emphasis(children) => {
                push_inlines(children, &context.clone().with_italic(), spans);
            }
            Inline::Strong(children) => {
                push_inlines(children, &context.clone().with_bold(), spans);
            }
            Inline::Strikethrough(children) => {
                let mut next = context.clone();
                next.strikethrough = true;
                push_inlines(children, &next, spans);
            }
            Inline::Highlight(children) => {
                let mut next = context.clone();
                next.highlight = Some(context.styles.tokens.selection);
                push_inlines(children, &next, spans);
            }
            Inline::Underline(children) => {
                let mut next = context.clone();
                next.underline = true;
                push_inlines(children, &next, spans);
            }
            Inline::Superscript(children) | Inline::Subscript(children) => {
                let mut next = context.clone().with_size(0.75);
                if matches!(inline, Inline::Subscript(_)) {
                    next.underline = false;
                }
                push_inlines(children, &next, spans);
            }
            Inline::Code(code) => {
                let mut next = context.clone();
                next.font = Font::MONOSPACE;
                next.color = context.styles.tokens.code_text;
                next.highlight = Some(context.styles.tokens.code_background);
                next.size *= 0.9;
                spans.push(span(code.clone(), &next));
            }
            Inline::Link { url, children, .. } => {
                let mut next = context.clone();
                next.color = context.styles.tokens.link;
                next.underline = true;
                next.link = Some(url.clone());
                push_inlines(children, &next, spans);
            }
            Inline::Image { alt, .. } => {
                let mut next = context.clone();
                next.color = context.styles.tokens.muted;
                spans.push(span(format!("[Image: {alt}]"), &next));
            }
            Inline::SoftBreak => spans.push(span(String::from(" "), context)),
            Inline::Break => spans.push(span(String::from("\n"), context)),
            Inline::FootnoteReference { number, .. } => {
                let mut next = context.clone();
                next.color = context.styles.tokens.accent;
                next.link = Some(format!("footnote:{number}"));
                spans.push(span(format!("[{number}]"), &next));
            }
            Inline::Math(source) => {
                let mut next = context.clone();
                next.font = Font::MONOSPACE;
                next.color = context.styles.tokens.muted;
                next.size *= 0.9;
                spans.push(span(format!("${source}$"), &next));
            }
            Inline::Html(html) => {
                let mut next = context.clone();
                if html.contains("<kbd>") || html.contains("<code>") {
                    next.font = Font::MONOSPACE;
                }
                if html.contains("<mark>") {
                    next.highlight = Some(context.styles.tokens.selection);
                }
                spans.push(span(html_to_text(html), &next));
            }
        }
    }
}

fn span<'a>(text: String, context: &SpanContext<'a>) -> iced::widget::text::Span<'a, String, Font> {
    let mut span = iced::widget::text::Span::new(text)
        .size(context.size)
        .color(context.color)
        .font(context.font)
        .underline(context.underline)
        .strikethrough(context.strikethrough);

    if let Some(link) = &context.link {
        span = span.link(link.clone());
    }
    if let Some(background) = context.highlight {
        span.highlight = Some(SpanHighlight {
            background: background.into(),
            border: Border::default(),
        });
    }

    span
}

fn html_to_text(html: &str) -> String {
    let html = sanitize::sanitize_inline(html)
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
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

    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn render_paragraph_with_images<'a>(
    inlines: &'a [Inline],
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    let mut col = column![].spacing(8).width(Length::Fill);
    let mut text_start = 0;

    for (index, inline) in inlines.iter().enumerate() {
        if let Inline::Image { url, alt, .. } = inline {
            if text_start < index {
                col = col.push(rich_text(
                    &inlines[text_start..index],
                    SpanContext::body(styles),
                    styles,
                ));
            }
            col = col.push(render_image(url, alt, styles));
            text_start = index + 1;
        }
    }

    if text_start < inlines.len() {
        col = col.push(rich_text(
            &inlines[text_start..],
            SpanContext::body(styles),
            styles,
        ));
    }

    col.into()
}

fn render_image<'a>(url: &'a str, alt: &'a str, styles: PreviewStyles<'a>) -> Element<'a, Message> {
    let Some(path) = resolve_image_path(styles.base_dir, url) else {
        return missing_image(alt, url, styles);
    };

    image(image::Handle::from_path(path))
        .width(Length::Fill)
        .height(Length::Shrink)
        .content_fit(ContentFit::Contain)
        .into()
}

fn missing_image<'a>(
    alt: &'a str,
    url: &'a str,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    container(
        column![
            text(format!("Image not found: {alt}")).color(styles.tokens.muted),
            text(url).size(12).color(styles.tokens.muted),
        ]
        .spacing(4),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| container::Style {
        border: Border {
            color: styles.tokens.border,
            width: 1.0,
            radius: 6.0.into(),
        },
        background: Some(styles.tokens.surface.into()),
        ..container::Style::default()
    })
    .into()
}

fn resolve_image_path(base_dir: Option<&Path>, url: &str) -> Option<PathBuf> {
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("data:")
        || url.starts_with("//")
    {
        return None;
    }

    let clean = url.split(['#', '?']).next().unwrap_or(url);
    let path = Path::new(clean);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };

    path.is_file().then_some(path)
}

fn render_list<'a>(
    ordered: bool,
    start: usize,
    items: &'a [super::model::ListItem],
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
) -> Element<'a, Message> {
    let mut col = column![].spacing(6).width(Length::Fill);

    for (index, item) in items.iter().enumerate() {
        let marker = if let Some(checked) = item.checked {
            if checked {
                String::from("☑")
            } else {
                String::from("☐")
            }
        } else if ordered {
            return_marker(start + index)
        } else {
            String::from("•")
        };

        let mut blocks = column![].spacing(6).width(Length::Fill);
        for block in &item.blocks {
            blocks = blocks.push(render_block(block, styles, caches));
        }

        col = col.push(
            row![
                container(text(marker).color(styles.tokens.muted)).width(Length::Fixed(28.0)),
                blocks,
            ]
            .spacing(4)
            .width(Length::Fill),
        );
    }

    col.into()
}

fn return_marker(number: usize) -> String {
    format!("{number}.")
}

fn render_table<'a>(
    alignments: &[TableAlignment],
    header: &'a [Vec<Inline>],
    rows: &'a [Vec<Vec<Inline>>],
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    let mut table = column![].spacing(0).width(Length::Fill);

    if !header.is_empty() {
        let mut header_row = row![].width(Length::Fill);
        for (index, cell) in header.iter().enumerate() {
            header_row = header_row.push(table_cell(
                cell,
                alignment_at(alignments, index),
                true,
                styles,
            ));
        }
        table = table.push(header_row);
    }

    for (row_index, cells) in rows.iter().enumerate() {
        let mut body_row = row![].width(Length::Fill);
        for (index, cell) in cells.iter().enumerate() {
            body_row = body_row.push(table_cell(
                cell,
                alignment_at(alignments, index),
                false,
                styles,
            ));
        }
        let _ = row_index;
        table = table.push(body_row);
    }

    table.into()
}

fn alignment_at(alignments: &[TableAlignment], index: usize) -> TableAlignment {
    alignments
        .get(index)
        .copied()
        .unwrap_or(TableAlignment::None)
}

fn table_cell<'a>(
    inlines: &'a [Inline],
    alignment: TableAlignment,
    header: bool,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    let background = if header {
        styles.tokens.surface_raised
    } else {
        styles.tokens.background
    };
    let border = styles.tokens.border;

    let content = rich_text(inlines, SpanContext::body(styles), styles);
    let horizontal = match alignment {
        TableAlignment::Center => iced::alignment::Horizontal::Center,
        TableAlignment::Right => iced::alignment::Horizontal::Right,
        TableAlignment::Left | TableAlignment::None => iced::alignment::Horizontal::Left,
    };

    container(content)
        .width(Length::Fill)
        .padding(8)
        .align_x(horizontal)
        .style(move |_| container::Style {
            background: Some(background.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn render_code<'a>(
    language: Option<&'a str>,
    code: &str,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    let mut header = row![].spacing(8).width(Length::Fill);
    header = header.push(
        text(language.unwrap_or("code"))
            .size(12)
            .color(styles.tokens.muted),
    );
    header = header.push(Space::new().width(Length::Fill));
    header = header.push(
        button(text("Copy").size(12))
            .padding([3, 8])
            .on_press(Message::CopyCode(code.to_string())),
    );

    let spans = code_spans(code, language, styles);
    let code_text: Element<'a, Message> = Rich::<String, Message>::with_spans(spans)
        .size(styles.typography.font_size * 0.86)
        .font(Font::MONOSPACE)
        .line_height(1.45)
        .width(Length::Fill)
        .into();

    container(column![header, code_text].spacing(8))
        .padding(12)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(styles.tokens.code_background.into()),
            border: Border {
                color: styles.tokens.border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn code_spans<'a>(
    code: &str,
    language: Option<&str>,
    styles: PreviewStyles<'a>,
) -> Vec<iced::widget::text::Span<'a, String, Font>> {
    let fallback = || {
        vec![
            iced::widget::text::Span::new(code.to_string())
                .font(Font::MONOSPACE)
                .color(styles.tokens.code_text),
        ]
    };

    let Some(language) = language else {
        return fallback();
    };

    let Some(syntax) = SYNTAXES
        .find_syntax_by_token(language)
        .or_else(|| SYNTAXES.find_syntax_by_name(language))
    else {
        return fallback();
    };

    let theme_name = if styles.tokens.is_dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    };
    let Some(theme) = THEMES.themes.get(theme_name) else {
        return fallback();
    };

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut spans = Vec::new();

    for line in code.split_inclusive('\n') {
        match highlighter.highlight_line(line, &SYNTAXES) {
            Ok(ranges) => {
                for (style, value) in ranges {
                    if value.is_empty() {
                        continue;
                    }
                    spans.push(
                        iced::widget::text::Span::new(value.to_string())
                            .font(Font::MONOSPACE)
                            .color(Color::from_rgb8(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            )),
                    );
                }
            }
            Err(_) => return fallback(),
        }
    }

    if spans.is_empty() { fallback() } else { spans }
}

fn render_math<'a>(
    source: &str,
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
) -> Element<'a, Message> {
    let font_size = styles.typography.font_size * 1.15;
    let key = cache_key(source, font_size, styles.tokens.text);

    let result = {
        let cache = caches.borrow();
        cache.math.get(&key).cloned()
    }
    .unwrap_or_else(|| {
        let result = math::render_svg(source, font_size, styles.tokens.text)
            .map_err(|error| error.to_string());
        caches.borrow_mut().math.insert(key, result.clone());
        result
    });

    match result {
        Ok(bytes) => svg(svg::Handle::from_memory(bytes))
            .width(Length::Fill)
            .height(Length::Shrink)
            .content_fit(ContentFit::Contain)
            .into(),
        Err(error) => error_block(source.to_string(), error, styles),
    }
}

fn render_mermaid<'a>(
    source: &str,
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
) -> Element<'a, Message> {
    let key = cache_key(source, 0.0, styles.tokens.text);

    let result = {
        let cache = caches.borrow();
        cache.mermaid.get(&key).cloned()
    }
    .unwrap_or_else(|| {
        let result = mermaid::render_svg(source).map_err(|error| error.to_string());
        caches.borrow_mut().mermaid.insert(key, result.clone());
        result
    });

    match result {
        Ok(svg_source) => container(
            svg(svg::Handle::from_memory(svg_source.into_bytes()))
                .width(Length::Fill)
                .height(Length::Shrink)
                .content_fit(ContentFit::Contain),
        )
        .width(Length::Fill)
        .padding(8)
        .into(),
        Err(error) => error_block(source.to_string(), error, styles),
    }
}

fn error_block<'a>(
    source: String,
    error: String,
    styles: PreviewStyles<'a>,
) -> Element<'a, Message> {
    container(
        column![
            text(error).size(12).color(styles.tokens.error),
            text(source)
                .font(Font::MONOSPACE)
                .size(12)
                .color(styles.tokens.text),
        ]
        .spacing(6),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(styles.tokens.surface.into()),
        border: Border {
            color: styles.tokens.error,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn render_footnotes<'a>(
    document: &'a PreviewDocument,
    styles: PreviewStyles<'a>,
    caches: &'a RefCell<RenderCaches>,
) -> Element<'a, Message> {
    let mut col = column![rule::horizontal(1).style(move |_| rule::Style {
        color: styles.tokens.border,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }),]
    .spacing(8)
    .width(Length::Fill);

    col = col.push(text("Footnotes").size(16).color(styles.tokens.heading));

    for footnote in &document.footnotes {
        let mut body = column![].spacing(4).width(Length::Fill);
        for block in &footnote.blocks {
            body = body.push(render_block(block, styles, caches));
        }
        col = col.push(
            row![
                text(format!("[{}]", footnote.label))
                    .color(styles.tokens.accent)
                    .size(13),
                body,
            ]
            .spacing(8)
            .width(Length::Fill),
        );
    }

    col.into()
}

fn cache_key(source: &str, font_size: f32, color: Color) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    font_size.to_bits().hash(&mut hasher);
    color.r.to_bits().hash(&mut hasher);
    color.g.to_bits().hash(&mut hasher);
    color.b.to_bits().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_paths_resolve_relative_to_the_document() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let image_path = tmp.path().join("image.png");
        std::fs::write(&image_path, b"png").expect("fixture");

        assert_eq!(
            resolve_image_path(Some(tmp.path()), "image.png"),
            Some(image_path)
        );
        assert!(resolve_image_path(Some(tmp.path()), "missing.png").is_none());
        assert!(resolve_image_path(None, "image.png").is_none());
        assert!(resolve_image_path(Some(tmp.path()), "https://example.com/a.png").is_none());
    }

    #[test]
    fn cache_keys_change_with_color_and_size() {
        let a = cache_key("x", 12.0, Color::BLACK);
        let b = cache_key("x", 13.0, Color::BLACK);
        let c = cache_key("x", 12.0, Color::WHITE);

        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn inline_html_preserves_safe_breaks() {
        assert_eq!(html_to_text("a<br>b"), "a\nb");
        assert_eq!(html_to_text("<kbd>Cmd</kbd>"), "Cmd");
    }

    #[test]
    fn long_documents_are_virtualized_with_spacers() {
        let mut source = String::new();
        for index in 0..400 {
            source.push_str(&format!("Paragraph {index} with a little text.\n\n"));
        }
        let document = crate::preview::parser::parse(&source, 0);
        let typography = Typography::preview_default();

        let first = block_window(
            &document,
            &typography,
            PreviewViewport {
                offset_y: 0.0,
                height: 800.0,
            },
        );
        assert_eq!(first.start, 0);
        assert!(first.end < document.blocks.len());
        assert!(first.bottom_pad > 0.0);

        let middle = block_window(
            &document,
            &typography,
            PreviewViewport {
                offset_y: 10_000.0,
                height: 800.0,
            },
        );
        assert!(middle.start > 0);
        assert!(middle.top_pad > 0.0);
        assert!(middle.end < document.blocks.len());
        assert!(middle.bottom_pad > 0.0);
    }

    #[test]
    fn short_documents_render_every_block() {
        let document = crate::preview::parser::parse("# One\n\nTwo\n", 0);
        let window = block_window(
            &document,
            &Typography::preview_default(),
            PreviewViewport::default(),
        );

        assert_eq!(window.start, 0);
        assert_eq!(window.end, document.blocks.len());
        assert_eq!(window.top_pad, 0.0);
        assert_eq!(window.bottom_pad, 0.0);
    }

    #[test]
    fn known_languages_highlight_and_unknown_languages_fall_back() {
        let tokens = ThemeTokens::light();
        let typography = Typography::preview_default();
        let styles = PreviewStyles {
            tokens: &tokens,
            typography: &typography,
            base_dir: None,
        };

        assert!(code_spans("fn main() {}\n", Some("rust"), styles).len() > 1);
        assert_eq!(
            code_spans("fn main() {}\n", Some("unknown"), styles).len(),
            1
        );
        assert_eq!(code_spans("", Some("rust"), styles).len(), 1);
    }
}
