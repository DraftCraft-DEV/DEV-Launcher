//! The menu bar icon (macOS), notification area icon (Windows) or system tray icon (Linux):
//! `dev-launcher --tray`. It lists the installed apps to open them in one click, and keeps
//! checking for updates (installing them when the setting asks) while the window is closed.
//!
//! The icon runs on a winit event loop on the main thread; a worker thread does the slow parts
//! (finding installs, asking GitHub, installing) and tells the loop when the menu should change.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tray_icon::menu::{Icon as MenuIcon, IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::agent::{self, Cmd, Endpoint};
use crate::catalog::Catalog;
use crate::i18n::{self, tr, trf};
use crate::install::{self, Stage, Target};
use crate::platform::Platform;
use crate::settings::Settings;
use crate::ui::AppEntry;
use crate::{assets, detect, net, system};

/// How often installs are looked for again (they can change outside the launcher).
const DETECT_EVERY: Duration = Duration::from_secs(30);
/// How often releases are checked when the setting asks.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// How soon a check that failed (no network yet after signing in, say) is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

const MENU_ICON: u32 = 32;

#[derive(Debug)]
enum Event {
    Menu(String),
    Tray(TrayIconEvent),
    Remote(Cmd),
    /// The worker changed the state: rebuild the menu.
    Changed,
}

enum Work {
    Refresh,
    Check,
    Update(String),
}

/// An update the tray is installing.
struct Busy {
    name: String,
    percent: Option<u8>,
}

struct State {
    platform: Platform,
    settings: Settings,
    apps: Vec<AppEntry>,
    checking: bool,
    busy: Option<Busy>,
}

impl State {
    fn updates(&self) -> Vec<(&AppEntry, String)> {
        self.apps
            .iter()
            .filter(|a| !a.def.coming_soon)
            .filter_map(|a| a.update(&self.platform, self.settings.prereleases).map(|r| (a, r.version_label())))
            .collect()
    }
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Run the icon until "Quit" (or until the window's settings turn it off). Returns the exit code.
pub fn run() -> i32 {
    let settings = Settings::load();
    i18n::set(settings.language);

    let mut builder = EventLoop::<Event>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // No Dock icon and no menu of its own: only the status item.
        builder.with_activation_policy(ActivationPolicy::Accessory).with_default_menu(false);
    }
    let event_loop = match builder.build() {
        Ok(l) => l,
        Err(e) => {
            log::error!("tray: no event loop ({e})");
            return 1;
        }
    };
    let proxy = event_loop.create_proxy();

    // Holding the port makes this the only tray; a second one just leaves.
    let remote = Mutex::new(proxy.clone());
    if agent::listen(Endpoint::Tray, move |cmd| send(&remote, Event::Remote(cmd))).is_none() {
        log::info!("tray: already running");
        return 0;
    }
    let menu_proxy = Mutex::new(proxy.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| send(&menu_proxy, Event::Menu(e.id.0))));
    let tray_proxy = Mutex::new(proxy.clone());
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| send(&tray_proxy, Event::Tray(e))));

    let platform = Platform::detect();
    let apps = Catalog::load().apps.into_iter().map(AppEntry::new).collect();
    let state: Shared = Arc::new(Mutex::new(State { platform, settings, apps, checking: false, busy: None }));
    let (work_tx, work_rx) = mpsc::channel();
    {
        let (state, proxy) = (state.clone(), proxy.clone());
        let spawned = std::thread::Builder::new().name("tray-worker".into()).spawn(move || worker(&state, &work_rx, &proxy));
        if let Err(e) = spawned {
            log::error!("tray: no worker ({e})");
            return 1;
        }
    }
    log::info!("tray started (D.E.V. Launcher {})", env!("CARGO_PKG_VERSION"));

    let mut app = TrayApp::new(state, work_tx);
    event_loop.set_control_flow(ControlFlow::Wait);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("tray: {e}");
        return 1;
    }
    0
}

