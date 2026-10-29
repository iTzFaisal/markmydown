//! Quick-open fuzzy search.

use std::path::{Path, PathBuf};

use iced::widget::{Space, button, column, container, scrollable, text, text_input};
use iced::{Element, Length};

use crate::theme::tokens::ThemeTokens;

/// User actions in the quick-open palette.
#[derive(Debug, Clone)]
pub enum Message {
    QueryChanged(String),
    Move(i32),
    Accept,
    Open(PathBuf),
    Close,
}

/// The search field id used to focus the palette.
pub fn input_id() -> iced::widget::Id {
    iced::widget::Id::new("markmydown-quick-open")
}

/// A workspace document offered by quick open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub relative: String,
}

/// Quick-open state.
#[derive(Debug, Clone)]
pub struct QuickOpen {
    pub query: String,
    pub selected: usize,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
}

impl QuickOpen {
    pub fn new(entries: Vec<Entry>) -> Self {
        let filtered = (0..entries.len()).collect();
        Self {
            query: String::new(),
            selected: 0,
            entries,
            filtered,
        }
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }

        let length = self.filtered.len() as i32;
        let mut next = self.selected as i32 + delta;
        next = next.rem_euclid(length);
        self.selected = next as usize;
    }

    pub fn filtered(&self) -> impl Iterator<Item = &Entry> {
        self.filtered
            .iter()
            .filter_map(|index| self.entries.get(*index))
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.filtered
            .get(self.selected)
            .and_then(|index| self.entries.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.filtered.is_empty()
    }

    fn refilter(&mut self) {
        let mut scored: Vec<(i32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                fuzzy_score(&self.query, &entry.title)
                    .or_else(|| fuzzy_score(&self.query, &entry.relative))
                    .map(|score| (score, index))
            })
            .collect();

        scored.sort_by(|(left_score, left_index), (right_score, right_index)| {
            right_score.cmp(left_score).then_with(|| {
                self.entries[*left_index]
                    .title
                    .cmp(&self.entries[*right_index].title)
            })
        });

        self.filtered = scored.into_iter().map(|(_, index)| index).collect();
        self.selected = 0;
    }
}

/// Case-insensitive subsequence scoring. Matching at the start of the string
/// or after a separator scores higher; consecutive matches score higher.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query: Vec<char> = query.to_lowercase().chars().collect();
    let candidate_lower = candidate.to_lowercase();
    let candidate_chars: Vec<char> = candidate_lower.chars().collect();

    let mut score = 0;
    let mut query_index = 0;
    let mut previous_match: Option<usize> = None;

    for (index, ch) in candidate_chars.iter().enumerate() {
        if query_index >= query.len() {
            break;
        }

        if *ch == query[query_index] {
            if index == 0 || !candidate_chars[index - 1].is_alphanumeric() {
                score += 12;
            }
            if previous_match == Some(index.saturating_sub(1)) {
                score += 8;
            }
            score += 2;
            previous_match = Some(index);
            query_index += 1;
        }
    }

    if query_index == query.len() {
        Some(score)
    } else {
        None
    }
}

/// Renders the quick-open palette over the workspace.
pub fn view<'a>(quick: &'a QuickOpen, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let results =
        quick
            .filtered()
            .take(12)
            .enumerate()
            .fold(column![].spacing(2), |list, (index, entry)| {
                let selected = index == quick.selected;
                let background = if selected {
                    tokens.selection
                } else {
                    tokens.surface
                };

                list.push(
                    button(
                        column![
                            text(entry.title.clone()).size(13).color(tokens.text),
                            text(entry.relative.clone()).size(10).color(tokens.muted),
                        ]
                        .spacing(1),
                    )
                    .width(Length::Fill)
                    .padding([5, 8])
                    .on_press(Message::Open(entry.path.clone()))
                    .style(move |theme, status| {
                        let mut style = button::text(theme, status);
                        style.background = Some(background.into());
                        style
                    }),
                )
            });

    let results: Element<'a, Message> = if quick.is_empty() {
        container(text("No matching documents").size(12).color(tokens.muted))
            .padding(10)
            .into()
    } else {
        scrollable(results).height(Length::Fixed(320.0)).into()
    };

    let card = container(
        column![
            text_input("Type to find a document…", &quick.query)
                .id(input_id())
                .on_input(Message::QueryChanged)
                .on_submit(Message::Accept)
                .padding(8),
            results,
            row_hint(tokens),
        ]
        .spacing(8)
        .padding(12),
    )
    .width(Length::Fixed(560.0))
    .style(move |_| container::Style {
        background: Some(tokens.surface_raised.into()),
        border: iced::border::Border {
            color: tokens.border,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.28),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 20.0,
        },
        ..container::Style::default()
    });

    container(card).center_x(Length::Fill).padding(60).into()
}

fn row_hint<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    iced::widget::row![
        text("↑↓ move").size(10).color(tokens.muted),
        Space::new().width(Length::Fixed(12.0)),
        text("Enter open").size(10).color(tokens.muted),
        Space::new().width(Length::Fixed(12.0)),
        text("Esc close").size(10).color(tokens.muted),
        Space::new().width(Length::Fill),
        button(text("Close").size(10)).on_press(Message::Close),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// Builds entries from a workspace tree.
pub fn entries_from_tree(root: &Path, tree: &crate::workspace::tree::WorkspaceTree) -> Vec<Entry> {
    tree.files()
        .into_iter()
        .map(|node| {
            let relative = node.path.strip_prefix(root).map_or_else(
                |_| node.path.display().to_string(),
                |path| path.display().to_string(),
            );
            Entry {
                path: node.path.clone(),
                title: node.name.clone(),
                relative,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                path: PathBuf::from("/w/readme.md"),
                title: String::from("readme"),
                relative: String::from("readme.md"),
            },
            Entry {
                path: PathBuf::from("/w/notes/rust.md"),
                title: String::from("rust"),
                relative: String::from("notes/rust.md"),
            },
            Entry {
                path: PathBuf::from("/w/notes/roadmap.md"),
                title: String::from("roadmap"),
                relative: String::from("notes/roadmap.md"),
            },
        ]
    }

    #[test]
    fn fuzzy_matching_prefers_prefixes_and_subsequences() {
        assert!(fuzzy_score("read", "readme").is_some());
        assert!(fuzzy_score("rdm", "roadmap").is_some());
        assert!(fuzzy_score("xyz", "readme").is_none());

        let prefix = fuzzy_score("rust", "rust").expect("prefix");
        let scattered = fuzzy_score("rust", "roadmap notes rust").expect("scattered");
        assert!(prefix > scattered);
    }

    #[test]
    fn filtering_and_selection_wrap() {
        let mut quick = QuickOpen::new(entries());
        quick.set_query("r");

        assert_eq!(quick.filtered().count(), 3);
        quick.move_selection(-1);
        assert!(quick.selected_entry().is_some());
        quick.move_selection(1);
        assert!(quick.selected_entry().is_some());
    }

    #[test]
    fn empty_query_lists_everything() {
        let quick = QuickOpen::new(entries());
        assert_eq!(quick.filtered().count(), 3);
    }

    #[test]
    fn no_matches_is_empty() {
        let mut quick = QuickOpen::new(entries());
        quick.set_query("zzzz");

        assert!(quick.is_empty());
        assert!(quick.selected_entry().is_none());
    }
}
