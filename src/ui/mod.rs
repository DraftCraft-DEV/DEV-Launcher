//! The launcher window: a sidebar with the pages and apps, and the selected page.

mod library;
mod pages;
pub mod setup;
pub mod theme;
pub mod widgets;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Align, Color32, CornerRadius, Layout, Margin, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2};

use crate::catalog::{AppDef, Catalog};
use crate::detect::Installed;
use crate::i18n::{self, tr, trf};
use crate::install::{self, Plan};
use crate::jobs::{self, Action, Inbox, Job, Msg, Status};
use crate::net::Release;
use crate::platform::{Os, Platform};
use crate::settings::{LangPref, Settings, ThemePref};
use crate::{agent, assets, system};
use theme::{Palette, medium, regular, semibold};
use widgets::{Icon, icon};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Library,
    Downloads,
    Settings,
}

/// Command-line options for the window (screenshots for the docs and for checking the UI).
#[derive(Default)]
pub struct Args {
    pub screenshot: Option<PathBuf>,
    pub page: Option<Page>,
    pub theme: Option<ThemePref>,
    pub lang: Option<LangPref>,
    /// Show this app first (`--app`, used by the tray).
    pub app: Option<String>,
}

/// An app of the catalog and what is known about it here.
pub struct AppEntry {
    pub def: AppDef,
    pub installs: Vec<Installed>,
    pub detected: bool,
    pub running: bool,
    pub releases: Option<Result<Vec<Release>, String>>,
    pub checking: bool,
}

impl AppEntry {
    pub fn new(def: AppDef) -> Self {
        Self { def, installs: Vec::new(), detected: false, running: false, releases: None, checking: false }
    }

    pub fn primary(&self) -> Option<&Installed> {
        self.installs.first()
    }

    /// Releases with a package for this computer, newest first.
    pub fn usable_releases(&self, platform: &Platform, prereleases: bool) -> Vec<&Release> {
        let Some(Ok(list)) = &self.releases else { return Vec::new() };
        list.iter()
            .filter(|r| prereleases || !r.prerelease)
            .filter(|r| r.assets.iter().any(|a| crate::platform::parse_asset(&self.def.asset_prefix, &a.name).is_some_and(|p| p.os == platform.os && platform.arch_rank(p.arch).is_some())))
            .collect()
    }

    pub fn latest(&self, platform: &Platform, prereleases: bool) -> Option<&Release> {
        self.usable_releases(platform, prereleases).into_iter().next()
    }

    /// The newer release the primary copy can update to.
    pub fn update(&self, platform: &Platform, prereleases: bool) -> Option<&Release> {
        let installed = self.primary()?;
        let latest = self.latest(platform, prereleases)?;
        let newer = matches!((&installed.version, latest.version()), (Some(have), Some(new)) if new > *have);
        (newer && install::update_plan(&self.def, platform, latest, installed).is_ok()).then_some(latest)
    }
}

struct Textures {
    mark: [TextureHandle; 2],
    wordmark: [TextureHandle; 2],
    apps: HashMap<String, TextureHandle>,
}

impl Textures {
    fn load(ctx: &egui::Context) -> Self {
        let tex = |name: &str, png: &[u8]| {
            let (size, rgba) = assets::decode_png(png).unwrap_or(([1, 1], vec![0; 4]));
            ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, &rgba), egui::TextureOptions::LINEAR)
        };
        Self {
            mark: [tex("mark-dark", assets::MARK_DARK), tex("mark-light", assets::MARK_LIGHT)],
            wordmark: [tex("wordmark-dark", assets::WORDMARK_DARK), tex("wordmark-light", assets::WORDMARK_LIGHT)],
            apps: HashMap::new(),
        }
    }

    fn app(&mut self, ctx: &egui::Context, id: &str) -> Option<TextureHandle> {
        if let Some(t) = self.apps.get(id) {
            return Some(t.clone());
        }
        let (size, rgba) = assets::decode_png(assets::app_icon_png(id)?)?;
        let t = ctx.load_texture(format!("app-{id}"), egui::ColorImage::from_rgba_unmultiplied(size, &rgba), egui::TextureOptions::LINEAR);
        self.apps.insert(id.into(), t.clone());
        Some(t)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Error,
    Info,
}

