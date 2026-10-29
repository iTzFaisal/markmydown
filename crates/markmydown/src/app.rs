//! Application shell: state, messages, update, view, and subscriptions.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::widget::{Space, button, column, container, pane_grid, row, scrollable, text};
use iced::{Element, Length, Subscription, Task};
use iced_code_editor::Message as EditorMessage;

use crate::config::paths::AppPaths;
use crate::config::settings::Settings;
use crate::config::updates::{self, UpdateInfo};
use crate::editor::markdown::FormatAction;
use crate::editor::{EditorPane, message_may_edit};
use crate::export::pdf::{PageSize, PdfOptions};
use crate::export::{self, ExportFailure, ExportFormat, ExportOutcome, ExportRequest};
use crate::preview::model::PreviewDocument;
use crate::preview::render as preview_render;
use crate::preview::sync as preview_sync;
use crate::theme::registry;
use crate::theme::tokens::{self, ThemeTokens};
use crate::ui::{
    dialogs, notifications, outline, palette, search as search_ui, settings as settings_ui,
    sidebar, status, welcome,
};
use crate::workspace::document::{Cursor, ExternalChange};
use crate::workspace::recovery::RecoveryRecord;
use crate::workspace::session::Session;
use crate::workspace::state::WorkspaceState;

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

    /// Parses the value stored in settings.
    pub fn from_settings(value: &str) -> Self {
        match value {
            "editor-only" => Self::EditorOnly,
            "preview-only" => Self::PreviewOnly,
            "read" => Self::Read,
            _ => Self::SideBySide,
        }
    }

    /// Serializes the value for settings.
    pub fn as_settings(self) -> &'static str {
        match self {
            Self::EditorOnly => "editor-only",
            Self::SideBySide => "side-by-side",
            Self::PreviewOnly => "preview-only",
            Self::Read => "read",
        }
    }
}

/// A built-in document shown over the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoOverlay {
    Welcome,
    CheatSheet,
}

impl InfoOverlay {
    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to markmydown",
            Self::CheatSheet => "Markdown cheat sheet",
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            Self::Welcome => welcome::WELCOME,
            Self::CheatSheet => welcome::CHEAT_SHEET,
        }
    }
}

/// Which view a resizable pane contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Preview,
}

/// The operation an input dialog will perform when submitted.
#[derive(Debug, Clone)]
pub enum InputAction {
    NewFile { parent: PathBuf },
    NewFolder { parent: PathBuf },
    Rename { path: PathBuf },
    Move { path: PathBuf },
    InsertLink { image: bool },
}

/// The operation a confirmation dialog will perform when confirmed.
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    Delete { path: PathBuf },
    CloseTab { id: u64 },
    RetryExport { format: ExportFormat },
    ResetSettings,
}

/// A modal dialog.
#[derive(Debug, Clone)]
pub enum Dialog {
    Input {
        action: InputAction,
        input: dialogs::Input,
    },
    Confirm {
        action: ConfirmAction,
        confirm: dialogs::Confirm,
    },
}

/// Side-by-side text for a conflict's compare action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compare {
    pub path: PathBuf,
    pub local: String,
    pub disk: String,
}

/// Top-level application state.
pub struct App {
    pub workspace: WorkspaceState,
    pub settings: Settings,
    pub paths: AppPaths,
    pub status: String,
    pub status_is_error: bool,
    pub tokens: ThemeTokens,
    pub system_dark: bool,
    pub layout: LayoutMode,
    pub sidebar_visible: bool,
    pub outline_visible: bool,
    pub focus_mode: bool,
    pub search: search_ui::SearchPanel,
    pub selected: Option<PathBuf>,
    pub expanded: HashSet<PathBuf>,
    pub dialog: Option<Dialog>,
    pub compare: Option<Compare>,
    pub recovery: Vec<RecoveryRecord>,
    pub settings_open: bool,
    pub preview_theme: Option<String>,
    preview_theme_backup: Option<ThemeTokens>,
    pub update_state: settings_ui::UpdateState,
    pub quick_open: Option<palette::QuickOpen>,
    pub notifications: notifications::NotificationCenter,
    pub info_overlay: Option<InfoOverlay>,
    info_preview: Option<PreviewDocument>,
    info_scroll_id: iced::widget::Id,
    editor: Option<EditorPane>,
    editor_doc: Option<u64>,
    editor_needs_position: bool,
    panes: pane_grid::State<PaneKind>,
    preview_caches: RefCell<preview_render::RenderCaches>,
    preview_scroll_id: iced::widget::Id,
    preview_offset: f32,
    preview_progress: f32,
    preview_viewport_height: f32,
    preview_last_edit: Instant,
    preview_needs_reset: bool,
    ignore_preview_scrolls_until: Option<Instant>,
    ignore_editor_scrolls_until: Option<Instant>,
    last_recovery: Instant,
    last_session: Instant,
}

impl App {
    /// Loads settings, restores the previous session, and prepares the first
    /// recovery prompt.
    pub fn boot() -> Self {
        Self::boot_with_paths(Vec::new())
    }

    /// Boots with documents or a folder passed by the operating system.
    ///
    /// Launch paths take precedence over the restored session. The first
    /// folder, or the parent of the first file, becomes the workspace root;
    /// every file becomes a tab.
    pub fn boot_with_paths(initial: Vec<PathBuf>) -> Self {
        let paths = AppPaths::discover().unwrap_or_else(|_| {
            AppPaths::with_base(std::env::temp_dir().join("markmydown-fallback"))
        });
        Self::boot_at_paths(paths, initial)
    }

    fn boot_at_paths(paths: AppPaths, initial: Vec<PathBuf>) -> Self {
        let _ = paths.ensure();

        let loaded = Settings::load(&paths.settings_file());
        let mut settings = loaded.settings.clone();
        settings.validate();

        let mut workspace = WorkspaceState::new(paths.session_file(), paths.recovery_dir());
        let mut status = String::from("Ready");
        let mut status_is_error = false;
        if let Some(warning) = loaded.warning.as_deref() {
            log::warn!("{warning}");
            status = warning.to_string();
            status_is_error = true;
        }
        if let Some(backup) = loaded.backup.as_ref() {
            log::warn!("unreadable settings preserved at {}", backup.display());
        }

        if initial.is_empty() {
            let session = Session::load(&paths.session_file());
            let mut restored = workspace.is_open();
            if session.workspace_root.is_some() {
                match workspace.restore_session(&session) {
                    Ok(_) => restored = workspace.is_open(),
                    Err(error) => {
                        log::warn!("could not restore session: {error}");
                        status = format!("Could not restore the previous session: {error}");
                        status_is_error = true;
                    }
                }
            }

            if !restored
                && let Some(root) = settings.last_workspace.clone()
                && root.is_dir()
            {
                match workspace.open_folder(&root) {
                    Ok(()) => {
                        settings.add_recent_workspace(root);
                        persisted_settings(&settings, &paths);
                    }
                    Err(error) => {
                        status = format!("Could not open the last workspace: {error}");
                        status_is_error = true;
                    }
                }
            }
        } else {
            if let Some(root) = crate::config::launch::launch_root(&initial) {
                match workspace.open_folder(&root) {
                    Ok(()) => {
                        settings.add_recent_workspace(root);
                        persisted_settings(&settings, &paths);
                        let mut first_id = None;
                        for path in &initial {
                            if crate::config::launch::is_folder(path) {
                                continue;
                            }
                            match workspace.open_document(path) {
                                Ok(id) => {
                                    first_id.get_or_insert(id);
                                }
                                Err(error) => {
                                    log::warn!(
                                        "could not open launch document {}: {error}",
                                        path.display()
                                    );
                                    status = format!("Could not open {}: {error}", path.display());
                                    status_is_error = true;
                                }
                            }
                        }
                        if let Some(id) = first_id {
                            workspace.activate(id);
                        }
                    }
                    Err(error) => {
                        status = format!("Could not open {}: {error}", root.display());
                        status_is_error = true;
                    }
                }
            }
        }

        let recovery = workspace.pending_recovery();
        let layout = LayoutMode::from_settings(&settings.layout);
        let system_dark = tokens::system_is_dark();
        let tokens = registry::resolve(&settings.theme, system_dark)
            .tokens
            .clone();
        let selected = workspace
            .active_document()
            .map(|document| document.path.clone());

        let first_launch = !settings.onboarding_seen && !workspace.is_open();
        if first_launch {
            settings.onboarding_seen = true;
            persisted_settings(&settings, &paths);
        }
        let info_overlay = first_launch.then_some(InfoOverlay::Welcome);
        let info_preview = info_overlay.map(|kind| crate::preview::parser::parse(kind.source(), 0));

        let panes = new_pane_state(settings.split_ratio);

        let mut app = Self {
            workspace,
            settings,
            paths,
            status,
            status_is_error,
            tokens,
            system_dark,
            layout,
            sidebar_visible: true,
            outline_visible: true,
            focus_mode: false,
            search: search_ui::SearchPanel::default(),
            selected,
            expanded: HashSet::new(),
            dialog: None,
            compare: None,
            recovery,
            settings_open: false,
            preview_theme: None,
            preview_theme_backup: None,
            update_state: settings_ui::UpdateState::Idle,
            quick_open: None,
            notifications: notifications::NotificationCenter::default(),
            info_overlay,
            info_preview,
            info_scroll_id: iced::widget::Id::new("markmydown-info"),
            editor: None,
            editor_doc: None,
            editor_needs_position: true,
            panes,
            preview_caches: RefCell::new(preview_render::RenderCaches::default()),
            preview_scroll_id: iced::widget::Id::new("markmydown-preview"),
            preview_offset: 0.0,
            preview_progress: 0.0,
            preview_viewport_height: 800.0,
            preview_last_edit: Instant::now(),
            preview_needs_reset: true,
            ignore_preview_scrolls_until: None,
            ignore_editor_scrolls_until: None,
            last_recovery: Instant::now(),
            last_session: Instant::now(),
        };

        app.sidebar_visible = app.settings.sidebar_visible;
        app.outline_visible = app.settings.outline_visible;
        app.focus_mode = app.settings.focus_mode;
        app.install_editor();
        app.refresh_preview(false);
        app
    }

    fn iced_theme(&self) -> iced::Theme {
        self.tokens.to_iced_theme("markmydown")
    }

    fn title(&self) -> String {
        match self.workspace.active_document() {
            Some(document) if document.is_dirty() => {
                format!("● {} — markmydown", document.display_title())
            }
            Some(document) => format!("{} — markmydown", document.display_title()),
            None => String::from("markmydown"),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick),
            iced::keyboard::listen().filter_map(shortcut_message),
            iced::system::theme_changes()
                .map(|mode| Message::SystemThemeChanged(mode == iced::theme::Mode::Dark)),
        ];

        if self.quick_open.is_some() {
            subscriptions.push(iced::keyboard::listen().filter_map(quick_open_key_message));
        }

