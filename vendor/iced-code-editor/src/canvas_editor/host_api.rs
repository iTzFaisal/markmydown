//! Host-facing editing API used by the markmydown adapter.
//!
//! These methods expose selection-aware edits as single undo steps and add
//! focus-mode controls without leaking the editor's internal cursor types.

use iced::Task;

use super::Message;
use super::editing::command::{
    Command, CompositeCommand, DeleteRangeCommand, InsertTextCommand,
};

impl super::CodeEditor {
    /// Primary selection as normalized `(start, end)` positions.
    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        self.get_selection_range()
    }

    /// Selected text of the primary cursor.
    pub fn selected_text(&self) -> Option<String> {
        self.get_selected_text()
    }

    /// Selects from `anchor` to `cursor`. Columns are clamped to line lengths.
    pub fn set_selection(
        &mut self,
        anchor: (usize, usize),
        cursor: (usize, usize),
    ) -> Task<Message> {
        let anchor = self.clamp_position(anchor);
        let cursor = self.clamp_position(cursor);

        self.cursors.set_single(anchor);
        let primary = self.cursors.primary_mut();
        primary.position = cursor;
        primary.anchor = if anchor == cursor {
            None
        } else {
            Some(anchor)
        };
        self.finish_navigation_operation();
        self.scroll_to_cursor()
    }

    /// Replaces the current selection (or inserts at the caret) as one undo
    /// step.
    pub fn replace_selection_with(&mut self, text: &str) -> Task<Message> {
        let selection = self.selection_range();
        let start = selection
            .map(|(start, _)| start)
            .unwrap_or_else(|| self.cursors.primary_position());

        let mut composite = CompositeCommand::new();
        if let Some((start, end)) = selection {
            composite.add(Box::new(DeleteRangeCommand::new(
                &self.buffer,
                start,
                end,
                self.cursors.primary_position(),
            )));
        }
        composite.add(Box::new(InsertTextCommand::new(
            start.0,
            start.1,
            text.to_string(),
            self.cursors.primary_position(),
        )));

        self.apply_composite(composite)
    }

    /// Replaces the whole document as one undo step. Used by replace-all and
    /// by external reloads that should remain undoable.
    pub fn replace_all_text(&mut self, text: &str) -> Task<Message> {
        if self.content() == text {
            return Task::none();
        }

        let last_line = self.buffer.line_count().saturating_sub(1);
        let last_column = self.buffer.line_len(last_line);

        let mut composite = CompositeCommand::new();
        composite.add(Box::new(DeleteRangeCommand::new(
            &self.buffer,
            (0, 0),
            (last_line, last_column),
            self.cursors.primary_position(),
        )));
        composite.add(Box::new(InsertTextCommand::new(
            0,
            0,
            text.to_string(),
            (0, 0),
        )));

        self.end_grouping_if_active();
        self.pre_edit_line = 0;
        let mut cursor = (0, 0);
        composite.execute(&mut self.buffer, &mut cursor);
        self.history.push(Box::new(composite));
        self.cursors.set_single(cursor);
        self.finish_edit_operation();
        self.scroll_to_cursor()
    }

    /// Scrolls the editor to an absolute vertical offset.
    ///
    /// Used by the host to restore a session's scroll position after opening
    /// a document.
    pub fn scroll_to_vertical(&mut self, offset: f32) -> Task<Message> {
        iced::widget::operation::scroll_to(
            self.scrollable_id.clone(),
            iced::widget::scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset.max(0.0)),
            },
        )
    }

    /// Moves the caret to a zero-based position.
    pub fn goto_position(&mut self, position: (usize, usize)) -> Task<Message> {
        let position = self.clamp_position(position);
        self.cursors.set_single(position);
        self.finish_navigation_operation();
        self.scroll_to_cursor()
    }

    /// Text of one logical line.
    pub fn line_text(&self, line: usize) -> Option<&str> {
        (line < self.buffer.line_count()).then(|| self.buffer.line(line))
    }

    /// Number of characters in one logical line.
    pub fn line_char_count(&self, line: usize) -> Option<usize> {
        (line < self.buffer.line_count()).then(|| self.buffer.line_len(line))
    }

    /// Number of logical lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    /// Enables focus mode: non-current text is dimmed and the caret is kept
    /// vertically centered.
    pub fn set_focus_mode(&mut self, enabled: bool) {
        if self.focus_mode == enabled {
            return;
        }

        self.focus_mode = enabled;
        self.content_cache.clear();
        self.overlay_cache.clear();
    }

    /// Whether focus mode is enabled.
    pub fn focus_mode(&self) -> bool {
        self.focus_mode
    }

    /// Replaces the host-provided search decorations.
    ///
    /// Ranges are half-open logical `(line, column)` pairs. They are drawn
    /// under the cursor and selection overlays, with `current` using the
    /// accent color. Pass an empty vector to clear them.
    pub fn set_host_highlights(
        &mut self,
        highlights: Vec<((usize, usize), (usize, usize))>,
        current: Option<usize>,
    ) {
        self.host_highlights = highlights;
        self.host_highlight_current = current;
        self.overlay_cache.clear();
    }

    /// Clears host-provided search decorations.
    pub fn clear_host_highlights(&mut self) {
        if self.host_highlights.is_empty() && self.host_highlight_current.is_none() {
            return;
        }
        self.host_highlights.clear();
        self.host_highlight_current = None;
        self.overlay_cache.clear();
    }

    /// 0 hides non-current text, 1 disables dimming. Clamped to 0–1.
    pub fn set_focus_dim_strength(&mut self, strength: f32) {
        self.focus_dim_strength = strength.clamp(0.0, 1.0);
        self.content_cache.clear();
    }

    fn apply_composite(&mut self, mut composite: CompositeCommand) -> Task<Message> {
        self.end_grouping_if_active();
        self.pre_edit_line = self.min_active_line();

        let mut cursor = self.cursors.primary_position();
        composite.execute(&mut self.buffer, &mut cursor);
        self.history.push(Box::new(composite));

        self.cursors.set_single(cursor);
        self.finish_edit_operation();
        self.scroll_to_cursor()
    }

    fn clamp_position(&self, position: (usize, usize)) -> (usize, usize) {
        let line = position.0.min(self.buffer.line_count().saturating_sub(1));
        let column = position.1.min(self.buffer.line_len(line));
        (line, column)
    }

    /// Opacity applied to a logical line in focus mode.
    pub(crate) fn focus_opacity(&self, line: usize) -> f32 {
        if !self.focus_mode {
            return 1.0;
        }

        let current = self.cursors.primary_position().0 as isize;
        let distance = (line as isize - current).unsigned_abs();
        match distance {
            0 | 1 => 1.0,
            2 => 1.0 - self.focus_dim_strength * 0.15,
            3 => 1.0 - self.focus_dim_strength * 0.35,
            4 => 1.0 - self.focus_dim_strength * 0.55,
            _ => 1.0 - self.focus_dim_strength,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::canvas_editor::CodeEditor;

    #[test]
    fn focus_mode_dims_lines_away_from_the_caret_and_restores_them() {
        let mut editor = CodeEditor::new("one\ntwo\nthree\nfour\nfive\nsix", "txt");
        editor.cursors.set_single((2, 0));

        editor.set_focus_mode(true);
        assert!(editor.focus_mode());
        assert_eq!(editor.focus_opacity(2), 1.0);
        assert_eq!(editor.focus_opacity(1), 1.0);
        assert!(editor.focus_opacity(0) < 1.0);
        assert!(editor.focus_opacity(5) < 1.0);

        editor.set_focus_mode(false);
        assert!(!editor.focus_mode());
        assert_eq!(editor.focus_opacity(0), 1.0);
    }

    #[test]
    fn host_highlights_accept_ranges_and_clear() {
        let mut editor = CodeEditor::new("one two", "txt");

        editor.set_host_highlights(vec![((0, 0), (0, 3)), ((0, 4), (0, 7))], Some(1));
        assert_eq!(editor.host_highlights.len(), 2);
        assert_eq!(editor.host_highlight_current, Some(1));

        editor.clear_host_highlights();
        assert!(editor.host_highlights.is_empty());
        assert!(editor.host_highlight_current.is_none());
    }
}
