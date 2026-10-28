//! Editor adapter over the vendored `iced-code-editor`.
//!
//! The adapter owns one editor widget per open tab, applies Markdown behaviors
//! as single undo steps, and keeps theme and typography changes in one place.

pub mod markdown;
pub mod outline;
pub mod search;
pub mod stats;

use iced::Task;
use iced::font::Family;
use iced_code_editor::theme::Style;
use iced_code_editor::{CodeEditor, Message as EditorMessage};

use crate::theme::tokens::{ThemeTokens, Typography};
use markdown::{FormatAction, Position, Selection, TextEdit};

/// One editor widget and its Markdown behaviors.
pub struct EditorPane {
    editor: CodeEditor,
}

impl EditorPane {
    pub fn new(text: &str, tokens: &ThemeTokens, typography: &Typography) -> Self {
        let editor = CodeEditor::new(text, "markdown");
        let mut pane = Self { editor };
        pane.apply_theme(tokens, typography);
        pane
    }

    pub fn view(&self) -> iced::Element<'_, EditorMessage> {
        self.editor.view()
    }

    /// Updates colors and typography from the shared tokens.
    pub fn apply_theme(&mut self, tokens: &ThemeTokens, typography: &Typography) {
        self.editor.set_theme(Style {
            background: tokens.background,
            text_color: tokens.text,
            gutter_background: tokens.surface,
            gutter_border: tokens.border,
            line_number_color: tokens.muted,
            scrollbar_background: tokens.surface,
            scroller_color: tokens.border,
            current_line_highlight: tokens.surface_raised,
            whitespace_color: tokens.muted,
            indent_guide_color: tokens.border,
            search_match_color: tokens.selection,
            search_match_current_color: tokens.accent,
            selection_color: tokens.selection,
            bracket_match_color: tokens.accent,
            ime_preedit_background_color: tokens.surface_raised,
        });

        self.editor.set_font(iced::Font {
            family: Family::Name(intern_font(&typography.font_family)),
            ..iced::Font::MONOSPACE
        });
        self.editor.set_font_size(typography.font_size, false);
        self.editor
            .set_line_height(typography.font_size * typography.line_height);
        self.editor.set_wrap_enabled(true);
        self.editor.set_line_numbers_enabled(true);
        self.editor.set_show_whitespace(false);
    }

    pub fn content(&self) -> String {
        self.editor.content()
    }

    pub fn cursor(&self) -> Position {
        self.editor.cursor_position()
    }

    pub fn selection(&self) -> Option<Selection> {
        self.editor.selection_range()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.editor.selected_text()
    }

    pub fn line_text(&self, line: usize) -> Option<&str> {
        self.editor.line_text(line)
    }

    pub fn line_count(&self) -> usize {
        self.editor.line_count()
    }

    pub fn set_focus_mode(&mut self, enabled: bool) {
        self.editor.set_focus_mode(enabled);
    }

    /// Selects a range and scrolls it into view.
    pub fn select_range(&mut self, start: Position, end: Position) -> Task<EditorMessage> {
        self.editor.set_selection(start, end)
    }

    /// Moves the caret to a position and scrolls it into view.
    pub fn goto_position(&mut self, position: Position) -> Task<EditorMessage> {
        self.editor.goto_position(position)
    }

    /// Applies a pure edit as one undo step, then restores the selection.
    pub fn apply_edit(&mut self, edit: TextEdit) -> Task<EditorMessage> {
        let mut tasks = Vec::with_capacity(2);

        if self.editor.content() != edit.text {
            tasks.push(self.editor.replace_all_text(&edit.text));
        }

        if let Some((start, end)) = edit.selection {
            tasks.push(self.editor.set_selection(start, end));
        } else {
            tasks.push(self.editor.goto_position(edit.cursor));
        }

        Task::batch(tasks)
    }

    /// Applies a formatting action to the selection or caret.
    pub fn apply_format(&mut self, action: FormatAction) -> Task<EditorMessage> {
        let selection = self.selection_or_caret();
        let edit = markdown::apply_format(&self.editor.content(), selection, action);
        self.apply_edit(edit)
    }

    /// Inserts a link or image, usually from the URL popover.
    pub fn insert_link(&mut self, url: &str, image: bool) -> Task<EditorMessage> {
        let selection = self.selection_or_caret();
        let edit = markdown::insert_link(&self.editor.content(), selection, url, image);
        self.apply_edit(edit)
    }

    pub fn replace_all(
        &mut self,
        query: &str,
        replacement: &str,
        options: search::SearchOptions,
    ) -> Result<Task<EditorMessage>, search::SearchError> {
        let edit = search::replace_all(&self.editor.content(), query, replacement, options)?;
        Ok(match edit {
            Some(edit) => self.apply_edit(edit),
            None => Task::none(),
        })
    }

    pub fn find_next(
        &self,
        cursor: Position,
        query: &str,
        options: search::SearchOptions,
    ) -> Result<Option<search::SearchMatch>, search::SearchError> {
        search::find_next(&self.editor.content(), cursor, query, options)
    }

    pub fn find_previous(
        &self,
        cursor: Position,
        query: &str,
        options: search::SearchOptions,
    ) -> Result<Option<search::SearchMatch>, search::SearchError> {
        search::find_previous(&self.editor.content(), cursor, query, options)
    }

    /// Replaces the editor content from disk and clears undo history.
    pub fn reset(&mut self, text: &str) -> Task<EditorMessage> {
        self.editor.reset(text)
    }

    pub fn update(&mut self, message: EditorMessage) -> Task<EditorMessage> {
        match &message {
            EditorMessage::Paste(text) if !text.is_empty() => {
                let selection = self.selection_or_caret();
                if let Some(edit) = markdown::paste_url(&self.editor.content(), selection, text) {
                    return self.apply_edit(edit);
                }
            }
            EditorMessage::Enter => {
                let cursor = self.editor.cursor_position();
                if let Some(edit) = markdown::continue_list(&self.editor.content(), cursor) {
                    return self.apply_edit(edit);
                }
            }
            EditorMessage::Tab => {
                let cursor = self.editor.cursor_position();
                if let Some(edit) =
                    markdown::indent_list_item(&self.editor.content(), cursor, false)
                {
                    return self.apply_edit(edit);
                }
            }
            EditorMessage::FocusNavigationShiftTab => {
                let cursor = self.editor.cursor_position();
                if let Some(edit) = markdown::indent_list_item(&self.editor.content(), cursor, true)
                {
                    return self.apply_edit(edit);
                }
                return Task::none();
            }
            _ => {}
        }

        self.editor.update(&message)
    }

    fn selection_or_caret(&self) -> Option<Selection> {
        let cursor = self.editor.cursor_position();
        Some(self.editor.selection_range().unwrap_or((cursor, cursor)))
    }
}