        Subscription::batch(subscriptions)
    }

    fn root(&self) -> Option<&Path> {
        self.workspace.root()
    }

    fn active_path(&self) -> Option<PathBuf> {
        self.workspace
            .active_document()
            .map(|document| document.path.clone())
    }

    /// The entry file operations act on: the selected tree entry, or the
    /// active document when nothing is selected.
    fn target_path(&self) -> Option<PathBuf> {
        self.selected.clone().or_else(|| self.active_path())
    }

    fn current_parent(&self) -> Option<PathBuf> {
        let root = self.root()?.to_path_buf();

        if let Some(selected) = self
            .selected
            .as_ref()
            .filter(|path| path.starts_with(&root))
        {
            if selected.is_dir() {
                return Some(selected.clone());
            }
            if let Some(parent) = selected.parent() {
                return Some(parent.to_path_buf());
            }
        }

        let parent = self
            .workspace
            .active_document()
            .and_then(|document| document.path.parent())
            .filter(|parent| parent.starts_with(&root))
            .map(Path::to_path_buf);

        Some(parent.unwrap_or(root))
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_is_error = false;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        log::warn!("{message}");
        self.status = message;
        self.status_is_error = true;
    }

    fn notify_success(&mut self, message: impl Into<String>) {
        self.notifications
            .push(notifications::NotificationKind::Success, message);
    }

    fn notify_error(&mut self, message: impl Into<String>) {
        self.notifications
            .push(notifications::NotificationKind::Error, message);
    }

    /// Re-applies the current tokens and editor typography to the editor.
    fn apply_editor_theme(&mut self) {
        let tokens = self.tokens.clone();
        let typography = self.settings.editor_typography.clone();
        if let Some(editor) = &mut self.editor {
            editor.apply_theme(&tokens, &typography);
        }
    }

    fn adopt_theme(&mut self, id: &str) {
        self.tokens = registry::resolve(id, self.system_dark).tokens.clone();
        self.apply_editor_theme();
    }

    /// Previews a theme without persisting it. The applied theme is restored
    /// when the pointer leaves the picker or the panel closes.
    fn preview_theme(&mut self, id: Option<String>) {
        match id {
            Some(id) => {
                if self.preview_theme.as_deref() == Some(id.as_str()) {
                    return;
                }
                if self.preview_theme_backup.is_none() {
                    self.preview_theme_backup = Some(self.tokens.clone());
                }
                self.preview_theme = Some(id.clone());
                self.tokens = registry::resolve(&id, self.system_dark).tokens.clone();
                self.apply_editor_theme();
            }
            None => self.cancel_theme_preview(),
        }
    }

    fn cancel_theme_preview(&mut self) {
        if let Some(tokens) = self.preview_theme_backup.take() {
            self.tokens = tokens;
            self.apply_editor_theme();
        }
        self.preview_theme = None;
    }

    fn select_theme(&mut self, id: String) {
        if registry::get(&id).is_none() && id != "system" {
            self.set_error(format!("Unknown theme: {id}"));
            return;
        }

        self.settings.theme = id;
        self.settings.validate();
        self.preview_theme = None;
        self.preview_theme_backup = None;
        let theme = self.settings.theme.clone();
        self.adopt_theme(&theme);
        persisted_settings(&self.settings, &self.paths);
        let name = registry::resolve(&theme, self.system_dark).name;
        self.set_status(format!("Theme: {name}"));
        self.notify_success(format!("Applied the {name} theme."));
    }

    fn system_theme_changed(&mut self, dark: bool) {
        self.system_dark = dark;
        if self.settings.theme == "system" && self.preview_theme.is_none() {
            self.adopt_theme("system");
        }
    }

    fn open_settings(&mut self) {
        self.settings_open = true;
        self.set_status("Settings");
    }

    fn close_settings(&mut self) {
        self.cancel_theme_preview();
        self.settings_open = false;
        self.settings.validate();
        self.apply_editor_theme();
        persisted_settings(&self.settings, &self.paths);
    }

    fn handle_settings(&mut self, message: settings_ui::Message) -> Task<Message> {
        match message {
            settings_ui::Message::SelectTheme(id) => self.select_theme(id),
            settings_ui::Message::PreviewTheme(id) => self.preview_theme(id),
            settings_ui::Message::EditorFontFamily(value) => {
                self.settings.editor_typography.font_family = value;
                self.apply_editor_theme();
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::EditorFontSize(value) => {
                self.settings.editor_typography.font_size = value;
                self.apply_editor_theme();
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::EditorLineHeight(value) => {
                self.settings.editor_typography.line_height = value;
                self.apply_editor_theme();
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::PreviewFontFamily(value) => {
                self.settings.preview_typography.font_family = value;
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::PreviewFontSize(value) => {
                self.settings.preview_typography.font_size = value;
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::PreviewLineHeight(value) => {
                self.settings.preview_typography.line_height = value;
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::PreviewMaxWidth(value) => {
                self.settings.preview_typography.max_width = value;
                persisted_settings(&self.settings, &self.paths);
            }
            settings_ui::Message::ResetEditorTypography => {
                self.settings.editor_typography =
                    crate::theme::tokens::Typography::editor_default();
                self.apply_editor_theme();
                persisted_settings(&self.settings, &self.paths);
                self.notify_success("Reset the editor typography.");
            }
            settings_ui::Message::ResetPreviewTypography => {
                self.settings.preview_typography =
                    crate::theme::tokens::Typography::preview_default();
                persisted_settings(&self.settings, &self.paths);
                self.notify_success("Reset the preview typography.");
            }
            settings_ui::Message::ResetAll => {
                self.dialog = Some(Dialog::Confirm {
                    action: ConfirmAction::ResetSettings,
                    confirm: dialogs::Confirm {
                        title: String::from("Reset all settings"),
                        message: String::from(
                            "Restore the theme, typography, layout, and preferences to their defaults?",
                        ),
                        confirm_label: String::from("Reset settings"),
                    },
                });
            }
            settings_ui::Message::CheckForUpdates => return self.check_for_updates(),
            settings_ui::Message::InstallUpdate => return self.install_update(),
            settings_ui::Message::OpenLogs => return self.open_logs(),
            settings_ui::Message::Close => self.close_settings(),
        }

        Task::none()
    }

    fn check_for_updates(&mut self) -> Task<Message> {
        self.update_state = settings_ui::UpdateState::Checking;
        Task::perform(
            async { updates::check().map_err(|error| error.to_string()) },
            Message::UpdateCheckFinished,
        )
    }

    fn install_update(&mut self) -> Task<Message> {
        let version = match &self.update_state {
            settings_ui::UpdateState::Available(info) => info.version.clone(),
            _ => String::from("latest"),
        };
        self.update_state = settings_ui::UpdateState::Installing(version);
        Task::perform(
            async { updates::install_and_restart().map_err(|error| error.to_string()) },
            Message::UpdateInstallFinished,
        )
    }

    fn update_check_finished(
        &mut self,
        result: Result<Option<UpdateInfo>, String>,
    ) -> Task<Message> {
        match result {
            Ok(Some(info)) => {
                self.update_state = settings_ui::UpdateState::Available(info.clone());
                self.set_status(format!("markmydown {} is available.", info.version));
                self.notify_success(format!("Version {} is available.", info.version));
            }
            Ok(None) => {
                self.update_state = settings_ui::UpdateState::UpToDate;
                self.set_status("markmydown is up to date.");
                self.notify_success("markmydown is up to date.");
            }
            Err(message) => {
                self.update_state = settings_ui::UpdateState::Error(message.clone());
                self.set_error(format!("Could not check for updates: {message}"));
                self.notify_error(format!("Update check failed: {message}"));
            }
        }

        Task::none()
    }

    fn update_install_finished(&mut self, result: Result<String, String>) -> Task<Message> {
        match result {
            Ok(version) => {
                self.set_status(format!("Installed {version}."));
                iced::exit()
            }
            Err(message) => {
                self.update_state = settings_ui::UpdateState::Error(message.clone());
                self.set_error(format!("Could not install the update: {message}"));
                self.notify_error(format!("Update install failed: {message}"));
                Task::none()
            }
        }
    }

    fn open_logs(&mut self) -> Task<Message> {
        let directory = self.paths.log_dir();
        match open::that(&directory) {
            Ok(()) => self.set_status(format!("Opened {}", directory.display())),
            Err(error) => {
                self.set_error(format!("Could not open {}: {error}", directory.display()))
            }
        }
        Task::none()
    }

    fn reset_settings(&mut self) {
        self.preview_theme = None;
        self.preview_theme_backup = None;
        self.settings.reset();
        self.settings.validate();
        self.system_dark = tokens::system_is_dark();
        let theme = self.settings.theme.clone();
        self.tokens = registry::resolve(&theme, self.system_dark).tokens.clone();
        self.layout = LayoutMode::from_settings(&self.settings.layout);
        self.sidebar_visible = self.settings.sidebar_visible;
        self.outline_visible = self.settings.outline_visible;
        self.focus_mode = self.settings.focus_mode;
        self.panes = new_pane_state(self.settings.split_ratio);
        if let Some(editor) = &mut self.editor {
            editor.set_focus_mode(self.focus_mode);
        }
        self.apply_editor_theme();
        self.settings_open = false;
        persisted_settings(&self.settings, &self.paths);
        self.set_status("Settings reset to defaults.");
        self.notify_success("All settings were reset.");
    }

    fn open_quick_open(&mut self) -> Task<Message> {
        let Some(root) = self.root().map(Path::to_path_buf) else {
            self.set_error("Open a folder before using Quick Open.");
            return Task::none();
        };
        let Some(tree) = self.workspace.tree() else {
            self.set_error("Open a folder before using Quick Open.");
            return Task::none();
        };
        let entries = palette::entries_from_tree(&root, tree);
        if entries.is_empty() {
            self.set_error("This folder has no Markdown documents.");
            return Task::none();
        }

        self.quick_open = Some(palette::QuickOpen::new(entries));
        iced::widget::operation::focus(palette::input_id())
    }

    fn handle_quick_open(&mut self, message: palette::Message) -> Task<Message> {
        match message {
            palette::Message::QueryChanged(query) => {
                if let Some(quick) = &mut self.quick_open {
                    quick.set_query(query);
                }
                Task::none()
            }
            palette::Message::Move(delta) => {
                if let Some(quick) = &mut self.quick_open {
                    quick.move_selection(delta);
                }
                Task::none()
            }
            palette::Message::Accept => {
                let path = self
                    .quick_open
                    .as_ref()
                    .and_then(|quick| quick.selected_entry())
                    .map(|entry| entry.path.clone());
                self.quick_open = None;
                match path {
                    Some(path) => self.open_file(path),
                    None => Task::none(),
                }
            }
            palette::Message::Open(path) => {
                self.quick_open = None;
                self.open_file(path)
            }
            palette::Message::Close => {
                self.quick_open = None;
                Task::none()
            }
        }
    }

    fn open_info(&mut self, kind: InfoOverlay) -> Task<Message> {
        self.info_preview = Some(crate::preview::parser::parse(kind.source(), 0));
        self.info_overlay = Some(kind);
        Task::none()
    }

    fn close_info(&mut self) {
        self.info_overlay = None;
        self.info_preview = None;
    }

    fn handle_info_preview(&mut self, message: preview_render::Message) -> Task<Message> {
        match message {
            preview_render::Message::LinkClicked(url) => self.activate_preview_link(url),
            preview_render::Message::CopyCode(code) => {
                match crate::export::clipboard::copy_text(&code) {
                    Ok(()) => self.set_status("Copied code to the clipboard."),
                    Err(error) => self.set_error(format!("Could not copy the code: {error}")),
                }
                Task::none()
            }
            preview_render::Message::Scrolled(_) => Task::none(),
        }
    }

    /// Whether the active document is large enough to pause live preview.
    fn preview_is_paused(&self) -> bool {
        self.workspace
            .active_document()
            .is_some_and(|document| document.text.len() > self.settings.large_file_threshold)
    }

    /// Rebuilds the active document's preview model.
    ///
    /// Unless `force` is set, documents above the configured size threshold
    /// keep their previous model until the user explicitly refreshes.
    fn refresh_preview(&mut self, force: bool) -> bool {
        let Some(id) = self.workspace.active_id() else {
            return false;
        };
        let Some(document) = self.workspace.document(id) else {
            return false;
        };
        if !force && document.preview_stale && self.preview_is_paused() {
            return false;
        }
        if !force && !document.preview_stale && document.preview.is_some() {
            return false;
        }

        let Some(document) = self.workspace.document_mut(id) else {
            return false;
        };
        document.refresh_preview();
        true
    }

    /// Refreshes the preview after the debounce interval, unless it is paused
    /// for a large document.
    fn refresh_preview_if_due(&mut self, now: Instant) -> bool {
        let Some(document) = self.workspace.active_document() else {
            return false;
        };
        if !document.preview_stale || self.preview_is_paused() {
            return false;
        }
        let debounce = Duration::from_millis(self.settings.preview_debounce_ms);
        if now.duration_since(self.preview_last_edit) < debounce {
            return false;
        }
        self.refresh_preview(false)
    }

    /// Scrolls the preview to a normalized vertical position.
    fn scroll_preview_to(&mut self, progress: f32) -> Task<Message> {
        let progress = progress.clamp(0.0, 1.0);
        self.preview_progress = progress;
        self.ignore_preview_scrolls_until = Some(Instant::now() + Duration::from_millis(200));
        iced::widget::operation::snap_to(
            self.preview_scroll_id.clone(),
            iced::widget::scrollable::RelativeOffset {
                x: None,
                y: Some(progress),
            },
        )
    }

    /// Scrolls the preview to the block containing a source line.
    fn scroll_preview_to_line(&mut self, line: usize) -> Task<Message> {
        let Some(document) = self.workspace.active_document() else {
            return Task::none();
        };
        let Some(preview) = &document.preview else {
            return Task::none();
        };
        let progress = preview_sync::preview_progress_for_line(preview, line);
        self.scroll_preview_to(progress)
    }

    fn handle_preview(&mut self, message: preview_render::Message) -> Task<Message> {
        match message {
            preview_render::Message::LinkClicked(url) => self.activate_preview_link(url),
            preview_render::Message::CopyCode(code) => {
                match crate::export::clipboard::copy_text(&code) {
                    Ok(()) => self.set_status("Copied code to the clipboard."),
                    Err(error) => self.set_error(format!("Could not copy the code: {error}")),
                }
                Task::none()
            }
            preview_render::Message::Scrolled(viewport) => {
                let now = Instant::now();
                self.preview_offset = viewport.absolute_offset().y;
                self.preview_viewport_height = viewport.bounds().height.max(1.0);
                self.preview_progress = preview_sync::reading_progress(
                    viewport.absolute_offset().y,
                    viewport.content_bounds().height,
                    viewport.bounds().height,
                );

                let programmatic = self
                    .ignore_preview_scrolls_until
                    .is_some_and(|until| now < until);
                if programmatic || self.layout != LayoutMode::SideBySide {
                    return Task::none();
                }

                let Some(document) = self.workspace.active_document() else {
                    return Task::none();
                };
                let Some(preview) = &document.preview else {
                    return Task::none();
                };
                let line =
                    preview_sync::editor_line_for_preview_progress(preview, self.preview_progress);
                let Some(editor) = &mut self.editor else {
                    return Task::none();
                };
                let offset = editor.vertical_offset_for_line(line);
                self.ignore_editor_scrolls_until = Some(now + Duration::from_millis(200));
                editor.scroll_to_vertical(offset).map(Message::Editor)
            }
        }
    }

    fn activate_preview_link(&mut self, url: String) -> Task<Message> {
        if let Some(label) = url.strip_prefix("footnote:") {
            let Some(document) = self.workspace.active_document() else {
                return Task::none();
            };
            let Some(preview) = &document.preview else {
                return Task::none();
            };
            let total = preview.blocks.len() + preview.footnotes.len();
            let index = preview
                .footnotes
                .iter()
                .position(|footnote| footnote.label == label)
                .unwrap_or(0);
            let progress = if total == 0 {
                1.0
            } else {
                (preview.blocks.len() + index) as f32 / total as f32
            };
            return self.scroll_preview_to(progress);
        }

        let Some(external) = external_url(&url) else {
            return Task::none();
        };
        match open::that(external) {
            Ok(()) => self.set_status(format!("Opened {external} in the default browser.")),
            Err(error) => self.set_error(format!("Could not open {external}: {error}")),
        }
        Task::none()
    }

    /// Copies the editor buffer, caret, and selection back into the document.
    fn sync_editor_to_workspace(&mut self) {
        let Some(id) = self.editor_doc else {
            return;
        };
        let Some(editor) = &self.editor else {
            return;
        };
        let content = editor.content();
        let (line, column) = editor.cursor();
        let scroll = self
            .workspace
            .document(id)
            .map_or(0.0, |document| document.scroll);
        self.workspace
            .set_document_content(id, content, Cursor::new(line, column), scroll);
    }

    /// Copies only the caret position back to the document.
    fn sync_editor_position(&mut self) {
        let Some(id) = self.editor_doc else {
            return;
        };
        let Some(editor) = &self.editor else {
            return;
        };
        let (line, column) = editor.cursor();
        let scroll = self
            .workspace
            .document(id)
            .map_or(0.0, |document| document.scroll);
        self.workspace
            .set_document_position(id, Cursor::new(line, column), scroll);
    }

    fn apply_format(&mut self, action: FormatAction) -> Task<Message> {
        if matches!(action, FormatAction::Link | FormatAction::Image) {
            self.begin_insert_link(action == FormatAction::Image);
            return Task::none();
        }

        let Some(editor) = &mut self.editor else {
            self.set_error("Open a document first.");
            return Task::none();
        };
        let task = editor.apply_format(action).map(Message::Editor);
        self.sync_editor_to_workspace();
        self.refresh_search_after_edit();
        task
    }

    fn begin_insert_link(&mut self, image: bool) {
        if self.editor.is_none() {
            self.set_error("Open a document first.");
            return;
        }
        self.dialog = Some(Self::new_input_dialog(
            InputAction::InsertLink { image },
            if image { "Insert image" } else { "Insert link" },
            "URL",
            "",
            if image {
                "images/diagram.png"
            } else {
                "https://example.com"
            },
        ));
    }

    fn insert_link(&mut self, url: &str, image: bool) -> Task<Message> {
        let Some(editor) = &mut self.editor else {
            self.set_error("Open a document first.");
            return Task::none();
        };
        let task = editor.insert_link(url, image).map(Message::Editor);
        self.sync_editor_to_workspace();
        self.refresh_search_after_edit();
        task
    }

    fn open_search(&mut self, replace: bool) -> Task<Message> {
        self.search.open(replace);
        self.refresh_search_matches(false);
        iced::widget::operation::focus(search_ui::query_input_id())
    }

    fn close_search(&mut self) {
        self.search.close();
        if let Some(editor) = &mut self.editor {
            editor.set_search_highlights(&[], None);
        }
    }

    /// Re-runs the query against the editor buffer and refreshes decorations.
    fn refresh_search_matches(&mut self, reveal: bool) {
        if !self.search.visible {
            return;
        }
        let text = match &self.editor {
            Some(editor) => editor.content(),
            None => return,
        };
        self.search.refresh(&text);
        let current = self.search.current;
        let start = self.search.current_match().map(|matched| matched.start);

        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.set_search_highlights(&self.search.matches, current);
        if reveal && let Some(start) = start {
            let _ = editor.goto_position(start);
        }
    }

    fn refresh_search_after_edit(&mut self) {
        if self.search.visible {
            self.refresh_search_matches(false);
        }
    }

    fn goto_search_match(&mut self) -> Task<Message> {
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };
        let Some(start) = self.search.current_match().map(|matched| matched.start) else {
            return Task::none();
        };
        let task = editor.goto_position(start).map(Message::Editor);
        self.sync_editor_position();
        task
    }

    fn handle_search(&mut self, message: search_ui::Message) -> Task<Message> {
        match message {
            search_ui::Message::QueryChanged(query) => {
                self.search.query = query;
                self.refresh_search_matches(true);
                self.sync_editor_position();
            }
            search_ui::Message::ReplacementChanged(replacement) => {
                self.search.replacement = replacement;
            }
            search_ui::Message::ToggleCaseSensitive => {
                self.search.case_sensitive = !self.search.case_sensitive;
                self.refresh_search_matches(true);
                self.sync_editor_position();
            }
            search_ui::Message::ToggleRegex => {
                self.search.regex = !self.search.regex;
                self.refresh_search_matches(true);
                self.sync_editor_position();
            }
            search_ui::Message::Next => {
                self.search.next();
                self.refresh_highlights();
                return self.goto_search_match();
            }
            search_ui::Message::Previous => {
                self.search.previous();
                self.refresh_highlights();
                return self.goto_search_match();
            }
            search_ui::Message::ReplaceCurrent => {
                return self.replace_current_match();
            }
            search_ui::Message::ReplaceAll => {
                return self.replace_all_matches();
            }
            search_ui::Message::Close => {
                self.close_search();
            }
        }
        Task::none()
    }

    fn refresh_highlights(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let current = self.search.current;
        editor.set_search_highlights(&self.search.matches, current);
    }

    fn replace_current_match(&mut self) -> Task<Message> {
        let Some(matched) = self.search.current_match().cloned() else {
            return Task::none();
        };
        let options = self.search.options();
        let replacement = self.search.replacement.clone();
        let query = self.search.query.clone();
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };

        match editor.replace_match(&query, &matched, &replacement, options) {
            Ok(task) => {
                self.sync_editor_to_workspace();
                self.refresh_search_matches(false);
                task.map(Message::Editor)
            }
            Err(error) => {
                self.search.error = Some(error.to_string());
                self.set_error(error.to_string());
                Task::none()
            }
        }
    }

    fn replace_all_matches(&mut self) -> Task<Message> {
        let query = self.search.query.clone();
        if query.is_empty() {
            return Task::none();
        }
        let replacement = self.search.replacement.clone();
        let options = self.search.options();
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };

        match editor.replace_all(&query, &replacement, options) {
            Ok(task) => {
                self.sync_editor_to_workspace();
                self.refresh_search_matches(false);
                task.map(Message::Editor)
            }
            Err(error) => {
                self.search.error = Some(error.to_string());
                self.set_error(error.to_string());
                Task::none()
            }
        }
    }

    fn goto_heading(&mut self, line: usize) -> Task<Message> {
        if !self.layout.shows_editor() {
            return self.scroll_preview_to_line(line);
        }
        let Some(editor) = &mut self.editor else {
            return self.scroll_preview_to_line(line);
        };
        let task = editor.goto_position((line, 0)).map(Message::Editor);
        self.sync_editor_position();
        let preview_task = self.scroll_preview_to_line(line);
        Task::batch([task, preview_task])
    }

    fn resize_panes(&mut self, event: pane_grid::ResizeEvent) {
        self.panes.resize(event.split, event.ratio);
        if (self.settings.split_ratio - event.ratio).abs() > 0.001 {
            self.settings.split_ratio = event.ratio;
            persisted_settings(&self.settings, &self.paths);
        }
    }

    /// Creates the editor widget for the active tab without applying a cursor
    /// task. Used during boot, when no task can be returned.
    fn install_editor(&mut self) {
        self.preview_offset = 0.0;
        self.preview_progress = 0.0;
        self.preview_needs_reset = true;
        let Some(document) = self.workspace.active_document() else {
            self.editor = None;
            self.editor_doc = None;
            self.close_search();
            return;
        };

        let id = document.id;
        let text = document.text.clone();
        let mut editor = EditorPane::new(&text, &self.tokens, &self.settings.editor_typography);
        editor.set_focus_mode(self.focus_mode);
        editor.focus();
        self.editor = Some(editor);
        self.editor_doc = Some(id);
        self.editor_needs_position = true;
        self.refresh_search_after_edit();
        self.refresh_preview(false);
    }

    /// Rebuilds the editor when the active tab changed.
    fn sync_editor(&mut self) -> Task<Message> {
        self.preview_offset = 0.0;
        self.preview_progress = 0.0;
        self.preview_needs_reset = true;
        let Some(document) = self.workspace.active_document() else {
            self.editor = None;
            self.editor_doc = None;
            self.close_search();
            return Task::none();
        };

        if self.editor_doc == Some(document.id) {
            return Task::none();
        }

        let id = document.id;
        let text = document.text.clone();
        let cursor = (document.cursor.line, document.cursor.column);
        let scroll = document.scroll;
        let mut editor = EditorPane::new(&text, &self.tokens, &self.settings.editor_typography);
        editor.set_focus_mode(self.focus_mode);
        editor.focus();
        self.editor = Some(editor);
        self.editor_doc = Some(id);
        self.refresh_search_after_edit();
        self.refresh_preview(false);
        let reset_preview = self.scroll_preview_to(0.0);
        Task::batch([
            self.restore_editor_position_for(id, cursor, scroll),
            reset_preview,
        ])
    }

    fn restore_editor_position(&mut self) -> Task<Message> {
        let Some(id) = self.editor_doc else {
            return Task::none();
        };
        let Some(document) = self.workspace.document(id) else {
            return Task::none();
        };
        let cursor = (document.cursor.line, document.cursor.column);
        let scroll = document.scroll;
        self.editor_needs_position = false;
        self.restore_editor_position_for(id, cursor, scroll)
    }

    fn restore_editor_position_for(
        &mut self,
        _id: u64,
        cursor: (usize, usize),
        scroll: f32,
    ) -> Task<Message> {
        let Some(editor) = &mut self.editor else {
            return Task::none();
        };

        let goto = editor.goto_position(cursor).map(Message::Editor);
        let scroll = editor.scroll_to_vertical(scroll).map(Message::Editor);
        Task::batch([goto, scroll])
    }

    /// Reloads the editor buffer after the document changed on disk.
    fn reset_editor(&mut self) -> Task<Message> {
        let Some(id) = self.editor_doc else {
            return self.sync_editor();
        };
        let Some(document) = self.workspace.document(id) else {
            self.editor = None;
            self.editor_doc = None;
            self.close_search();
            return Task::none();
        };

        let text = document.text.clone();
        let cursor = (document.cursor.line, document.cursor.column);
        let scroll = document.scroll;
        let Some(editor) = &mut self.editor else {
            return self.sync_editor();
        };

        let reset = editor.reset(&text).map(Message::Editor);
        let goto = editor.goto_position(cursor).map(Message::Editor);
        let scroll = editor.scroll_to_vertical(scroll).map(Message::Editor);
        self.refresh_search_after_edit();
        Task::batch([reset, goto, scroll])
    }

    fn handle_editor(&mut self, message: EditorMessage) -> Task<Message> {
        match &message {
            EditorMessage::WriteRequested => return self.save_active(),
            EditorMessage::OpenSearch => return self.open_search(false),
            EditorMessage::OpenSearchReplace => return self.open_search(true),
            EditorMessage::CloseSearch if self.search.visible => {
                self.close_search();
                return Task::none();
            }
            EditorMessage::FindNext if self.search.visible => {
                return self.handle_search(search_ui::Message::Next);
            }
            EditorMessage::FindPrevious if self.search.visible => {
                return self.handle_search(search_ui::Message::Previous);
            }
            _ => {}
        }

        let scroll = match &message {
            EditorMessage::Scrolled(viewport) => Some(viewport.absolute_offset().y),
            _ => None,
        };
        let scrolled = scroll.is_some();
        let may_edit = message_may_edit(&message);

        let task = match &mut self.editor {
            Some(editor) => editor.update(message).map(Message::Editor),
            None => Task::none(),
        };

        let Some(id) = self.editor_doc else {
            return task;
        };
        let Some(editor) = &self.editor else {
            return task;
        };

        let (line, column) = editor.cursor();
        let cursor = Cursor::new(line, column);
        let scroll = scroll.unwrap_or_else(|| {
            self.workspace
                .document(id)
                .map_or(0.0, |document| document.scroll)
        });

        if may_edit {
            let content = editor.content();
            let search_visible = self.search.visible;
            self.workspace
                .set_document_content(id, content.clone(), cursor, scroll);
            self.preview_last_edit = Instant::now();
            if search_visible {
                self.search.refresh(&content);
                self.refresh_highlights();
            }
        } else {
            self.workspace.set_document_position(id, cursor, scroll);
        }

        let now = Instant::now();
        let programmatic = self
            .ignore_editor_scrolls_until
            .is_some_and(|until| now < until);
        let preview_target = if scrolled && !programmatic && self.layout == LayoutMode::SideBySide {
            self.editor.as_ref().and_then(|editor| {
                self.workspace.active_document().and_then(|document| {
                    document.preview.as_ref().map(|preview| {
                        preview_sync::preview_progress_for_line(
                            preview,
                            editor.first_visible_line(),
                        )
                    })
                })
            })
        } else {
            None
        };
        if let Some(progress) = preview_target {
            let preview_task = self.scroll_preview_to(progress);
            return Task::batch([task, preview_task]);
        }

        task
    }

    fn save_active(&mut self) -> Task<Message> {
        let Some(document) = self.workspace.active_document() else {
            self.set_error("There is no document to save.");
            return Task::none();
        };
        let title = document.display_title().to_string();

        match self.workspace.save_active() {
            Ok(()) => {
                self.set_status(format!("Saved {title}"));
                let _ = self.workspace.save_session();
            }
            Err(error) => self.set_error(format!("Could not save {title}: {error}")),
        }

        Task::none()
    }

    /// Builds an export snapshot from the current document.
    ///
    /// The preview is refreshed first so exports always reflect the live
    /// editor buffer, including when live preview is paused for a large file.
    fn build_export_request(&mut self, format: ExportFormat) -> Option<ExportRequest> {
        if let Some(document) = self.workspace.active_document_mut()
            && (document.preview.is_none() || document.preview_stale)
        {
            document.refresh_preview();
        }

        let document = self.workspace.active_document()?;
        let preview = document.preview.clone()?;
        let typography = self.settings.preview_typography.clone();
        let pdf = PdfOptions {
            page_size: PageSize::parse(&self.settings.page_size),
            margin: self.settings.page_margin,
            font_family: typography.font_family.clone(),
            font_size: typography.font_size,
        };

        Some(ExportRequest {
            format,
            source: document.text.clone(),
            preview,
            base_dir: document.path.parent().map(Path::to_path_buf),
            title: document.display_title().to_string(),
            theme: self.tokens.clone(),
            typography,
            pdf,
        })
    }

    fn begin_export(&mut self, format: ExportFormat) -> Task<Message> {
        let Some(request) = self.build_export_request(format) else {
            self.set_error("Open a document before exporting.");
            return Task::none();
        };

        Task::perform(run_export_dialog(request), Message::ExportFinished)
    }

    fn finish_export(
        &mut self,
        result: Result<Option<ExportOutcome>, ExportFailure>,
    ) -> Task<Message> {
        match result {
            Ok(Some(outcome)) => {
                self.set_status(format!(
                    "Exported {} to {}",
                    outcome.format.label(),
                    outcome.path.display()
                ));
            }
            Ok(None) => self.set_status("Export cancelled."),
            Err(failure) => {
                self.set_error(format!("Export failed: {}", failure.message));
                self.notify_error(format!("Export failed: {}", failure.message));
                self.dialog = Some(Dialog::Confirm {
                    action: ConfirmAction::RetryExport {
                        format: failure.format,
                    },
                    confirm: dialogs::Confirm {
                        title: String::from("Export failed"),
                        message: format!(
                            "{}\n\nChoose another location and try again?",
                            failure.message
                        ),
                        confirm_label: String::from("Choose another location"),
                    },
                });
            }
        }

        Task::none()
    }

    fn copy_as_html(&mut self) -> Task<Message> {
        let Some(request) = self.build_export_request(ExportFormat::Html) else {
            self.set_error("Open a document before copying.");
            return Task::none();
        };
        let options = request.html_options();

        match export::clipboard::copy_document(&request.preview, &options) {
            Ok(()) => self.set_status("Copied the document as HTML."),
            Err(error) => self.set_error(format!("Could not copy the document: {error}")),
        }

        Task::none()
    }

    fn copy_as_text(&mut self) -> Task<Message> {
        let Some(request) = self.build_export_request(ExportFormat::Html) else {
            self.set_error("Open a document before copying.");
            return Task::none();
        };
        let text = request.preview.plain_text();

        match export::clipboard::copy_text(&text) {
            Ok(()) => self.set_status("Copied the document as plain text."),
            Err(error) => self.set_error(format!("Could not copy the document: {error}")),
        }

        Task::none()
    }

    fn print_active(&mut self) -> Task<Message> {
        let Some(request) = self.build_export_request(ExportFormat::Pdf) else {
            self.set_error("Open a document before printing.");
            return Task::none();
        };

        match export::print::print_document(
            &request.preview,
            &request.pdf,
            &request.theme,
            &request.typography,
            request.base_dir.as_deref(),
        ) {
            Ok(path) => self.set_status(format!("Opened the print view for {}", path.display())),
            Err(error) => self.set_error(format!("Could not print the document: {error}")),
        }

        Task::none()
    }

    fn open_folder(&mut self, root: PathBuf) -> Task<Message> {
        let failures: Vec<_> = self
            .workspace
            .save_all()
            .into_iter()
            .filter(|outcome| !outcome.saved)
            .collect();
        if let Some(failure) = failures.first() {
            self.set_error(format!(
                "Could not save {} before switching folders: {}",
                failure.path.display(),
                failure.error.as_deref().unwrap_or("unknown error")
            ));
            return Task::none();
        }

        match self.workspace.open_folder(&root) {
            Ok(()) => {
                self.settings.add_recent_workspace(root.clone());
                persisted_settings(&self.settings, &self.paths);
                self.expanded.clear();
                self.dialog = None;
                self.compare = None;
                self.selected = None;
                self.install_editor();
                self.selected = self.active_path();
                self.set_status(format!("Opened {}", root.display()));
                let _ = self.workspace.save_session();
            }
            Err(error) => self.set_error(format!("Could not open {}: {error}", root.display())),
        }

        Task::none()
    }

    fn open_file(&mut self, path: PathBuf) -> Task<Message> {
        match self.workspace.open_document(&path) {
            Ok(_) => {
                self.dialog = None;
                self.selected = Some(path.clone());
                self.set_status(format!("Opened {}", path.display()));
                self.sync_editor()
            }
            Err(error) => {
                self.set_error(format!("Could not open {}: {error}", path.display()));
                Task::none()
            }
        }
    }

    fn select_tab(&mut self, id: u64) -> Task<Message> {
        if self.workspace.activate(id) {
            self.selected = self.active_path();
            self.sync_editor()
        } else {
            Task::none()
        }
    }

    fn request_close_tab(&mut self, id: u64) {
        let Some(document) = self.workspace.document(id) else {
            return;
        };

        if document.is_dirty() {
            self.dialog = Some(Dialog::Confirm {
                action: ConfirmAction::CloseTab { id },
                confirm: dialogs::Confirm {
                    title: String::from("Close tab"),
                    message: format!(
                        "{} has unsaved changes. Close it without saving?",
                        document.display_title()
                    ),
                    confirm_label: String::from("Close without saving"),
                },
            });
        } else {
            self.close_tab(id);
        }
    }

    fn close_tab(&mut self, id: u64) {
        let was_active = self.workspace.active_id() == Some(id);
        let closed = self.workspace.close_document(id);
        if let Some(document) = &closed
            && self.selected.as_deref() == Some(document.path.as_path())
        {
            self.selected = None;
        }
        if was_active {
            self.install_editor();
            if self.editor.is_some() {
                self.editor_needs_position = true;
            }
        }
        self.selected = self.selected.take().or_else(|| self.active_path());
        let _ = self.workspace.save_session();
    }

    fn request_delete(&mut self, path: PathBuf) {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.dialog = Some(Dialog::Confirm {
            action: ConfirmAction::Delete { path },
            confirm: dialogs::Confirm {
                title: String::from("Move to trash"),
                message: format!("Move {name} to the operating system trash?"),
                confirm_label: String::from("Move to trash"),
            },
        });
    }

    fn new_input_dialog(
        action: InputAction,
        title: &str,
        label: &str,
        value: &str,
        placeholder: &str,
    ) -> Dialog {
        Dialog::Input {
            action,
            input: dialogs::Input {
                title: title.to_string(),
                label: label.to_string(),
                value: value.to_string(),
                placeholder: placeholder.to_string(),
            },
        }
    }

    fn begin_new_file(&mut self) {
        let Some(parent) = self.current_parent() else {
            self.set_error("Open a folder first.");
            return;
        };
        self.dialog = Some(Self::new_input_dialog(
            InputAction::NewFile { parent },
            "New document",
            "File name",
            "",
            "note.md",
        ));
    }

    fn begin_new_folder(&mut self) {
        let Some(parent) = self.current_parent() else {
            self.set_error("Open a folder first.");
            return;
        };
        self.dialog = Some(Self::new_input_dialog(
            InputAction::NewFolder { parent },
            "New folder",
            "Folder name",
            "",
            "notes",
        ));
    }

    fn begin_rename(&mut self) {
        let Some(path) = self.target_path() else {
            self.set_error("Select a file or folder first.");
            return;
        };
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        self.dialog = Some(Self::new_input_dialog(
            InputAction::Rename { path },
            "Rename",
            "New name",
            &name,
            "name.md",
        ));
    }

    fn begin_move(&mut self) {
        let Some(path) = self.target_path() else {
            self.set_error("Select a file or folder first.");
            return;
        };
        self.dialog = Some(Self::new_input_dialog(
            InputAction::Move { path },
            "Move",
            "Destination folder relative to the workspace (empty for the root)",
            "",
            "notes",
        ));
    }

    fn submit_dialog(&mut self) -> Task<Message> {
        let Some(dialog) = self.dialog.take() else {
            return Task::none();
        };

        match dialog {
            Dialog::Input { action, input } => self.submit_input(action, input.value),
            Dialog::Confirm { action, .. } => match action {
                ConfirmAction::Delete { path } => {
                    match self.workspace.delete_entry(&path) {
                        Ok(()) => {
                            if self.selected.as_deref().is_some_and(|selected| {
                                selected == path || selected.starts_with(&path)
                            }) {
                                self.selected = None;
                            }
                            self.set_status(format!("Moved {} to trash", path.display()));
                            self.install_editor();
                            self.selected = self.active_path();
                            let _ = self.workspace.save_session();
                        }
                        Err(error) => {
                            self.set_error(format!("Could not delete {}: {error}", path.display()))
                        }
                    }
                    Task::none()
                }
                ConfirmAction::CloseTab { id } => {
                    self.close_tab(id);
                    Task::none()
                }
                ConfirmAction::RetryExport { format } => self.begin_export(format),
                ConfirmAction::ResetSettings => {
                    self.reset_settings();
                    Task::none()
                }
            },
        }
    }

    fn submit_input(&mut self, action: InputAction, value: String) -> Task<Message> {
        match action {
            InputAction::NewFile { parent } => {
                let name = value.trim();
                if name.is_empty() {
                    self.set_error("Enter a file name.");
                    return Task::none();
                }
                match self.workspace.create_document(&parent, name, "") {
                    Ok(id) => {
                        self.selected = self
                            .workspace
                            .document(id)
                            .map(|document| document.path.clone());
                        self.set_status(format!("Created {name}"));
                        self.sync_editor()
                    }
                    Err(error) => {
                        self.set_error(format!("Could not create {name}: {error}"));
                        Task::none()
                    }
                }
            }
            InputAction::NewFolder { parent } => {
                let name = value.trim();
                if name.is_empty() {
                    self.set_error("Enter a folder name.");
                    return Task::none();
                }
                match self.workspace.create_folder(&parent, name) {
                    Ok(path) => {
                        self.expanded.insert(path.clone());
                        self.selected = Some(path);
                        self.set_status(format!("Created {name}"));
                    }
                    Err(error) => self.set_error(format!("Could not create {name}: {error}")),
                }
                Task::none()
            }
            InputAction::Rename { path } => {
                let new_name = value.trim();
                if new_name.is_empty() {
                    self.set_error("Enter a name.");
                    return Task::none();
                }
                match self.workspace.rename_entry(&path, new_name) {
                    Ok(new_path) => {
                        if self.selected.as_deref() == Some(path.as_path()) {
                            self.selected = Some(new_path.clone());
                        }
                        self.set_status(format!("Renamed to {}", new_path.display()));
                        let _ = self.workspace.save_session();
                    }
                    Err(error) => self.set_error(format!("Could not rename: {error}")),
                }
                Task::none()
            }
            InputAction::Move { path } => {
                let Some(root) = self.root().map(Path::to_path_buf) else {
                    self.set_error("Open a folder first.");
                    return Task::none();
                };
                let destination = value.trim();
                let destination = if destination.is_empty() {
                    root
                } else {
                    root.join(destination)
                };

                match self.workspace.move_entry(&path, &destination) {
                    Ok(new_path) => {
                        if self.selected.as_deref() == Some(path.as_path()) {
                            self.selected = Some(new_path.clone());
                        }
                        self.set_status(format!("Moved to {}", new_path.display()));
                        let _ = self.workspace.save_session();
                    }
                    Err(error) => self.set_error(format!("Could not move: {error}")),
                }
                Task::none()
            }
            InputAction::InsertLink { image } => {
                let url = value.trim();
                if url.is_empty() {
                    self.set_error("Enter a URL.");
                    return Task::none();
                }
                self.insert_link(url, image)
            }
        }
    }

    fn tick(&mut self) -> Task<Message> {
        let now = Instant::now();
        let mut tasks = Vec::new();
        self.notifications.expire(Duration::from_secs(6));

        for outcome in self
            .workspace
            .autosave_due(now, Duration::from_millis(self.settings.autosave_ms))
        {
            if outcome.saved {
                self.set_status(format!("Autosaved {}", outcome.path.display()));
            } else {
                self.set_error(format!(
                    "Autosave failed for {}: {}",
                    outcome.path.display(),
                    outcome.error.as_deref().unwrap_or("unknown error")
                ));
            }
        }

        let active_path = self.active_path();
        for event in self.workspace.poll_external_changes() {
            let is_active = active_path.as_deref() == Some(event.path.as_path());
            match event.change {
                ExternalChange::Reloaded => {
                    self.set_status(format!("Reloaded {}", event.path.display()));
                    if is_active {
                        tasks.push(self.reset_editor());
                        self.preview_last_edit = now;
                        self.refresh_preview(false);
                    }
                }
                ExternalChange::Conflict => {
                    self.set_error(format!(
                        "{} changed on disk while it had unsaved edits.",
                        event.path.display()
                    ));
                }
                ExternalChange::Missing => {
                    self.set_error(format!("{} was deleted on disk.", event.path.display()));
                }
                ExternalChange::None => {}
            }
        }

        if self.last_recovery.elapsed() >= Duration::from_secs(5) {
            if let Err(error) = self.workspace.write_recovery() {
                log::warn!("could not write recovery buffers: {error}");
            }
            self.last_recovery = now;
        }

        if self.last_session.elapsed() >= Duration::from_secs(2) {
            let _ = self.workspace.save_session();
            self.last_session = now;
        }

        if self.preview_needs_reset {
            self.preview_needs_reset = false;
            tasks.push(self.scroll_preview_to(0.0));
        }

        self.refresh_preview_if_due(now);

        if self.editor_needs_position {
            tasks.push(self.restore_editor_position());
        }

        Task::batch(tasks)
    }

    fn recover(&mut self, index: usize) -> Task<Message> {
        if index >= self.recovery.len() {
            return Task::none();
        }
        let record = self.recovery.remove(index);
        let title = record.title.clone();
        self.workspace.accept_recovery(record);
        self.selected = self.active_path();
        self.set_status(format!("Recovered {title}"));
        self.sync_editor()
    }

    fn discard_recovery(&mut self, index: usize) {
        if index >= self.recovery.len() {
            return;
        }
        let record = self.recovery.remove(index);
        if let Err(error) = self.workspace.discard_recovery(&record) {
            self.set_error(format!("Could not discard the recovery buffer: {error}"));
        } else {
            self.set_status(format!("Discarded recovery buffer for {}", record.title));
        }
    }
}

fn new_pane_state(ratio: f32) -> pane_grid::State<PaneKind> {
    let (mut panes, editor_pane) = pane_grid::State::new(PaneKind::Editor);
    if let Some((_, split)) = panes.split(pane_grid::Axis::Vertical, editor_pane, PaneKind::Preview)
    {
        panes.resize(split, ratio);
    }
    panes
}

fn persisted_settings(settings: &Settings, paths: &AppPaths) {
    if let Err(error) = settings.save(&paths.settings_file()) {
        log::warn!("could not save settings: {error}");
    }
}

/// Asks for a destination and writes the export.
///
/// Cancelling the dialog returns `Ok(None)`; a failed write returns an
/// [`ExportFailure`] so the UI can offer another location.
async fn run_export_dialog(request: ExportRequest) -> Result<Option<ExportOutcome>, ExportFailure> {
    let path = rfd::AsyncFileDialog::new()
        .set_title(format!("Export {}", request.format.label()))
        .set_file_name(request.suggested_name())
        .add_filter(request.format.filter_name(), &[request.format.extension()])
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf());

    export::export_to_choice(&request, path)
}

/// Returns the URL to open when a preview link points outside the document.
fn external_url(url: &str) -> Option<&str> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        Some(url)
    } else {
        None
    }
}

