//! The always-on half of the launcher, seen from both sides: starting it with the computer,
//! starting it from the window, and the one-line messages the window and the tray send each
//! other over a local socket.
//!
//! Both the window and the tray listen on a port of their own (derived from the user's home
//! folder, so two people signed in at once don't collide). Holding the port is also what makes
//! each of them single-instance: a second window just asks the first one to come forward.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::settings::Settings;

/// Which listener a message is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Window,
    Tray,
}

/// A message between the window and the tray.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Window: come to the front, showing this app if given.
    Show(Option<String>),
    /// Tray: settings or installs changed, look again.
    Refresh,
    /// Are you there? (no answer needed: connecting is enough)
    Ping,
    Quit,
}

const MAGIC: &str = "dev-launcher";

impl Cmd {
    fn encode(&self) -> String {
        match self {
            Self::Show(Some(app)) => format!("{MAGIC} show {app}\n"),
            Self::Show(None) => format!("{MAGIC} show\n"),
            Self::Refresh => format!("{MAGIC} refresh\n"),
            Self::Ping => format!("{MAGIC} ping\n"),
            Self::Quit => format!("{MAGIC} quit\n"),
        }
    }

    fn decode(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        if words.next()? != MAGIC {
            return None;
        }
        Some(match words.next()? {
            "show" => Self::Show(words.next().filter(|a| a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).map(String::from)),
            "refresh" => Self::Refresh,
            "ping" => Self::Ping,
            "quit" => Self::Quit,
            _ => return None,
        })
    }
}

fn port(endpoint: Endpoint) -> u16 {
    let who = dirs::home_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let hash = who.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let base = 47_100 + (hash % 400) as u16 * 2;
    base + u16::from(endpoint == Endpoint::Tray)
}

fn addr(endpoint: Endpoint) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port(endpoint)))
}

/// Send `cmd`; false when nobody listens there.
pub fn send(endpoint: Endpoint, cmd: &Cmd) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr(endpoint), Duration::from_millis(300)) else { return false };
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    stream.write_all(cmd.encode().as_bytes()).is_ok()
}

/// Whether the window or the tray is running.
pub fn is_running(endpoint: Endpoint) -> bool {
    send(endpoint, &Cmd::Ping)
}

/// Take `endpoint`'s port and pass each message to `on_cmd` (on a thread of its own). `None` when
/// another copy already holds it.
pub fn listen(endpoint: Endpoint, on_cmd: impl Fn(Cmd) + Send + 'static) -> Option<()> {
    let listener = TcpListener::bind(addr(endpoint)).ok()?;
    let spawned = std::thread::Builder::new().name("agent-listen".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            if BufReader::new(std::io::Read::take(stream, 256)).read_line(&mut line).is_ok()
                && let Some(cmd) = Cmd::decode(&line) {
                    on_cmd(cmd);
                }
        }
    });
    spawned.ok().map(|_| ())
}

// ---------------------------------------------------------------------------------------------
// Where the programs are

/// The launcher as the user started it: the AppImage file rather than its mounted inside.
pub fn this_program() -> Option<PathBuf> {
    if cfg!(target_os = "linux")
        && let Some(a) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(a);
        }
    std::env::current_exe().ok()
}

/// macOS: the `.app` this copy belongs to (the main one, also when this is its helper).
pub fn mac_main_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut found = None;
    for dir in exe.ancestors() {
        if dir.extension().is_some_and(|e| e == "app") {
            found = Some(dir.to_path_buf());
        }
    }
    found
}

/// macOS: the helper app inside the main bundle that shows the menu bar icon. It is a separate
/// app (with its own bundle id and no Dock icon) so that opening the launcher while the icon is
/// up starts the window instead of just activating the icon's process.
fn mac_helper() -> Option<PathBuf> {
    let exe = mac_main_bundle()?.join("Contents/Library/LoginItems/D.E.V. Launcher Helper.app/Contents/MacOS/dev-launcher");
    exe.is_file().then_some(exe)
}

/// The program to run with `--tray`.
fn tray_program() -> Option<PathBuf> {
    if cfg!(target_os = "macos")
        && let Some(h) = mac_helper() {
            return Some(h);
        }
    this_program()
}

/// A copy that will still be there after a restart (not a mounted disk image, not a translocated
/// quarantine copy).
fn stable_location(p: &Path) -> bool {
    let s = p.to_string_lossy();
    !(s.starts_with("/Volumes/") || s.contains("/AppTranslocation/") || s.contains("/.mount_"))
}

fn detached(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    for var in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
        cmd.env_remove(var);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    cmd
}

/// Start the tray unless it runs.
pub fn start_tray() {
    if is_running(Endpoint::Tray) {
        return;
    }
    let Some(program) = tray_program() else { return };
    match detached(Command::new(&program).arg("--tray")).spawn() {
        Ok(_) => log::info!("started the tray ({})", program.display()),
        Err(e) => log::warn!("couldn't start the tray: {e}"),
    }
}