struct Toast {
    title: String,
    body: String,
    kind: ToastKind,
    until: f64,
}

/// Something the UI asked for; applied after drawing.
pub enum UiAction {
    Select(usize),
    Page(Page),
    Launch(usize),
    Install { app: usize, plan: Plan, release: Release },
    Update(usize),
    Cancel(u64),
    AskUninstall(usize, usize),
    Uninstall(usize, usize),
    Reveal(PathBuf),
    OpenUrl(String),
    CheckUpdates,
    Restart(usize),
    ClearFinished,
    ClearCache,
    SettingsChanged,
}

/// Saves one picture of the window and closes it (`--screenshot`): for the docs and for checking
/// the UI without screen-recording permission.
pub struct Screenshot {
    path: PathBuf,
    start: Option<f64>,
    requested: bool,
    done: Arc<AtomicBool>,
}

impl Screenshot {
    pub fn new(path: PathBuf) -> Self {
        Self { path, start: None, requested: false, done: Arc::default() }
    }

    /// Call every frame; `ready` once the content has loaded (gives up waiting after 20 s).
    pub fn step(&mut self, ctx: &egui::Context, ready: bool) {
        if self.done.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let now = ctx.input(|i| i.time);
        let waited = now - *self.start.get_or_insert(now);
        ctx.request_repaint();
        if self.requested || !(ready || waited > 20.0) || waited < 1.5 {
            return;
        }
        self.requested = true;
        let (path, done, ctx2) = (self.path.clone(), self.done.clone(), ctx.clone());
        ctx.request_screenshot(move |img| {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
            match image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba).map(|i| i.save(&path)) {
                Some(Ok(())) => eprintln!("screenshot saved to {}", path.display()),
                other => eprintln!("screenshot failed: {other:?}"),
            }
            done.store(true, Ordering::Relaxed);
            ctx2.request_repaint();
        });
    }
}

pub struct Launcher {
    pub platform: Platform,
    pub settings: Settings,
    pub apps: Vec<AppEntry>,
    pub selected: usize,
    pub page: Page,
    pub jobs: Vec<Job>,
    inbox: Inbox,
    textures: Textures,
    toasts: Vec<Toast>,
    pub palette: Palette,
    applied: Option<Palette>,
    last_check: f64,
    last_detect: f64,
    detect_at: Option<f64>,
    was_focused: bool,
    confirm: Option<(usize, usize)>,
    /// Chosen install format per app (index into the install plans).
    pub format_choice: HashMap<String, usize>,
    actions: Vec<UiAction>,
    shot: Option<Screenshot>,
    theme_override: Option<ThemePref>,
}

const CHECK_EVERY: f64 = 60.0 * 60.0;
const DETECT_EVERY: f64 = 20.0;

