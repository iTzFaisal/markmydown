//! Markdown formatting actions and smart editing behaviors.
//!
//! Everything here is a pure text transformation over `(text, selection)` so
//! the same behavior can be unit-tested without an Iced runtime. The app maps
//! the returned [`TextEdit`] onto the vendored editor as one undo step.

/// Zero-based `(line, column)` cursor position. Columns count characters.
pub type Position = (usize, usize);
/// Normalized or unnormalized selection endpoints.
pub type Selection = (Position, Position);

/// A pure text edit and the selection to apply afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub text: String,
    pub selection: Option<Selection>,
    pub cursor: Position,
}

impl TextEdit {
    fn cursor(text: String, cursor: Position) -> Self {
        Self {
            text,
            selection: None,
            cursor,
        }
    }

    fn select(text: String, start: Position, end: Position) -> Self {
        Self {
            text,
            selection: Some((start, end)),
            cursor: end,
        }
    }
}

/// Formatting commands exposed by the toolbar and keyboard shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAction {
    Bold,
    Italic,
    Heading(u8),
    Strikethrough,
    BulletList,
    OrderedList,
    TaskList,
    Blockquote,
    InlineCode,
    CodeBlock,
    Table,
    Link,
    Image,
}

impl FormatAction {
    pub const ALL: [Self; 15] = [
        Self::Bold,
        Self::Italic,
        Self::Heading(1),
        Self::Strikethrough,
        Self::BulletList,
        Self::OrderedList,
        Self::TaskList,
        Self::Blockquote,
        Self::InlineCode,
        Self::CodeBlock,
        Self::Table,
        Self::Link,
        Self::Image,
        Self::Heading(2),
        Self::Heading(3),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Heading(1) => "Heading 1",
            Self::Heading(2) => "Heading 2",
            Self::Heading(3) => "Heading 3",
            Self::Heading(_) => "Heading",
            Self::Strikethrough => "Strikethrough",
            Self::BulletList => "Bullet list",
            Self::OrderedList => "Numbered list",
            Self::TaskList => "Check list",
            Self::Blockquote => "Blockquote",
            Self::InlineCode => "Inline code",
            Self::CodeBlock => "Code block",
            Self::Table => "Table",
            Self::Link => "Link",
            Self::Image => "Image",
        }
    }

    /// Display shortcut, shown in the in-app reference.
    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Bold => "Cmd/Ctrl+B",
            Self::Italic => "Cmd/Ctrl+I",
            Self::Heading(1) => "Cmd/Ctrl+1",
            Self::Heading(2) => "Cmd/Ctrl+2",
            Self::Heading(3) => "Cmd/Ctrl+3",
            Self::Heading(_) => "Cmd/Ctrl+1..6",
            Self::Strikethrough => "Cmd/Ctrl+Shift+X",
            Self::BulletList => "Cmd/Ctrl+Shift+8",
            Self::OrderedList => "Cmd/Ctrl+Shift+7",
            Self::TaskList => "Cmd/Ctrl+Shift+9",
            Self::Blockquote => "Cmd/Ctrl+Shift+.",
            Self::InlineCode => "Cmd/Ctrl+`",
            Self::CodeBlock => "Cmd/Ctrl+Shift+`",
            Self::Table => "Cmd/Ctrl+T",
            Self::Link => "Cmd/Ctrl+K",
            Self::Image => "Cmd/Ctrl+Shift+I",
        }
    }
}

/// Applies a formatting action.
pub fn apply_format(text: &str, selection: Option<Selection>, action: FormatAction) -> TextEdit {
    match action {
        FormatAction::Bold => wrap(text, selection, "**", "**", "bold text"),
        FormatAction::Italic => wrap(text, selection, "_", "_", "italic text"),
        FormatAction::Strikethrough => wrap(text, selection, "~~", "~~", "text"),
        FormatAction::InlineCode => wrap(text, selection, "`", "`", "code"),
        FormatAction::Heading(level) => heading(text, selection, level.clamp(1, 6)),
        FormatAction::BulletList => list(text, selection, ListKind::Bullet),
        FormatAction::OrderedList => list(text, selection, ListKind::Ordered),
        FormatAction::TaskList => list(text, selection, ListKind::Task),
        FormatAction::Blockquote => list(text, selection, ListKind::Quote),
        FormatAction::CodeBlock => code_block(text, selection),
        FormatAction::Table => table(text, selection),
        FormatAction::Link => insert_link(text, selection, "", false),
        FormatAction::Image => insert_link(text, selection, "", true),
    }
}

