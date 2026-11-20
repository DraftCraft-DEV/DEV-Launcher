//! Starting apps, showing files, opening links, and knowing what runs.

use std::path::Path;
use std::process::{Command, Stdio};

use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::catalog::AppDef;
use crate::detect::{Installed, Method};

#[cfg(windows)]
const DETACHED: u32 = 0x0000_0008 | 0x0000_0200; // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP

fn detach(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(home) = dirs::home_dir() {
        cmd.current_dir(home);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(DETACHED);
    }
    cmd
}

/// Start an installed app.
pub fn launch(app: &AppDef, installed: &Installed) -> Result<(), String> {
    let program = installed.program(app);
    let mut cmd = match &installed.method {
        Method::MacApp => {
            let mut c = Command::new("/usr/bin/open");
            c.arg("-a").arg(&program);
            c
        }
        _ => Command::new(&program),
    };
    // An AppImage launched from here shouldn't think it was the launcher's own AppImage.
    cmd.env_remove("APPIMAGE").env_remove("APPDIR").env_remove("ARGV0").env_remove("OWD");
    detach(&mut cmd).spawn().map(|_| ()).map_err(|e| format!("couldn't start {} ({e})", program.display()))
}

/// Show a file or folder in the file manager.
pub fn reveal(path: &Path) {
    let result = if cfg!(target_os = "macos") {
        detach(Command::new("/usr/bin/open").arg("-R").arg(path)).spawn()
    } else if cfg!(windows) {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        detach(Command::new("explorer").arg(arg)).spawn()
    } else {
        let dir = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        detach(Command::new("xdg-open").arg(dir)).spawn()
    };
    if let Err(e) = result {
        log::warn!("couldn't show {}: {e}", path.display());
    }
}

/// Open an https link in the browser.
pub fn open_url(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    let result = if cfg!(target_os = "macos") {
        detach(Command::new("/usr/bin/open").arg(url)).spawn()
    } else if cfg!(windows) {
        detach(Command::new("rundll32").args(["url.dll,FileProtocolHandler", url])).spawn()
    } else {
        detach(Command::new("xdg-open").arg(url)).spawn()
    };
    if let Err(e) = result {
        log::warn!("couldn't open {url}: {e}");
    }
}

/// Ids of the processes running `installed` (from its folder or bundle, or for packages and
/// AppImages, by program name).
pub fn running(app: &AppDef, installed: &Installed) -> Vec<u32> {
    let Ok(sys) = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    ) else {
        return Vec::new();
    };
    // A Mac app is its main program: tools inside the bundle (a command-line helper, an MCP
    // server started by an AI agent) don't make it "running".
    let main_exe = matches!(installed.method, Method::MacApp)
        .then(|| crate::detect::mac_bundle_info(&installed.path))
        .flatten()
        .and_then(|info| info.get("CFBundleExecutable").cloned())
        .map(|exe| installed.path.join("Contents/MacOS").join(exe));
    let root = match &installed.method {
        Method::MacApp | Method::WinPortable => Some(installed.path.clone()),
        Method::WinSetup { .. } => installed.path.parent().map(Path::to_path_buf),
        Method::AppImage | Method::Package(_) => None,
    };
    let names: Vec<String> = [&app.linux.binary, &app.windows.exe, &app.windows.installed_exe, &app.asset_prefix]
        .iter()
        .filter(|n| !n.is_empty())
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let me = std::process::id();
    sys.processes()
        .iter()
        .filter(|(pid, p)| {
            if pid.as_u32() == me || p.thread_kind().is_some() {
                return false;
            }
            match (&root, p.exe()) {
                (Some(_), Some(exe)) if main_exe.is_some() => Some(exe) == main_exe.as_deref(),
                (Some(root), Some(exe)) => exe.starts_with(root),
                _ => {
                    let name = p.name().to_string_lossy().to_ascii_lowercase();
                    names.iter().any(|n| name == *n || name.trim_end_matches(".exe") == n.trim_end_matches(".exe"))
                        || p.exe().is_some_and(|e| e == installed.path)
                }
            }
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Ask a running app to quit the normal way (it may ask to save first). macOS only; elsewhere
/// the user closes it.
pub fn ask_to_quit(app: &AppDef) -> bool {
    if !cfg!(target_os = "macos") || app.macos.bundle_id.is_empty() {
        return false;
    }
    let script = format!("tell application id \"{}\" to quit", app.macos.bundle_id.replace('"', ""));
    Command::new("/usr/bin/osascript").args(["-e", &script]).status().is_ok_and(|s| s.success())
}

/// Wait for no process of `installed` to run, up to `secs`.
pub fn wait_until_closed(app: &AppDef, installed: &Installed, secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < secs {
        if running(app, installed).is_empty() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    false
}

/// Run a system tool; its error output on failure.
pub fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("couldn't run {program} ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        Err(if err.is_empty() { format!("{program} failed ({})", out.status) } else { format!("{program}: {err}") })
    }
}
