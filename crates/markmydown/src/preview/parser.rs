//! comrak AST to preview model conversion.
//!
//! Parsing enables the full v1 extension set: GFM, footnotes, definition
//! lists, task lists, math, shortcodes, highlight, sub/sup, and smart
//! punctuation. The front matter block is stripped before parsing and its line
//! count is added back to every source range.

use comrak::nodes::{NodeValue, Sourcepos, TableAlignment as ComrakAlignment};
use comrak::{Arena, Options, parse_document};

use super::model::{
    Alignment, Block, DefinitionItem, Footnote, Inline, ListItem, PreviewDocument, SourceRange,
};
use super::sanitize;

/// Parses Markdown into the preview model.
pub fn parse(source: &str, line_offset: usize) -> PreviewDocument {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.superscript = true;
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    options.extension.shortcodes = true;
    options.extension.highlight = true;
    options.extension.subscript = true;
    options.extension.underline = true;
    options.extension.math_dollars = true;
    options.extension.math_code = true;
    options.extension.alerts = true;
    options.parse.smart = true;

    let arena = Arena::new();
    let root = parse_document(&arena, source, &options);
    let mut footnotes = Vec::new();
    let blocks = convert_blocks(root.children(), line_offset, &mut footnotes);

    PreviewDocument {
        blocks,
        footnotes,
        line_offset,
    }
}

/// Number of source lines hidden by a valid front matter block.
pub fn line_offset_for(source: &str, body_offset: usize) -> usize {
    source
        .get(..body_offset)
        .map_or(0, |prefix| prefix.matches('\n').count())
}

fn convert_blocks<'a>(
    nodes: impl IntoIterator<Item = comrak::nodes::Node<'a>>,
    line_offset: usize,
    footnotes: &mut Vec<Footnote>,
) -> Vec<Block> {
    nodes
        .into_iter()
        .filter_map(|node| convert_block(node, line_offset, footnotes))
        .collect()
}

fn convert_block<'a>(
    node: comrak::nodes::Node<'a>,
    line_offset: usize,
    footnotes: &mut Vec<Footnote>,
) -> Option<Block> {
    let value = node.data.borrow().value.clone();
    let range = source_range(node, line_offset);

    match value {
        NodeValue::Document | NodeValue::FrontMatter(_) => None,
        NodeValue::Heading(heading) => Some(Block::Heading {
            level: heading.level,
            inlines: convert_inlines(node.children()),
            range,
        }),
        NodeValue::Paragraph => {
            let children: Vec<_> = node.children().collect();
            if children.len() == 1
                && let NodeValue::Math(math) = children[0].data.borrow().value.clone()
                && math.display_math
            {
                return Some(Block::Math {
                    source: math.literal,
                    range,
                });
            }

            Some(Block::Paragraph {
                inlines: convert_inlines(children),
                range,
            })
        }
        NodeValue::List(list) => {
            let items = node
                .children()
                .map(|item| {
                    let checked = match item.data.borrow().value.clone() {
                        NodeValue::TaskItem(task) => {
                            Some(task.symbol.is_some_and(|symbol| symbol != ' '))
                        }
                        _ => None,
                    };

                    ListItem {
                        checked,
                        blocks: convert_blocks(item.children(), line_offset, footnotes),
                    }
                })
                .collect();

            Some(Block::List {
                ordered: matches!(list.list_type, comrak::nodes::ListType::Ordered),
                start: list.start,
                tight: list.tight,
                items,
                range,
            })
        }
        NodeValue::DescriptionList => {
            let items = node
                .children()
                .map(|item| {
                    let mut term = Vec::new();
                    let mut definitions = Vec::new();

                    for child in item.children() {
                        match child.data.borrow().value.clone() {
                            NodeValue::DescriptionTerm => {
                                let block =
                                    convert_blocks(child.children(), line_offset, footnotes)
                                        .into_iter()
                                        .next();
                                term = match block {
                                    Some(Block::Paragraph { inlines, .. }) => inlines,
                                    Some(other) => vec![Inline::Text(other.plain_text())],
                                    None => Vec::new(),
                                };
                            }
                            NodeValue::DescriptionDetails => {
                                definitions.push(convert_blocks(
                                    child.children(),
                                    line_offset,
                                    footnotes,
                                ));
                            }
                            _ => {}
                        }
                    }

                    DefinitionItem { term, definitions }
                })
                .collect();

            Some(Block::DefinitionList { items, range })
        }
        NodeValue::CodeBlock(code) => {
            let language = code
                .info
                .split_whitespace()
                .next()
                .filter(|info| !info.is_empty())
                .map(str::to_lowercase);
            let source = code.literal;

            match language.as_deref() {
                Some("mermaid") => Some(Block::Mermaid { source, range }),
                Some("math") | Some("latex") => Some(Block::Math { source, range }),
                _ => Some(Block::Code {
                    language,
                    code: source,
                    range,
                }),
            }
        }
        NodeValue::BlockQuote
        | NodeValue::MultilineBlockQuote(_)
        | NodeValue::Alert(_)
        | NodeValue::Callout(_) => Some(Block::Quote {
            blocks: convert_blocks(node.children(), line_offset, footnotes),
            range,
        }),
        NodeValue::Table(table) => {
            let mut header = Vec::new();
            let mut rows = Vec::new();

            for row in node.children() {
                let is_header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
                let cells: Vec<_> = row
                    .children()
                    .map(|cell| convert_inlines(cell.children()))
                    .collect();

                if is_header {
                    header = cells;
                } else {
                    rows.push(cells);
                }
            }

            Some(Block::Table {
                alignments: table.alignments.iter().map(map_alignment).collect(),
                header,
                rows,
                range,
            })
        }
        NodeValue::ThematicBreak => Some(Block::Rule { range }),
        NodeValue::HtmlBlock(html) => {
            let html = sanitize::sanitize_html(&html.literal);
            if html.trim().is_empty() {
                None
            } else {
                Some(Block::Html { html, range })
            }
        }
        NodeValue::FootnoteDefinition(definition) => {
            let blocks = convert_blocks(node.children(), line_offset, footnotes);
            footnotes.push(Footnote {
                label: definition.name,
                blocks,
                range,
            });
            None
        }
        NodeValue::Math(math) if math.display_math => Some(Block::Math {
            source: math.literal,
            range,
        }),
        _ => None,
    }
}

