//! The launcher updating itself from its GitHub releases, in place: no setup screen, no disk
//! image to open, no installer to click through.
//!
//! Whoever finds the new version (the window, or the menu bar / tray icon while the window is
//! closed) downloads the file for this kind of install, checks its SHA-256, puts it in place and
//! starts the new copy:
//!
//! - **macOS**: the `.app` from the DMG replaces this bundle (the old one is moved aside and
//!   deleted at the next start).
//! - **Windows**: the new executable replaces `dev-launcher.exe` (a running executable can be
//!   renamed away). An install for every user, in Program Files, runs the new setup as
//!   administrator instead.
//! - **Linux**: the new AppImage replaces this one; a `.deb`, `.rpm` or Arch package install is
//!   updated with the distribution's tool through pkexec.
//! - Anything else (a build from source, the `.tar.gz`, a copy run from the disk image) can't be
//!   replaced safely: the launcher only links to the release.
//!
//! The new copy is started with `--updated`, which waits for the old one to let go of its port.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use semver::Version;

use crate::agent::{self, Endpoint};
use crate::net::{self, Release, Transfer};
use crate::platform::{Arch, Kind, Platform};
use crate::{install, paths};

pub const REPO: &str = "DraftCraft-DEV/DEV-Launcher";
const DOWNLOAD_PREFIX: &str = "https://github.com/DraftCraft-DEV/DEV-Launcher/releases/download/";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BUNDLE_ID: &str = "dev.skryx.devlauncher";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const APP_NAME: &str = "D.E.V. Launcher.app";

/// How this copy gets replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    /// macOS: replace this `.app`.
    MacBundle(PathBuf),
    /// Windows: replace this executable (installed for this account, or the portable copy).
    WinExe(PathBuf),
    /// Windows: installed for every user; the new setup runs as administrator.
    WinMachine,
    /// Linux: replace this AppImage.
    AppImage(PathBuf),
    /// Linux: installed from the distribution's package.
    Package(Kind),
    /// Nothing that can be replaced safely.
    Manual,
}

impl Method {
    pub fn detect(platform: &Platform) -> Self {
        // A build from source updates with `git pull`. (Tests set a scratch bundle instead.)
        if cfg!(debug_assertions) {
            return match std::env::var_os("DEV_LAUNCHER_TEST_BUNDLE") {
                Some(b) if cfg!(target_os = "macos") => Self::MacBundle(PathBuf::from(b)),
                _ => Self::Manual,
            };
        }
        let Ok(exe) = std::env::current_exe() else { return Self::Manual };
        if cfg!(target_os = "macos") {
            return match agent::mac_main_bundle() {
                Some(b) if agent::stable_location(&b) => Self::MacBundle(b),
                _ => Self::Manual,
            };
        }
        if cfg!(windows) {
            if let Some((dir, everyone)) = crate::setup::windows_install()
                && same_place(&exe, &dir)
            {
                return if everyone { Self::WinMachine } else { Self::WinExe(exe) };
            }
            // The portable copy, where this account may write.
            return match exe.parent() {
                Some(dir) if paths::writable(dir) => Self::WinExe(exe),
                _ => Self::Manual,
            };
        }
        if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from)
            && appimage.is_file()
        {
            return Self::AppImage(appimage);
        }
        if exe.starts_with("/usr/")
            && let Some(kind) = platform.native_package()
        {
            return Self::Package(kind);
        }
        Self::Manual
    }

    /// Asks for a password or administrator permission: never done without the user looking.
    pub fn needs_permission(&self) -> bool {
        matches!(self, Self::WinMachine | Self::Package(_))
    }

    /// Can be done in the background, on its own.
    pub fn automatic(&self) -> bool {
        !matches!(self, Self::Manual) && !self.needs_permission()
    }

    /// The release file this copy is replaced with.
    pub fn asset_name(&self, platform: &Platform) -> Option<String> {
        let win = match platform.arch {
            Arch::X64 => "x64",
            Arch::Arm64 => "arm64",
        };
        let linux = match platform.arch {
            Arch::X64 => "x86_64",
            Arch::Arm64 => "aarch64",
        };
        Some(match self {
            Self::MacBundle(_) => "dev-launcher-macos-universal.dmg".to_string(),
            Self::WinExe(_) | Self::WinMachine => format!("dev-launcher-windows-{win}-setup.exe"),
            Self::AppImage(_) => format!("dev-launcher-linux-{linux}.AppImage"),
            Self::Package(Kind::Deb) => format!("dev-launcher-linux-{linux}.deb"),
            Self::Package(Kind::Rpm) => format!("dev-launcher-linux-{linux}.rpm"),
            Self::Package(Kind::Pacman) => format!("dev-launcher-linux-{linux}.pkg.tar.zst"),
            Self::Package(_) | Self::Manual => return None,
        })
    }
}

