//! Appearance and preferences panel.
//!
//! The panel edits the same `Settings` value that is persisted to disk. Theme
//! hover previews are temporary: they change the live tokens but never the
//! saved theme until the user applies one.

use iced::widget::{
    Space, button, column, container, mouse_area, row, scrollable, slider, text, text_input,
};
use iced::{Element, Length};

use crate::config::settings::Settings;
use crate::config::updates::UpdateInfo;
use crate::theme::registry;
use crate::theme::tokens::{ThemeTokens, Typography};

/// The visible state of the update checker.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Installing(String),
    Error(String),
}

/// User actions inside the settings panel.
#[derive(Debug, Clone)]
pub enum Message {
    SelectTheme(String),
    PreviewTheme(Option<String>),
    EditorFontFamily(String),
    EditorFontSize(f32),
    EditorLineHeight(f32),
    PreviewFontFamily(String),
    PreviewFontSize(f32),
    PreviewLineHeight(f32),
    PreviewMaxWidth(f32),
    ResetEditorTypography,
    ResetPreviewTypography,
    ResetAll,
    CheckForUpdates,
    InstallUpdate,
    OpenLogs,
    Close,
}

/// Renders the modal settings panel.
pub fn view<'a>(
    settings: &'a Settings,
    tokens: &'a ThemeTokens,
    system_dark: bool,
    preview_theme: Option<&'a str>,
    update_state: &'a UpdateState,
) -> Element<'a, Message> {
    let header = row![
        column![
            text("Settings").size(20).color(tokens.heading),
            text("Appearance and typography are saved on this computer.")
                .size(12)
                .color(tokens.muted),
        ]
        .spacing(2),
        Space::new().width(Length::Fill),
        button(text("Close").size(12)).on_press(Message::Close),
    ]
    .align_y(iced::Alignment::Center);

    let body = column![
        theme_section(settings, tokens, system_dark, preview_theme),
        editor_typography_section(settings, tokens),
        preview_typography_section(settings, tokens),
        update_section(update_state, tokens),
        diagnostics_section(tokens),
        reset_section(tokens),
    ]
    .spacing(22);

    let card = container(
        column![header, scrollable(body).height(Length::Fill)]
            .spacing(14)
            .padding(18)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .max_width(760.0)
    .height(Length::Fill)
    .max_height(700.0)
    .style(move |_| container::Style {
        background: Some(tokens.surface_raised.into()),
        border: iced::border::Border {
            color: tokens.border,
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 24.0,
        },
        ..container::Style::default()
    });

    container(card)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(18)
        .into()
}

fn section_title<'a>(title: &'a str, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    column![
        text(title).size(15).color(tokens.heading),
        container("")
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style {
                background: Some(tokens.border.into()),
                ..container::Style::default()
            }),
    ]
    .spacing(6)
    .into()
}

fn theme_section<'a>(
    settings: &'a Settings,
    tokens: &'a ThemeTokens,
    system_dark: bool,
    preview_theme: Option<&'a str>,
) -> Element<'a, Message> {
    let system_label = if system_dark {
        "System (currently dark)"
    } else {
        "System (currently light)"
    };

    let mut quick = row![].spacing(6);
    for (id, label) in [
        ("system", system_label),
        ("light", "Light"),
        ("dark", "Dark"),
    ] {
        let active = settings.theme == id;
        quick = quick.push(
            button(text(label).size(12))
                .on_press(Message::SelectTheme(id.to_string()))
                .style(theme_button_style(tokens, active)),
        );
    }

    let cards: Vec<_> = registry::gallery().collect();
    let mut gallery = column![].spacing(8);
    for chunk in cards.chunks(3) {
        let mut row = row![].spacing(8);
        for theme in chunk {
            row = row.push(theme_card(theme, settings, tokens, preview_theme));
        }
        gallery = gallery.push(row);
    }

    column![
        section_title("Theme", tokens),
        quick,
        text("Hover a theme to preview it. Apply to keep it.")
            .size(11)
            .color(tokens.muted),
        gallery,
    ]
    .spacing(10)
    .into()
}

fn theme_card<'a>(
    theme: &'a registry::ThemeDefinition,
    settings: &'a Settings,
    tokens: &'a ThemeTokens,
    preview_theme: Option<&'a str>,
) -> Element<'a, Message> {
    let active = settings.theme == theme.id;
    let previewing = preview_theme == Some(theme.id);
    let border = if active {
        tokens.accent
    } else if previewing {
        tokens.warning
    } else {
        tokens.border
    };
    let card_tokens = &theme.tokens;

    let swatches = row![
        swatch(card_tokens.background),
        swatch(card_tokens.text),
        swatch(card_tokens.accent),
        swatch(card_tokens.code_background),
    ]
    .spacing(3);

    let state = if active {
        "Applied"
    } else if previewing {
        "Previewing"
    } else {
        " "
    };

    let card = container(
        column![
            row![
                text(theme.name).size(12).color(tokens.text),
                Space::new().width(Length::Fill),
                text(state).size(10).color(border),
            ]
            .align_y(iced::Alignment::Center),
            swatches,
            row![
                button(text("Preview").size(10))
                    .on_press(Message::PreviewTheme(Some(theme.id.to_string()))),
                button(text("Apply").size(10)).on_press(Message::SelectTheme(theme.id.to_string())),
            ]
            .spacing(4),
        ]
        .spacing(6),
    )
    .padding(8)
    .width(Length::Fixed(228.0))
    .style(move |_| container::Style {
        background: Some(tokens.surface.into()),
        border: iced::border::Border {
            color: border,
            width: if active || previewing { 2.0 } else { 1.0 },
            radius: 7.0.into(),
        },
        ..container::Style::default()
    });

    mouse_area(card)
        .on_enter(Message::PreviewTheme(Some(theme.id.to_string())))
        .on_exit(Message::PreviewTheme(None))
        .into()
}