fn wrap(
    text: &str,
    selection: Option<Selection>,
    prefix: &str,
    suffix: &str,
    placeholder: &str,
) -> TextEdit {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = selection_offsets(&chars, selection);
    let selected: String = chars[start..end].iter().collect();
    let content = if selected.is_empty() {
        placeholder
    } else {
        selected.as_str()
    };

    let mut new_chars = Vec::with_capacity(chars.len() + prefix.len() + suffix.len());
    new_chars.extend_from_slice(&chars[..start]);
    new_chars.extend(prefix.chars());
    new_chars.extend(content.chars());
    new_chars.extend(suffix.chars());
    new_chars.extend_from_slice(&chars[end..]);

    let new_text: String = new_chars.into_iter().collect();
    let new_text_chars: Vec<char> = new_text.chars().collect();
    let content_start = start + prefix.chars().count();
    let content_end = content_start + content.chars().count();

    TextEdit::select(
        new_text,
        char_to_position(&new_text_chars, content_start),
        char_to_position(&new_text_chars, content_end),
    )
}

fn heading(text: &str, selection: Option<Selection>, level: u8) -> TextEdit {
    let chars: Vec<char> = text.chars().collect();
    let (start_line, end_line) = line_range(&chars, selection);
    let lines = split_lines(&chars);

    let levels: Vec<Option<u8>> = lines[start_line..=end_line]
        .iter()
        .map(|line| heading_level(line))
        .collect();
    let toggle_off = levels.iter().all(|existing| *existing == Some(level));

    let mut new_lines = lines.clone();
    for line in new_lines.iter_mut().take(end_line + 1).skip(start_line) {
        let (indent, content) = split_indent(line);
        let content = strip_heading(content);
        *line = if toggle_off {
            format!("{indent}{content}")
        } else {
            format!("{indent}{} {content}", "#".repeat(level as usize))
        };
    }

    let new_text = join_lines(&new_lines);
    let line = start_line.min(new_lines.len().saturating_sub(1));
    let column = if toggle_off { 0 } else { level as usize + 1 };
    TextEdit::cursor(new_text, (line, column))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Ordered,
    Task,
    Quote,
}

impl ListKind {
    fn prefix(self, index: usize) -> String {
        match self {
            Self::Bullet => String::from("- "),
            Self::Ordered => format!("{}. ", index + 1),
            Self::Task => String::from("- [ ] "),
            Self::Quote => String::from("> "),
        }
    }

    fn matches(self, line: &str) -> bool {
        let (_, content) = split_indent(line);
        match self {
            Self::Bullet => {
                strip_marker(content, &["- ", "* ", "+ "]).is_some() && !Self::Task.matches(line)
            }
            Self::Ordered => strip_ordered(content).is_some(),
            Self::Task => {
                (content.starts_with("- [ ] ") || content.starts_with("- [x] "))
                    || content.starts_with("- [X] ")
            }
            Self::Quote => content.starts_with("> "),
        }
    }
}

fn list(text: &str, selection: Option<Selection>, kind: ListKind) -> TextEdit {
    let chars: Vec<char> = text.chars().collect();
    let (start_line, end_line) = line_range(&chars, selection);
    let lines = split_lines(&chars);
    let toggle_off = lines[start_line..=end_line]
        .iter()
        .all(|line| kind.matches(line));

    let mut new_lines = lines.clone();
    let mut ordered_index = 0;
    for line in new_lines.iter_mut().take(end_line + 1).skip(start_line) {
        let (indent, content) = split_indent(line);
        let content = strip_list_prefix(content);
        *line = if toggle_off {
            format!("{indent}{content}")
        } else {
            let prefix = kind.prefix(ordered_index);
            if kind == ListKind::Ordered {
                ordered_index += 1;
            }
            format!("{indent}{prefix}{content}")
        };
    }

    let new_text = join_lines(&new_lines);
    let column = if toggle_off {
        0
    } else {
        let (indent, _) = split_indent(&new_lines[start_line]);
        indent.chars().count() + kind.prefix(0).chars().count()
    };
    TextEdit::cursor(new_text, (start_line, column))
}