/// Maps fixed application shortcuts to messages.
///
/// The editor consumes the shortcuts it owns (save, built-in find, undo), so
/// this listener only sees keys the editor left ignored. The editor adapter
/// still routes its find messages into the shared search panel.
fn shortcut_message(event: iced::keyboard::Event) -> Option<Message> {
    let iced::keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    } = event
    else {
        return None;
    };

    shortcut_for(&key, modifiers, repeat)
}

/// Arrow and Enter handling while the quick-open palette is visible.
fn quick_open_key_message(event: iced::keyboard::Event) -> Option<Message> {
    let iced::keyboard::Event::KeyPressed { key, .. } = event else {
        return None;
    };

    match key {
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
            Some(Message::QuickOpen(palette::Message::Move(1)))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
            Some(Message::QuickOpen(palette::Message::Move(-1)))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
            Some(Message::QuickOpen(palette::Message::Accept))
        }
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
            Some(Message::QuickOpen(palette::Message::Close))
        }
        _ => None,
    }
}

fn shortcut_for(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    repeat: bool,
) -> Option<Message> {
    if repeat {
        return None;
    }

    if matches!(
        key,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
    ) {
        return Some(Message::Escape);
    }

    let command = if cfg!(target_os = "macos") {
        modifiers.command()
    } else {
        modifiers.control()
    };
    if !command || modifiers.alt() {
        return None;
    }

    let iced::keyboard::Key::Character(character) = key else {
        return None;
    };
    let character = character.to_lowercase();
    let shift = modifiers.shift();

    Some(match (character.as_str(), shift) {
        ("s", false) => Message::Save,
        ("p", false) => Message::OpenQuickOpen,
        (",", false) => Message::ToggleSettings,
        ("/", false) => Message::OpenCheatSheet,
        ("f", false) => Message::OpenFind,
        ("h", false) => Message::OpenReplace,
        ("b", false) => Message::Format(FormatAction::Bold),
        ("i", false) => Message::Format(FormatAction::Italic),
        ("i", true) => Message::Format(FormatAction::Image),
        ("x", true) => Message::Format(FormatAction::Strikethrough),
        ("8", true) => Message::Format(FormatAction::BulletList),
        ("7", true) => Message::Format(FormatAction::OrderedList),
        ("9", true) => Message::Format(FormatAction::TaskList),
        (".", true) => Message::Format(FormatAction::Blockquote),
        ("`", false) => Message::Format(FormatAction::InlineCode),
        ("`", true) => Message::Format(FormatAction::CodeBlock),
        ("t", false) => Message::Format(FormatAction::Table),
        ("k", false) => Message::Format(FormatAction::Link),
        ("1", false) => Message::Format(FormatAction::Heading(1)),
        ("2", false) => Message::Format(FormatAction::Heading(2)),
        ("3", false) => Message::Format(FormatAction::Heading(3)),
        ("4", false) => Message::Format(FormatAction::Heading(4)),
        ("5", false) => Message::Format(FormatAction::Heading(5)),
        ("6", false) => Message::Format(FormatAction::Heading(6)),
        _ => return None,
    })
}

