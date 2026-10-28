//! Application shell: state, messages, update, view, and subscriptions.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::widget::{Space, button, column, container, row, scrollable, stack, text};
use iced::{Element, Length, Subscription, Task};
use iced_code_editor::Message as EditorMessage;

use crate::config::paths::AppPaths;
use crate::config::settings::Settings;
use crate::editor::{EditorPane, message_may_edit};
use crate::theme::registry;
use crate::theme::tokens::ThemeTokens;
use crate::ui::{dialogs, sidebar, status};
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
}

/// The operation an input dialog will perform when submitted.
#[derive(Debug, Clone)]
pub enum InputAction {
    NewFile { parent: PathBuf },
    NewFolder { parent: PathBuf },
    Rename { path: PathBuf },
    Move { path: PathBuf },
}

/// The operation a confirmation dialog will perform when confirmed.
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    Delete { path: PathBuf },
    CloseTab { id: u64 },
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
    pub layout: LayoutMode,
    pub sidebar_visible: bool,
    pub focus_mode: bool,
    pub selected: Option<PathBuf>,
    pub expanded: HashSet<PathBuf>,
    pub dialog: Option<Dialog>,
    pub compare: Option<Compare>,
    pub recovery: Vec<RecoveryRecord>,
    editor: Option<EditorPane>,
    editor_doc: Option<u64>,
    editor_needs_position: bool,
    last_recovery: Instant,
    last_session: Instant,
}

impl App {
    /// Loads settings, restores the previous session, and prepares the first
    /// recovery prompt.
    pub fn boot() -> Self {
        let paths = AppPaths::discover().unwrap_or_else(|_| {
            AppPaths::with_base(std::env::temp_dir().join("markmydown-fallback"))
        });
        let _ = paths.ensure();

        let loaded = Settings::load(&paths.settings_file());
        let mut settings = loaded.settings;
        settings.validate();

        let mut workspace = WorkspaceState::new(paths.session_file(), paths.recovery_dir());
        let mut status = String::from("Ready");
        let mut status_is_error = false;

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

        let recovery = workspace.pending_recovery();
        let layout = layout_from_settings(&settings.layout);
        let tokens = registry::resolve(&settings.theme, false).tokens.clone();
        let selected = workspace
            .active_document()
            .map(|document| document.path.clone());

        let mut app = Self {
            workspace,
            settings,
            paths,
            status,
            status_is_error,
            tokens,
            layout,
            sidebar_visible: true,
            focus_mode: false,
            selected,
            expanded: HashSet::new(),
            dialog: None,
            compare: None,
            recovery,
            editor: None,
            editor_doc: None,
            editor_needs_position: true,
            last_recovery: Instant::now(),
            last_session: Instant::now(),
        };

        app.install_editor();
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
        iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick)
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

    /// Creates the editor widget for the active tab without applying a cursor
    /// task. Used during boot, when no task can be returned.
    fn install_editor(&mut self) {
        let Some(document) = self.workspace.active_document() else {
            self.editor = None;
            self.editor_doc = None;
            return;
        };

        let id = document.id;
        let text = document.text.clone();
        let mut editor = EditorPane::new(&text, &self.tokens, &self.settings.editor_typography);
        editor.set_focus_mode(self.focus_mode);
        self.editor = Some(editor);
        self.editor_doc = Some(id);
        self.editor_needs_position = true;
    }

    /// Rebuilds the editor when the active tab changed.
    fn sync_editor(&mut self) -> Task<Message> {
        let Some(document) = self.workspace.active_document() else {
            self.editor = None;
            self.editor_doc = None;
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
        self.editor = Some(editor);
        self.editor_doc = Some(id);
        self.restore_editor_position_for(id, cursor, scroll)
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
        Task::batch([reset, goto, scroll])
    }

    fn handle_editor(&mut self, message: EditorMessage) -> Task<Message> {
        if matches!(message, EditorMessage::WriteRequested) {
            return self.save_active();
        }

        let scroll = match &message {
            EditorMessage::Scrolled(viewport) => Some(viewport.absolute_offset().y),
            _ => None,
        };
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
            self.workspace
                .set_document_content(id, content, cursor, scroll);
        } else {
            self.workspace.set_document_position(id, cursor, scroll);
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
        }
    }