fn send(proxy: &Mutex<EventLoopProxy<Event>>, event: Event) {
    if let Ok(p) = proxy.lock() {
        let _ = p.send_event(event);
    }
}

// ---------------------------------------------------------------------------------------------
// The worker

fn worker(state: &Shared, rx: &Receiver<Work>, proxy: &EventLoopProxy<Event>) {
    let changed = || {
        let _ = proxy.send_event(Event::Changed);
    };
    let mut last_detect: Option<Instant> = None;
    // When releases were last fetched, and whether that worked.
    let mut last_check: Option<(Instant, bool)> = None;
    // Automatic updates that failed, not to be retried every few seconds.
    let mut failed: HashSet<(String, String)> = HashSet::new();
    let mut work = Some(Work::Refresh);
    loop {
        let settings = Settings::load();
        i18n::set(settings.language);
        lock(state).settings = settings.clone();

        let refresh = matches!(work, Some(Work::Refresh));
        if refresh || last_detect.is_none_or(|t| t.elapsed() >= DETECT_EVERY) {
            detect_all(state);
            last_detect = Some(Instant::now());
            changed();
        }

        let check_due = match last_check {
            None => true,
            Some((at, true)) => settings.check_on_start && at.elapsed() >= CHECK_EVERY,
            Some((at, false)) => at.elapsed() >= RETRY_AFTER,
        };
        if matches!(work, Some(Work::Check)) || check_due {
            lock(state).checking = true;
            changed();
            let ok = check_all(state);
            last_check = Some((Instant::now(), ok));
            lock(state).checking = false;
            changed();
        }

        match work.take() {
            Some(Work::Update(app)) => {
                if let Err(e) = update(state, &app, &changed) {
                    log::error!("tray: updating {app}: {e}");
                }
            }
            _ => auto_update(state, &settings, &mut failed, &changed),
        }

        work = match rx.recv_timeout(DETECT_EVERY) {
            Ok(w) => Some(w),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
    }
}

fn detect_all(state: &Shared) {
    let (defs, platform) = {
        let s = lock(state);
        (s.apps.iter().filter(|a| !a.def.coming_soon).map(|a| a.def.clone()).collect::<Vec<_>>(), s.platform.clone())
    };
    for def in defs {
        let installs = detect::detect(&def, &platform);
        if let Some(a) = lock(state).apps.iter_mut().find(|a| a.def.id == def.id) {
            a.installs = installs;
            a.detected = true;
        }
    }
}

/// Fetch the catalog and every app's releases; false when GitHub couldn't be reached.
fn check_all(state: &Shared) -> bool {
    if let Some(catalog) = Catalog::fetch() {
        let mut s = lock(state);
        let mut old = std::mem::take(&mut s.apps);
        s.apps = catalog
            .apps
            .into_iter()
            .map(|def| match old.iter().position(|a| a.def.id == def.id) {
                Some(i) => {
                    let mut e = old.remove(i);
                    e.def = def;
                    e
                }
                None => AppEntry::new(def),
            })
            .collect();
    }
    let defs: Vec<_> = lock(state).apps.iter().filter(|a| !a.def.coming_soon).map(|a| a.def.clone()).collect();
    let mut ok = true;
    for def in defs {
        let result = net::releases(&def.repo);
        if let Err(e) = &result {
            log::warn!("tray: releases of {}: {e}", def.name);
            ok = false;
        }
        if let Some(a) = lock(state).apps.iter_mut().find(|a| a.def.id == def.id) {
            // Keep what was known when the network is down.
            if result.is_ok() || a.releases.is_none() {
                a.releases = Some(result);
            }
        }
    }
    detect_all(state);
    ok
}

/// Plans that would ask for a password or administrator permission: not started from the
/// background without the user looking.
fn needs_permission(target: &Target) -> bool {
    matches!(target, Target::Package(_) | Target::WinSetup { for_everyone: true })
}

fn auto_update(state: &Shared, settings: &Settings, failed: &mut HashSet<(String, String)>, changed: &dyn Fn()) {
    // The window does its own automatic updates while it's open.
    if !settings.auto_update || agent::is_running(Endpoint::Window) {
        return;
    }
    let due: Vec<(String, String)> = {
        let s = lock(state);
        s.updates().into_iter().map(|(a, v)| (a.def.id.clone(), v)).filter(|key| !failed.contains(key)).collect()
    };
    for (app, version) in due {
        let (def, installed) = {
            let s = lock(state);
            let Some(a) = s.apps.iter().find(|a| a.def.id == app) else { continue };
            let Some(i) = a.primary() else { continue };
            (a.def.clone(), i.clone())
        };
        // "Only while the app is closed."
        if !system::running(&def, &installed).is_empty() {
            continue;
        }
        log::info!("tray: updating {} to {version} automatically", def.name);
        if let Err(e) = update(state, &app, changed) {
            log::error!("tray: {e}");
            failed.insert((app, version));
        }
    }
}

/// Update `app` to the newest release, reporting progress in the menu.
fn update(state: &Shared, app: &str, changed: &dyn Fn()) -> Result<(), String> {
    let (def, release, plan, installed) = {
        let s = lock(state);
        let a = s.apps.iter().find(|a| a.def.id == app).ok_or("unknown app")?;
        let installed = a.primary().ok_or("not installed")?.clone();
        let release = a.update(&s.platform, s.settings.prereleases).ok_or("no update")?.clone();
        let plan = install::update_plan(&a.def, &s.platform, &release, &installed)?;
        (a.def.clone(), release, plan, installed)
    };
    if needs_permission(&plan.target) {
        // The window shows what's happening and asks for the password.
        agent::show_window(Some(app));
        return Ok(());
    }
    lock(state).busy = Some(Busy { name: def.name.clone(), percent: None });
    changed();
    let stop = AtomicBool::new(false);
    let mut shown = None;
    let mut report = |stage: Stage| {
        if let Stage::Downloading(t) = stage {
            let pct = (t.total > 0).then(|| (t.done * 100 / t.total).min(100) as u8);
            // Every 5% is plenty for a menu.
            let step = pct.map(|p| p / 5);
            if step != shown {
                shown = step;
                if let Some(b) = lock(state).busy.as_mut() {
                    b.percent = pct;
                }
                changed();
            }
        }
    };
    let result = install::execute(&def, &release, &plan, Some(&installed), &mut install::Ctx { stop: &stop, report: &mut report });
    lock(state).busy = None;
    detect_all(state);
    changed();
    result.map(|o| log::info!("tray: {} {} installed", def.name, o.version))
}

// ---------------------------------------------------------------------------------------------
// The icon and its menu

struct TrayApp {
    state: Shared,
    work: Sender<Work>,
    tray: Option<TrayIcon>,
    /// What the shown menu says, to rebuild it only when that changes.
    shown: String,
    badged: bool,
    app_icons: HashMap<String, Option<Vec<u8>>>,
}

/// The icon: a template image of the mark on macOS (the system tints it), the app icon elsewhere.
fn base_icon() -> ([usize; 2], Vec<u8>) {
    let png = if cfg!(target_os = "macos") { assets::TRAY_MACOS } else { assets::APP_ICON_64 };
    assets::decode_png(png).unwrap_or(([1, 1], vec![0; 4]))
}

/// The icon with a dot in the top-right corner: an update is ready.
fn badge(size: [usize; 2], mut rgba: Vec<u8>) -> Vec<u8> {
    let [w, h] = size;
    let r = w as f32 * 0.2;
    let (cx, cy) = (w as f32 - r - 0.5, r + 0.5);
    let template = cfg!(target_os = "macos");
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let i = (y * w + x) * 4;
            if d <= r {
                // Template images only use alpha; elsewhere the brand lime.
                let c = if template { [0, 0, 0, 255] } else { [0xB6, 0xEE, 0x2B, 255] };
                rgba[i..i + 4].copy_from_slice(&c);
            } else if d <= r + w as f32 * 0.08 {
                // A clear ring keeps the dot apart from the mark.
                rgba[i + 3] = 0;
            }
        }
    }
    rgba
}