impl Launcher {
    pub fn new(cc: &eframe::CreationContext, args: Args) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        let settings = Settings::load();
        i18n::set(args.lang.unwrap_or(settings.language));
        let catalog = Catalog::load();
        let platform = Platform::detect();
        log::info!("D.E.V. Launcher {} on {}", env!("CARGO_PKG_VERSION"), platform.label());
        let apps: Vec<AppEntry> = catalog.apps.into_iter().map(AppEntry::new).collect();
        let first = args.app.as_ref().or(settings.last_app.as_ref());
        let selected = first.and_then(|id| apps.iter().position(|a| &a.def.id == id)).unwrap_or(0);
        let mut me = Self {
            inbox: Inbox::new(ctx.clone()),
            textures: Textures::load(&ctx),
            platform,
            apps,
            selected,
            page: args.page.unwrap_or(Page::Library),
            jobs: Vec::new(),
            toasts: Vec::new(),
            palette: Palette::dark(),
            applied: None,
            last_check: 0.0,
            last_detect: 0.0,
            detect_at: None,
            was_focused: true,
            confirm: None,
            format_choice: HashMap::new(),
            actions: Vec::new(),
            shot: args.screenshot.map(Screenshot::new),
            theme_override: args.theme,
            settings,
        };
        // Screenshots for the docs leave the tray and the login item alone.
        if me.shot.is_none() {
            let inbox = me.inbox.clone();
            agent::listen(agent::Endpoint::Window, move |cmd| inbox.post(Msg::Remote(cmd)));
            me.apply_background();
        }
        jobs::catalog(&me.inbox);
        me.detect_all();
        // The release list is always fetched once; the setting decides about later checks.
        me.check_all();
        me
    }

    /// Start (or stop) the tray and the login item to match the settings.
    fn apply_background(&self) {
        let settings = self.settings.clone();
        std::thread::spawn(move || {
            agent::apply(&settings);
            agent::send(agent::Endpoint::Tray, &agent::Cmd::Refresh);
        });
    }

    /// Tell the tray installs changed.
    fn notify_tray(&self) {
        if self.shot.is_none() {
            std::thread::spawn(|| agent::send(agent::Endpoint::Tray, &agent::Cmd::Refresh));
        }
    }

    fn detect_all(&mut self) {
        for a in &self.apps {
            if !a.def.coming_soon {
                jobs::detect(&self.inbox, a.def.clone(), self.platform.clone());
            }
        }
    }

    fn check_all(&mut self) {
        for a in &mut self.apps {
            if !a.def.coming_soon && !a.checking {
                a.checking = true;
                jobs::releases(&self.inbox, a.def.clone());
            }
        }
    }

    pub fn job_for(&self, app_id: &str) -> Option<&Job> {
        self.jobs.iter().rev().find(|j| j.app == app_id && j.is_running())
    }

    fn handle_msgs(&mut self, now: f64) {
        for msg in self.inbox.drain() {
            match msg {
                Msg::Catalog(c) => self.merge_catalog(c),
                Msg::Detected { app, installs, running } => {
                    if let Some(a) = self.apps.iter_mut().find(|a| a.def.id == app) {
                        a.installs = installs;
                        a.running = running;
                        a.detected = true;
                    }
                    self.auto_update(&app);
                }
                Msg::Releases { app, result } => {
                    if let Some(a) = self.apps.iter_mut().find(|a| a.def.id == app) {
                        if let Err(e) = &result {
                            log::warn!("releases of {}: {e}", a.def.name);
                        }
                        a.releases = Some(result);
                        a.checking = false;
                    }
                    self.auto_update(&app);
                }
                Msg::Finished { job } => self.job_finished(job, now),
                Msg::Remote(cmd) => self.remote(cmd),
            }
        }
    }

    fn remote(&mut self, cmd: agent::Cmd) {
        let ctx = self.inbox.ctx().clone();
        match cmd {
            agent::Cmd::Show(app) => {
                if let Some(i) = app.and_then(|id| self.apps.iter().position(|a| a.def.id == id)) {
                    self.selected = i;
                    self.page = Page::Library;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.detect_all();
            }
            agent::Cmd::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            agent::Cmd::Refresh => self.detect_all(),
            agent::Cmd::Ping => {}
        }
    }

    fn merge_catalog(&mut self, catalog: Catalog) {
        let selected_id = self.apps.get(self.selected).map(|a| a.def.id.clone());
        let mut old: Vec<AppEntry> = std::mem::take(&mut self.apps);
        for def in catalog.apps {
            match old.iter().position(|a| a.def.id == def.id) {
                Some(i) => {
                    let mut e = old.remove(i);
                    e.def = def;
                    self.apps.push(e);
                }
                None => {
                    let e = AppEntry::new(def);
                    if !e.def.coming_soon {
                        jobs::detect(&self.inbox, e.def.clone(), self.platform.clone());
                        jobs::releases(&self.inbox, e.def.clone());
                    }
                    self.apps.push(e);
                }
            }
        }
        self.selected = selected_id.and_then(|id| self.apps.iter().position(|a| a.def.id == id)).unwrap_or(0);
    }

    fn job_finished(&mut self, id: u64, now: f64) {
        let Some(job) = self.jobs.iter().find(|j| j.id == id) else { return };
        let (title, body, kind) = match (job.action, job.status()) {
            (Action::Uninstall, Status::Done(_)) => (trf("{} was removed", &job.app_name), String::new(), ToastKind::Info),
            (_, Status::Done(outcome)) => {
                let body = if outcome.is_some_and(|o| o.restart_needed) { tr("Restart the app to use the new version.").to_string() } else { String::new() };
                (trf("{} is ready", format!("{} {}", job.app_name, job.version)), body, ToastKind::Success)
            }
            (_, Status::Failed(e)) => (format!("{} · {}", job.app_name, tr("Failed")), e, ToastKind::Error),
            (_, Status::Cancelled) => (format!("{} · {}", job.app_name, tr("Cancelled")), String::new(), ToastKind::Info),
            (_, Status::Running(_)) => return,
        };
        let app = job.app.clone();
        self.toasts.push(Toast { title, body, kind, until: now + if kind == ToastKind::Error { 12.0 } else { 6.0 } });
        self.notify_tray();
        if let Some(a) = self.apps.iter().find(|a| a.def.id == app) {
            jobs::detect(&self.inbox, a.def.clone(), self.platform.clone());
        }
    }

    /// Update an app on its own when the setting asks and it isn't running.
    fn auto_update(&mut self, app_id: &str) {
        if !self.settings.auto_update || self.job_for(app_id).is_some() {
            return;
        }
        let Some(idx) = self.apps.iter().position(|a| a.def.id == app_id) else { return };
        let a = &self.apps[idx];
        if a.detected && !a.running && a.update(&self.platform, self.settings.prereleases).is_some() {
            log::info!("updating {} automatically", a.def.name);
            self.actions.push(UiAction::Update(idx));
        }
    }

    fn apply_actions(&mut self, ctx: &egui::Context, now: f64) {
        for action in std::mem::take(&mut self.actions) {
            match action {
                UiAction::Select(i) => {
                    self.selected = i;
                    self.page = Page::Library;
                    self.settings.last_app = self.apps.get(i).map(|a| a.def.id.clone());
                    self.settings.save();
                }
                UiAction::Page(p) => self.page = p,
                UiAction::Launch(i) => {
                    let Some(a) = self.apps.get(i) else { continue };
                    let Some(inst) = a.primary() else { continue };
                    match system::launch(&a.def, inst) {
                        Ok(()) => {
                            self.detect_at = Some(now + 2.5);
                            if self.settings.close_on_launch {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        Err(e) => self.toasts.push(Toast { title: a.def.name.clone(), body: e, kind: ToastKind::Error, until: now + 10.0 }),
                    }
                }
                UiAction::Install { app, plan, release } => {
                    let Some(a) = self.apps.get(app) else { continue };
                    if self.job_for(&a.def.id).is_some() {
                        continue;
                    }
                    // Installing over an existing copy of the same kind updates it in place.
                    let existing = a.installs.iter().find(|i| install::update_plan(&a.def, &self.platform, &release, i).is_ok_and(|p| p.target == plan.target)).cloned();
                    let job = jobs::install(&self.inbox, a.def.clone(), release, plan, existing);
                    self.jobs.push(job);
                }
                UiAction::Update(i) => {
                    let Some(a) = self.apps.get(i) else { continue };
                    let (Some(inst), Some(rel)) = (a.primary(), a.update(&self.platform, self.settings.prereleases)) else { continue };
                    if self.job_for(&a.def.id).is_some() {
                        continue;
                    }
                    match install::update_plan(&a.def, &self.platform, rel, inst) {
                        Ok(plan) => {
                            let job = jobs::install(&self.inbox, a.def.clone(), rel.clone(), plan, Some(inst.clone()));
                            self.jobs.push(job);
                        }
                        Err(e) => self.toasts.push(Toast { title: a.def.name.clone(), body: e, kind: ToastKind::Error, until: now + 10.0 }),
                    }
                }
                UiAction::Cancel(id) => {
                    if let Some(j) = self.jobs.iter().find(|j| j.id == id) {
                        j.cancel();
                    }
                }
                UiAction::AskUninstall(app, inst) => self.confirm = Some((app, inst)),
                UiAction::Uninstall(app, inst) => {
                    self.confirm = None;
                    let Some(a) = self.apps.get(app) else { continue };
                    let Some(i) = a.installs.get(inst) else { continue };
                    let job = jobs::uninstall(&self.inbox, a.def.clone(), i.clone());
                    self.jobs.push(job);
                }
                UiAction::Reveal(p) => system::reveal(&p),
                UiAction::OpenUrl(u) => system::open_url(&u),
                UiAction::CheckUpdates => {
                    self.last_check = now;
                    self.check_all();
                    self.detect_all();
                }
                UiAction::Restart(i) => {
                    let Some(a) = self.apps.get(i) else { continue };
                    let (def, inst) = (a.def.clone(), a.primary().cloned());
                    if let Some(inst) = inst {
                        std::thread::spawn(move || {
                            if system::ask_to_quit(&def) && system::wait_until_closed(&def, &inst, 30) {
                                let _ = system::launch(&def, &inst);
                            }
                        });
                        self.detect_at = Some(now + 4.0);
                    }
                }
                UiAction::ClearFinished => self.jobs.retain(Job::is_running),
                UiAction::ClearCache => {
                    crate::paths::remove_any(&crate::paths::downloads_dir());
                    self.toasts.push(Toast { title: tr("Download cache cleared").into(), body: String::new(), kind: ToastKind::Info, until: now + 4.0 });
                }
                UiAction::SettingsChanged => {
                    self.settings.save();
                    i18n::set(self.settings.language);
                    self.applied = None;
                    if self.shot.is_none() {
                        self.apply_background();
                    }
                }
            }
        }
    }

    fn theme(&self, ctx: &egui::Context) -> Palette {
        let pref = self.theme_override.unwrap_or(self.settings.theme);
        let dark = match pref {
            ThemePref::Dark => true,
            ThemePref::Light => false,
            ThemePref::System => ctx.input(|i| i.raw.system_theme).is_none_or(|t| t == egui::Theme::Dark),
        };
        if dark { Palette::dark() } else { Palette::light() }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let ready = self.apps.iter().all(|a| a.def.coming_soon || (a.detected && a.releases.is_some()));
        if let Some(shot) = &mut self.shot {
            shot.step(ctx, ready);
        }
    }
}

impl eframe::App for Launcher {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        let palette = self.theme(ctx);
        if self.applied != Some(palette) {
            theme::apply(ctx, &palette);
            self.palette = palette;
            self.applied = Some(palette);
        }
        self.handle_msgs(now);
        self.apply_actions(ctx, now);
        // Look again when the window comes back to the front, and now and then.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if (focused && !self.was_focused && now - self.last_detect > 3.0) || now - self.last_detect > DETECT_EVERY || self.detect_at.is_some_and(|t| now >= t) {
            self.last_detect = now;
            self.detect_at = None;
            self.detect_all();
        }
        self.was_focused = focused;
        if self.settings.check_on_start && now - self.last_check > CHECK_EVERY {
            self.last_check = now;
            if now > 1.0 {
                self.check_all();
            }
        }
        self.toasts.retain(|t| t.until > now);
        // Keep the periodic checks and running jobs ticking without a busy loop.
        let busy = self.jobs.iter().any(Job::is_running);
        ctx.request_repaint_after(std::time::Duration::from_millis(if busy { 100 } else { 1000 }));
        self.screenshot_step(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let p = self.palette;
        let top = if self.platform.os == Os::MacOs { 46 } else { 20 };
        egui::Panel::left("sidebar")
            .exact_size(244.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.sidebar).inner_margin(Margin { left: 14, right: 14, top, bottom: 14 }))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg))
            .show(ui, |ui| {
                // The top strip moves the window (the title bar is hidden on macOS).
                let strip = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), 36.0));
                let drag = ui.interact(strip, ui.scope_id().with("drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                match self.page {
                    Page::Library => self.library(ui),
                    Page::Downloads => self.downloads(ui),
                    Page::Settings => self.settings_page(ui),
                }
            });
        self.toasts_ui(ui);
        self.confirm_ui(ui);
    }
}

