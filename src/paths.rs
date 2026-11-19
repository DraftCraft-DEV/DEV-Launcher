//! Where the launcher keeps its files.

use std::path::{Path, PathBuf};

/// Folder name under the system's data/config/cache folders.
fn folder() -> &'static str {
    if cfg!(target_os = "linux") { "dev-launcher" } else { "D.E.V. Launcher" }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

/// Settings and the record of what the launcher installed.
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(home).join(folder())
}

/// App data (Linux AppImages, Windows portable copies the launcher manages).
pub fn data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(home).join(folder())
}

/// Release lists and downloads in progress.
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join(folder())
}

pub fn downloads_dir() -> PathBuf {
    cache_dir().join("downloads")
}

pub fn log_file() -> PathBuf {
    cache_dir().join("launcher.log")
}

/// `~/Applications` (macOS: the per-user Applications folder; Linux: where AppImages go by
/// convention, also used by AppImageLauncher).
pub fn user_applications() -> PathBuf {
    home().join("Applications")
}

/// Windows: `%LOCALAPPDATA%\Programs`, where per-user programs live.
pub fn windows_user_programs() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(home).join("Programs")
}

/// Linux: `~/.local/share/applications` and the icon theme folder.
pub fn linux_desktop_entries() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| home().join(".local/share")).join("applications")
}

pub fn linux_icons() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| home().join(".local/share")).join("icons/hicolor/256x256/apps")
}

/// Whether this account can create files in `dir` (creating it if needed).
pub fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".dev-launcher-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Remove a file or folder, ignoring errors.
pub fn remove_any(path: &Path) {
    if path.is_dir() && !path.is_symlink() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Write `bytes` to `path` atomically (temporary file, then rename).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