/// Every user-visible action flows through this enum.
#[derive(Debug, Clone)]
pub enum Message {
    OpenFolder,
    FolderPicked(Option<PathBuf>),
    OpenFile(PathBuf),
    ToggleFolder(PathBuf),
    SelectTab(u64),
    CloseTab(u64),
    NewFile,
    NewFolder,
    Rename,
    Move,
    Delete,
    Import,
    ImportPicked(Option<PathBuf>),
    DialogValueChanged(String),
    DialogSubmit,
    DialogCancel,
    Save,
    SaveAll,
    Export(ExportFormat),
    ExportFinished(Result<Option<ExportOutcome>, ExportFailure>),
    CopyAsHtml,
    CopyAsText,
    Print,
    ToggleSettings,
    Settings(settings_ui::Message),
    SystemThemeChanged(bool),
    UpdateCheckFinished(Result<Option<UpdateInfo>, String>),
    UpdateInstallFinished(Result<String, String>),
    DismissNotification(u64),
    OpenQuickOpen,
    QuickOpen(palette::Message),
    OpenWelcome,
    OpenCheatSheet,
    CloseInfoOverlay,
    InfoPreview(preview_render::Message),
    Sidebar(sidebar::Message),
    Editor(EditorMessage),
    Preview(preview_render::Message),
    RefreshPreview,
    ConflictReload,
    ConflictKeepLocal,
    ConflictCompare,
    CompareClose,
    MissingSave,
    MissingClose,
    Recover(usize),
    DiscardRecovery(usize),
    RecoverAll,
    DiscardAllRecovery,
    ToggleLayout(LayoutMode),
    ToggleSidebar,
    ToggleOutline,
    ToggleFocusMode,
    Format(FormatAction),
    OpenFind,
    OpenReplace,
    Search(search_ui::Message),
    Outline(outline::Message),
    PaneResized(pane_grid::ResizeEvent),
    Escape,
    Tick,
    Quit,
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::OpenFolder => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Open a Markdown folder")
                    .pick_folder()
                    .await
                    .map(|handle| handle.path().to_path_buf())
            },
            Message::FolderPicked,
        ),
        Message::FolderPicked(Some(root)) => app.open_folder(root),
        Message::FolderPicked(None) => Task::none(),
        Message::OpenFile(path) => app.open_file(path),
        Message::ToggleFolder(path) => {
            app.selected = Some(path.clone());
            if !app.expanded.remove(&path) {
                app.expanded.insert(path);
            }
            Task::none()
        }
        Message::SelectTab(id) => app.select_tab(id),
        Message::CloseTab(id) => {
            app.request_close_tab(id);
            Task::none()
        }
        Message::NewFile => {
            app.begin_new_file();
            Task::none()
        }
        Message::NewFolder => {
            app.begin_new_folder();
            Task::none()
        }
        Message::Rename => {
            app.begin_rename();
            Task::none()
        }
        Message::Move => {
            app.begin_move();
            Task::none()
        }
        Message::Delete => {
            if let Some(path) = app.target_path() {
                app.request_delete(path);
            } else {
                app.set_error("Select a file or folder first.");
            }
            Task::none()
        }
        Message::Import => {
            let Some(parent) = app.current_parent() else {
                app.set_error("Open a folder first.");
                return Task::none();
            };
            app.dialog = None;
            Task::perform(
                async move {
                    let handle = rfd::AsyncFileDialog::new()
                        .set_title("Import a document")
                        .add_filter(
                            "Markdown and HTML",
                            &["md", "markdown", "mdown", "html", "htm"],
                        )
                        .pick_file()
                        .await?;
                    Some((handle.path().to_path_buf(), parent))
                },
                |selection| match selection {
                    Some((path, _)) => Message::ImportPicked(Some(path)),
                    None => Message::ImportPicked(None),
                },
            )
        }
        Message::ImportPicked(Some(source)) => {
            let Some(destination) = app.current_parent() else {
                app.set_error("Open a folder first.");
                return Task::none();
            };
            match app.workspace.import_file(&source, destination) {
                Ok(result) => {
                    app.set_status(format!("Imported {}", result.path.display()));
                    let _ = app.workspace.save_session();
                    app.sync_editor()
                }
                Err(error) => {
                    app.set_error(format!("Could not import {}: {error}", source.display()));
                    Task::none()
                }
            }
        }
        Message::ImportPicked(None) => Task::none(),
        Message::DialogValueChanged(value) => {
            if let Some(Dialog::Input { input, .. }) = &mut app.dialog {
                input.value = value;
            }
            Task::none()
        }
        Message::DialogSubmit => app.submit_dialog(),
        Message::DialogCancel => {
            app.dialog = None;
            Task::none()
        }
        Message::Save => app.save_active(),
        Message::SaveAll => {
            let outcomes = app.workspace.save_all();
            let failures: Vec<_> = outcomes.iter().filter(|outcome| !outcome.saved).collect();
            if failures.is_empty() {
                app.set_status(format!("Saved {} document(s)", outcomes.len()));
            } else {
                app.set_error(format!(
                    "{} document(s) could not be saved: {}",
                    failures.len(),
                    failures[0].error.as_deref().unwrap_or("unknown error")
                ));
            }
            let _ = app.workspace.save_session();
            Task::none()
        }
        Message::Export(format) => app.begin_export(format),
        Message::ExportFinished(result) => app.finish_export(result),
        Message::CopyAsHtml => app.copy_as_html(),
        Message::CopyAsText => app.copy_as_text(),
        Message::Print => app.print_active(),
        Message::ToggleSettings => {
            if app.settings_open {
                app.close_settings();
            } else {
                app.open_settings();
            }
            Task::none()
        }
        Message::Settings(message) => app.handle_settings(message),
        Message::SystemThemeChanged(dark) => {
            app.system_theme_changed(dark);
            Task::none()
        }
        Message::UpdateCheckFinished(result) => app.update_check_finished(result),
        Message::UpdateInstallFinished(result) => app.update_install_finished(result),
        Message::DismissNotification(id) => {
            app.notifications.dismiss(id);
            Task::none()
        }
        Message::OpenQuickOpen => app.open_quick_open(),
        Message::QuickOpen(message) => app.handle_quick_open(message),
        Message::OpenWelcome => app.open_info(InfoOverlay::Welcome),
        Message::OpenCheatSheet => app.open_info(InfoOverlay::CheatSheet),
        Message::CloseInfoOverlay => {
            app.close_info();
            Task::none()
        }
        Message::InfoPreview(message) => app.handle_info_preview(message),
        Message::Sidebar(message) => match message {
            sidebar::Message::OpenFile(path) => app.open_file(path),
            sidebar::Message::ToggleFolder(path) => {
                app.selected = Some(path.clone());
                if !app.expanded.remove(&path) {
                    app.expanded.insert(path);
                }
                Task::none()
            }
            sidebar::Message::OpenFolder => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Open a Markdown folder")
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::FolderPicked,
            ),
            sidebar::Message::NewFile => {
                app.begin_new_file();
                Task::none()
            }
            sidebar::Message::NewFolder => {
                app.begin_new_folder();
                Task::none()
            }
            sidebar::Message::ImportFile => update(app, Message::Import),
            sidebar::Message::Rename => {
                app.begin_rename();
                Task::none()
            }
            sidebar::Message::Delete => {
                if let Some(path) = app.target_path() {
                    app.request_delete(path);
                }
                Task::none()
            }
            sidebar::Message::Move => {
                app.begin_move();
                Task::none()
            }
        },
        Message::Editor(message) => app.handle_editor(message),
        Message::Preview(message) => app.handle_preview(message),
        Message::RefreshPreview => {
            if app.refresh_preview(true) {
                app.set_status("Refreshed the preview.");
            } else {
                app.set_status("There is no document to refresh.");
            }
            Task::none()
        }
        Message::ConflictReload => {
            let Some(document) = app.workspace.active_document_mut() else {
                return Task::none();
            };
            match document.accept_disk_version() {
                Ok(()) => {
                    app.set_status("Reloaded the version on disk.");
                    app.reset_editor()
                }
                Err(error) => {
                    app.set_error(format!("Could not reload the document: {error}"));
                    Task::none()
                }
            }
        }
        Message::ConflictKeepLocal => {
            if let Some(document) = app.workspace.active_document_mut() {
                document.keep_local_version();
                app.set_status("Keeping local edits. The next save will overwrite the file.");
            }
            Task::none()
        }
        Message::ConflictCompare => {
            let Some(document) = app.workspace.active_document() else {
                return Task::none();
            };
            match document.disk_text() {
                Ok(disk) => {
                    app.compare = Some(Compare {
                        path: document.path.clone(),
                        local: document.text.clone(),
                        disk,
                    });
                }
                Err(error) => app.set_error(format!("Could not read the disk version: {error}")),
            }
            Task::none()
        }
        Message::CompareClose => {
            app.compare = None;
            Task::none()
        }
        Message::MissingSave => {
            let Some(id) = app.workspace.active_id() else {
                return Task::none();
            };
            match app.workspace.recreate_missing_file(id) {
                Ok(()) => {
                    app.set_status("Saved the document again.");
                    app.reset_editor()
                }
                Err(error) => {
                    app.set_error(format!("Could not save the document: {error}"));
                    Task::none()
                }
            }
        }
        Message::MissingClose => {
            if let Some(id) = app.workspace.active_id() {
                app.close_tab(id);
            }
            Task::none()
        }
        Message::Recover(index) => app.recover(index),
        Message::DiscardRecovery(index) => {
            app.discard_recovery(index);
            Task::none()
        }
        Message::RecoverAll => {
            let mut task = Task::none();
            while let Some(record) = app.recovery.first().cloned() {
                app.recovery.remove(0);
                app.workspace.accept_recovery(record);
            }
            app.selected = app.active_path();
            if let Some(id) = app.workspace.active_id() {
                app.workspace.activate(id);
                task = app.sync_editor();
            }
            app.set_status("Recovered unsaved changes.");
            task
        }
        Message::DiscardAllRecovery => {
            if let Err(error) = app.workspace.discard_all_recovery() {
                app.set_error(format!("Could not discard recovery buffers: {error}"));
            } else {
                app.recovery.clear();
                app.set_status("Discarded all recovery buffers.");
            }
            Task::none()
        }
        Message::ToggleLayout(layout) => {
            app.layout = layout;
            app.settings.layout = layout.as_settings().to_string();
            persisted_settings(&app.settings, &app.paths);
            app.set_status(format!("{} layout", layout.label()));
            Task::none()
        }
        Message::ToggleSidebar => {
            app.sidebar_visible = !app.sidebar_visible;
            app.settings.sidebar_visible = app.sidebar_visible;
            persisted_settings(&app.settings, &app.paths);
            Task::none()
        }
        Message::ToggleOutline => {
            app.outline_visible = !app.outline_visible;
            app.settings.outline_visible = app.outline_visible;
            persisted_settings(&app.settings, &app.paths);
            Task::none()
        }
        Message::ToggleFocusMode => {
            app.focus_mode = !app.focus_mode;
            app.settings.focus_mode = app.focus_mode;
            if let Some(editor) = &mut app.editor {
                editor.set_focus_mode(app.focus_mode);
            }
            persisted_settings(&app.settings, &app.paths);
            app.set_status(if app.focus_mode {
                "Focus mode on"
            } else {
                "Focus mode off"
            });
            Task::none()
        }
        Message::Format(action) => app.apply_format(action),
        Message::OpenFind => app.open_search(false),
        Message::OpenReplace => app.open_search(true),
        Message::Search(message) => app.handle_search(message),
        Message::Outline(message) => match message {
            outline::Message::Goto(line) => app.goto_heading(line),
        },
        Message::PaneResized(event) => {
            app.resize_panes(event);
            Task::none()
        }
        Message::Escape => {
            if app.quick_open.is_some() {
                app.quick_open = None;
            } else if app.search.visible {
                app.close_search();
            } else if app.dialog.is_some() {
                app.dialog = None;
            } else if app.compare.is_some() {
                app.compare = None;
            } else if app.settings_open {
                app.close_settings();
            } else if app.info_overlay.is_some() {
                app.close_info();
            }
            Task::none()
        }
        Message::Tick => app.tick(),
        Message::Quit => {
            let _ = app.workspace.save_session();
            let _ = app.workspace.write_recovery();
            iced::exit()
        }
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let tokens = &app.tokens;
    let mut layers = iced::widget::Stack::new();
    layers = layers.push(base_view(app, tokens));

    if let Some(kind) = app.info_overlay
        && let Some(preview) = &app.info_preview
    {
        layers = layers.push(info_overlay_view(app, kind, preview, tokens));
    }

    if app.settings_open {
        layers = layers.push(
            settings_ui::view(
                &app.settings,
                tokens,
                app.system_dark,
                app.preview_theme.as_deref(),
                &app.update_state,
            )
            .map(Message::Settings),
        );
    }

    if let Some(quick) = &app.quick_open {
        layers = layers.push(palette::view(quick, tokens).map(Message::QuickOpen));
    }

    if !app.recovery.is_empty() {
        layers = layers.push(recovery_overlay(app, tokens));
    } else if let Some(dialog) = &app.dialog {
        layers = layers.push(dialog_view(dialog, tokens));
    } else if let Some(compare) = &app.compare {
        layers = layers.push(compare_view(compare, tokens));
    }

    if !app.notifications.is_empty() {
        layers = layers.push(notifications_layer(app, tokens));
    }

    layers.into()
}