impl Launcher {
    fn sidebar(&mut self, ui: &mut Ui) {
        let p = self.palette;
        let ti = usize::from(!p.dark);
        // The brand: mark + wordmark, "LAUNCHER" under it.
        let drag_rect = Rect::from_min_size(ui.max_rect().min - vec2(14.0, 46.0), vec2(244.0, 46.0));
        let drag = ui.interact(drag_rect, ui.scope_id().with("sdrag"), Sense::drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.add(egui::Image::new((self.textures.mark[ti].id(), vec2(40.0, 40.0))));
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(3.0);
                let wm = &self.textures.wordmark[ti];
                let h = 21.0;
                let w = h * wm.size()[0] as f32 / wm.size()[1] as f32;
                ui.add(egui::Image::new((wm.id(), vec2(w, h))));
                ui.add_space(3.0);
                ui.label(widgets::spaced("Launcher", semibold(10.0), p.muted, 3.2));
            });
        });
        ui.add_space(22.0);
        let downloads_badge = self.jobs.iter().filter(|j| j.is_running()).count();
        for (page, icon_kind, label, badge) in [
            (Page::Library, Icon::Library, tr("Library"), 0),
            (Page::Downloads, Icon::Inbox, tr("Downloads"), downloads_badge),
            (Page::Settings, Icon::Gear, tr("Settings"), 0),
        ] {
            if nav_item(ui, &p, icon_kind, label, self.page == page, badge).clicked() {
                self.actions.push(UiAction::Page(page));
            }
        }
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            widgets::section_label(ui, &p, tr("D.E.V. apps"));
        });
        ui.add_space(2.0);
        let ctx = ui.ctx().clone();
        // D.E.V.'s own apps, then, under a quiet rule, the open-source apps made by others.
        let dev: Vec<usize> = (0..self.apps.len()).filter(|&i| !self.apps[i].def.is_outside()).collect();
        let outside: Vec<usize> = (0..self.apps.len()).filter(|&i| self.apps[i].def.is_outside()).collect();
        let mut order: Vec<Option<usize>> = dev.into_iter().map(Some).collect();
        if !outside.is_empty() {
            order.push(None);
            order.extend(outside.into_iter().map(Some));
        }
        for slot in order {
            let Some(i) = slot else {
                ui.add_space(10.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline((r.left() + 8.0)..=(r.right() - 8.0), r.center().y, Stroke::new(1.0, theme::alpha(p.faint, 60)));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    widgets::section_label(ui, &p, tr("Open source · outside D.E.V."));
                });
                ui.add_space(2.0);
                continue;
            };
            let tex = self.textures.app(&ctx, &self.apps[i].def.id);
            let a = &self.apps[i];
            let job = self.job_for(&a.def.id).map(|j| j.status());
            let update = a.update(&self.platform, self.settings.prereleases).map(|r| r.version_label());
            let (sub, dot) = match (&job, a.primary(), &update) {
                (Some(Status::Running(stage)), _, _) => (stage_label(stage, &a.def.name), Some(p.info)),
                (_, _, _) if a.def.is_outside() => (a.def.publisher.as_ref().map_or_else(String::new, |by| trf("by {}", &by.name)), None),
                (_, _, _) if a.def.coming_soon => (tr("More apps are on the way").to_string(), None),
                (_, None, _) if !a.detected => (tr("Checking…").to_string(), None),
                (_, None, _) => (tr("Not installed").to_string(), Some(p.faint)),
                (_, Some(_), Some(v)) => (trf("v{} available", v), Some(p.warn)),
                (_, Some(_), None) if a.running => (tr("Running").to_string(), Some(p.accent_text)),
                (_, Some(inst), None) => (inst.version.as_ref().map_or_else(|| tr("Installed").to_string(), |v| trf("Version {}", v)), Some(p.accent_text)),
            };
            let selected = self.page == Page::Library && self.selected == i;
            if app_row(ui, &p, tex.as_ref(), &a.def.name, &sub, dot, selected).clicked() {
                self.actions.push(UiAction::Select(i));
            }
        }
        // A quiet hint that the suite grows.
        ui.add_space(6.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        ui.painter().rect_stroke(r.shrink(1.0), CornerRadius::same(10), Stroke::new(1.0, theme::alpha(p.faint, 70)), StrokeKind::Inside);
        icon(ui.painter(), Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(16.0, 16.0)), Icon::Hexagon, p.faint);
        ui.painter().text(pos2(r.left() + 42.0, r.center().y), egui::Align2::LEFT_CENTER, tr("More apps are on the way"), regular(12.5), p.faint);

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("D.E.V. Launcher {}", env!("CARGO_PKG_VERSION"))).font(regular(11.5)).color(p.faint));
            });
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
                icon(ui.painter(), r, Icon::Cpu, p.faint);
                ui.label(egui::RichText::new(self.platform.label()).font(regular(11.5)).color(p.muted));
            });
        });
    }

    fn toasts_ui(&mut self, ui: &mut Ui) {
        if self.toasts.is_empty() {
            return;
        }
        let p = self.palette;
        let screen = ui.ctx().content_rect();
        let mut y = screen.bottom() - 20.0;
        let mut close = None;
        for (i, t) in self.toasts.iter().enumerate().rev() {
            let width = 340.0;
            let id = egui::Id::unique(("toast", i));
            let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos2(screen.right() - width - 20.0, y)).pivot(egui::Align2::LEFT_BOTTOM);
            let resp = area.show(ui.ctx(), |ui| {
                widgets::card(&p).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    ui.horizontal(|ui| {
                        let (color, k) = match t.kind {
                            ToastKind::Success => (p.accent_text, Icon::Check),
                            ToastKind::Error => (p.danger, Icon::Alert),
                            ToastKind::Info => (p.info, Icon::Info),
                        };
                        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        icon(ui.painter(), r, k, color);
                        ui.vertical(|ui| {
                            ui.set_width(width - 90.0);
                            ui.add(egui::Label::new(egui::RichText::new(&t.title).font(semibold(13.5))).wrap());
                            if !t.body.is_empty() {
                                ui.add(egui::Label::new(egui::RichText::new(&t.body).font(regular(12.5)).color(p.muted)).wrap());
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                            icon(ui.painter(), r.shrink(3.0), Icon::Close, if resp.hovered() { p.text } else { p.faint });
                            if resp.clicked() {
                                close = Some(i);
                            }
                        });
                    });
                });
            });
            y = resp.response.rect.top() - 10.0;
        }
        if let Some(i) = close {
            self.toasts.remove(i);
        }
    }

    fn confirm_ui(&mut self, ui: &mut Ui) {
        let Some((app, inst)) = self.confirm else { return };
        let Some(a) = self.apps.get(app) else { return };
        let Some(i) = a.installs.get(inst) else { return };
        let p = self.palette;
        let name = a.def.name.clone();
        let is_mac = matches!(i.method, crate::detect::Method::MacApp);
        let modal = egui::Modal::new(egui::Id::unique("confirm-uninstall")).frame(widgets::card(&p).inner_margin(Margin::same(24))).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(trf("Uninstall {}?", &name)).font(semibold(18.0)));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(i.path.display().to_string()).font(regular(12.5)).color(p.faint));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tr("Your drawings and settings are kept.")).color(p.muted));
            if is_mac {
                ui.label(egui::RichText::new(tr("The app is moved to the Trash.")).color(p.muted));
            }
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, &p, widgets::Style::Danger, Some(Icon::Trash), tr("Uninstall"), 0.0, true).clicked() {
                    self.actions.push(UiAction::Uninstall(app, inst));
                }
                if widgets::button(ui, &p, widgets::Style::Secondary, None, tr("Cancel"), 0.0, true).clicked() {
                    self.confirm = None;
                }
            });
        });
        if modal.should_close() {
            self.confirm = None;
        }
    }
}

