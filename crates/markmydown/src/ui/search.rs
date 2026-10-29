//! Find and replace panel.
//!
//! The panel owns only search UI state. Matching, replacement, and undo are
//! delegated to [`crate::editor::search`] and the editor adapter so the same
//! rules apply to the toolbar and the keyboard.

use iced::widget::Id;
use iced::widget::{button, container, row, text, text_input};
use iced::{Element, Length};

use crate::editor::search::{self, SearchMatch, SearchOptions};
use crate::theme::tokens::ThemeTokens;

/// Widget id for the find field; used to focus it when the panel opens.
pub fn query_input_id() -> Id {
    Id::new("markmydown-find-query")
}

/// Widget id for the replacement field.
pub fn replacement_input_id() -> Id {
    Id::new("markmydown-find-replacement")
}

/// Actions emitted by the search panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    QueryChanged(String),
    ReplacementChanged(String),
    ToggleCaseSensitive,
    ToggleRegex,
    Next,
    Previous,
    ReplaceCurrent,
    ReplaceAll,
    Close,
}

/// State for the find/replace panel.
#[derive(Debug, Clone, Default)]
pub struct SearchPanel {
    pub visible: bool,
    pub replace_mode: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub matches: Vec<SearchMatch>,
    pub current: Option<usize>,
    pub error: Option<String>,
}

impl SearchPanel {
    pub fn open(&mut self, replace_mode: bool) {
        self.visible = true;
        self.replace_mode = replace_mode;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.matches.clear();
        self.current = None;
        self.error = None;
    }

    pub fn options(&self) -> SearchOptions {
        SearchOptions {
            case_sensitive: self.case_sensitive,
            regex: self.regex,
        }
    }

    /// Re-runs the search against `text`, preserving the current match when it
    /// still exists.
    pub fn refresh(&mut self, text: &str) {
        match search::find_all(text, &self.query, self.options()) {
            Ok(matches) => {
                let previous_start = self.current_match().map(|matched| matched.start);
                self.matches = matches;
                self.error = None;
                self.current = if self.matches.is_empty() {
                    None
                } else {
                    previous_start
                        .and_then(|start| {
                            self.matches
                                .iter()
                                .position(|matched| matched.start == start)
                        })
                        .or(Some(0))
                };
            }
            Err(error) => {
                self.matches.clear();
                self.current = None;
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.current.and_then(|index| self.matches.get(index))
    }

    pub fn next(&mut self) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            Some(index) if index + 1 < self.matches.len() => index + 1,
            _ => 0,
        });
    }

    pub fn previous(&mut self) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            Some(0) | None => self.matches.len() - 1,
            Some(index) => index - 1,
        });
    }

    /// Renders the panel above the editor.
    pub fn view<'a>(&'a self, tokens: &'a ThemeTokens) -> Element<'a, Message> {
        let mut controls = row![
            text_input("Find", &self.query)
                .id(query_input_id())
                .on_input(Message::QueryChanged)
                .padding(4)
                .width(Length::Fixed(190.0)),
            toggle(
                "Aa",
                self.case_sensitive,
                Message::ToggleCaseSensitive,
                tokens
            ),
            toggle(".*", self.regex, Message::ToggleRegex, tokens),
            button(text("Previous").size(11)).on_press(Message::Previous),
            button(text("Next").size(11)).on_press(Message::Next),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        if self.replace_mode {
            controls = controls.push(
                text_input("Replace with", &self.replacement)
                    .id(replacement_input_id())
                    .on_input(Message::ReplacementChanged)
                    .padding(4)
                    .width(Length::Fixed(170.0)),
            );
            controls =
                controls.push(button(text("Replace").size(11)).on_press(Message::ReplaceCurrent));
            controls =
                controls.push(button(text("Replace all").size(11)).on_press(Message::ReplaceAll));
        }

        let count = if let Some(error) = &self.error {
            text(error).size(11).color(tokens.error)
        } else if self.query.is_empty() {
            text("Type to search").size(11).color(tokens.muted)
        } else if self.matches.is_empty() {
            text("No matches").size(11).color(tokens.muted)
        } else {
            text(format!(
                "{} of {}",
                self.current.map_or(0, |index| index + 1),
                self.matches.len()
            ))
            .size(11)
            .color(tokens.muted)
        };

        controls = controls.push(count);
        controls = controls.push(iced::widget::Space::new().width(Length::Fill));
        controls = controls.push(button(text("Close").size(11)).on_press(Message::Close));

        container(controls)
            .padding([4, 8])
            .width(Length::Fill)
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
}

fn toggle<'a>(
    label: &'a str,
    active: bool,
    message: Message,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    button(text(label).size(11))
        .padding([3, 6])
        .on_press(message)
        .style(move |theme, status| {
            let mut style = button::text(theme, status);
            if active {
                style.background = Some(tokens.accent.into());
                style.text_color = tokens.background;
            }
            style
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_finds_matches_and_cycles_through_them() {
        let mut panel = SearchPanel {
            visible: true,
            query: "one".into(),
            ..SearchPanel::default()
        };
        panel.refresh("one two one");

        assert_eq!(panel.matches.len(), 2);
        assert_eq!(panel.current, Some(0));

        panel.next();
        assert_eq!(panel.current, Some(1));
        panel.next();
        assert_eq!(panel.current, Some(0));
        panel.previous();
        assert_eq!(panel.current, Some(1));
    }

    #[test]
    fn case_sensitivity_and_regex_change_the_match_set() {
        let mut panel = SearchPanel {
            query: "one".into(),
            ..SearchPanel::default()
        };
        panel.refresh("One one ONE");
        assert_eq!(panel.matches.len(), 3);

        panel.case_sensitive = true;
        panel.refresh("One one ONE");
        assert_eq!(panel.matches.len(), 1);

        panel.query = r"\b\w+\b".into();
        panel.regex = true;
        panel.case_sensitive = false;
        panel.refresh("one two");
        assert_eq!(panel.matches.len(), 2);
    }

    #[test]
    fn invalid_regex_surfaces_an_error_and_clears_matches() {
        let mut panel = SearchPanel {
            query: "[".into(),
            regex: true,
            ..SearchPanel::default()
        };
        panel.refresh("text");

        assert!(panel.matches.is_empty());
        assert!(panel.current.is_none());
        assert!(
            panel
                .error
                .as_deref()
                .is_some_and(|error| error.contains("invalid regular expression"))
        );
    }

    #[test]
    fn closing_clears_search_state() {
        let mut panel = SearchPanel::default();
        panel.open(true);
        panel.query = "x".into();
        panel.refresh("x x");
        panel.close();

        assert!(!panel.visible);
        assert!(panel.matches.is_empty());
        assert!(panel.current.is_none());
        assert!(panel.error.is_none());
    }
}
