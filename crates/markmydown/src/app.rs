//! Application shell: state, messages, update, view, and subscriptions.

use iced::{Element, Length, Task};

/// How the content area divides the editor and preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Only the source editor is visible.
    EditorOnly,
    /// Editor and preview share the content area.
    #[default]
    SideBySide,
    /// Only the rendered preview is visible.
    PreviewOnly,
    /// Distraction-free reading with no editing chrome.
    Read,
}

impl LayoutMode {
    pub const ALL: [Self; 4] = [
        Self::EditorOnly,
        Self::SideBySide,
        Self::PreviewOnly,
        Self::Read,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::EditorOnly => "Editor",
            Self::SideBySide => "Split",
            Self::PreviewOnly => "Preview",
            Self::Read => "Read",
        }
    }

    pub fn shows_editor(self) -> bool {
        matches!(self, Self::EditorOnly | Self::SideBySide)
    }

    pub fn shows_preview(self) -> bool {
        matches!(self, Self::SideBySide | Self::PreviewOnly | Self::Read)
    }
}

/// Top-level application state.
#[derive(Debug)]
pub struct App {
    /// Short status message shown at the bottom of the window.
    pub status: String,
    /// Current content layout.
    pub layout: LayoutMode,
    /// Whether the workspace sidebar is visible.
    pub sidebar_visible: bool,
    /// Whether the editor dims non-current text and centers the caret.
    pub focus_mode: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            status: String::from("Ready"),
            layout: LayoutMode::default(),
            sidebar_visible: true,
            focus_mode: false,
        }
    }
}

impl App {
    fn title(&self) -> String {
        String::from("markmydown")
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::Light
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::none()
    }
}

/// Every user-visible action flows through this enum.
#[derive(Debug, Clone)]
pub enum Message {
    /// Switch the content layout.
    SetLayout(LayoutMode),
    /// Show or hide the workspace sidebar.
    ToggleSidebar,
    /// Toggle focus mode in the editor.
    ToggleFocusMode,
    /// Exit the application.
    Quit,
    /// Reserved no-op for placeholder buttons.
    Noop,
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::SetLayout(layout) => {
            app.layout = layout;
            app.status = format!("{} layout", layout.label());
        }
        Message::ToggleSidebar => {
            app.sidebar_visible = !app.sidebar_visible;
        }
        Message::ToggleFocusMode => {
            app.focus_mode = !app.focus_mode;
            app.status = if app.focus_mode {
                String::from("Focus mode on")
            } else {
                String::from("Focus mode off")
            };
        }
        Message::Quit => return iced::exit(),
        Message::Noop => {}
    }

    Task::none()
}

pub fn view(_app: &App) -> Element<'_, Message> {
    iced::widget::container("")
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Runs the native application.
pub fn run() -> iced::Result {
    iced::application(App::default, update, view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 820.0),
            min_size: Some(iced::Size::new(760.0, 520.0)),
            position: iced::window::Position::Centered,
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}
