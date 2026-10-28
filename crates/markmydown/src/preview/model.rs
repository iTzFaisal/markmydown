//! The derived preview document model with source ranges.
//!
//! The model is intentionally independent of Iced so it can be snapshotted,
//! rendered to HTML/PDF, and tested without a GUI.

use serde::{Deserialize, Serialize};

/// A zero-based inclusive line range in the original Markdown source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: usize,
    pub end_line: usize,
}

impl SourceRange {
    pub fn new(start_line: usize, end_line: usize) -> Self {
        Self {
            start_line,
            end_line: end_line.max(start_line),
        }
    }

    pub fn contains_line(self, line: usize) -> bool {
        (self.start_line..=self.end_line).contains(&line)
    }
}

/// A rendered document.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PreviewDocument {
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    /// Lines hidden by valid front matter. Block ranges already include it.
    pub line_offset: usize,
}

impl PreviewDocument {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn block_for_line(&self, line: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| block.range().contains_line(line))
    }

    pub fn line_for_block(&self, index: usize) -> Option<usize> {
        self.blocks.get(index).map(|block| block.range().start_line)
    }

    /// Plain text of the whole document, used by copy-as-text.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            out.push_str(&block.plain_text());
            out.push('\n');
        }
        out
    }
}

/// A block-level element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
        range: SourceRange,
    },
    Paragraph {
        inlines: Vec<Inline>,
        range: SourceRange,
    },
    List {
        ordered: bool,
        start: usize,
        tight: bool,
        items: Vec<ListItem>,
        range: SourceRange,
    },
    DefinitionList {
        items: Vec<DefinitionItem>,
        range: SourceRange,
    },
    Code {
        language: Option<String>,
        code: String,
        range: SourceRange,
    },
    Quote {
        blocks: Vec<Block>,
        range: SourceRange,
    },
    Table {
        alignments: Vec<Alignment>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        range: SourceRange,
    },
    Rule {
        range: SourceRange,
    },
    Math {
        source: String,
        range: SourceRange,
    },
    Mermaid {
        source: String,
        range: SourceRange,
    },
    Html {
        html: String,
        range: SourceRange,
    },
}

impl Block {
    pub fn range(&self) -> SourceRange {
        match self {
            Self::Heading { range, .. }
            | Self::Paragraph { range, .. }
            | Self::List { range, .. }
            | Self::DefinitionList { range, .. }
            | Self::Code { range, .. }
            | Self::Quote { range, .. }
            | Self::Table { range, .. }
            | Self::Rule { range }
            | Self::Math { range, .. }
            | Self::Mermaid { range, .. }
            | Self::Html { range, .. } => *range,
        }
    }

    pub fn plain_text(&self) -> String {
        match self {
            Self::Heading { inlines, .. } | Self::Paragraph { inlines, .. } => {
                Inline::plain_text(inlines)
            }
            Self::List { items, .. } => items
                .iter()
                .map(ListItem::plain_text)
                .collect::<Vec<_>>()
                .join("\n"),
            Self::DefinitionList { items, .. } => items
                .iter()
                .map(DefinitionItem::plain_text)
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Code { code, .. } => code.clone(),
            Self::Quote { blocks, .. } => blocks
                .iter()
                .map(Block::plain_text)
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Table { header, rows, .. } => {
                let mut lines = Vec::new();
                lines.push(
                    header
                        .iter()
                        .map(|cell| Inline::plain_text(cell))
                        .collect::<Vec<_>>()
                        .join("\t"),
                );
                for row in rows {
                    lines.push(
                        row.iter()
                            .map(|cell| Inline::plain_text(cell))
                            .collect::<Vec<_>>()
                            .join("\t"),
                    );
                }
                lines.join("\n")
            }
            Self::Rule { .. } => String::from("---"),
            Self::Math { source, .. } => source.clone(),
            Self::Mermaid { source, .. } => source.clone(),
            Self::Html { html, .. } => html.clone(),
        }
    }
}