fn code_block(text: &str, selection: Option<Selection>) -> TextEdit {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = selection_offsets(&chars, selection);
    let selected: String = chars[start..end].iter().collect();

    let body = if selected.is_empty() {
        String::new()
    } else {
        selected
    };
    let block = format!("```\n{body}\n```");
    let mut new_chars = Vec::with_capacity(chars.len() + block.chars().count());
    new_chars.extend_from_slice(&chars[..start]);
    new_chars.extend(block.chars());
    new_chars.extend_from_slice(&chars[end..]);

    let new_text: String = new_chars.into_iter().collect();
    let new_text_chars: Vec<char> = new_text.chars().collect();
    let cursor_offset = start + 4;
    TextEdit::cursor(new_text, char_to_position(&new_text_chars, cursor_offset))
}

fn table(text: &str, selection: Option<Selection>) -> TextEdit {
    let skeleton = "| Column 1 | Column 2 |\n| --- | --- |\n| Cell 1 | Cell 2 |";
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = selection_offsets(&chars, selection);

    let mut new_chars = Vec::with_capacity(chars.len() + skeleton.len() + 2);
    new_chars.extend_from_slice(&chars[..start]);
    new_chars.extend(skeleton.chars());
    new_chars.extend_from_slice(&chars[end..]);

    let new_text: String = new_chars.into_iter().collect();
    let new_text_chars: Vec<char> = new_text.chars().collect();
    let cursor_offset = start + 2; // first character of "Column 1"
    TextEdit::cursor(new_text, char_to_position(&new_text_chars, cursor_offset))
}

/// Inserts a link or image. When `url` is empty the placeholder `url` is
/// selected so the user can type over it.
pub fn insert_link(text: &str, selection: Option<Selection>, url: &str, image: bool) -> TextEdit {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = selection_offsets(&chars, selection);
    let selected: String = chars[start..end].iter().collect();
    let label = if selected.is_empty() {
        if image { "alt text" } else { "text" }
    } else {
        selected.as_str()
    };

    let prefix = if image { "![" } else { "[" };
    let mut link = format!("{prefix}{label}](");
    let label_start = start + prefix.chars().count();
    let label_end = label_start + label.chars().count();
    let url_start = label_end + 2;
    link.push_str(if url.is_empty() { "url" } else { url });
    let url_end = url_start
        + if url.is_empty() {
            3
        } else {
            url.chars().count()
        };
    link.push(')');

    let mut new_chars = Vec::with_capacity(chars.len() + link.chars().count());
    new_chars.extend_from_slice(&chars[..start]);
    new_chars.extend(link.chars());
    new_chars.extend_from_slice(&chars[end..]);
    let new_text: String = new_chars.into_iter().collect();
    let new_text_chars: Vec<char> = new_text.chars().collect();

    let new_start = char_to_position(&new_text_chars, label_start);
    let new_end = char_to_position(&new_text_chars, label_end);
    let url_start_pos = char_to_position(&new_text_chars, url_start);
    let url_end_pos = char_to_position(&new_text_chars, url_end);

    if url.is_empty() {
        TextEdit::select(new_text, url_start_pos, url_end_pos)
    } else {
        TextEdit::select(new_text, new_start, new_end)
    }
}