fn tray_icon(badged: bool) -> Option<Icon> {
    let (size, rgba) = base_icon();
    let rgba = if badged { badge(size, rgba) } else { rgba };
    Icon::from_rgba(rgba, size[0] as u32, size[1] as u32).ok()
}

/// An app's icon, small enough for a menu.
fn menu_icon_rgba(id: &str) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(assets::app_icon_png(id)?, image::ImageFormat::Png).ok()?;
    Some(img.resize_exact(MENU_ICON, MENU_ICON, image::imageops::FilterType::Lanczos3).to_rgba8().into_raw())
}

/// One menu line: its id (empty when it does nothing), its text and whether it can be clicked.
struct Line {
    id: String,
    text: String,
    enabled: bool,
    icon_of: Option<String>,
}

fn line(id: impl Into<String>, text: impl Into<String>, enabled: bool) -> Line {
    Line { id: id.into(), text: text.into(), enabled, icon_of: None }
}

impl TrayApp {
    fn new(state: Shared, work: Sender<Work>) -> Self {
        Self { state, work, tray: None, shown: String::new(), badged: false, app_icons: HashMap::new() }
    }

    /// The menu as lines, groups separated by `None`.
    fn lines(&self) -> Vec<Option<Line>> {
        let s = lock(&self.state);
        let mut out = Vec::new();
        let apps: Vec<&AppEntry> = s.apps.iter().filter(|a| !a.def.coming_soon).collect();
        for a in &apps {
            if a.primary().is_some() {
                out.push(Some(Line { icon_of: Some(a.def.id.clone()), ..line(format!("open:{}", a.def.id), a.def.name.clone(), true) }));
            }
        }
        for a in &apps {
            if a.detected && a.primary().is_none() {
                out.push(Some(line(format!("install:{}", a.def.id), format!("{}…", trf("Install {}", &a.def.name)), true)));
            }
        }
        out.push(None);
        if let Some(b) = &s.busy {
            let pct = b.percent.map(|p| format!(" {p}%")).unwrap_or_default();
            out.push(Some(line("", format!("{}{pct}", trf("Updating {}…", &b.name)), false)));
            out.push(None);
        } else {
            let updates = s.updates();
            for (a, version) in &updates {
                out.push(Some(line(format!("update:{}", a.def.id), format!("{} — {}", a.def.name, trf("Update to {}", version)), true)));
            }
            if !updates.is_empty() {
                out.push(None);
            }
        }
        out.push(Some(line("window", tr("Open D.E.V. Launcher"), true)));
        out.push(Some(if s.checking { line("", tr("Checking…"), false) } else { line("check", tr("Check for updates"), true) }));
        out.push(None);
        out.push(Some(line("quit", tr("Quit D.E.V. Launcher"), true)));
        out
    }