/// A list item, optionally a task checkbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

impl ListItem {
    fn plain_text(&self) -> String {
        let text = self
            .blocks
            .iter()
            .map(Block::plain_text)
            .collect::<Vec<_>>()
            .join(" ");
        match self.checked {
            Some(true) => format!("[x] {text}"),
            Some(false) => format!("[ ] {text}"),
            None => text,
        }
    }
}

/// One term and its definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinitionItem {
    pub term: Vec<Inline>,
    pub definitions: Vec<Vec<Block>>,
}

impl DefinitionItem {
    fn plain_text(&self) -> String {
        let mut out = Inline::plain_text(&self.term);
        for definition in &self.definitions {
            out.push_str(": ");
            out.push_str(
                &definition
                    .iter()
                    .map(Block::plain_text)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// A footnote definition, rendered after the main document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footnote {
    pub label: String,
    pub blocks: Vec<Block>,
    pub range: SourceRange,
}

/// An inline element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Highlight(Vec<Inline>),
    Underline(Vec<Inline>),
    Superscript(Vec<Inline>),
    Subscript(Vec<Inline>),
    Code(String),
    Link {
        url: String,
        title: String,
        children: Vec<Inline>,
    },
    Image {
        url: String,
        title: String,
        alt: String,
    },
    SoftBreak,
    Break,
    FootnoteReference {
        label: String,
        number: u32,
    },
    /// Inline math is outside v1 and is rendered as literal text.
    Math(String),
    /// Sanitized inline HTML such as `<kbd>` and `<br>`.
    Html(String),
}

impl Inline {
    pub fn plain_text(inlines: &[Inline]) -> String {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                Self::Text(text) | Self::Code(text) | Self::Math(text) => out.push_str(text),
                Self::Emphasis(children)
                | Self::Strong(children)
                | Self::Strikethrough(children)
                | Self::Highlight(children)
                | Self::Underline(children)
                | Self::Superscript(children)
                | Self::Subscript(children) => out.push_str(&Self::plain_text(children)),
                Self::Link { children, .. } => out.push_str(&Self::plain_text(children)),
                Self::Image { alt, .. } => out.push_str(alt),
                Self::SoftBreak => out.push(' '),
                Self::Break => out.push('\n'),
                Self::FootnoteReference { number, .. } => {
                    out.push_str(&format!("[{number}]"));
                }
                Self::Html(html) => out.push_str(&strip_html(html)),
            }
        }
        out
    }
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

    #[test]
    fn source_ranges_are_inclusive() {
        let range = SourceRange::new(3, 5);

        assert!(range.contains_line(3));
        assert!(range.contains_line(5));
        assert!(!range.contains_line(2));
        assert!(!range.contains_line(6));
    }

    #[test]
    fn plain_text_walks_nested_blocks() {
        let document = PreviewDocument {
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Title".into())],
                    range: SourceRange::new(0, 0),
                },
                Block::Paragraph {
                    inlines: vec![
                        Inline::Text("Hello ".into()),
                        Inline::Strong(vec![Inline::Text("world".into())]),
                    ],
                    range: SourceRange::new(1, 1),
                },
            ],
            footnotes: Vec::new(),
            line_offset: 0,
        };

        assert!(document.plain_text().contains("Title"));
        assert!(document.plain_text().contains("Hello world"));
    }

    #[test]
    fn block_for_line_finds_the_containing_block() {
        let document = PreviewDocument {
            blocks: vec![
                Block::Paragraph {
                    inlines: Vec::new(),
                    range: SourceRange::new(0, 2),
                },
                Block::Paragraph {
                    inlines: Vec::new(),
                    range: SourceRange::new(4, 6),
                },
            ],
            footnotes: Vec::new(),
            line_offset: 0,
        };

        assert_eq!(document.block_for_line(1), Some(0));
        assert_eq!(document.block_for_line(5), Some(1));
        assert_eq!(document.block_for_line(3), None);
    }
}