/// Continues a list after Enter. Returns `None` when the line is not a list.
pub fn continue_list(text: &str, cursor: Position) -> Option<TextEdit> {
    let chars: Vec<char> = text.chars().collect();
    let lines = split_lines(&chars);
    let line = lines.get(cursor.0)?;
    let (indent, content) = split_indent(line);
    let (marker, rest) = parse_list_marker(content)?;

    if cursor.1 < indent.chars().count() + marker.chars().count() {
        return None;
    }

    let content_after_cursor: String = line.chars().skip(cursor.1).collect::<String>();
    let empty_item = rest.trim().is_empty() && content_after_cursor.trim().is_empty();

    let offset = position_to_char(&chars, cursor);
    if empty_item {
        let line_start = position_to_char(&chars, (cursor.0, 0));
        let indent_len = indent.chars().count();
        let replacement = indent;
        let mut new_chars = Vec::with_capacity(chars.len());
        new_chars.extend_from_slice(&chars[..line_start]);
        new_chars.extend(replacement.chars());
        new_chars.extend_from_slice(&chars[offset..]);
        let new_text: String = new_chars.into_iter().collect();
        return Some(TextEdit::cursor(new_text, (cursor.0, indent_len)));
    }

    let next_marker = next_list_marker(&marker)?;
    let insertion = format!("\n{indent}{next_marker} ");
    let mut new_chars = Vec::with_capacity(chars.len() + insertion.chars().count());
    new_chars.extend_from_slice(&chars[..offset]);
    new_chars.extend(insertion.chars());
    new_chars.extend_from_slice(&chars[offset..]);
    let new_text: String = new_chars.into_iter().collect();

    let mut position = cursor;
    for ch in insertion.chars() {
        if ch == '\n' {
            position = (position.0 + 1, 0);
        } else {
            position.1 += 1;
        }
    }

    Some(TextEdit::cursor(new_text, position))
}

/// Indents or outdents a list item by one level (four spaces).
pub fn indent_list_item(text: &str, cursor: Position, outdent: bool) -> Option<TextEdit> {
    let chars: Vec<char> = text.chars().collect();
    let lines = split_lines(&chars);
    let line = lines.get(cursor.0)?;
    let (_, content) = split_indent(line);

    if parse_list_marker(content).is_none() && !content.starts_with("> ") {
        return None;
    }

    let line_start = position_to_char(&chars, (cursor.0, 0));
    let line_end = line_start + line.chars().count();
    let indent = "    ";

    if outdent {
        if line.starts_with("    ") {
            let mut new_chars = Vec::with_capacity(chars.len() - 4);
            new_chars.extend_from_slice(&chars[..line_start]);
            new_chars.extend_from_slice(&chars[line_start + 4..]);
            let new_text: String = new_chars.into_iter().collect();
            return Some(TextEdit::cursor(
                new_text,
                (cursor.0, cursor.1.saturating_sub(4)),
            ));
        }
        return None;
    }

    let mut new_chars = Vec::with_capacity(chars.len() + 4);
    new_chars.extend_from_slice(&chars[..line_start]);
    new_chars.extend(indent.chars());
    new_chars.extend_from_slice(&chars[line_start..line_end]);
    new_chars.extend_from_slice(&chars[line_end..]);
    let new_text: String = new_chars.into_iter().collect();

    Some(TextEdit::cursor(new_text, (cursor.0, cursor.1 + 4)))
}

/// Converts a pasted URL over a selection into a Markdown link.
pub fn paste_url(text: &str, selection: Option<Selection>, url: &str) -> Option<TextEdit> {
    let trimmed = url.trim();
    if trimmed.is_empty()
        || !(trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("www."))
    {
        return None;
    }

    let chars: Vec<char> = text.chars().collect();
    let (start, end) = selection_offsets(&chars, selection);
    if start == end {
        return None;
    }

    Some(insert_link(text, selection, trimmed, false))
}

fn next_list_marker(marker: &str) -> Option<String> {
    if marker.starts_with("- [ ] ") || marker.starts_with("- [x] ") || marker.starts_with("- [X] ")
    {
        return Some(String::from("- [ ]"));
    }

    let trimmed = marker.trim_end();
    if trimmed == "-" || trimmed == "*" || trimmed == "+" {
        return Some(trimmed.to_string());
    }

    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        let number: u64 = digits.parse().ok()?;
        let delimiter = trimmed.chars().nth(digits.len())?;
        return Some(format!("{}{delimiter}", number + 1));
    }

    None
}

/// Parses `- `, `* `, `+ `, `1. `, `1) `, and task markers.
fn parse_list_marker(content: &str) -> Option<(String, &str)> {
    for marker in ["- [ ] ", "- [x] ", "- [X] "] {
        if let Some(rest) = content.strip_prefix(marker) {
            return Some((marker.to_string(), rest));
        }
    }

    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = content.strip_prefix(marker) {
            return Some((marker.to_string(), rest));
        }
    }

    let digits = content.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &content[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            let marker = &content[..digits + 2];
            return Some((marker.to_string(), rest));
        }
    }

    None
}