fn notifications_layer<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let stack = container(notifications::view(
        &app.notifications,
        tokens,
        Message::DismissNotification,
    ))
    .width(Length::Fixed(380.0));

    container(stack)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::End)
        .align_y(iced::Alignment::Start)
        .padding(12)
        .into()
}

fn info_overlay_view<'a>(
    app: &'a App,
    kind: InfoOverlay,
    preview: &'a PreviewDocument,
    tokens: &'a ThemeTokens,
) -> Element<'a, Message> {
    let styles = preview_render::PreviewStyles {
        tokens,
        typography: &app.settings.preview_typography,
        base_dir: None,
    };
    let viewport = preview_render::PreviewViewport {
        offset_y: 0.0,
        height: 520.0,
    };
    let preview: Element<'a, preview_render::Message> = preview_render::view(
        preview,
        styles,
        &app.preview_caches,
        &app.info_scroll_id,
        viewport,
    );

    let card = container(
        column![
            row![
                text(kind.title()).size(18).color(tokens.heading),
                Space::new().width(Length::Fill),
                button(text("Close").size(12)).on_press(Message::CloseInfoOverlay),
            ]
            .align_y(iced::Alignment::Center),
            container(preview.map(Message::InfoPreview))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(tokens.background.into()),
                    border: iced::border::Border {
                        color: tokens.border,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                }),
        ]
        .spacing(10)
        .padding(16)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .max_width(860.0)
    .height(Length::Fill)
    .max_height(680.0)
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