    fn menu_icon(&mut self, id: &str) -> Option<MenuIcon> {
        let rgba = self.app_icons.entry(id.to_string()).or_insert_with(|| menu_icon_rgba(id)).clone()?;
        MenuIcon::from_rgba(rgba, MENU_ICON, MENU_ICON).ok()
    }

    /// Rebuild the menu (and the icon's dot) when what they show changed.
    fn refresh_menu(&mut self) {
        let lines = self.lines();
        let signature: String = lines
            .iter()
            .map(|l| l.as_ref().map_or_else(|| "--".to_string(), |l| format!("{}|{}|{}", l.id, l.text, l.enabled)))
            .collect::<Vec<_>>()
            .join("\n");
        let badged = {
            let s = lock(&self.state);
            s.busy.is_none() && !s.updates().is_empty()
        };
        if badged != self.badged {
            self.badged = badged;
            if let Some(t) = &self.tray {
                let icon = tray_icon(badged);
                let _ = if cfg!(target_os = "macos") { t.set_icon_templated(icon) } else { t.set_icon(icon) };
                let tip = if badged { format!("D.E.V. Launcher — {}", tr("Update available")) } else { "D.E.V. Launcher".into() };
                let _ = t.set_tooltip(Some(tip));
            }
        }
        if signature == self.shown && self.tray.is_some() {
            return;
        }
        self.shown = signature;
        let menu = Menu::new();
        // No separator first or twice in a row (when a group is empty).
        let mut after_separator = true;
        for l in lines {
            let appended = match l {
                None if after_separator => continue,
                None => {
                    after_separator = true;
                    menu.append(&PredefinedMenuItem::separator())
                }
                Some(l) => {
                    after_separator = false;
                    match l.icon_of.as_deref().and_then(|id| self.menu_icon(id)) {
                        Some(icon) => menu.append(&IconMenuItem::with_id(l.id, l.text, l.enabled, Some(icon), None)),
                        None => menu.append(&MenuItem::with_id(l.id, l.text, l.enabled, None)),
                    }
                }
            };
            if let Err(e) = appended {
                log::warn!("tray menu: {e}");
            }
        }
        match &self.tray {
            Some(t) => t.set_menu(Some(Box::new(menu))),
            None => self.create(menu),
        }
    }

