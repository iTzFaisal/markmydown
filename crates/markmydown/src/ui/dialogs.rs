//! Modal dialogs and overlays.

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::theme::tokens::ThemeTokens;

/// A confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

/// An input dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub title: String,
    pub label: String,
    pub value: String,
    pub placeholder: String,
}

/// Renders a modal confirmation card.
pub fn confirm<'a, M: Clone + 'a>(
    state: &'a Confirm,
    tokens: &'a ThemeTokens,
    on_confirm: M,
    on_cancel: M,
) -> Element<'a, M> {
    modal(
        column![
            text(state.title.clone()).size(16).color(tokens.heading),
            text(state.message.clone()).size(13).color(tokens.text),
            row![
                button(text("Cancel").size(12)).on_press(on_cancel),
                button(text(state.confirm_label.clone()).size(12)).on_press(on_confirm),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .width(Length::Fill),
        tokens,
    )
}

/// Renders a modal input card.
pub fn input<'a, M: Clone + 'a>(
    state: &'a Input,
    tokens: &'a ThemeTokens,
    on_input: impl Fn(String) -> M + 'a,
    on_submit: M,
    on_cancel: M,
) -> Element<'a, M> {
    modal(
        column![
            text(state.title.clone()).size(16).color(tokens.heading),
            text(state.label.clone()).size(12).color(tokens.muted),
            text_input(state.placeholder.as_str(), state.value.as_str())
                .on_input(on_input)
                .on_submit(on_submit.clone())
                .padding(8),
            row![
                button(text("Cancel").size(12)).on_press(on_cancel),
                button(text("OK").size(12)).on_press(on_submit),
            ]
            .spacing(8),
        ]
        .spacing(10)
        .width(Length::Fill),
        tokens,
    )
}

fn modal<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    tokens: &'a ThemeTokens,
) -> Element<'a, M> {
    container(
        container(content)
            .padding(18)
            .width(Length::Fixed(420.0))
            .style(move |_| container::Style {
                background: Some(tokens.surface.into()),
                border: iced::border::Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 16.0,
                },
                ..container::Style::default()
            }),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .padding(20)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_state_is_plain_data() {
        let confirm = Confirm {
            title: String::from("Delete"),
            message: String::from("Move to trash?"),
            confirm_label: String::from("Delete"),
        };
        let input = Input {
            title: String::from("Rename"),
            label: String::from("New name"),
            value: String::from("note.md"),
            placeholder: String::from("name"),
        };

        assert_eq!(confirm.title, "Delete");
        assert_eq!(input.value, "note.md");
    }
}
