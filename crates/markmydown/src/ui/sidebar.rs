//! Workspace sidebar and file tree.

use std::collections::HashSet;
use std::path::PathBuf;

use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Font, Length};

use crate::theme::tokens::ThemeTokens;
use crate::workspace::tree::{TreeNode, WorkspaceTree};

/// Sidebar actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    OpenFile(PathBuf),
    ToggleFolder(PathBuf),
    OpenFolder,
    NewFile,
    NewFolder,
    ImportFile,
    Rename,
    Delete,
    Move,
}

/// Renders the workspace tree or the empty state.
pub fn view<'a>(
    tree: Option<&'a WorkspaceTree>,
    expanded: &'a HashSet<PathBuf>,
    active: Option<&'a PathBuf>,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    let Some(tree) = tree else {
        return empty_state("Open a folder to start writing.", tokens);
    };

    if tree.files().is_empty() {
        return empty_state("This folder has no Markdown documents yet.", tokens);
    }

    let mut nodes = Vec::new();
    for child in &tree.root_node().children {
        collect_visible(child, 0, expanded, &mut nodes);
    }

    let mut list = column![].spacing(2).width(Length::Fill);
    for (node, depth) in nodes {
        let is_active = active.is_some_and(|path| path == &node.path);
        let label = if node.is_dir {
            let marker = if expanded.contains(&node.path) {
                "▾"
            } else {
                "▸"
            };
            format!("{marker}  {}", node.name)
        } else {
            format!("   {}", node.name)
        };

        let mut row = iced::widget::row![].spacing(2);
        if depth > 0 {
            row = row.push(iced::widget::Space::new().width(depth as f32 * 12.0));
        }

        let color = if is_active {
            tokens.accent
        } else if node.is_dir {
            tokens.muted
        } else {
            tokens.text
        };

        let action = if node.is_dir {
            Message::ToggleFolder(node.path.clone())
        } else {
            Message::OpenFile(node.path.clone())
        };

        row = row.push(
            button(text(label).size(13).font(Font::MONOSPACE).color(color))
                .padding([4, 6])
                .width(Length::Fill)
                .style(move |theme, status| {
                    let mut style = button::text(theme, status);
                    style.text_color = color;
                    if is_active {
                        style.background = Some(tokens.surface_raised.into());
                    }
                    style
                })
                .on_press(action),
        );

        list = list.push(row);
    }

    column![
        scrollable(container(list).padding(8))
            .height(Length::Fill)
            .width(Length::Fill),
        separator(tokens),
        row_of_actions(active.is_some()),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn collect_visible<'a>(
    node: &'a TreeNode,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    out: &mut Vec<(&'a TreeNode, usize)>,
) {
    out.push((node, depth));
    if node.is_dir && expanded.contains(&node.path) {
        for child in &node.children {
            collect_visible(child, depth + 1, expanded, out);
        }
    }
}

fn row_of_actions<'a>(has_active: bool) -> Element<'a, Message> {
    let mut rename = button(text("Rename").size(11));
    let mut delete = button(text("Delete").size(11));
    let mut move_button = button(text("Move").size(11));
    if has_active {
        rename = rename.on_press(Message::Rename);
        delete = delete.on_press(Message::Delete);
        move_button = move_button.on_press(Message::Move);
    }

    iced::widget::column![
        iced::widget::row![
            button(text("New").size(11)).on_press(Message::NewFile),
            button(text("Folder").size(11)).on_press(Message::NewFolder),
            button(text("Import").size(11)).on_press(Message::ImportFile),
        ]
        .spacing(4),
        iced::widget::row![rename, move_button, delete].spacing(4),
    ]
    .spacing(4)
    .padding(8)
    .into()
}

fn empty_state<'a>(message: &'a str, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    container(
        column![
            text(message).color(tokens.muted).size(13),
            iced::widget::row![
                button(text("Open folder").size(12)).on_press(Message::OpenFolder),
                button(text("New document").size(12)).on_press(Message::NewFile),
            ]
            .spacing(6),
        ]
        .spacing(10)
        .width(Length::Fill),
    )
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn separator<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    container("")
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(tokens.border.into()),
            ..container::Style::default()
        })
        .into()
}