fn swatch(color: iced::Color) -> Element<'static, Message> {
    container("")
        .width(Length::Fixed(46.0))
        .height(Length::Fixed(18.0))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::border::Border {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.18),
                width: 1.0,
                radius: 3.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn theme_button_style<'a>(
    tokens: &'a ThemeTokens,
    active: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'a {
    let accent = tokens.accent;
    let background = tokens.background;
    move |theme, status| {
        let mut style = button::text(theme, status);
        if active {
            style.background = Some(accent.into());
            style.text_color = background;
        }
        style
    }
}

fn editor_typography_section<'a>(
    settings: &'a Settings,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    let typography = &settings.editor_typography;

    column![
        section_title("Editor typography", tokens),
        font_family_row("Font family", typography, Message::EditorFontFamily, tokens),
        slider_row(
            "Font size",
            format!("{:.1} px", typography.font_size),
            slider(9.0..=32.0, typography.font_size, Message::EditorFontSize).step(0.5_f32),
            tokens,
        ),
        slider_row(
            "Line height",
            format!("{:.2}", typography.line_height),
            slider(1.0..=2.5, typography.line_height, Message::EditorLineHeight).step(0.05_f32),
            tokens,
        ),
        row![
            Space::new().width(Length::Fill),
            button(text("Reset editor typography").size(11))
                .on_press(Message::ResetEditorTypography),
        ],
    ]
    .spacing(8)
    .into()
}

fn preview_typography_section<'a>(
    settings: &'a Settings,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    let typography = &settings.preview_typography;

    column![
        section_title("Preview typography", tokens),
        font_family_row(
            "Font family",
            typography,
            Message::PreviewFontFamily,
            tokens
        ),
        slider_row(
            "Font size",
            format!("{:.1} px", typography.font_size),
            slider(9.0..=36.0, typography.font_size, Message::PreviewFontSize).step(0.5_f32),
            tokens,
        ),
        slider_row(
            "Line height",
            format!("{:.2}", typography.line_height),
            slider(
                1.0..=2.5,
                typography.line_height,
                Message::PreviewLineHeight
            )
            .step(0.05_f32),
            tokens,
        ),
        slider_row(
            "Text width",
            if typography.max_width > 0.0 {
                format!("{:.0} px", typography.max_width)
            } else {
                String::from("Full width")
            },
            slider(0.0..=1200.0, typography.max_width, Message::PreviewMaxWidth).step(10.0_f32),
            tokens,
        ),
        row![
            Space::new().width(Length::Fill),
            button(text("Reset preview typography").size(11))
                .on_press(Message::ResetPreviewTypography),
        ],
    ]
    .spacing(8)
    .into()
}

fn font_family_row<'a>(
    label: &'a str,
    typography: &'a Typography,
    on_input: fn(String) -> Message,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(12)
            .color(tokens.text)
            .width(Length::Fixed(100.0)),
        text_input("Font family", &typography.font_family)
            .on_input(on_input)
            .padding(6)
            .width(Length::Fixed(280.0)),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

fn slider_row<'a>(
    label: &'a str,
    value: String,
    slider: impl Into<Element<'a, Message>>,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(12)
            .color(tokens.text)
            .width(Length::Fixed(100.0)),
        slider.into(),
        text(value)
            .size(11)
            .color(tokens.muted)
            .width(Length::Fixed(80.0)),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

fn update_section<'a>(state: &'a UpdateState, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let (status, action): (String, Element<'a, Message>) = match state {
        UpdateState::Idle => (
            String::from("Check the GitHub release feed for a newer version."),
            button(text("Check for updates").size(12))
                .on_press(Message::CheckForUpdates)
                .into(),
        ),
        UpdateState::Checking => (
            String::from("Checking for updates…"),
            text("Working…").size(11).color(tokens.muted).into(),
        ),
        UpdateState::UpToDate => (
            String::from("markmydown is up to date."),
            button(text("Check again").size(12))
                .on_press(Message::CheckForUpdates)
                .into(),
        ),
        UpdateState::Available(info) => (
            format!(
                "Version {} is available. Download, verify, and restart to install.",
                info.version
            ),
            button(text("Install and restart").size(12))
                .on_press(Message::InstallUpdate)
                .into(),
        ),
        UpdateState::Installing(version) => (
            format!("Installing version {version}…"),
            text("Working…").size(11).color(tokens.muted).into(),
        ),
        UpdateState::Error(message) => (
            format!("Update check failed: {message}"),
            button(text("Try again").size(12))
                .on_press(Message::CheckForUpdates)
                .into(),
        ),
    };

    column![
        section_title("Updates", tokens),
        text(status).size(12).color(tokens.muted),
        action,
    ]
    .spacing(8)
    .into()
}

fn diagnostics_section<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    column![
        section_title("Diagnostics", tokens),
        text("Logs contain error descriptions and paths, never document content.")
            .size(12)
            .color(tokens.muted),
        button(text("Open log folder").size(12)).on_press(Message::OpenLogs),
    ]
    .spacing(8)
    .into()
}

fn reset_section<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    column![
        section_title("Reset", tokens),
        text("Restore every setting to its default value. This cannot be undone.")
            .size(12)
            .color(tokens.muted),
        button(text("Reset all settings").size(12)).on_press(Message::ResetAll),
    ]
    .spacing(8)
    .into()
}
