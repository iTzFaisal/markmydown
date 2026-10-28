//! Platform-specific config, data, log, and recovery locations.
//!
//! All mutable state lives under the platform's standard directories:
//!
//! | Data | macOS | Windows | Linux |
//! |---|---|---|---|
//! | Config | `~/Library/Application Support/dev.markmydown.markmydown` | `%APPDATA%\\markmydown` | `~/.config/markmydown` |
//! | Data | same as config | `%LOCALAPPDATA%\\markmydown` | `~/.local/share/markmydown` |
//!
//! The exact paths are always available through [`AppPaths`] and are logged at
//! startup so a support report can find them.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Resolved application directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl AppPaths {
    /// Discovers the standard directories for the current user.
    ///
    /// Returns an error when the operating system does not expose a home or
    /// app-data directory (for example, a stripped-down container without
    /// `$HOME`).
    pub fn discover() -> Result<Self, PathError> {
        let project = ProjectDirs::from("dev", "markmydown", "markmydown")
            .ok_or(PathError::NoHomeDirectory)?;

        Ok(Self {
            config_dir: project.config_dir().to_path_buf(),
            data_dir: project.data_dir().to_path_buf(),
        })
    }

    /// Builds paths rooted at a caller-provided directory. Used by tests and by
    /// portable/CI runs so no real user data is touched.
    pub fn with_base(base: impl Into<PathBuf>) -> Self {
        let base = base.into();
        Self {
            config_dir: base.join("config"),
            data_dir: base.join("data"),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn log_file(&self) -> PathBuf {
        self.log_dir().join("markmydown.log")
    }

    pub fn recovery_dir(&self) -> PathBuf {
        self.data_dir.join("recovery")
    }

    pub fn session_file(&self) -> PathBuf {
        self.data_dir.join("session.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// Creates every directory the app writes to.
    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [
            self.config_dir.clone(),
            self.data_dir.clone(),
            self.log_dir(),
            self.recovery_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("the operating system did not provide a home or app-data directory")]
    NoHomeDirectory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_paths_are_nested_and_ensured() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::with_base(tmp.path());

        paths.ensure().expect("create directories");

        assert!(paths.config_dir().is_dir());
        assert!(paths.data_dir().is_dir());
        assert!(paths.log_dir().is_dir());
        assert!(paths.recovery_dir().is_dir());
        assert_eq!(
            paths.settings_file(),
            paths.config_dir().join("settings.json")
        );
        assert_eq!(paths.session_file(), paths.data_dir().join("session.json"));
        assert_eq!(paths.log_file(), paths.log_dir().join("markmydown.log"));
    }
}