    fn create(&mut self, menu: Menu) {
        let mut builder = TrayIconBuilder::new().with_id("dev-launcher").with_tooltip("D.E.V. Launcher").with_menu(Box::new(menu));
        if let Some(icon) = tray_icon(false) {
            builder = if cfg!(target_os = "macos") { builder.with_icon_templated(icon) } else { builder.with_icon(icon) };
        }
        if cfg!(windows) {
            // Windows' convention: a click opens the app, a right-click shows the menu.
            builder = builder.with_menu_on_left_click(false);
        }
        match builder.build() {
            Ok(t) => self.tray = Some(t),
            Err(e) => log::error!("tray: couldn't show the icon ({e})"),
        }
    }

    fn on_menu(&mut self, id: &str, event_loop: &ActiveEventLoop) {
        match id {
            "window" => agent::show_window(None),
            "check" => {
                let _ = self.work.send(Work::Check);
            }
            "quit" => self.quit(event_loop),
            _ => {
                if let Some(app) = id.strip_prefix("open:") {
                    self.open(app);
                } else if let Some(app) = id.strip_prefix("install:") {
                    agent::show_window(Some(app));
                } else if let Some(app) = id.strip_prefix("update:") {
                    let _ = self.work.send(Work::Update(app.to_string()));
                }
            }
        }
    }

    fn open(&self, app: &str) {
        let found = {
            let s = lock(&self.state);
            s.apps.iter().find(|a| a.def.id == app).and_then(|a| a.primary().map(|i| (a.def.clone(), i.clone())))
        };
        let Some((def, installed)) = found else { return };
        if let Err(e) = system::launch(&def, &installed) {
            log::error!("tray: {e}");
            agent::show_window(Some(app));
        }
    }

    fn quit(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("tray: quit");
        self.tray = None;
        event_loop.exit();
    }
}

impl ApplicationHandler<Event> for TrayApp {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // macOS wants the status item made once the loop runs, not before.
        if cause == StartCause::Init {
            self.refresh_menu();
        }
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::Changed => self.refresh_menu(),
            Event::Menu(id) => self.on_menu(&id, event_loop),
            Event::Tray(TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) if cfg!(windows) => {
                agent::show_window(None);
            }
            Event::Tray(_) => {}
            Event::Remote(Cmd::Refresh) => {
                let _ = self.work.send(Work::Refresh);
            }
            Event::Remote(Cmd::Quit) => self.quit(event_loop),
            Event::Remote(Cmd::Show(_) | Cmd::Ping) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_draws_a_dot_in_the_corner() {
        let size = [32, 32];
        let out = badge(size, vec![0; 32 * 32 * 4]);
        let alpha = |x: usize, y: usize| out[(y * 32 + x) * 4 + 3];
        assert_eq!(alpha(25, 6), 255);
        assert_eq!(alpha(4, 28), 0);
    }
}
