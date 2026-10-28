//! Document statistics and reading progress.

use iced::widget::{container, row, text};
use iced::{Element, Length};

use crate::editor::stats::TextStats;
use crate::theme::tokens::ThemeTokens;

/// Everything the status bar displays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusInfo {
    pub stats: TextStats,
    pub selection: Option<TextStats>,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub reading_progress: Option<f32>,
    pub message: String,
}

/// Renders the bottom status bar.
pub fn view<'a, M: Clone + 'a>(info: StatusInfo, tokens: &'a ThemeTokens) -> Element<'a, M> {
    let mut content = row![]
        .spacing(14)
        .align_y(iced::alignment::Vertical::Center);

    content = content.push(
        text(if info.message.is_empty() {
            String::from("Ready")
        } else {
            info.message.clone()
        })
        .size(12)
        .color(tokens.muted),
    );

    content = content.push(
        text(format!(
            "{} words  ·  {} chars  ·  {} min read",
            info.stats.words, info.stats.characters, info.stats.reading_minutes
        ))
        .size(12)
        .color(tokens.muted),
    );

    if let Some(selection) = &info.selection {
        content = content.push(
            text(format!(
                "sel {} words  ·  {} chars",
                selection.words, selection.characters
            ))
            .size(12)
            .color(tokens.accent),
        );
    }

    if let Some(progress) = info.reading_progress {
        content = content.push(
            text(format!("{:.0}% read", progress * 100.0))
                .size(12)
                .color(tokens.muted),
        );
    }

    content = content.push(iced::widget::Space::new().width(Length::Fill));
    content = content.push(
        text(format!(
            "Ln {}, Col {}",
            info.cursor_line, info.cursor_column
        ))
        .size(12)
        .color(tokens.muted),
    );

    container(content)
        .width(Length::Fill)
        .padding([5, 10])
        .style(move |_| container::Style {
            background: Some(tokens.surface.into()),
            border: iced::border::Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
