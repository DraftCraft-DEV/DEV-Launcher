//! The launcher's preferences, saved as JSON in the config folder.

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LangPref {
    #[default]
    Auto,
    English,
    Italian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePref {
    #[default]
    System,
    Dark,
    Light,
}

/// Which package the launcher installs on Linux.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinuxFormat {
    /// The distribution's own package when the release has one, otherwise the AppImage.
    #[default]
    Auto,
    AppImage,
    Native,
}

/// Who a Windows install is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinScope {
    #[default]
    User,
    Machine,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: LangPref,
    pub theme: ThemePref,
    /// Look for new versions when the launcher starts (and every hour while it runs).
    pub check_on_start: bool,
    /// Install app updates as soon as they are found.
    pub auto_update: bool,
    /// Offer pre-release versions.
    pub prereleases: bool,
    pub linux_format: LinuxFormat,
    pub windows_scope: WinScope,
    /// macOS: install into `~/Applications` instead of `/Applications`.
    pub mac_user_applications: bool,
    /// Close the launcher after starting an app.
    pub close_on_launch: bool,
    /// Keep an icon in the menu bar / notification area that opens the apps and checks for
    /// updates while the window is closed.
    pub tray: bool,
    /// Start that icon when the user signs in.
    pub autostart: bool,
    /// The app shown when the launcher opens.
    pub last_app: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LangPref::Auto,
            theme: ThemePref::System,
            check_on_start: true,
            auto_update: false,
            prereleases: false,
            linux_format: LinuxFormat::Auto,
            windows_scope: WinScope::User,
            mac_user_applications: false,
            close_on_launch: false,
            tray: true,
            autostart: true,
            last_app: None,
        }
    }
}

fn file() -> std::path::PathBuf {
    paths::config_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(e) = paths::write_atomic(&file(), &bytes) {
                    log::warn!("couldn't save settings: {e}");
                }
            }
            Err(e) => log::warn!("couldn't encode settings: {e}"),
        }
    }
}