    fn tick(&mut self) -> Task<Message> {
        let now = Instant::now();
        let mut tasks = Vec::new();

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

fn persisted_settings(settings: &Settings, paths: &AppPaths) {
    if let Err(error) = settings.save(&paths.settings_file()) {
        log::warn!("could not save settings: {error}");
    }
}

fn layout_from_settings(layout: &str) -> LayoutMode {
    match layout {
        "editor-only" => LayoutMode::EditorOnly,
        "preview-only" => LayoutMode::PreviewOnly,
        "read" => LayoutMode::Read,
        _ => LayoutMode::SideBySide,
    }
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
    Sidebar(sidebar::Message),
    Editor(EditorMessage),
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
    ToggleFocusMode,
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
            app.settings.layout = match layout {
                LayoutMode::EditorOnly => "editor-only",
                LayoutMode::SideBySide => "side-by-side",
                LayoutMode::PreviewOnly => "preview-only",
                LayoutMode::Read => "read",
            }
            .to_string();
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
    let base = base_view(app, tokens);

    if !app.recovery.is_empty() {
        return stack![base, recovery_overlay(app, tokens)].into();
    }

    if let Some(dialog) = &app.dialog {
        return stack![base, dialog_view(dialog, tokens)].into();
    }

    if let Some(compare) = &app.compare {
        return stack![base, compare_view(compare, tokens)].into();
    }

    base
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
            button(text("Open a folder").size(14))
                .padding([8, 16])
                .on_press(Message::OpenFolder),
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
    let mut content = column![toolbar(app, tokens), tab_bar(app, tokens)]
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(banner) = banners(app, tokens) {
        content = content.push(banner);
    }

    let selected = app.selected.as_ref();
    let main: Element<'a, Message> = if app.sidebar_visible {
        row![
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
        .height(Length::Fill)
        .into()
    } else {
        content_area(app, tokens)
    };

    content = content.push(main).push(status_bar(app, tokens));
    content.into()
}

fn toolbar<'a>(app: &'a App, tokens: &'a ThemeTokens) -> Element<'a, Message> {
    let title = app.root().and_then(|root| root.file_name()).map_or_else(
        || String::from("No folder"),
        |name| name.to_string_lossy().into_owned(),
    );

    container(
        row![
            button(text("Open folder").size(12)).on_press(Message::OpenFolder),
            button(text("Save").size(12)).on_press(Message::Save),
            button(text("Save all").size(12)).on_press(Message::SaveAll),
            button(text("New").size(12)).on_press(Message::NewFile),
            button(text("Folder").size(12)).on_press(Message::NewFolder),
            button(text("Import").size(12)).on_press(Message::Import),
            button(text("Rename").size(12)).on_press(Message::Rename),
            button(text("Move").size(12)).on_press(Message::Move),
            button(text("Delete").size(12)).on_press(Message::Delete),
            Space::new().width(Length::Fill),
            text(title).size(12).color(tokens.muted),
            button(text("Sidebar").size(12)).on_press(Message::ToggleSidebar),
            button(text("Focus").size(12)).on_press(Message::ToggleFocusMode),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
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

    let editor = editor_view(app, tokens);
    if app.layout.shows_editor() && app.layout.shows_preview() {
        row![
            container(editor)
                .width(Length::FillPortion(3))
                .height(Length::Fill),
            preview_placeholder(tokens),
        ]
        .height(Length::Fill)
        .into()
    } else if app.layout.shows_preview() && !app.layout.shows_editor() {
        preview_placeholder(tokens)
    } else {
        editor
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

fn preview_placeholder<'a>(tokens: &'a ThemeTokens) -> Element<'a, Message> {
    container(
        column![
            text("Preview").size(14).color(tokens.muted),
            text("The rendered preview arrives with the preview milestone.")
                .size(12)
                .color(tokens.muted),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::FillPortion(2))
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
            status::StatusInfo {
                stats: document.stats(),
                selection: None,
                cursor_line: line,
                cursor_column: column,
                reading_progress: None,
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
    iced::application(App::boot, update, view)
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