fn base_view<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let content: Element<'a, Message> = if app.workspace.is_open() {
        workspace_view(app, tokens)
    } else {
        welcome_view(tokens)
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(tokens.background.into()),
            text_color: Some(tokens.text),
            ..container::Style::default()
        })
        .into()
}

fn welcome_view<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    container(
        column![
            text("markmydown").size(30).color(tokens.heading),
            text("A fast, native, offline Markdown workbench.")
                .size(14)
                .color(tokens.muted),
            row![
                button(text("Open a folder").size(14))
                    .padding([8, 16])
                    .on_press(Message::OpenFolder),
                button(text("Read the welcome guide").size(14))
                    .padding([8, 16])
                    .on_press(Message::OpenWelcome),
                button(text("Cheat sheet").size(14))
                    .padding([8, 16])
                    .on_press(Message::OpenCheatSheet),
            ]
            .spacing(8),
            text("Your files stay on this computer.")
                .size(12)
                .color(tokens.muted),
        ]
        .spacing(14)
        .align_x(iced::Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn workspace_view<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    if app.layout == LayoutMode::Read {
        return read_view(app, tokens);
    }

    let mut content = column![toolbar(app, tokens), tab_bar(app, tokens)]
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(banner) = banners(app, tokens) {
        content = content.push(banner);
    }

    if app.search.visible {
        content = content.push(app.search.view(tokens).map(Message::Search));
    }

    if app.layout.shows_editor() {
        content = content.push(formatting_toolbar(tokens));
    }

    let selected = app.selected.as_ref();
    let main: Element<'a, Message> = if app.sidebar_visible {
        let mut panes = row![
            container(
                sidebar::view(app.workspace.tree(), &app.expanded, selected, tokens)
                    .map(Message::Sidebar)
            )
            .width(Length::Fixed(250.0))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(tokens.surface.into()),
                border: iced::border::Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }),
            content_area(app, tokens),
        ]
        .height(Length::Fill);

        if app.outline_visible {
            let headings = app
                .workspace
                .active_document()
                .map(|document| document.outline())
                .unwrap_or_default();
            let active_line = app
                .workspace
                .active_document()
                .map(|document| document.cursor.line);
            panes = panes.push(outline::view(&headings, active_line, tokens).map(Message::Outline));
        }

        panes.into()
    } else {
        content_area(app, tokens)
    };

    content = content.push(main).push(status_bar(app, tokens));
    content.into()
}