fn strip_list_prefix(content: &str) -> String {
    if let Some((_, rest)) = parse_list_marker(content) {
        return rest.to_string();
    }
    content.strip_prefix("> ").unwrap_or(content).to_string()
}

fn strip_marker<'a>(content: &'a str, markers: &[&str]) -> Option<&'a str> {
    markers
        .iter()
        .find_map(|marker| content.strip_prefix(marker))
}

fn strip_ordered(content: &str) -> Option<&str> {
    let digits = content.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &content[digits..];
    rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))
}

fn heading_level(line: &str) -> Option<u8> {
    let (_, content) = split_indent(line);
    let level = content.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &content[level..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some(level as u8)
    } else {
        None
    }
}

fn strip_heading(content: &str) -> String {
    let level = content.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) {
        return content.to_string();
    }
    let rest = &content[level..];
    if rest.is_empty() || rest.starts_with(' ') {
        rest.trim_start().to_string()
    } else {
        content.to_string()
    }
}

fn split_indent(line: &str) -> (&str, &str) {
    let index = line
        .char_indices()
        .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
        .map_or(line.len(), |(index, _)| index);
    line.split_at(index)
}

fn split_lines(chars: &[char]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for ch in chars {
        if *ch == '\n' {
            lines.push(current);
            current = String::new();
        } else {
            current.push(*ch);
        }
    }
    lines.push(current);
    lines
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn line_range(chars: &[char], selection: Option<Selection>) -> (usize, usize) {
    let last_line = chars.iter().filter(|ch| **ch == '\n').count();
    match selection {
        Some((start, end)) => {
            let (start, end) = normalize(start, end);
            (start.0.min(last_line), end.0.min(last_line))
        }
        None => (0, 0),
    }
}

fn selection_offsets(chars: &[char], selection: Option<Selection>) -> (usize, usize) {
    match selection {
        Some((start, end)) => {
            let (start, end) = normalize(start, end);
            (position_to_char(chars, start), position_to_char(chars, end))
        }
        None => (0, 0),
    }
}

fn normalize(a: Position, b: Position) -> (Position, Position) {
    if a <= b { (a, b) } else { (b, a) }
}

fn position_to_char(chars: &[char], position: Position) -> usize {
    let mut line = 0;
    let mut column = 0;
    let mut index = 0;

    while index < chars.len() && (line < position.0 || (line == position.0 && column < position.1))
    {
        if chars[index] == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
        index += 1;
    }

    index
}

fn char_to_position(chars: &[char], offset: usize) -> Position {
    let offset = offset.min(chars.len());
    let mut line = 0;
    let mut column = 0;

    for ch in &chars[..offset] {
        if *ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(start: Position, end: Position) -> Option<Selection> {
        Some((start, end))
    }

    #[test]
    fn bold_wraps_the_selection() {
        let edit = apply_format(
            "hello world",
            selection((0, 6), (0, 11)),
            FormatAction::Bold,
        );

        assert_eq!(edit.text, "hello **world**");
        assert_eq!(edit.selection, Some(((0, 8), (0, 13))));
    }

    #[test]
    fn bold_without_a_selection_inserts_a_placeholder() {
        let edit = apply_format("hello", None, FormatAction::Bold);

        assert_eq!(edit.text, "**bold text**hello");
        assert_eq!(edit.selection, Some(((0, 2), (0, 11))));
    }

    #[test]
    fn italic_and_strikethrough_use_the_right_markers() {
        assert_eq!(
            apply_format("text", selection((0, 0), (0, 4)), FormatAction::Italic).text,
            "_text_"
        );
        assert_eq!(
            apply_format(
                "text",
                selection((0, 0), (0, 4)),
                FormatAction::Strikethrough
            )
            .text,
            "~~text~~"
        );
    }

    #[test]
    fn headings_toggle_on_and_off() {
        let on = apply_format("Title", None, FormatAction::Heading(2));
        assert_eq!(on.text, "## Title");

        let off = apply_format("## Title", None, FormatAction::Heading(2));
        assert_eq!(off.text, "Title");

        let change = apply_format("# Title", None, FormatAction::Heading(3));
        assert_eq!(change.text, "### Title");
    }

    #[test]
    fn lists_toggle_and_number_sequentially() {
        let bullet = apply_format(
            "one\ntwo",
            selection((0, 0), (1, 3)),
            FormatAction::BulletList,
        );
        assert_eq!(bullet.text, "- one\n- two");

        let ordered = apply_format(
            "one\ntwo",
            selection((0, 0), (1, 3)),
            FormatAction::OrderedList,
        );
        assert_eq!(ordered.text, "1. one\n2. two");

        let task = apply_format("one", None, FormatAction::TaskList);
        assert_eq!(task.text, "- [ ] one");

        let off = apply_format(
            "- one\n- two",
            selection((0, 0), (1, 5)),
            FormatAction::BulletList,
        );
        assert_eq!(off.text, "one\ntwo");
    }

    #[test]
    fn blockquote_prefixes_each_selected_line() {
        let edit = apply_format(
            "one\ntwo",
            selection((0, 0), (1, 3)),
            FormatAction::Blockquote,
        );
        assert_eq!(edit.text, "> one\n> two");
    }

    #[test]
    fn table_inserts_a_skeleton_and_places_the_cursor_in_the_first_cell() {
        let edit = apply_format("", None, FormatAction::Table);

        assert!(edit.text.contains("| Column 1 | Column 2 |"));
        assert_eq!(edit.cursor, (0, 2));
    }

    #[test]
    fn link_and_image_accept_urls_and_select_placeholders() {
        let link = insert_link(
            "docs",
            selection((0, 0), (0, 4)),
            "https://example.com",
            false,
        );
        assert_eq!(link.text, "[docs](https://example.com)");

        let placeholder = insert_link("docs", selection((0, 0), (0, 4)), "", false);
        assert_eq!(placeholder.text, "[docs](url)");
        assert_eq!(placeholder.selection, Some(((0, 7), (0, 10))));

        let image = insert_link("alt", selection((0, 0), (0, 3)), "image.png", true);
        assert_eq!(image.text, "![alt](image.png)");
    }

    #[test]
    fn code_block_wraps_or_inserts_fences() {
        let edit = apply_format(
            "let x = 1;",
            selection((0, 0), (0, 10)),
            FormatAction::CodeBlock,
        );
        assert_eq!(edit.text, "```\nlet x = 1;\n```");
    }

    #[test]
    fn continues_bullet_and_ordered_lists() {
        let edit = continue_list("- one", (0, 5)).expect("continue");
        assert_eq!(edit.text, "- one\n- ");
        assert_eq!(edit.cursor, (1, 2));

        let ordered = continue_list("1. one", (0, 6)).expect("continue");
        assert_eq!(ordered.text, "1. one\n2. ");
    }

    #[test]
    fn continues_task_lists() {
        let edit = continue_list("- [x] done", (0, 10)).expect("continue");
        assert_eq!(edit.text, "- [x] done\n- [ ] ");
    }

    #[test]
    fn enter_on_an_empty_item_leaves_the_list() {
        let edit = continue_list("- ", (0, 2)).expect("exit");
        assert_eq!(edit.text, "");
        assert_eq!(edit.cursor, (0, 0));
    }

    #[test]
    fn indents_and_outdents_list_items() {
        let indented = indent_list_item("- item", (0, 2), false).expect("indent");
        assert_eq!(indented.text, "    - item");

        let outdented = indent_list_item("    - item", (0, 6), true).expect("outdent");
        assert_eq!(outdented.text, "- item");
    }

    #[test]
    fn pasted_url_over_selection_becomes_a_link() {
        let edit =
            paste_url("label", selection((0, 0), (0, 5)), "https://example.com").expect("link");

        assert_eq!(edit.text, "[label](https://example.com)");
    }

    #[test]
    fn pasted_url_without_selection_or_scheme_is_ignored() {
        assert!(paste_url("label", None, "https://example.com").is_none());
        assert!(paste_url("label", selection((0, 0), (0, 5)), "not a url").is_none());
    }

    #[test]
    fn multibyte_text_positions_are_character_based() {
        let edit = apply_format("héllo", selection((0, 1), (0, 4)), FormatAction::Bold);
        assert_eq!(edit.text, "h**éll**o");
    }
}