/// Bring the window forward (starting it if needed), showing `app`.
pub fn show_window(app: Option<&str>) {
    if send(Endpoint::Window, &Cmd::Show(app.map(String::from))) {
        return;
    }
    let mut args: Vec<&str> = Vec::new();
    if let Some(a) = app {
        args.extend(["--app", a]);
    }
    let result = match (cfg!(target_os = "macos"), mac_main_bundle()) {
        (true, Some(bundle)) => {
            let mut c = Command::new("/usr/bin/open");
            c.arg("-a").arg(bundle);
            if !args.is_empty() {
                c.arg("--args").args(&args);
            }
            detached(&mut c).spawn()
        }
        _ => match this_program() {
            Some(p) => detached(Command::new(p).args(&args)).spawn(),
            None => return,
        },
    };
    if let Err(e) = result {
        log::warn!("couldn't open the launcher window: {e}");
    }
}

/// Make the tray and the start-at-login entry match the settings. Called when the window opens
/// and when the settings change.
pub fn apply(settings: &Settings) {
    let wanted = settings.tray && settings.autostart;
    // A development build would leave a login item pointing into `target/`.
    if !cfg!(debug_assertions) {
        if !wanted {
            set_autostart(None);
        } else if let Some(program) = tray_program().filter(|p| stable_location(p)) {
            set_autostart(Some(&program));
        }
    }
    if settings.tray {
        start_tray();
    } else {
        send(Endpoint::Tray, &Cmd::Quit);
    }
}

/// Undo everything this module set up (the launcher's own uninstall).
pub fn remove_all() {
    send(Endpoint::Tray, &Cmd::Quit);
    set_autostart(None);
}

// ---------------------------------------------------------------------------------------------
// Start with the computer

/// Register `program --tray` to start at login, or (`None`) remove it.
fn set_autostart(program: Option<&Path>) {
    if let Err(e) = autostart::set(program) {
        log::warn!("couldn't change the login item: {e}");
    }
}

#[cfg(target_os = "macos")]
mod autostart {
    use std::path::{Path, PathBuf};

    /// A LaunchAgent: works on every supported macOS, and shows up in System Settings › Login Items.
    fn plist() -> Option<PathBuf> {
        Some(dirs::home_dir()?.join("Library/LaunchAgents/dev.skryx.devlauncher.plist"))
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn set(program: Option<&Path>) -> Result<(), String> {
        let file = plist().ok_or("no home folder")?;
        let Some(program) = program else {
            return match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        };
        let text = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>dev.skryx.devlauncher</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>--tray</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#,
            xml_escape(&program.to_string_lossy())
        );
        if std::fs::read_to_string(&file).is_ok_and(|t| t == text) {
            return Ok(());
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        crate::paths::write_atomic(&file, text.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
mod autostart {
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "D.E.V. Launcher";

    pub fn set(program: Option<&Path>) -> Result<(), String> {
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, KEY_READ | KEY_WRITE).map_err(|e| e.to_string())?;
        match program {
            Some(p) => {
                let command = format!("\"{}\" --tray", p.display());
                if key.get_value::<String, _>(VALUE).is_ok_and(|v| v == command) {
                    return Ok(());
                }
                key.set_value(VALUE, &command).map_err(|e| e.to_string())
            }
            None => match key.delete_value(VALUE) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            },
        }
    }
}

#[cfg(target_os = "linux")]
mod autostart {
    use std::path::{Path, PathBuf};

    /// The XDG autostart entry every desktop (GNOME, KDE, Xfce, …) reads at login.
    fn entry() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("autostart/dev-launcher.desktop"))
    }

    /// Quote an `Exec=` argument as the Desktop Entry spec asks.
    fn quote(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }

    pub fn set(program: Option<&Path>) -> Result<(), String> {
        let file = entry().ok_or("no config folder")?;
        let Some(program) = program else {
            return match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        };
        let path = program.to_string_lossy();
        // `TryExec` skips the entry once the program is gone (a removed package, a deleted AppImage).
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=D.E.V. Launcher\nComment=Quick access to the DraftCraft apps and their updates\nTryExec={path}\nExec={} --tray\nIcon=dev-launcher\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\nX-KDE-autostart-after=panel\n",
            quote(&path)
        );
        if std::fs::read_to_string(&file).is_ok_and(|t| t == text) {
            return Ok(());
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        crate::paths::write_atomic(&file, text.as_bytes()).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn exec_quoting() {
            assert_eq!(super::quote("/home/a b/$x\"y"), "\"/home/a b/\\$x\\\"y\"");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for cmd in [Cmd::Show(None), Cmd::Show(Some("draftcraft".into())), Cmd::Refresh, Cmd::Ping, Cmd::Quit] {
            assert_eq!(Cmd::decode(&cmd.encode()), Some(cmd));
        }
        assert_eq!(Cmd::decode("something else"), None);
        assert_eq!(Cmd::decode("dev-launcher show ../../x"), Some(Cmd::Show(None)));
    }

    #[test]
    fn ports_differ() {
        assert_ne!(port(Endpoint::Window), port(Endpoint::Tray));
        assert!(port(Endpoint::Window) >= 47_100);
    }
}