fn read_view<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    if app.workspace.active_document().is_none() {
        return empty_workspace(app, tokens);
    }

    let title = app
        .workspace
        .active_document()
        .map(|document| document.display_title().to_string())
        .unwrap_or_default();
    let headings = app
        .workspace
        .active_document()
        .map(|document| document.outline())
        .unwrap_or_default();
    let active_line = app
        .workspace
        .active_document()
        .and_then(|document| document.preview.as_ref())
        .map(|preview| {
            preview_sync::editor_line_for_preview_progress(preview, app.preview_progress)
        });

    let header = container(
        row![
            text(title).size(14).color(tokens.heading),
            Space::new().width(Length::Fill),
            text(format!("{:.0}% read", app.preview_progress * 100.0))
                .size(12)
                .color(tokens.muted),
            button(text("Outline").size(12)).on_press(Message::ToggleOutline),
            button(text("Edit").size(12)).on_press(Message::ToggleLayout(LayoutMode::SideBySide)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(tokens.surface.into()),
        border: iced::border::Border {
            color: tokens.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let mut body = row![].width(Length::Fill).height(Length::Fill);
    if app.outline_visible {
        body = body.push(outline::view(&headings, active_line, tokens).map(Message::Outline));
    }
    body = body.push(preview_view(app, tokens));

    column![header, body, status_bar(app, tokens)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn toolbar<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let title = app.root().and_then(|root| root.file_name()).map_or_else(
        || String::from("No folder"),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut controls = row![
        button(text("Open folder").size(12)).on_press(Message::OpenFolder),
        button(text("Save").size(12)).on_press(Message::Save),
        button(text("Save all").size(12)).on_press(Message::SaveAll),
        button(text("New").size(12)).on_press(Message::NewFile),
        button(text("Folder").size(12)).on_press(Message::NewFolder),
        button(text("Import").size(12)).on_press(Message::Import),
        button(text("Rename").size(12)).on_press(Message::Rename),
        button(text("Move").size(12)).on_press(Message::Move),
        button(text("Delete").size(12)).on_press(Message::Delete),
        button(text("Find").size(12)).on_press(Message::OpenFind),
        button(text("Replace").size(12)).on_press(Message::OpenReplace),
        button(text("Quick open").size(12)).on_press(Message::OpenQuickOpen),
        button(text("Settings").size(12)).on_press(Message::ToggleSettings),
        button(text("Cheat sheet").size(12)).on_press(Message::OpenCheatSheet),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    for format in ExportFormat::ALL {
        controls = controls
            .push(button(text(format.action_label()).size(12)).on_press(Message::Export(format)));
    }
    controls = controls.push(button(text("Copy HTML").size(12)).on_press(Message::CopyAsHtml));
    controls = controls.push(button(text("Copy text").size(12)).on_press(Message::CopyAsText));
    controls = controls.push(button(text("Print").size(12)).on_press(Message::Print));

    for mode in LayoutMode::ALL {
        let active = app.layout == mode;
        controls = controls.push(
            button(text(mode.label()).size(12))
                .on_press(Message::ToggleLayout(mode))
                .style(move |theme, status| {
                    let mut style = button::text(theme, status);
                    if active {
                        style.background = Some(tokens.accent.into());
                        style.text_color = tokens.background;
                    }
                    style
                }),
        );
    }

    controls = controls.push(button(text("Sidebar").size(12)).on_press(Message::ToggleSidebar));
    controls = controls.push(button(text("Outline").size(12)).on_press(Message::ToggleOutline));
    controls = controls.push(button(text("Focus").size(12)).on_press(Message::ToggleFocusMode));

    let bar = row![
        container(
            scrollable(controls).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
        )
        .width(Length::Fill),
        container(text(title).size(12).color(tokens.muted)).padding([0, 8]),
    ]
    .align_y(iced::Alignment::Center);

    container(bar)
        .padding([6, 8])
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

fn formatting_toolbar<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let actions = [
        ("Bold", FormatAction::Bold),
        ("Italic", FormatAction::Italic),
        ("H1", FormatAction::Heading(1)),
        ("H2", FormatAction::Heading(2)),
        ("Strike", FormatAction::Strikethrough),
        ("List", FormatAction::BulletList),
        ("Numbered", FormatAction::OrderedList),
        ("Check", FormatAction::TaskList),
        ("Quote", FormatAction::Blockquote),
        ("Code", FormatAction::InlineCode),
        ("Code block", FormatAction::CodeBlock),
        ("Table", FormatAction::Table),
        ("Link", FormatAction::Link),
        ("Image", FormatAction::Image),
    ];

    let mut controls = row![].spacing(3).align_y(iced::Alignment::Center);
    for (label, action) in actions {
        controls = controls.push(
            button(text(label).size(11))
                .padding([3, 6])
                .on_press(Message::Format(action)),
        );
    }

    container(
        scrollable(controls).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        )),
    )
    .padding([4, 8])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(tokens.background.into()),
        border: iced::border::Border {
            color: tokens.border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn tab_bar<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let tabs = app.workspace.tabs();
    if tabs.is_empty() {
        return container("").height(Length::Fixed(0.0)).into();
    }

    let mut bar = row![].spacing(2).align_y(iced::Alignment::Center);
    for tab in tabs {
        let active = app.workspace.active_id() == Some(tab.id);
        let marker = if tab.missing || tab.conflict {
            "!"
        } else if tab.dirty {
            "●"
        } else {
            " "
        };
        let label = format!("{marker} {}", tab.title);
        let color = if active {
            tokens.accent
        } else if tab.missing || tab.conflict {
            tokens.error
        } else {
            tokens.text
        };

        bar = bar.push(
            button(text(label).size(12).color(color))
                .padding([4, 8])
                .on_press(Message::SelectTab(tab.id)),
        );
        bar = bar.push(
            button(text("×").size(12).color(tokens.muted))
                .padding([4, 4])
                .on_press(Message::CloseTab(tab.id)),
        );
    }

    container(scrollable(bar).direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::default(),
    )))
    .padding([2, 6])
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

fn banners<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Option<Element<'a, Message>> {
    let document = app.workspace.active_document()?;

    if app.preview_is_paused() && document.preview_stale {
        return Some(
            container(
                row![
                    text("Live preview is paused for this large document.")
                        .size(12)
                        .color(tokens.warning)
                        .width(Length::Fill),
                    button(text("Refresh preview").size(12)).on_press(Message::RefreshPreview),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| banner_style(tokens.surface_raised, tokens.warning))
            .into(),
        );
    }

    if document.conflict.is_some() {
        return Some(
            container(
                row![
                    text("This file changed on disk while you had unsaved edits.")
                        .size(12)
                        .color(tokens.warning)
                        .width(Length::Fill),
                    button(text("Reload disk").size(12)).on_press(Message::ConflictReload),
                    button(text("Keep mine").size(12)).on_press(Message::ConflictKeepLocal),
                    button(text("Compare").size(12)).on_press(Message::ConflictCompare),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| banner_style(tokens.surface_raised, tokens.warning))
            .into(),
        );
    }

    if document.is_missing() {
        return Some(
            container(
                row![
                    text("This file was deleted on disk.")
                        .size(12)
                        .color(tokens.error),
                    button(text("Save again").size(12)).on_press(Message::MissingSave),
                    button(text("Close tab").size(12)).on_press(Message::MissingClose),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| banner_style(tokens.surface_raised, tokens.error))
            .into(),
        );
    }

    if let Some(error) = document.front_matter().error() {
        return Some(
            container(
                text(format!("Front matter: {error}"))
                    .size(12)
                    .color(tokens.warning),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| banner_style(tokens.surface_raised, tokens.warning))
            .into(),
        );
    }

    if let Some(error) = &document.save_error {
        return Some(
            container(
                text(format!("Save failed: {error}"))
                    .size(12)
                    .color(tokens.error),
            )
            .padding([6, 10])
            .width(Length::Fill)
            .style(move |_| banner_style(tokens.surface_raised, tokens.error))
            .into(),
        );
    }

    None
}

fn banner_style(background: iced::Color, border: iced::Color) -> container::Style {
    container::Style {
        background: Some(background.into()),
        border: iced::border::Border {
            color: border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

fn content_area<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    if app.workspace.active_document().is_none() {
        return empty_workspace(app, tokens);
    }

    match app.layout {
        LayoutMode::EditorOnly => editor_view(app, tokens),
        LayoutMode::PreviewOnly | LayoutMode::Read => preview_view(app, tokens),
        LayoutMode::SideBySide => {
            let tokens_for_style = tokens;
            pane_grid(&app.panes, |_pane, kind, _maximized| {
                let content = match kind {
                    PaneKind::Editor => editor_view(app, tokens),
                    PaneKind::Preview => preview_view(app, tokens),
                };
                pane_grid::Content::new(content)
            })
            .spacing(2)
            .on_resize(6.0, Message::PaneResized)
            .style(move |_| pane_grid::Style {
                hovered_region: pane_grid::Highlight {
                    background: tokens_for_style.selection.into(),
                    border: iced::border::Border {
                        color: tokens_for_style.accent,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                },
                picked_split: pane_grid::Line {
                    color: tokens_for_style.accent,
                    width: 2.0,
                },
                hovered_split: pane_grid::Line {
                    color: tokens_for_style.accent,
                    width: 2.0,
                },
            })
            .into()
        }
    }
}

fn editor_view<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    match &app.editor {
        Some(editor) => container(editor.view().map(Message::Editor))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(tokens.background.into()),
                ..container::Style::default()
            })
            .into(),
        None => container(text("Preparing the editor…").size(13).color(tokens.muted))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
    }
}

fn preview_view<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let Some(document) = app.workspace.active_document() else {
        return preview_empty(tokens, "Open a document to see its preview.");
    };

    let Some(preview) = &document.preview else {
        let message = if app.preview_is_paused() {
            "Preview paused for a large document. Use Refresh preview to render it."
        } else {
            "Rendering preview…"
        };
        return preview_empty(tokens, message);
    };

    let styles = preview_render::PreviewStyles {
        tokens,
        typography: &app.settings.preview_typography,
        base_dir: document.path.parent(),
    };
    let viewport = preview_render::PreviewViewport {
        offset_y: app.preview_offset,
        height: app.preview_viewport_height,
    };

    let element: Element<'a, preview_render::Message> = container(preview_render::view(
        preview,
        styles,
        &app.preview_caches,
        &app.preview_scroll_id,
        viewport,
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(tokens.surface.into()),
        ..container::Style::default()
    })
    .into();

    element.map(Message::Preview)
}

fn preview_empty<'a>(tokens: &'a ThemeTokens, message: &'a str) -> Element<'a, Message> {
    container(
        column![
            text("Preview").size(14).color(tokens.muted),
            text(message).size(12).color(tokens.muted),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::Fill)
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

fn empty_workspace<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let root = app.root().map_or_else(
        || String::from("No folder open"),
        |root| root.display().to_string(),
    );

    container(
        column![
            text(
                if app
                    .workspace
                    .tree()
                    .is_some_and(|tree| tree.files().is_empty())
                {
                    "This folder has no Markdown documents yet."
                } else {
                    "Open a document from the sidebar, or create a new one."
                }
            )
            .size(14)
            .color(tokens.text),
            text(root).size(12).color(tokens.muted),
            row![
                button(text("New document").size(12)).on_press(Message::NewFile),
                button(text("New folder").size(12)).on_press(Message::NewFolder),
                button(text("Import").size(12)).on_press(Message::Import),
                button(text("Choose another folder").size(12)).on_press(Message::OpenFolder),
            ]
            .spacing(6),
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn status_bar<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let info = match app.workspace.active_document() {
        Some(document) => {
            let (line, column) = document.cursor.display();
            let selection = app
                .editor
                .as_ref()
                .and_then(|editor| editor.selection())
                .filter(|(start, end)| start != end)
                .and_then(|(start, end)| document.selection_stats(start, end));
            status::StatusInfo {
                stats: document.stats(),
                selection,
                cursor_line: line,
                cursor_column: column,
                reading_progress: (app.layout == LayoutMode::Read).then_some(app.preview_progress),
                message: app.status.clone(),
            }
        }
        None => status::StatusInfo {
            message: app.status.clone(),
            ..status::StatusInfo::default()
        },
    };

    status::view(info, tokens)
}

fn dialog_view<'a>(dialog: &'a Dialog, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    match dialog {
        Dialog::Input { input, .. } => dialogs::input(
            input,
            tokens,
            Message::DialogValueChanged,
            Message::DialogSubmit,
            Message::DialogCancel,
        ),
        Dialog::Confirm { confirm, .. } => dialogs::confirm(
            confirm,
            tokens,
            Message::DialogSubmit,
            Message::DialogCancel,
        ),
    }
}

fn recovery_overlay<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let mut records = column![].spacing(6);
    for (index, record) in app.recovery.iter().enumerate() {
        records = records.push(
            container(
                row![
                    column![
                        text(record.title.clone()).size(13).color(tokens.text),
                        text(record.path.display().to_string())
                            .size(11)
                            .color(tokens.muted),
                    ]
                    .width(Length::Fill),
                    button(text("Recover").size(12)).on_press(Message::Recover(index)),
                    button(text("Discard").size(12)).on_press(Message::DiscardRecovery(index)),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .padding(6)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(tokens.surface.into()),
                border: iced::border::Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..container::Style::default()
            }),
        );
    }

    let card = container(
        column![
            text("Recover unsaved work?").size(16).color(tokens.heading),
            text("markmydown found buffers from a previous run.")
                .size(12)
                .color(tokens.muted),
            scrollable(records).height(Length::Fixed(220.0)),
            row![
                button(text("Recover all").size(12)).on_press(Message::RecoverAll),
                button(text("Discard all").size(12)).on_press(Message::DiscardAllRecovery),
            ]
            .spacing(8),
        ]
        .spacing(12),
    )
    .padding(18)
    .width(Length::Fixed(520.0))
    .style(move |_| container::Style {
        background: Some(tokens.surface_raised.into()),
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
    });

    container(card)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(20)
        .into()
}

fn compare_view<'a>(compare: &'a Compare, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let local = column![
        text("Local edits").size(13).color(tokens.text),
        scrollable(container(text(compare.local.as_str()).size(12)).padding(8))
            .height(Length::Fixed(300.0)),
    ]
    .spacing(6)
    .width(Length::Fill);

    let disk = column![
        text("Version on disk").size(13).color(tokens.text),
        scrollable(container(text(compare.disk.as_str()).size(12)).padding(8))
            .height(Length::Fixed(300.0)),
    ]
    .spacing(6)
    .width(Length::Fill);

    let card = container(
        column![
            text(format!("Compare {}", compare.path.display()))
                .size(15)
                .color(tokens.heading),
            row![local, disk].spacing(12),
            button(text("Close").size(12)).on_press(Message::CompareClose),
        ]
        .spacing(12),
    )
    .padding(18)
    .width(Length::Fixed(760.0))
    .style(move |_| container::Style {
        background: Some(tokens.surface_raised.into()),
        border: iced::border::Border {
            color: tokens.border,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    });

    container(card)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(20)
        .into()
}

/// Runs the native application.
pub fn run() -> iced::Result {
    run_with_paths(Vec::new())
}

/// Runs the native application with documents or a folder from the OS.
pub fn run_with_paths(initial: Vec<PathBuf>) -> iced::Result {
    iced::application(move || App::boot_with_paths(initial.clone()), update, view)
        .title(App::title)
        .theme(App::iced_theme)
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Key, Modifiers, key};

    fn character(value: &str) -> Key {
        Key::Character(value.into())
    }

    fn fixture_app() -> (tempfile::TempDir, App) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("notes");
        std::fs::write(root.join("alpha.md"), "# Alpha\n\nOne.\n").expect("alpha");
        std::fs::write(root.join("beta.md"), "# Beta\n\nTwo.\n").expect("beta");

        let paths = AppPaths::with_base(tmp.path().join("app"));
        paths.ensure().expect("paths");
        let mut workspace = WorkspaceState::new(paths.session_file(), paths.recovery_dir());
        workspace.open_folder(&root).expect("open folder");
        workspace
            .open_document(root.join("alpha.md"))
            .expect("open document");

        let mut app = App {
            workspace,
            settings: Settings::default(),
            paths,
            status: String::from("Ready"),
            status_is_error: false,
            tokens: ThemeTokens::light(),
            system_dark: false,
            layout: LayoutMode::SideBySide,
            sidebar_visible: true,
            outline_visible: true,
            focus_mode: false,
            search: search_ui::SearchPanel::default(),
            selected: Some(root.join("alpha.md")),
            expanded: HashSet::new(),
            dialog: None,
            compare: None,
            recovery: Vec::new(),
            settings_open: false,
            preview_theme: None,
            preview_theme_backup: None,
            update_state: settings_ui::UpdateState::Idle,
            quick_open: None,
            notifications: notifications::NotificationCenter::default(),
            info_overlay: None,
            info_preview: None,
            info_scroll_id: iced::widget::Id::new("markmydown-info"),
            editor: None,
            editor_doc: None,
            editor_needs_position: true,
            panes: new_pane_state(0.5),
            preview_caches: RefCell::new(preview_render::RenderCaches::default()),
            preview_scroll_id: iced::widget::Id::new("markmydown-preview"),
            preview_offset: 0.0,
            preview_progress: 0.0,
            preview_viewport_height: 800.0,
            preview_last_edit: Instant::now(),
            preview_needs_reset: true,
            ignore_preview_scrolls_until: None,
            ignore_editor_scrolls_until: None,
            last_recovery: Instant::now(),
            last_session: Instant::now(),
        };
        app.install_editor();
        (tmp, app)
    }

    #[test]
    fn themes_can_be_previewed_applied_and_persisted() {
        let (_tmp, mut app) = fixture_app();
        app.select_theme(String::from("light"));
        let original = app.tokens.clone();

        app.preview_theme(Some(String::from("nord")));
        assert_ne!(app.tokens, original);
        assert_eq!(app.settings.theme, "light", "previewing does not persist");
        assert!(app.tokens.is_dark);

        app.cancel_theme_preview();
        assert_eq!(app.tokens, original);
        assert!(app.preview_theme.is_none());

        app.preview_theme(Some(String::from("nord")));
        app.select_theme(String::from("nord"));
        assert_eq!(app.settings.theme, "nord");
        assert!(app.tokens.is_dark);
        let loaded = Settings::load(&app.paths.settings_file());
        assert_eq!(loaded.settings.theme, "nord");
    }

    #[test]
    fn system_theme_changes_follow_the_os_only_when_selected() {
        let (_tmp, mut app) = fixture_app();
        app.select_theme(String::from("system"));
        app.system_theme_changed(true);
        assert!(app.tokens.is_dark);

        app.select_theme(String::from("light"));
        app.system_theme_changed(true);
        assert!(!app.tokens.is_dark);
    }

    #[test]
    fn editor_and_preview_typography_are_independent() {
        let (_tmp, mut app) = fixture_app();
        let editor_default = app.settings.editor_typography.clone();
        let preview_default = app.settings.preview_typography.clone();

        let _ = app.handle_settings(settings_ui::Message::EditorFontSize(20.0));
        assert_eq!(app.settings.editor_typography.font_size, 20.0);
        assert_eq!(app.settings.preview_typography, preview_default);

        let _ = app.handle_settings(settings_ui::Message::PreviewFontSize(23.0));
        assert_eq!(app.settings.preview_typography.font_size, 23.0);
        assert_eq!(
            app.settings.editor_typography.font_size, 20.0,
            "the editor font did not change with the preview"
        );

        let _ = app.handle_settings(settings_ui::Message::ResetEditorTypography);
        assert_eq!(app.settings.editor_typography, editor_default);
        let _ = app.handle_settings(settings_ui::Message::ResetPreviewTypography);
        assert_eq!(app.settings.preview_typography, preview_default);
    }

    #[test]
    fn resetting_settings_restores_and_persists_defaults() {
        let (_tmp, mut app) = fixture_app();
        app.select_theme(String::from("nord"));
        app.settings.editor_typography.font_size = 30.0;
        app.settings.layout = String::from("read");
        app.settings.sidebar_visible = false;

        app.reset_settings();

        assert_eq!(app.settings, Settings::default());
        assert!(app.layout == LayoutMode::SideBySide);
        assert!(app.sidebar_visible);
        let loaded = Settings::load(&app.paths.settings_file());
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn welcome_and_cheat_sheet_open_and_close() {
        let (_tmp, mut app) = fixture_app();

        let _ = app.open_info(InfoOverlay::Welcome);
        assert_eq!(app.info_overlay, Some(InfoOverlay::Welcome));
        let welcome = app.info_preview.as_ref().expect("welcome preview");
        assert!(!welcome.blocks.is_empty());
        assert!(welcome::WELCOME.contains("E = mc^2"));
        assert!(welcome::WELCOME.contains("```mermaid"));
        assert!(welcome::WELCOME.contains("println!"));
        assert!(welcome::WELCOME.contains("https://commonmark.org"));

        let _ = view(&app);
        app.close_info();
        assert!(app.info_overlay.is_none());
        assert!(app.info_preview.is_none());

        let _ = app.open_info(InfoOverlay::CheatSheet);
        assert_eq!(app.info_overlay, Some(InfoOverlay::CheatSheet));
        assert!(welcome::CHEAT_SHEET.contains("Quick open"));
        assert!(welcome::CHEAT_SHEET.contains("Cmd/Ctrl+B"));
        let _ = view(&app);
        app.close_info();
        assert!(app.info_overlay.is_none());
    }

    #[test]
    fn quick_open_filters_and_opens_documents() {
        let (_tmp, mut app) = fixture_app();
        let _ = app.open_quick_open();
        assert!(app.quick_open.is_some());
        let _ = view(&app);

        let quick = app.quick_open.as_mut().expect("quick open");
        quick.set_query("beta");
        assert_eq!(quick.filtered().count(), 1);
        let path = quick.selected_entry().expect("selected").path.clone();
        let _ = app.handle_quick_open(palette::Message::Open(path.clone()));

        assert!(app.quick_open.is_none());
        assert_eq!(
            app.workspace
                .active_document()
                .expect("active document")
                .path,
            path
        );
    }

    #[test]
    fn notifications_support_success_and_failure_states() {
        let (_tmp, mut app) = fixture_app();
        app.notify_success("saved");
        app.notify_error("failed");

        assert_eq!(app.notifications.len(), 2);
        let id = app.notifications.latest().expect("latest").id;
        app.notifications.dismiss(id);
        assert_eq!(app.notifications.len(), 1);
        let _ = view(&app);
    }

    #[test]
    fn update_state_handles_available_up_to_date_and_offline() {
        let (_tmp, mut app) = fixture_app();

        let _ = app.update_check_finished(Ok(Some(UpdateInfo {
            version: String::from("2.0.0"),
            name: String::from("v2.0.0"),
            notes: None,
        })));
        assert!(matches!(
            app.update_state,
            settings_ui::UpdateState::Available(_)
        ));

        let _ = app.update_check_finished(Ok(None));
        assert_eq!(app.update_state, settings_ui::UpdateState::UpToDate);

        app.update_state = settings_ui::UpdateState::Checking;
        let _ = app.update_check_finished(Err(String::from("network unavailable")));
        assert!(matches!(
            app.update_state,
            settings_ui::UpdateState::Error(_)
        ));
        assert!(
            app.workspace.active_document().is_some(),
            "an offline check does not block the app"
        );
    }

    #[test]
    fn launch_paths_open_multiple_documents_as_tabs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("notes");
        let one = root.join("one.md");
        let two = root.join("two.markdown");
        std::fs::write(&one, "# One\n").expect("one");
        std::fs::write(&two, "# Two\n").expect("two");
        let args: Vec<std::ffi::OsString> = vec![
            std::ffi::OsString::from("markmydown"),
            one.clone().into_os_string(),
            two.clone().into_os_string(),
        ];

        let paths = crate::config::launch::document_paths(args);
        let root = crate::config::launch::launch_root(&paths).expect("root");
        let mut workspace =
            WorkspaceState::new(tmp.path().join("session.json"), tmp.path().join("recovery"));
        workspace.open_folder(root).expect("open folder");
        for path in &paths {
            workspace.open_document(path).expect("open document");
        }

        assert_eq!(workspace.tabs().len(), 2);
    }

    #[test]
    fn boot_with_launch_paths_opens_the_workspace_and_tabs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("notes");
        let one = root.join("one.md");
        let two = root.join("two.markdown");
        std::fs::write(&one, "# One\n").expect("one");
        std::fs::write(&two, "# Two\n").expect("two");

        let paths = AppPaths::with_base(tmp.path().join("app"));
        let app = App::boot_at_paths(paths, vec![one.clone(), two.clone()]);

        assert!(app.workspace.is_open());
        assert_eq!(app.workspace.tabs().len(), 2);
        assert_eq!(
            app.workspace
                .active_document()
                .expect("active document")
                .path,
            one.canonicalize().expect("canonical")
        );
    }

    #[test]
    fn settings_and_welcome_layers_build_a_view() {
        let (_tmp, mut app) = fixture_app();
        app.settings_open = true;
        app.preview_theme = Some(String::from("sepia"));
        let _ = view(&app);

        app.settings_open = false;
        app.preview_theme = None;
        let _ = app.open_info(InfoOverlay::Welcome);
        let _ = view(&app);
    }

    #[test]
    fn fixed_formatting_shortcuts_map_to_actions() {
        let command = Modifiers::COMMAND;
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;

        assert!(matches!(
            shortcut_for(&character("b"), command, false),
            Some(Message::Format(FormatAction::Bold))
        ));
        assert!(matches!(
            shortcut_for(&character("i"), command, false),
            Some(Message::Format(FormatAction::Italic))
        ));
        assert!(matches!(
            shortcut_for(&character("i"), shift, false),
            Some(Message::Format(FormatAction::Image))
        ));
        assert!(matches!(
            shortcut_for(&character("x"), shift, false),
            Some(Message::Format(FormatAction::Strikethrough))
        ));
        assert!(matches!(
            shortcut_for(&character("8"), shift, false),
            Some(Message::Format(FormatAction::BulletList))
        ));
        assert!(matches!(
            shortcut_for(&character("7"), shift, false),
            Some(Message::Format(FormatAction::OrderedList))
        ));
        assert!(matches!(
            shortcut_for(&character("9"), shift, false),
            Some(Message::Format(FormatAction::TaskList))
        ));
        assert!(matches!(
            shortcut_for(&character("."), shift, false),
            Some(Message::Format(FormatAction::Blockquote))
        ));
        assert!(matches!(
            shortcut_for(&character("`"), command, false),
            Some(Message::Format(FormatAction::InlineCode))
        ));
        assert!(matches!(
            shortcut_for(&character("`"), shift, false),
            Some(Message::Format(FormatAction::CodeBlock))
        ));
        assert!(matches!(
            shortcut_for(&character("t"), command, false),
            Some(Message::Format(FormatAction::Table))
        ));
        assert!(matches!(
            shortcut_for(&character("k"), command, false),
            Some(Message::Format(FormatAction::Link))
        ));
        for level in 1..=6 {
            assert!(matches!(
                shortcut_for(&character(&level.to_string()), command, false),
                Some(Message::Format(FormatAction::Heading(found))) if found == level
            ));
        }
    }

    #[test]
    fn file_and_search_shortcuts_are_fixed() {
        assert!(matches!(
            shortcut_for(&character("s"), Modifiers::COMMAND, false),
            Some(Message::Save)
        ));
        assert!(matches!(
            shortcut_for(&character("f"), Modifiers::COMMAND, false),
            Some(Message::OpenFind)
        ));
        assert!(matches!(
            shortcut_for(&character("h"), Modifiers::COMMAND, false),
            Some(Message::OpenReplace)
        ));
        assert!(matches!(
            shortcut_for(&character("p"), Modifiers::COMMAND, false),
            Some(Message::OpenQuickOpen)
        ));
        assert!(matches!(
            shortcut_for(&character(","), Modifiers::COMMAND, false),
            Some(Message::ToggleSettings)
        ));
        assert!(matches!(
            shortcut_for(&character("/"), Modifiers::COMMAND, false),
            Some(Message::OpenCheatSheet)
        ));
        assert!(matches!(
            shortcut_for(&Key::Named(key::Named::Escape), Modifiers::NONE, false),
            Some(Message::Escape)
        ));
    }

    #[test]
    fn quick_open_arrow_keys_move_and_enter_accepts() {
        let down = quick_open_key_message(iced::keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::ArrowDown),
            modified_key: Key::Named(key::Named::ArrowDown),
            physical_key: key::Physical::Code(key::Code::ArrowDown),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        })
        .expect("down");
        assert!(matches!(
            down,
            Message::QuickOpen(palette::Message::Move(1))
        ));

        let enter = quick_open_key_message(iced::keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Enter),
            modified_key: Key::Named(key::Named::Enter),
            physical_key: key::Physical::Code(key::Code::Enter),
            location: iced::keyboard::Location::Standard,
            modifiers: Modifiers::NONE,
            text: None,
            repeat: false,
        })
        .expect("enter");
        assert!(matches!(
            enter,
            Message::QuickOpen(palette::Message::Accept)
        ));
    }

    #[test]
    fn only_external_links_are_opened() {
        assert_eq!(
            external_url("https://example.com/docs"),
            Some("https://example.com/docs")
        );
        assert_eq!(
            external_url("mailto:writer@example.com"),
            Some("mailto:writer@example.com")
        );
        assert_eq!(external_url("javascript:alert(1)"), None);
        assert_eq!(external_url("file:///etc/passwd"), None);
        assert_eq!(external_url("#heading"), None);
    }

    #[test]
    fn shortcuts_require_the_command_modifier_and_ignore_repeats() {
        assert!(shortcut_for(&character("b"), Modifiers::NONE, false).is_none());
        assert!(shortcut_for(&character("b"), Modifiers::COMMAND, true).is_none());
        assert!(
            shortcut_for(&character("b"), Modifiers::COMMAND | Modifiers::ALT, false).is_none()
        );
    }

    #[test]
    fn editor_layouts_and_panels_build_a_view() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("notes");
        let path = root.join("note.md");
        std::fs::write(&path, "# Title\n\n## Section\n\nBody\n").expect("fixture");

        let paths = AppPaths::with_base(tmp.path().join("app"));
        let mut workspace = WorkspaceState::new(paths.session_file(), paths.recovery_dir());
        workspace.open_folder(&root).expect("open folder");
        workspace.open_document(&path).expect("open document");

        let mut app = App {
            workspace,
            settings: Settings::default(),
            paths,
            status: String::from("Ready"),
            status_is_error: false,
            tokens: ThemeTokens::light(),
            system_dark: false,
            layout: LayoutMode::SideBySide,
            sidebar_visible: true,
            outline_visible: true,
            focus_mode: false,
            search: search_ui::SearchPanel::default(),
            selected: Some(path.clone()),
            expanded: HashSet::new(),
            dialog: None,
            compare: None,
            recovery: Vec::new(),
            settings_open: false,
            preview_theme: None,
            preview_theme_backup: None,
            update_state: settings_ui::UpdateState::Idle,
            quick_open: None,
            notifications: notifications::NotificationCenter::default(),
            info_overlay: None,
            info_preview: None,
            info_scroll_id: iced::widget::Id::new("markmydown-info"),
            editor: None,
            editor_doc: None,
            editor_needs_position: true,
            panes: new_pane_state(0.5),
            preview_caches: RefCell::new(preview_render::RenderCaches::default()),
            preview_scroll_id: iced::widget::Id::new("markmydown-preview"),
            preview_offset: 0.0,
            preview_progress: 0.0,
            preview_viewport_height: 800.0,
            preview_last_edit: Instant::now(),
            preview_needs_reset: true,
            ignore_preview_scrolls_until: None,
            ignore_editor_scrolls_until: None,
            last_recovery: Instant::now(),
            last_session: Instant::now(),
        };
        app.install_editor();
        assert!(
            app.workspace
                .active_document()
                .expect("active document")
                .preview
                .is_some(),
            "installing the editor builds the preview model"
        );

        for layout in LayoutMode::ALL {
            app.layout = layout;
            let _ = view(&app);
        }

        app.layout = LayoutMode::SideBySide;
        if let Some(editor) = &mut app.editor {
            let _ = editor.select_range((0, 0), (0, 5));
        }
        app.search.open(true);
        app.search.query = "Body".into();
        app.search.refresh("# Title\n\n## Section\n\nBody\n");
        let _ = view(&app);

        app.dialog = Some(App::new_input_dialog(
            InputAction::InsertLink { image: false },
            "Insert link",
            "URL",
            "",
            "https://example.com",
        ));
        let _ = view(&app);

        app.dialog = None;
        let request = app
            .build_export_request(ExportFormat::Markdown)
            .expect("export request");
        assert_eq!(request.source, "# Title\n\n## Section\n\nBody\n");
        assert_eq!(request.title, "note");
        assert_eq!(request.suggested_name(), "note.md");
        assert_eq!(request.preview.blocks.len(), 3);

        let _ = app.finish_export(Err(ExportFailure {
            format: ExportFormat::Html,
            message: String::from("disk is full"),
        }));
        assert!(matches!(
            app.dialog,
            Some(Dialog::Confirm {
                action: ConfirmAction::RetryExport {
                    format: ExportFormat::Html
                },
                ..
            })
        ));
        assert!(
            app.workspace.active_document().is_some(),
            "a failed export keeps the document open"
        );

        app.dialog = None;
        let _ = app.finish_export(Ok(None));
        assert!(app.dialog.is_none());
        assert_eq!(app.status, "Export cancelled.");
    }

    #[test]
    fn large_documents_pause_live_preview_until_refreshed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("notes");
        std::fs::create_dir_all(&root).expect("notes");
        let path = root.join("large.md");
        let source = format!("# Large\n\n{}\n", "word ".repeat(64));
        std::fs::write(&path, &source).expect("fixture");

        let paths = AppPaths::with_base(tmp.path().join("app"));
        let mut workspace = WorkspaceState::new(paths.session_file(), paths.recovery_dir());
        workspace.open_folder(&root).expect("open folder");
        workspace.open_document(&path).expect("open document");

        let settings = Settings {
            large_file_threshold: 32,
            ..Settings::default()
        };
        let mut app = App {
            workspace,
            settings,
            paths,
            status: String::from("Ready"),
            status_is_error: false,
            tokens: ThemeTokens::light(),
            system_dark: false,
            layout: LayoutMode::SideBySide,
            sidebar_visible: true,
            outline_visible: true,
            focus_mode: false,
            search: search_ui::SearchPanel::default(),
            selected: Some(path.clone()),
            expanded: HashSet::new(),
            dialog: None,
            compare: None,
            recovery: Vec::new(),
            settings_open: false,
            preview_theme: None,
            preview_theme_backup: None,
            update_state: settings_ui::UpdateState::Idle,
            quick_open: None,
            notifications: notifications::NotificationCenter::default(),
            info_overlay: None,
            info_preview: None,
            info_scroll_id: iced::widget::Id::new("markmydown-info"),
            editor: None,
            editor_doc: None,
            editor_needs_position: true,
            panes: new_pane_state(0.5),
            preview_caches: RefCell::new(preview_render::RenderCaches::default()),
            preview_scroll_id: iced::widget::Id::new("markmydown-preview"),
            preview_offset: 0.0,
            preview_progress: 0.0,
            preview_viewport_height: 800.0,
            preview_last_edit: Instant::now(),
            preview_needs_reset: true,
            ignore_preview_scrolls_until: None,
            ignore_editor_scrolls_until: None,
            last_recovery: Instant::now(),
            last_session: Instant::now(),
        };

        app.install_editor();
        assert!(app.preview_is_paused());
        assert!(
            app.workspace
                .active_document()
                .expect("active document")
                .preview
                .is_none(),
            "live preview is skipped for a large document"
        );
        assert!(!app.refresh_preview_if_due(Instant::now()));

        assert!(app.refresh_preview(true));
        assert!(
            app.workspace
                .active_document()
                .expect("active document")
                .preview
                .is_some(),
            "manual refresh builds the preview"
        );
    }
}