fn same_place(exe: &Path, dir: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(exe).starts_with(canon(dir))
}

/// The version running now.
pub fn running_version() -> Version {
    if cfg!(debug_assertions)
        && let Some(v) = std::env::var("DEV_LAUNCHER_TEST_VERSION").ok().and_then(|v| net::parse_version(&v))
    {
        return v;
    }
    net::parse_version(env!("CARGO_PKG_VERSION")).unwrap_or_else(|| Version::new(0, 0, 0))
}

/// The newest release above `current` that has a file for this copy (and its checksums: a
/// release still uploading isn't offered yet).
fn newest<'a>(releases: &'a [Release], current: &Version, prereleases: bool, asset: Option<&str>) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|r| prereleases || !(r.prerelease || r.version().is_some_and(|v| !v.pre.is_empty())))
        .filter(|r| r.version().is_some_and(|v| v > *current))
        .filter(|r| match asset {
            Some(name) => r.assets.iter().any(|a| a.name == name) && r.assets.iter().any(|a| a.name.eq_ignore_ascii_case("SHA256SUMS.txt")),
            // Only the link is shown: any newer release will do.
            None => true,
        })
        .max_by(|a, b| a.version().cmp(&b.version()))
}

/// Ask GitHub for a newer launcher.
pub fn check(method: &Method, platform: &Platform, prereleases: bool) -> Result<Option<Release>, String> {
    let releases = net::releases(REPO)?;
    let asset = method.asset_name(platform);
    Ok(newest(&releases, &running_version(), prereleases, asset.as_deref()).cloned())
}

/// Download `release` and put it in place of this copy. The running process carries on as the
/// old version until [`restart_window`] / [`restart_tray`].
pub fn apply(release: &Release, method: &Method, platform: &Platform, stop: &AtomicBool, progress: &mut dyn FnMut(Transfer)) -> Result<(), String> {
    let name = method.asset_name(platform).ok_or("this copy of D.E.V. Launcher can't update itself; download the new version from GitHub")?;
    let mut release = release.clone();
    net::add_checksums(&mut release, DOWNLOAD_PREFIX)?;
    let asset = release.assets.iter().find(|a| a.name == name).ok_or_else(|| format!("the release has no {name}"))?;
    if !net::url_under(&asset.url, DOWNLOAD_PREFIX) {
        return Err(format!("{name} isn't on the launcher's GitHub releases"));
    }
    let version = release.version_label();
    let dir = downloads();
    let file = dir.join(sanitize(&version)).join(&asset.name);
    net::download(asset, &file, stop, progress)?;
    log::info!("self-update: installing D.E.V. Launcher {version} ({method:?})");
    let result = match method {
        Method::MacBundle(bundle) => install::install_mac(&file, bundle, BUNDLE_ID, APP_NAME, Some(&version), &[std::process::id()]),
        Method::WinExe(exe) => replace_exe(&file, exe).inspect(|()| crate::setup::record_version(&version)),
        Method::WinMachine => crate::setup::run_elevated_program(&file, &["--install", "--everyone"]),
        Method::AppImage(path) => install::install_appimage(&file, path),
        Method::Package(kind) => install::install_package(*kind, &file),
        Method::Manual => Err("this copy can't update itself".into()),
    };
    paths::remove_any(&dir);
    result
}

