//! Installing the launcher itself, without a third-party wizard: the launcher's own executable
//! is the installer.
//!
//! - **Windows**: `dev-launcher-windows-<arch>-setup.exe` is the launcher. Started under that
//!   name (or with `--setup`) it shows the install screen, copies itself to
//!   `%LOCALAPPDATA%\Programs\D.E.V. Launcher` (or `Program Files` for everyone, elevated through
//!   UAC), adds the Start menu and desktop shortcuts and the "Installed apps" entry, whose
//!   uninstall command is `dev-launcher.exe --uninstall`.
//! - **Linux AppImage**: on first start from anywhere else it offers to move itself to
//!   `~/Applications`, add a menu entry and icon, and link `~/.local/bin/dev-launcher`.
//! - **macOS**: the DMG is drag-and-drop; nothing to do here.
//!
//! Headless: `--install [--everyone] [--desktop-shortcut]`, `--uninstall --quiet [--remove-data]`.

use std::path::{Path, PathBuf};

use crate::paths;

pub const NAME: &str = "D.E.V. Launcher";
#[cfg_attr(not(windows), allow(dead_code))]
const REG_ID: &str = "dev.skryx.devlauncher";
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const APPIMAGE_NAME: &str = "DEV-Launcher.AppImage";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Windows: install in Program Files for every user (asks for administrator rights).
    pub everyone: bool,
    /// Windows: a shortcut on the desktop too.
    pub desktop_shortcut: bool,
}

/// What the setup screen is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
}

/// Whether this start should show the setup screen instead of the launcher.
pub fn mode_for_this_start(forced: Option<Mode>) -> Option<Mode> {
    if forced.is_some() {
        return forced;
    }
    let exe = std::env::current_exe().ok()?;
    if cfg!(windows) {
        let stem = exe.file_stem()?.to_string_lossy().to_ascii_lowercase();
        return stem.contains("setup").then_some(Mode::Install);
    }
    if cfg!(target_os = "linux") {
        // An AppImage started from Downloads (or anywhere but its installed place).
        let appimage = PathBuf::from(std::env::var_os("APPIMAGE")?);
        if Some(&appimage) == installed_program().as_ref() || appimage_setup_declined(&appimage) {
            return None;
        }
        return Some(Mode::Install);
    }
    None
}

/// The installed launcher, if any.
pub fn installed_program() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return win::installed().map(|(dir, _)| dir.join("dev-launcher.exe")).filter(|p| p.exists());
    }
    #[cfg(target_os = "linux")]
    {
        let p = linux_target();
        return p.exists().then_some(p);
    }
    #[allow(unreachable_code)]
    None
}

/// Where the launcher goes.
pub fn target_dir(opts: Options) -> PathBuf {
    if cfg!(windows) {
        if opts.everyone {
            std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from).join(NAME)
        } else {
            paths::windows_user_programs().join(NAME)
        }
    } else {
        paths::user_applications()
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_target() -> PathBuf {
    paths::user_applications().join(APPIMAGE_NAME)
}

fn declined_marker() -> PathBuf {
    paths::config_dir().join("appimage-setup-declined")
}

/// "Run without installing" was chosen for this AppImage.
fn appimage_setup_declined(appimage: &Path) -> bool {
    std::fs::read_to_string(declined_marker()).is_ok_and(|s| s.lines().any(|l| Path::new(l) == appimage))
}

pub fn decline_appimage_setup() {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        let mut s = std::fs::read_to_string(declined_marker()).unwrap_or_default();
        s.push_str(&format!("{}\n", PathBuf::from(a).display()));
        let _ = paths::write_atomic(&declined_marker(), s.as_bytes());
    }
}

/// Install the running executable. Returns the installed program.
pub fn install(opts: Options) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        if opts.everyone && !win::is_elevated() {
            let mut args = vec!["--install", "--everyone"];
            if opts.desktop_shortcut {
                args.push("--desktop-shortcut");
            }
            win::run_elevated(&args)?;
            return Ok(target_dir(opts).join("dev-launcher.exe"));
        }
        return win::install(opts);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = opts;
        return linux::install();
    }
    #[allow(unreachable_code)]
    {
        let _ = opts;
        Err("the launcher installs itself on Windows and Linux; on macOS drag it to Applications".into())
    }
}

/// Remove the installed launcher (the running one, when started with `--uninstall`).
pub fn uninstall(remove_data: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if win::installed().is_some_and(|(_, everyone)| everyone) && !win::is_elevated() {
            let mut args = vec!["--uninstall", "--quiet"];
            if remove_data {
                args.push("--remove-data");
            }
            return win::run_elevated(&args);
        }
        win::uninstall()?;
    }
    #[cfg(target_os = "linux")]
    linux::uninstall()?;
    if remove_data {
        for dir in [paths::config_dir(), paths::data_dir(), paths::cache_dir()] {
            paths::remove_any(&dir);
        }
    }
    Ok(())
}

