//! D.E.V. Launcher — installs, updates and opens the apps of DraftCraft Engineering Ventures.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod assets;
mod catalog;
mod detect;
mod i18n;
mod install;
mod jobs;
mod net;
mod paths;
mod platform;
mod selfupdate;
mod settings;
mod setup;
mod system;
mod tray;
mod ui;
#[cfg(windows)]
mod winshell;

use std::io::Write;
use std::sync::Mutex;

use settings::{LangPref, ThemePref};
use ui::setup::Preview;
use ui::{Args, Page};

/// Logs to stderr and to the log file (rotated when it grows past 1 MB).
struct Logger {
    file: Mutex<Option<std::fs::File>>,
}

impl log::Log for Logger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        if meta.target().starts_with("dev_launcher") { meta.level() <= log::Level::Debug } else { meta.level() <= log::Level::Warn }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = format!("{secs} {:<5} {}\n", record.level(), record.args());
        eprint!("{line}");
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
    }

    fn flush(&self) {}
}

fn init_logging() {
    let path = paths::log_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok();
    let logger = Box::leak(Box::new(Logger { file: Mutex::new(file) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

/// Everything on the command line.
#[derive(Default)]
struct Cli {
    window: Args,
    /// Show the setup screen (`--setup`, `--uninstall`).
    setup: Option<setup::Mode>,
    /// Install without a window (`--install`).
    install: bool,
    everyone: bool,
    desktop_shortcut: bool,
    /// Uninstall without a window (`--uninstall --quiet`).
    quiet: bool,
    remove_data: bool,
    preview: Option<Preview>,
    /// Run the menu bar / tray icon instead of the window (`--tray`).
    tray: bool,
    /// Started by the previous version after it updated itself: wait for it to quit first.
    updated: bool,
}

fn parse_args() -> Result<Cli, String> {
    let mut cli = Cli::default();
    let args = &mut cli.window;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--setup" => cli.setup = Some(setup::Mode::Install),
            "--uninstall" => cli.setup = Some(setup::Mode::Uninstall),
            "--install" => cli.install = true,
            "--everyone" => cli.everyone = true,
            "--desktop-shortcut" => cli.desktop_shortcut = true,
            "--quiet" => cli.quiet = true,
            "--remove-data" => cli.remove_data = true,
            "--tray" => cli.tray = true,
            "--updated" => cli.updated = true,
            "--app" => args.app = Some(it.next().ok_or("--app needs an app id")?),
            "--setup-preview" => {
                cli.preview = Some(match it.next().as_deref() {
                    Some("ask") => Preview::Ask,
                    Some("working") => Preview::Working,
                    Some("done") => Preview::Done,
                    Some("failed") => Preview::Failed,
                    other => return Err(format!("unknown preview {other:?}")),
                })
            }
            "--version" | "-V" => {
                println!("dev-launcher {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "D.E.V. Launcher {}\n\nOptions:\n  --screenshot <file.png>   save a picture of the window and quit\n  --page <library|downloads|settings>\n  --theme <dark|light|system>\n  --lang <en|it>\n  --app <id>                show this app\n  --tray                    run the menu bar / tray icon (started at login)\n  --setup | --uninstall        show the install / uninstall screen (Windows, Linux)\n  --install [--everyone] [--desktop-shortcut]   install without a window\n  --uninstall --quiet [--remove-data]          uninstall without a window\n  --version",
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            "--screenshot" => args.screenshot = Some(it.next().ok_or("--screenshot needs a file")?.into()),
            "--page" => {
                args.page = Some(match it.next().as_deref() {
                    Some("library") => Page::Library,
                    Some("downloads") => Page::Downloads,
                    Some("settings") => Page::Settings,
                    other => return Err(format!("unknown page {other:?}")),
                })
            }
            "--theme" => {
                args.theme = Some(match it.next().as_deref() {
                    Some("dark") => ThemePref::Dark,
                    Some("light") => ThemePref::Light,
                    Some("system") => ThemePref::System,
                    other => return Err(format!("unknown theme {other:?}")),
                })
            }
            "--lang" => {
                args.lang = Some(match it.next().as_deref() {
                    Some("en") => LangPref::English,
                    Some("it") => LangPref::Italian,
                    other => return Err(format!("unknown language {other:?}")),
                })
            }
            // macOS adds this when started from Finder.
            s if s.starts_with("-psn_") => {}
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(cli)
}

fn main() -> eframe::Result {
    let cli = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("dev-launcher: {e}");
            std::process::exit(2);
        }
    };
    init_logging();

    // Headless setup, for scripts and for the elevated half of a machine-wide install.
    if cli.install || (cli.quiet && cli.setup == Some(setup::Mode::Uninstall)) {
        let result = if cli.install {
            setup::install(setup::Options { everyone: cli.everyone, desktop_shortcut: cli.desktop_shortcut }).map(|_| ())
        } else {
            setup::uninstall(cli.remove_data)
        };
        if let Err(e) = &result {
            log::error!("setup: {e}");
        }
        std::process::exit(i32::from(result.is_err()));
    }
    if cli.tray {
        if cli.updated {
            selfupdate::wait_for_old(agent::Endpoint::Tray);
        }
        std::process::exit(tray::run());
    }
    // macOS has no setup: the DMG is drag and drop, and the Trash uninstalls.
    // (`--setup-preview` still opens the screen, with Windows' options, for the docs.)
    if cfg!(target_os = "macos") && cli.setup.is_some() && cli.preview.is_none() {
        println!("On macOS, drag D.E.V. Launcher from the disk image into Applications; move it to the Trash to remove it.");
        std::process::exit(0);
    }
    if let Some(mode) = setup::mode_for_this_start(cli.setup) {
        return run_setup(mode, cli);
    }
    let args = cli.window;
    if cli.updated {
        selfupdate::wait_for_old(agent::Endpoint::Window);
    }
    // One window: a second start brings the first one forward.
    if args.screenshot.is_none() && agent::send(agent::Endpoint::Window, &agent::Cmd::Show(args.app.clone())) {
        return Ok(());
    }
    if args.screenshot.is_none() {
        // No older window runs now, so what earlier updates left can go.
        std::thread::spawn(selfupdate::clean_leftovers);
    }

    let icon = app_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("D.E.V. Launcher")
        .with_app_id("dev.skryx.devlauncher")
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([900.0, 600.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    if platform::Os::current() == platform::Os::MacOs {
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("D.E.V. Launcher", options, Box::new(|cc| Ok(Box::new(ui::Launcher::new(cc, args)))))
}

fn app_icon() -> Option<egui::IconData> {
    assets::decode_png(assets::APP_ICON_256).map(|([w, h], rgba)| egui::IconData { rgba, width: w as u32, height: h as u32 })
}

/// The install / uninstall window: small, borderless, centred.
fn run_setup(mode: setup::Mode, cli: Cli) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(if mode == setup::Mode::Install { "D.E.V. Launcher Setup" } else { "Uninstall D.E.V. Launcher" })
        .with_app_id("dev.skryx.devlauncher.setup")
        .with_inner_size([780.0, 500.0])
        .with_resizable(false)
        .with_decorations(false);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    let (shot, preview, lang) = (cli.window.screenshot, cli.preview, cli.window.lang);
    eframe::run_native("D.E.V. Launcher Setup", options, Box::new(move |cc| Ok(Box::new(ui::setup::SetupApp::new(cc, mode, lang, shot, preview)))))
}
