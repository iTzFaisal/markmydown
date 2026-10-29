//! Update checks and installation through GitHub Releases.
//!
//! The update feed is the project's public GitHub Releases page. A check only
//! requests release metadata; the artifact is downloaded only when the user
//! explicitly accepts an available update.

use self_update::backends::github;
use self_update::cargo_crate_version;

/// Repository that publishes signed release artifacts.
pub const REPO_OWNER: &str = "iTzFaisal";
pub const REPO_NAME: &str = "markmydown";
pub const BIN_NAME: &str = "markmydown";

/// Metadata for an available update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub name: String,
    pub notes: Option<String>,
}

/// Builds the GitHub updater. No network request is made until `check` or
/// `install` runs.
fn updater() -> Result<github::Update, UpdateError> {
    github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(cargo_crate_version!())
        .show_output(false)
        .no_confirm(true)
        .show_download_progress(false)
        .build()
        .map_err(|error| UpdateError::Check(error.to_string()))
}

/// Checks whether a newer release exists.
///
/// This never downloads an artifact and never blocks application startup.
pub fn check() -> Result<Option<UpdateInfo>, UpdateError> {
    let available = updater()?
        .is_update_available()
        .map_err(|error| UpdateError::Check(error.to_string()))?;

    Ok(available.map(|release| UpdateInfo {
        version: release.version().to_string(),
        name: release.name().to_string(),
        notes: release.body().map(str::to_string),
    }))
}

/// Downloads, verifies, installs the newest release, and restarts the app.
///
/// This is only called after the user accepts an available update. If the
/// process is replaced by the new binary, this function does not return.
pub fn install_and_restart() -> Result<String, UpdateError> {
    let status = updater()?
        .update()
        .map_err(|error| UpdateError::Install(error.to_string()))?;
    let version = status.version().to_string();

    self_update::restart::restart().map_err(|error| UpdateError::Restart(error.to_string()))?;

    Ok(version)
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("could not check for updates: {0}")]
    Check(String),
    #[error("could not install the update: {0}")]
    Install(String),
    #[error("the update was installed but the app could not restart: {0}")]
    Restart(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_configuration_is_valid_without_network_access() {
        let updater = updater().expect("updater config");
        let _ = updater;
    }

    #[test]
    fn update_metadata_is_plain_data() {
        let info = UpdateInfo {
            version: String::from("1.2.3"),
            name: String::from("v1.2.3"),
            notes: Some(String::from("Fixes and improvements")),
        };

        assert_eq!(info.version, "1.2.3");
        assert!(info.notes.is_some());
    }

    #[test]
    #[ignore = "network; run manually against the live release feed"]
    fn live_release_feed_is_reachable() {
        let result = check();
        assert!(
            result.is_ok(),
            "the live GitHub release feed must be reachable: {result:?}"
        );
    }
}