/// Start the installed launcher and let this process end.
pub fn launch_installed(program: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    // The new copy must not think it runs inside this AppImage.
    for var in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
        cmd.env_remove(var);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("couldn't start {} ({e})", program.display()))
}

/// Size of the installed files, for the "Installed apps" entry.
#[cfg_attr(not(windows), allow(dead_code))]
fn size_kb(path: &Path) -> u32 {
    std::fs::metadata(path).map_or(0, |m| (m.len() / 1024) as u32)
}

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};

    use super::{NAME, Options, REG_ID, size_kb, target_dir};
    use crate::system;

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\dev-launcher.exe";

    fn root(everyone: bool) -> RegKey {
        RegKey::predef(if everyone { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
    }

    /// (install folder, for everyone) from the "Installed apps" entry.
    pub fn installed() -> Option<(PathBuf, bool)> {
        for everyone in [false, true] {
            if let Ok(k) = root(everyone).open_subkey_with_flags(format!(r"{UNINSTALL}\{REG_ID}"), KEY_READ) {
                if let Ok(dir) = k.get_value::<String, _>("InstallLocation") {
                    return Some((PathBuf::from(dir), everyone));
                }
            }
        }
        None
    }

    pub fn is_elevated() -> bool {
        // Writing under HKLM works only for an elevated process.
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"Software", KEY_WRITE).is_ok()
    }

    /// Run this executable again as administrator (UAC) with `args`, and wait for it.
    pub fn run_elevated(args: &[&str]) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let q = |s: &str| s.replace('\'', "''");
        let list = args.iter().map(|a| format!("'{}'", q(a))).collect::<Vec<_>>().join(",");
        let script = format!(
            "try {{ $p = Start-Process -FilePath '{}' -ArgumentList {list} -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
            q(&exe.display().to_string())
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| format!("couldn't ask for administrator rights ({e})"))?;
        match status.code() {
            Some(0) => Ok(()),
            Some(1223) => Err("administrator permission was refused".into()),
            other => Err(format!("the elevated setup failed (exit {other:?})")),
        }
    }

    fn start_menu(everyone: bool) -> Option<PathBuf> {
        let base = if everyone { std::env::var_os("ProgramData").map(PathBuf::from) } else { dirs::data_dir() };
        base.map(|b| b.join(r"Microsoft\Windows\Start Menu\Programs"))
    }

    fn desktop(everyone: bool) -> Option<PathBuf> {
        if everyone { std::env::var_os("PUBLIC").map(|p| PathBuf::from(p).join("Desktop")) } else { dirs::desktop_dir() }
    }

    fn shortcut(lnk: &Path, exe: &Path) {
        let q = |p: &Path| p.display().to_string().replace('\'', "''");
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Description='Install, update and open the DraftCraft apps';$s.Save()",
            q(lnk),
            q(exe),
            q(exe.parent().unwrap_or(exe)),
            q(exe)
        );
        if let Err(e) = system::run("powershell", &[OsStr::new("-NoProfile"), OsStr::new("-NonInteractive"), OsStr::new("-Command"), OsStr::new(&script)]) {
            log::warn!("no shortcut {}: {e}", lnk.display());
        }
    }

    pub fn install(opts: Options) -> Result<PathBuf, String> {
        let dir = target_dir(opts);
        std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = dir.join("dev-launcher.exe");
        if me.canonicalize().ok() != exe.canonicalize().ok() {
            // A running launcher keeps its file locked, but it can be renamed away.
            if exe.exists() {
                let old = dir.join("dev-launcher.exe.old");
                let _ = std::fs::remove_file(&old);
                std::fs::rename(&exe, &old).map_err(|e| format!("couldn't replace the installed launcher ({e})"))?;
            }
            let staged = dir.join("dev-launcher.exe.new");
            std::fs::copy(&me, &staged).map_err(|e| format!("couldn't copy the launcher ({e})"))?;
            std::fs::rename(&staged, &exe).map_err(|e| format!("couldn't put the launcher in place ({e})"))?;
        }
        if let Some(menu) = start_menu(opts.everyone) {
            let _ = std::fs::create_dir_all(&menu);
            shortcut(&menu.join(format!("{NAME}.lnk")), &exe);
        }
        if opts.desktop_shortcut {
            if let Some(d) = desktop(opts.everyone) {
                shortcut(&d.join(format!("{NAME}.lnk")), &exe);
            }
        }
        let root = root(opts.everyone);
        let (key, _) = root.create_subkey(format!(r"{UNINSTALL}\{REG_ID}")).map_err(|e| format!("couldn't register the launcher ({e})"))?;
        let exe_s = exe.display().to_string();
        let set = |name: &str, value: &str| key.set_value(name, &value.to_string()).map_err(|e| e.to_string());
        set("DisplayName", NAME)?;
        set("DisplayVersion", env!("CARGO_PKG_VERSION"))?;
        set("Publisher", "DraftCraft Engineering Ventures")?;
        set("DisplayIcon", &format!("{exe_s},0"))?;
        set("InstallLocation", &dir.display().to_string())?;
        set("UninstallString", &format!("\"{exe_s}\" --uninstall"))?;
        set("QuietUninstallString", &format!("\"{exe_s}\" --uninstall --quiet"))?;
        set("URLInfoAbout", "https://github.com/DraftCraft-DEV/DEV-Launcher")?;
        let _ = key.set_value("NoModify", &1u32);
        let _ = key.set_value("NoRepair", &1u32);
        let _ = key.set_value("EstimatedSize", &size_kb(&exe));
        if let Ok((paths, _)) = root.create_subkey(APP_PATHS) {
            let _ = paths.set_value("", &exe_s);
        }
        Ok(exe)
    }

    pub fn uninstall() -> Result<(), String> {
        let Some((dir, everyone)) = installed() else { return Err("D.E.V. Launcher isn't installed".into()) };
        for base in [start_menu(everyone), desktop(everyone)].into_iter().flatten() {
            let _ = std::fs::remove_file(base.join(format!("{NAME}.lnk")));
        }
        let root = root(everyone);
        let _ = root.delete_subkey_all(format!(r"{UNINSTALL}\{REG_ID}"));
        let _ = root.delete_subkey_all(APP_PATHS);
        // The running executable can't delete itself: a hidden shell removes the folder once it exits.
        let me = std::env::current_exe().ok();
        if me.as_ref().is_some_and(|m| m.starts_with(&dir)) {
            let script = format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display());
            use std::os::windows::process::CommandExt;
            std::process::Command::new("cmd")
                .args(["/c", &script])
                .current_dir(std::env::temp_dir())
                .creation_flags(0x0800_0000)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            crate::paths::remove_any(&dir);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;

    use super::linux_target;
    use crate::{assets, install, paths};

    fn entry() -> PathBuf {
        paths::linux_desktop_entries().join("dev-launcher.desktop")
    }

    fn icon() -> PathBuf {
        dirs::data_dir().unwrap_or_default().join("icons/hicolor/256x256/apps/dev-launcher.png")
    }

    fn bin_link() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".local/bin/dev-launcher")
    }

    pub fn install() -> Result<PathBuf, String> {
        let source = std::env::var_os("APPIMAGE").map(PathBuf::from).ok_or("this copy isn't an AppImage")?;
        let target = linux_target();
        if source != target {
            install::install_appimage(&source, &target)?;
        }
        let _ = paths::write_atomic(&icon(), assets::APP_ICON_256);
        let exec = target.display().to_string().replace('"', "\\\"");
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=D.E.V. Launcher\nComment=Install, update and open the DraftCraft Engineering Ventures apps\nComment[it]=Installa, aggiorna e apri le app di DraftCraft Engineering Ventures\nExec=\"{exec}\"\nIcon=dev-launcher\nTerminal=false\nCategories=Utility;Development;\nStartupWMClass=dev.skryx.devlauncher\nActions=Uninstall;\n\n[Desktop Action Uninstall]\nName=Uninstall D.E.V. Launcher\nName[it]=Disinstalla D.E.V. Launcher\nExec=\"{exec}\" --uninstall\n"
        );
        paths::write_atomic(&entry(), desktop.as_bytes()).map_err(|e| format!("couldn't add the menu entry ({e})"))?;
        let link = bin_link();
        if let Some(dir) = link.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::remove_file(&link);
        let _ = std::os::unix::fs::symlink(&target, &link);
        let _ = std::process::Command::new("update-desktop-database").arg(paths::linux_desktop_entries()).output();
        // The source in Downloads stays; it's the user's file.
        Ok(target)
    }

    pub fn uninstall() -> Result<(), String> {
        let target = linux_target();
        if std::fs::read_link(bin_link()).is_ok_and(|t| t == target) {
            let _ = std::fs::remove_file(bin_link());
        }
        let _ = std::fs::remove_file(entry());
        let _ = std::fs::remove_file(icon());
        let _ = std::process::Command::new("update-desktop-database").arg(paths::linux_desktop_entries()).output();
        // Deleting the running AppImage file is fine on Linux.
        match std::fs::remove_file(&target) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {} ({e})", target.display())),
        }
    }
}