/// Whether a message can change buffer content. Used to avoid cloning a large
/// document on cursor-only and scroll messages.
pub fn message_may_edit(message: &EditorMessage) -> bool {
    matches!(
        message,
        EditorMessage::CharacterInput(_)
            | EditorMessage::VimKey(_)
            | EditorMessage::Backspace
            | EditorMessage::Delete
            | EditorMessage::Enter
            | EditorMessage::Tab
            | EditorMessage::Cut
            | EditorMessage::Paste(_)
            | EditorMessage::DeleteSelection
            | EditorMessage::Undo
            | EditorMessage::Redo
            | EditorMessage::ReplaceNext
            | EditorMessage::ReplaceAll
    )
}

fn intern_font(name: &str) -> &'static str {
    Box::leak(name.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane() -> EditorPane {
        EditorPane::new(
            "# Title\n\n- item",
            &ThemeTokens::light(),
            &Typography::editor_default(),
        )
    }

    #[test]
    fn uses_the_markdown_grammar() {
        let pane = pane();
        assert_eq!(pane.content(), "# Title\n\n- item");
    }

    #[test]
    fn formatting_replaces_the_document_and_selects_content() {
        let mut pane = pane();
        let task = pane.apply_format(FormatAction::Bold);
        let _ = task;

        assert!(pane.content().starts_with("**bold text**"));
    }

    #[test]
    fn focus_mode_is_forwarded() {
        let mut pane = pane();
        pane.set_focus_mode(true);
        assert!(pane.editor.focus_mode());
    }

    #[test]
    fn smart_enter_continues_a_list() {
        let mut pane = EditorPane::new(
            "- item",
            &ThemeTokens::light(),
            &Typography::editor_default(),
        );
        let _ = pane.update(EditorMessage::End(false));
        let _ = pane.update(EditorMessage::Enter);

        assert_eq!(pane.content(), "- item\n- ");
    }

    #[test]
    fn message_classification_covers_edits() {
        assert!(message_may_edit(&EditorMessage::CharacterInput('a')));
        assert!(message_may_edit(&EditorMessage::Undo));
        assert!(!message_may_edit(&EditorMessage::Copy));
        assert!(!message_may_edit(&EditorMessage::Tick));
    }
}