fn convert_inlines<'a>(nodes: impl IntoIterator<Item = comrak::nodes::Node<'a>>) -> Vec<Inline> {
    nodes.into_iter().filter_map(convert_inline).collect()
}

fn convert_inline<'a>(node: comrak::nodes::Node<'a>) -> Option<Inline> {
    let value = node.data.borrow().value.clone();

    Some(match value {
        NodeValue::Text(text) => Inline::Text(text.into_owned()),
        NodeValue::SoftBreak => Inline::SoftBreak,
        NodeValue::LineBreak => Inline::Break,
        NodeValue::Code(code) => Inline::Code(code.literal),
        NodeValue::HtmlInline(html) => Inline::Html(sanitize::sanitize_inline(&html)),
        NodeValue::Emph => Inline::Emphasis(convert_inlines(node.children())),
        NodeValue::Strong => Inline::Strong(convert_inlines(node.children())),
        NodeValue::Strikethrough => Inline::Strikethrough(convert_inlines(node.children())),
        NodeValue::Highlight => Inline::Highlight(convert_inlines(node.children())),
        NodeValue::Underline | NodeValue::Insert => {
            Inline::Underline(convert_inlines(node.children()))
        }
        NodeValue::Superscript => Inline::Superscript(convert_inlines(node.children())),
        NodeValue::Subscript => Inline::Subscript(convert_inlines(node.children())),
        NodeValue::Link(link) => Inline::Link {
            url: link.url,
            title: link.title,
            children: convert_inlines(node.children()),
        },
        NodeValue::Image(link) => {
            let children = convert_inlines(node.children());
            Inline::Image {
                url: link.url,
                title: link.title,
                alt: Inline::plain_text(&children),
            }
        }
        NodeValue::FootnoteReference(reference) => Inline::FootnoteReference {
            label: reference.name,
            number: reference.ix,
        },
        NodeValue::ShortCode(code) => Inline::Text(code.emoji),
        NodeValue::Math(math) => Inline::Math(math.literal),
        NodeValue::WikiLink(link) => Inline::Link {
            url: link.url,
            title: String::new(),
            children: convert_inlines(node.children()),
        },
        NodeValue::SpoileredText => Inline::Strikethrough(convert_inlines(node.children())),
        NodeValue::Escaped | NodeValue::Raw(_) => {
            let children = convert_inlines(node.children());
            if children.is_empty() {
                return None;
            }
            Inline::Emphasis(children)
        }
        _ => return None,
    })
}

fn source_range(node: comrak::nodes::Node<'_>, line_offset: usize) -> SourceRange {
    let Sourcepos { start, end } = node.data.borrow().sourcepos;
    SourceRange::new(
        start.line.saturating_sub(1) + line_offset,
        end.line.saturating_sub(1) + line_offset,
    )
}