fn downloads() -> PathBuf {
    paths::downloads_dir().join("dev-launcher")
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
}

/// Put the executable `new` at `exe`. Windows can't overwrite or delete a running executable,
/// but it can rename it: the old one moves aside and is deleted at the next start.
fn replace_exe(new: &Path, exe: &Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("the launcher isn't in a folder")?;
    let staged = dir.join(".dev-launcher-update.exe");
    std::fs::copy(new, &staged).map_err(|e| format!("couldn't copy the new launcher to {} ({e})", dir.display()))?;
    let mut old = exe.with_extension("exe.old");
    paths::remove_any(&old);
    if old.exists() {
        // Still held by a copy that hasn't exited since the last update.
        old = exe.with_extension(format!("exe.{}.old", std::process::id()));
    }
    install::swap_in(&staged, exe, &old).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })
}

/// Delete what earlier updates left: the old bundle or executable and the downloads. Called
/// when the window starts (no older window can still be running then) and when the tray starts
/// with no window up.
pub fn clean_leftovers() {
    let Ok(exe) = std::env::current_exe() else { return };
    if cfg!(target_os = "macos") {
        if let Some(bundle) = agent::mac_main_bundle()
            && let Some(parent) = bundle.parent()
        {
            paths::remove_any(&parent.join(".D.E.V. Launcher-old.app"));
        }
    } else if cfg!(windows)
        && let Some(dir) = exe.parent()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("dev-launcher.exe") && name.ends_with(".old") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    paths::remove_any(&downloads());
}

