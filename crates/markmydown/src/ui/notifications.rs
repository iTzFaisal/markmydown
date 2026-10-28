//! Toasts and error notifications.

use std::time::{Duration, Instant};

use iced::widget::{button, container, row, text};
use iced::{Element, Length};

use crate::theme::tokens::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub message: String,
    pub kind: NotificationKind,
    pub created_at: Instant,
}

/// A small queue of recent notifications.
#[derive(Debug, Default)]
pub struct NotificationCenter {
    next_id: u64,
    items: Vec<Notification>,
}

impl NotificationCenter {
    pub fn push(&mut self, kind: NotificationKind, message: impl Into<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Notification {
            id,
            message: message.into(),
            kind,
            created_at: Instant::now(),
        });

        if self.items.len() > 4 {
            self.items.remove(0);
        }
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|notification| notification.id != id);
    }

    pub fn expire(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.items
            .retain(|notification| now.duration_since(notification.created_at) < max_age);
    }

    pub fn latest(&self) -> Option<&Notification> {
        self.items.last()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Renders the notification stack.
pub fn view<'a, M: Clone + 'a>(
    center: &'a NotificationCenter,
    tokens: &'a ThemeTokens,
    on_dismiss: impl Fn(u64) -> M + 'a,
) -> Element<'a, M> {
    let mut stack = iced::widget::column![].spacing(4).width(Length::Fill);

    for notification in &center.items {
        let color = match notification.kind {
            NotificationKind::Info => tokens.muted,
            NotificationKind::Success => tokens.success,
            NotificationKind::Error => tokens.error,
        };
        let background = tokens.surface_raised;
        let border = tokens.border;

        stack = stack.push(
            container(
                row![
                    text(notification.message.clone())
                        .size(12)
                        .color(color)
                        .width(Length::Fill),
                    button(text("×").size(12).color(color))
                        .padding([0, 4])
                        .on_press(on_dismiss(notification.id)),
                ]
                .spacing(6)
                .align_y(iced::alignment::Vertical::Center),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(background.into()),
                border: iced::border::Border {
                    color: border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    stack.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_can_be_dismissed_and_expired() {
        let mut center = NotificationCenter::default();
        center.push(NotificationKind::Info, "one");
        center.push(NotificationKind::Error, "two");

        assert_eq!(center.len(), 2);
        assert_eq!(center.latest().expect("latest").message, "two");

        center.dismiss(0);
        assert_eq!(center.len(), 1);

        center.expire(Duration::ZERO);
        assert!(center.is_empty());
    }

    #[test]
    fn queue_is_bounded() {
        let mut center = NotificationCenter::default();
        for index in 0..10 {
            center.push(NotificationKind::Info, format!("message {index}"));
        }

        assert_eq!(center.len(), 4);
    }
}