fn map_alignment(alignment: &ComrakAlignment) -> Alignment {
    match alignment {
        ComrakAlignment::None => Alignment::None,
        ComrakAlignment::Left => Alignment::Left,
        ComrakAlignment::Center => Alignment::Center,
        ComrakAlignment::Right => Alignment::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comrak::nodes::NodeMath;

    fn first_block(source: &str) -> Block {
        parse(source, 0).blocks.remove(0)
    }

    #[test]
    fn parses_gfm_blocks_and_inlines() {
        let document = parse(
            "# Title\n\nA **bold** and *italic* [link](https://example.com).\n\n- one\n- [x] done\n",
            0,
        );

        assert!(matches!(
            document.blocks[0],
            Block::Heading { level: 1, .. }
        ));
        assert!(matches!(document.blocks[1], Block::Paragraph { .. }));
        assert!(matches!(document.blocks[2], Block::List { .. }));
    }

    #[test]
    fn parses_tables_with_alignment() {
        let block = first_block("| A | B |\n|:--|--:|\n| 1 | 2 |\n");

        let Block::Table {
            alignments,
            header,
            rows,
            ..
        } = block
        else {
            panic!("expected table");
        };

        assert_eq!(alignments, vec![Alignment::Left, Alignment::Right]);
        assert_eq!(header.len(), 2);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn parses_task_lists() {
        let block = first_block("- [x] done\n- [ ] todo\n");

        let Block::List { items, .. } = block else {
            panic!("expected list");
        };

        assert_eq!(items[0].checked, Some(true));
        assert_eq!(items[1].checked, Some(false));
    }

    #[test]
    fn parses_footnotes() {
        let document = parse("A note[^1].\n\n[^1]: The note.\n", 0);

        assert_eq!(document.footnotes.len(), 1);
        assert_eq!(document.footnotes[0].label, "1");
        let text = format!("{:?}", document.blocks[0]);
        assert!(text.contains("FootnoteReference"));
    }

    #[test]
    fn parses_definition_lists() {
        let block = first_block("Term\n\n: Definition\n");

        let Block::DefinitionList { items, .. } = block else {
            panic!("expected definition list");
        };

        assert_eq!(items[0].term.len(), 1);
        assert_eq!(items[0].definitions.len(), 1);
    }

    #[test]
    fn parses_extended_inline_syntax() {
        let block = first_block("H~2~O, x^2^, ==mark==, :smile:, \"quotes\"\n");
        let debug = format!("{block:?}");

        assert!(debug.contains("Subscript"));
        assert!(debug.contains("Superscript"));
        assert!(debug.contains("Highlight"));
        assert!(debug.contains('😄'));
        assert!(debug.contains('“'));
    }

    #[test]
    fn display_math_becomes_a_block_and_inline_math_stays_literal() {
        let display = first_block("$$\nx^2\n$$\n");
        assert!(matches!(display, Block::Math { .. }));

        let paragraph = first_block("Inline $x^2$ math.\n");
        let debug = format!("{paragraph:?}");
        assert!(debug.contains("Math(\"x^2\")"));
    }

    #[test]
    fn mermaid_blocks_are_detected() {
        let block = first_block("```mermaid\nflowchart TD\nA --> B\n```\n");

        assert!(matches!(block, Block::Mermaid { .. }));
    }

    #[test]
    fn unsafe_html_is_sanitized() {
        let block = first_block("<p onclick=\"x\">safe</p>\n");
        let debug = format!("{block:?}");

        assert!(!debug.contains("script"));
        assert!(!debug.contains("onclick"));
        assert!(debug.contains("safe"));
    }

    #[test]
    fn source_ranges_include_front_matter_offset() {
        let source = "---\ntitle: T\n---\n# Body\n";
        let offset = line_offset_for(source, source.find("# Body").unwrap());
        let document = parse("# Body\n", offset);

        assert_eq!(document.blocks[0].range().start_line, 3);
        assert_eq!(offset, 3);
    }

    #[test]
    fn image_alt_text_is_plain_text() {
        let block = first_block("![A **bold** alt](image.png)\n");
        let debug = format!("{block:?}");

        assert!(debug.contains("A bold alt"));
        assert!(debug.contains("image.png"));
    }

    #[test]
    fn autolinks_are_links() {
        let block = first_block("Visit https://example.com now.\n");
        let debug = format!("{block:?}");

        assert!(debug.contains("https://example.com"));
        assert!(debug.contains("Link"));
    }

    #[test]
    fn math_node_type_can_be_inspected() {
        let math = NodeMath {
            dollar_math: true,
            display_math: true,
            literal: String::from("x"),
        };
        assert!(math.display_math);
    }
}
