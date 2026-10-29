//! Heading outline panel.

use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Length};

use crate::editor::outline::Heading;
use crate::theme::tokens::ThemeTokens;

/// Actions emitted by the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Jump to a zero-based source line.
    Goto(usize),
}

/// Renders the headings of the active document.
pub fn view<'a>(
    headings: &[Heading],
    active_line: Option<usize>,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    let mut list = column![].spacing(1).width(Length::Fill);

    for heading in headings {
        let indent = (heading.level.saturating_sub(1) as f32) * 10.0;
        let active = active_line == Some(heading.line);
        let color = if active { tokens.accent } else { tokens.text };
        let label = if heading.text.is_empty() {
            String::from("(untitled)")
        } else {
            heading.text.clone()
        };

        list = list.push(
            iced::widget::row![
                iced::widget::Space::new().width(indent),
                button(
                    text(label)
                        .size(12 - u32::from(heading.level.min(3)))
                        .color(color)
                        .width(Length::Fill),
                )
                .padding([2, 4])
                .width(Length::Fill)
                .style(move |theme, status| {
                    let mut style = button::text(theme, status);
                    if active {
                        style.background = Some(tokens.surface_raised.into());
                    }
                    style
                })
                .on_press(Message::Goto(heading.line)),
            ]
            .width(Length::Fill),
        );
    }

    if headings.is_empty() {
        list = list.push(text("No headings").size(12).color(tokens.muted));
    }

    container(
        column![
            text("Outline").size(12).color(tokens.muted),
            scrollable(container(list).padding([4, 6])).height(Length::Fill),
        ]
        .spacing(4)
        .height(Length::Fill),
    )
    .padding([6, 4])
    .width(Length::Fixed(200.0))
    .height(Length::Fill)
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
