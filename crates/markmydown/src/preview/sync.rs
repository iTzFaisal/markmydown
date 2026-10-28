//! Source-range to scroll-position synchronization.
//!
//! The editor and preview do not share a layout engine, so synchronization is
//! block based: a source line maps to the block that contains it, and a
//! preview scroll fraction maps to the start line of the corresponding block.
//! The mapping is monotonic, which keeps long documents from visibly jumping.

use super::model::{PreviewDocument, SourceRange};

/// Finds the block containing `line`, or the closest preceding block.
pub fn block_for_line(document: &PreviewDocument, line: usize) -> Option<usize> {
    if document.blocks.is_empty() {
        return None;
    }

    if let Some(index) = document.block_for_line(line) {
        return Some(index);
    }

    document
        .blocks
        .iter()
        .rposition(|block| block.range().end_line < line)
}

/// Returns the editor line for a block index.
pub fn line_for_block(document: &PreviewDocument, index: usize) -> Option<usize> {
    document.line_for_block(index)
}

/// Converts an editor line to a normalized preview scroll fraction.
pub fn preview_progress_for_line(document: &PreviewDocument, line: usize) -> f32 {
    let Some(index) = block_for_line(document, line) else {
        return 0.0;
    };

    block_progress(document, index)
}

fn block_progress(document: &PreviewDocument, index: usize) -> f32 {
    if document.blocks.len() <= 1 {
        return 0.0;
    }
    index as f32 / (document.blocks.len() - 1) as f32
}

/// Converts a normalized preview scroll fraction to an editor line.
pub fn editor_line_for_editor_progress(document: &PreviewDocument, progress: f32) -> usize {
    if document.blocks.is_empty() {
        return 0;
    }

    let progress = progress.clamp(0.0, 1.0);
    let index = ((document.blocks.len() - 1) as f32 * progress).round() as usize;
    document
        .blocks
        .get(index)
        .map_or(0, |block| block.range().start_line)
}

/// Converts a normalized preview scroll fraction to an editor line.
pub fn editor_line_for_preview_progress(document: &PreviewDocument, progress: f32) -> usize {
    editor_line_for_editor_progress(document, progress)
}

/// Reading progress from a scrollable viewport.
pub fn reading_progress(scroll: f32, content_height: f32, viewport_height: f32) -> f32 {
    let scrollable = (content_height - viewport_height).max(1.0);
    (scroll / scrollable).clamp(0.0, 1.0)
}

/// A monotonic pair used by the scroll-sync tests and app state.
pub fn ranges(document: &PreviewDocument) -> Vec<SourceRange> {
    document.blocks.iter().map(|block| block.range()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::model::Block;

    fn document() -> PreviewDocument {
        PreviewDocument {
            blocks: vec![
                Block::Paragraph {
                    inlines: Vec::new(),
                    range: SourceRange::new(0, 2),
                },
                Block::Paragraph {
                    inlines: Vec::new(),
                    range: SourceRange::new(4, 8),
                },
                Block::Paragraph {
                    inlines: Vec::new(),
                    range: SourceRange::new(10, 14),
                },
            ],
            footnotes: Vec::new(),
            line_offset: 0,
        }
    }

    #[test]
    fn maps_lines_to_blocks_and_back() {
        let document = document();

        assert_eq!(block_for_line(&document, 1), Some(0));
        assert_eq!(block_for_line(&document, 5), Some(1));
        assert_eq!(block_for_line(&document, 9), Some(1));
        assert_eq!(line_for_block(&document, 2), Some(10));
    }

    #[test]
    fn progress_is_monotonic_and_reversible() {
        let document = document();
        let first = preview_progress_for_line(&document, 1);
        let middle = preview_progress_for_line(&document, 5);
        let last = preview_progress_for_line(&document, 12);

        assert!(first < middle);
        assert!(middle < last);
        assert_eq!(editor_line_for_preview_progress(&document, 0.0), 0);
        assert_eq!(editor_line_for_preview_progress(&document, 1.0), 10);
    }

    #[test]
    fn long_document_sync_never_moves_backwards() {
        let document = document();
        let mut previous = -1.0;

        for line in 0..=14 {
            let progress = preview_progress_for_line(&document, line);
            assert!(progress >= previous);
            previous = progress;
        }
    }

    #[test]
    fn reading_progress_clamps() {
        assert_eq!(reading_progress(0.0, 1000.0, 500.0), 0.0);
        assert_eq!(reading_progress(250.0, 1000.0, 500.0), 0.5);
        assert_eq!(reading_progress(900.0, 1000.0, 500.0), 1.0);
    }
}