/// `--updated`: wait (a few seconds at most) for the copy that started this one to let go of
/// `endpoint`, so this one doesn't just hand over to it and leave.
pub fn wait_for_old(endpoint: Endpoint) {
    let until = Instant::now() + Duration::from_secs(15);
    while agent::is_running(endpoint) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The new copy's program. (What a running process reports about itself can point at the file
/// that was just replaced: Linux shows `/usr/bin/dev-launcher (deleted)`.)
fn new_program(method: &Method) -> Option<PathBuf> {
    match method {
        Method::MacBundle(b) => Some(b.join("Contents/MacOS/dev-launcher")),
        Method::WinExe(exe) => Some(exe.clone()),
        Method::WinMachine => crate::setup::windows_install().map(|(dir, _)| dir.join("dev-launcher.exe")),
        Method::AppImage(p) => Some(p.clone()),
        Method::Package(_) => Some(PathBuf::from("/usr/bin/dev-launcher")),
        Method::Manual => agent::this_program(),
    }
}

/// Start the new version's window. The caller then closes its own.
pub fn restart_window(method: &Method) -> Result<(), String> {
    let mut cmd = match method {
        Method::MacBundle(bundle) => {
            // `-n`: a new process even though this one (the old version) still runs.
            let mut c = Command::new("/usr/bin/open");
            c.arg("-n").arg(bundle).args(["--args", "--updated"]);
            c
        }
        _ => {
            let mut c = Command::new(new_program(method).ok_or("can't find the launcher")?);
            c.arg("--updated");
            c
        }
    };
    agent::detached(&mut cmd).spawn().map(|_| ()).map_err(|e| format!("couldn't start the new version ({e})"))
}

/// Start the new version's tray; it waits for the running one to quit (`--updated`).
pub fn restart_tray(method: &Method) {
    let program = match method {
        // The menu bar icon runs from the helper app inside the bundle.
        Method::MacBundle(b) => Some(b.join("Contents/Library/LoginItems/D.E.V. Launcher Helper.app/Contents/MacOS/dev-launcher")).filter(|h| h.is_file()),
        _ => None,
    }
    .or_else(|| new_program(method));
    let Some(program) = program else { return };
    match agent::detached(Command::new(&program).args(["--tray", "--updated"])).spawn() {
        Ok(_) => log::info!("self-update: started the new tray ({})", program.display()),
        Err(e) => log::warn!("self-update: couldn't start the new tray ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Asset;
    use crate::platform::{LinuxFamily, Os};

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        Release {
            tag: tag.into(),
            prerelease,
            assets: assets.iter().map(|n| Asset { name: (*n).into(), ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    const DMG: &str = "dev-launcher-macos-universal.dmg";
    const SUMS: &str = "SHA256SUMS.txt";

    #[test]
    fn picks_the_newest_complete_release() {
        let list = vec![
            release("v1.2.0", false, &[DMG]),
            release("v1.1.0", false, &[DMG, SUMS]),
            release("v1.3.0-rc.1", true, &[DMG, SUMS]),
            release("v0.9.0", false, &[DMG, SUMS]),
        ];
        let now = Version::parse("1.0.0").unwrap();
        // 1.2.0 is still uploading (no checksums), 1.3.0-rc.1 is a pre-release.
        assert_eq!(newest(&list, &now, false, Some(DMG)).map(|r| r.tag.as_str()), Some("v1.1.0"));
        assert_eq!(newest(&list, &now, true, Some(DMG)).map(|r| r.tag.as_str()), Some("v1.3.0-rc.1"));
        assert_eq!(newest(&list, &Version::parse("1.1.0").unwrap(), false, Some(DMG)), None);
        // 1.0.0 is newer than the beta it follows.
        let beta = Version::parse("1.0.0-beta").unwrap();
        let list = vec![release("v1.0.0", false, &[DMG, SUMS])];
        assert!(newest(&list, &beta, false, Some(DMG)).is_some());
    }

    #[test]
    fn asset_per_install() {
        let p = |os, arch| Platform { os, arch, family: Some(LinuxFamily::Debian), system_name: String::new() };
        let linux = p(Os::Linux, Arch::Arm64);
        assert_eq!(Method::AppImage(PathBuf::new()).asset_name(&linux).as_deref(), Some("dev-launcher-linux-aarch64.AppImage"));
        assert_eq!(Method::Package(Kind::Pacman).asset_name(&p(Os::Linux, Arch::X64)).as_deref(), Some("dev-launcher-linux-x86_64.pkg.tar.zst"));
        assert_eq!(Method::WinMachine.asset_name(&p(Os::Windows, Arch::Arm64)).as_deref(), Some("dev-launcher-windows-arm64-setup.exe"));
        assert_eq!(Method::MacBundle(PathBuf::new()).asset_name(&p(Os::MacOs, Arch::X64)).as_deref(), Some(DMG));
        assert_eq!(Method::Manual.asset_name(&linux), None);
        assert!(Method::WinExe(PathBuf::new()).automatic());
        assert!(!Method::Package(Kind::Deb).automatic());
    }

    /// Downloads the real release and swaps it into a scratch bundle:
    /// `DEV_LAUNCHER_TEST_BUNDLE=/tmp/x/D.E.V.\ Launcher.app DEV_LAUNCHER_TEST_VERSION=0.9.0 cargo test real_update -- --ignored`
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn real_update_into_a_scratch_bundle() {
        let platform = Platform::detect();
        let method = Method::detect(&platform);
        let Method::MacBundle(bundle) = &method else { panic!("set DEV_LAUNCHER_TEST_BUNDLE") };
        let release = check(&method, &platform, true).unwrap().expect("a newer release");
        let stop = AtomicBool::new(false);
        let mut last = 0;
        apply(&release, &method, &platform, &stop, &mut |t| {
            let pct = t.done * 100 / t.total.max(1);
            if pct / 25 != last / 25 {
                eprintln!("{pct}%");
            }
            last = pct;
        })
        .unwrap();
        let info = crate::detect::mac_bundle_info(bundle).unwrap();
        assert_eq!(info.get("CFBundleIdentifier").map(String::as_str), Some(BUNDLE_ID));
        assert_eq!(net::parse_version(info.get("CFBundleShortVersionString").unwrap()), release.version());
        eprintln!("{} now holds {}", bundle.display(), release.version_label());
    }
}
