//! Locally persisted settings with a versioned schema.
//!
//! Unreadable settings never block startup: the bad file is preserved for
//! diagnosis and defaults are used.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::registry;
use crate::theme::tokens::Typography;
use crate::workspace::atomic;

/// Current settings schema version.
pub const CURRENT_SCHEMA: u32 = 1;

/// User preferences. Every new field needs a serde default so older files
/// continue to load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// `system`, `light`, `dark`, or a gallery theme id.
    pub theme: String,
    /// `editor-only`, `side-by-side`, `preview-only`, or `read`.
    pub layout: String,
    pub sidebar_visible: bool,
    pub outline_visible: bool,
    /// Fraction of the content width used by the editor in side-by-side mode.
    pub split_ratio: f32,
    pub focus_mode: bool,
    pub editor_typography: Typography,
    pub preview_typography: Typography,
    pub autosave_ms: u64,
    pub preview_debounce_ms: u64,
    pub large_file_threshold: usize,
    pub page_size: String,
    pub page_margin: f32,
    pub recent_workspaces: Vec<PathBuf>,
    pub last_workspace: Option<PathBuf>,
    pub onboarding_seen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA,
            theme: String::from("system"),
            layout: String::from("side-by-side"),
            sidebar_visible: true,
            outline_visible: true,
            split_ratio: 0.5,
            focus_mode: false,
            editor_typography: Typography::editor_default(),
            preview_typography: Typography::preview_default(),
            autosave_ms: 1_000,
            preview_debounce_ms: 80,
            large_file_threshold: 1_000_000,
            page_size: String::from("letter"),
            page_margin: 72.0,
            recent_workspaces: Vec::new(),
            last_workspace: None,
            onboarding_seen: false,
        }
    }
}

/// Result of loading settings.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    pub settings: Settings,
    /// Human-readable warning when defaults had to be used.
    pub warning: Option<String>,
    /// Preserved copy of an unreadable settings file.
    pub backup: Option<PathBuf>,
}

impl Settings {
    /// Loads settings, preserving an unreadable file and falling back to
    /// defaults.
    pub fn load(path: &Path) -> LoadOutcome {
        let Ok(bytes) = std::fs::read(path) else {
            return LoadOutcome {
                settings: Self::default(),
                warning: None,
                backup: None,
            };
        };

        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) if settings.schema_version <= CURRENT_SCHEMA => LoadOutcome {
                settings,
                warning: None,
                backup: None,
            },
            Ok(settings) => {
                let backup = preserve(path);
                LoadOutcome {
                    settings: Self::default(),
                    warning: Some(format!(
                        "settings schema {} is newer than supported schema {CURRENT_SCHEMA}; defaults were used",
                        settings.schema_version
                    )),
                    backup,
                }
            }
            Err(error) => {
                let backup = preserve(path);
                LoadOutcome {
                    settings: Self::default(),
                    warning: Some(format!(
                        "settings could not be read ({error}); defaults were used"
                    )),
                    backup,
                }
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SettingsError::Io)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(SettingsError::Serialize)?;
        atomic::atomic_write(path, json.as_bytes()).map_err(SettingsError::Write)?;
        Ok(())
    }

    /// Restores every setting to its default value.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn resolve_theme(&self, system_dark: bool) -> &'static registry::ThemeDefinition {
        registry::resolve(&self.theme, system_dark)
    }

    pub fn add_recent_workspace(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.recent_workspaces.retain(|existing| existing != &path);
        self.recent_workspaces.insert(0, path.clone());
        self.recent_workspaces.truncate(10);
        self.last_workspace = Some(path);
    }

    pub fn validate(&mut self) {
        if self.theme != "system"
            && self.theme != "light"
            && self.theme != "dark"
            && registry::get(&self.theme).is_none()
        {
            self.theme = String::from("system");
        }
        if !matches!(
            self.layout.as_str(),
            "editor-only" | "side-by-side" | "preview-only" | "read"
        ) {
            self.layout = String::from("side-by-side");
        }
        self.autosave_ms = self.autosave_ms.clamp(250, 30_000);
        self.preview_debounce_ms = self.preview_debounce_ms.clamp(30, 2_000);
        self.page_margin = self.page_margin.clamp(18.0, 144.0);
        if !self.split_ratio.is_finite() {
            self.split_ratio = 0.5;
        }
        self.split_ratio = self.split_ratio.clamp(0.2, 0.8);
    }
}

fn preserve(path: &Path) -> Option<PathBuf> {
    let backup = path.with_extension(format!("corrupt-{}", std::process::id()));
    std::fs::rename(path, &backup).ok().map(|()| backup)
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("settings serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("settings write failed: {0}")]
    Write(#[source] atomic::AtomicError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let mut settings = Settings::default();
        settings.validate();

        assert_eq!(settings.theme, "system");
        assert_eq!(settings.layout, "side-by-side");
        assert!(settings.autosave_ms >= 250);
        assert_eq!(settings.resolve_theme(false).id, "light");
        assert_eq!(settings.resolve_theme(true).id, "dark");
    }

    #[test]
    fn round_trips_through_disk() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        let mut settings = Settings {
            theme: String::from("nord"),
            preview_typography: Typography {
                font_size: 21.0,
                ..Typography::preview_default()
            },
            ..Settings::default()
        };
        settings.add_recent_workspace("/tmp/notes");

        settings.save(&path).expect("save");
        let loaded = Settings::load(&path);

        assert!(loaded.warning.is_none());
        assert_eq!(loaded.settings, settings);
    }

    #[test]
    fn unreadable_settings_preserve_the_file_and_use_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, b"{ not json").expect("fixture");

        let outcome = Settings::load(&path);

        assert_eq!(outcome.settings, Settings::default());
        assert!(outcome.warning.is_some());
        assert!(
            outcome
                .backup
                .as_ref()
                .is_some_and(|backup| backup.exists())
        );
        assert!(!path.exists());
    }

    #[test]
    fn newer_schema_uses_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, br#"{"schema_version": 99, "theme": "nord"}"#).expect("fixture");

        let outcome = Settings::load(&path);

        assert_eq!(outcome.settings, Settings::default());
        assert!(outcome.warning.is_some());
    }

    #[test]
    fn invalid_values_are_repaired() {
        let mut settings = Settings {
            theme: String::from("missing-theme"),
            layout: String::from("diagonal"),
            autosave_ms: 10,
            page_margin: 999.0,
            split_ratio: 9.0,
            ..Settings::default()
        };

        settings.validate();

        assert_eq!(settings.theme, "system");
        assert_eq!(settings.layout, "side-by-side");
        assert_eq!(settings.autosave_ms, 250);
        assert_eq!(settings.page_margin, 144.0);
        assert!((settings.split_ratio - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn recent_workspaces_are_unique_and_bounded() {
        let mut settings = Settings::default();
        for index in 0..15 {
            settings.add_recent_workspace(format!("/tmp/{index}"));
        }
        settings.add_recent_workspace("/tmp/3");

        assert_eq!(settings.recent_workspaces.len(), 10);
        assert_eq!(settings.recent_workspaces[0], PathBuf::from("/tmp/3"));
    }
}