/// What a running job is doing, in a few words.
pub fn stage_label(stage: &install::Stage, app_name: &str) -> String {
    match stage {
        install::Stage::Preparing => tr("Preparing…").into(),
        install::Stage::Downloading(_) => tr("Downloading…").into(),
        install::Stage::Installing => tr("Installing…").into(),
        install::Stage::WaitingForApp => trf("Waiting for {} to close…", app_name),
        install::Stage::Authorizing => tr("Waiting for permission…").into(),
    }
}

fn nav_item(ui: &mut Ui, p: &Palette, icon_kind: Icon, label: &str, selected: bool, badge: usize) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let hovered = resp.hovered();
    if selected || hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(10), if selected { p.raised } else { theme::alpha(p.raised, 150) });
    }
    if selected {
        let bar = Rect::from_min_size(pos2(rect.left(), rect.top() + 10.0), vec2(3.0, rect.height() - 20.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), p.accent);
    }
    let color = if selected { p.text } else if hovered { p.text } else { p.muted };
    icon(ui.painter(), Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(17.0, 17.0)), icon_kind, if selected { p.accent_text } else { color });
    ui.painter().text(pos2(rect.left() + 42.0, rect.center().y), egui::Align2::LEFT_CENTER, label, medium(14.0), color);
    if badge > 0 {
        let c = pos2(rect.right() - 18.0, rect.center().y);
        ui.painter().circle_filled(c, 9.0, p.accent);
        ui.painter().text(c, egui::Align2::CENTER_CENTER, badge.to_string(), semibold(11.0), p.on_accent);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn app_row(ui: &mut Ui, p: &Palette, tex: Option<&TextureHandle>, name: &str, sub: &str, dot: Option<Color32>, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(10), if selected { p.raised } else { theme::alpha(p.raised, 150) });
    }
    if selected {
        ui.painter().rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, p.stroke), StrokeKind::Inside);
    }
    let icon_rect = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 17.0), vec2(34.0, 34.0));
    match tex {
        Some(t) => {
            egui::Image::new((t.id(), icon_rect.size())).corner_radius(CornerRadius::same(8)).paint_at(ui, icon_rect);
        }
        None => {
            ui.painter().rect_filled(icon_rect, CornerRadius::same(8), p.raised_hover);
            ui.painter().text(icon_rect.center(), egui::Align2::CENTER_CENTER, name.chars().next().unwrap_or('?'), semibold(16.0), p.muted);
        }
    }
    let x = icon_rect.right() + 12.0;
    ui.painter().text(pos2(x, rect.center().y - 8.5), egui::Align2::LEFT_CENTER, name, semibold(14.0), p.text);
    let sub_rect = ui.painter().text(pos2(x, rect.center().y + 9.0), egui::Align2::LEFT_CENTER, sub, regular(12.0), p.muted);
    let _ = sub_rect;
    if let Some(c) = dot {
        ui.painter().circle_filled(pos2(rect.right() - 14.0, rect.center().y), 4.0, c);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
