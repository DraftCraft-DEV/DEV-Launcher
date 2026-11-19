//! The launcher's own install and uninstall screen: a borderless window with the brand on the
//! left and one step at a time on the right. No wizard pages, no "Next" buttons.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use super::theme::{self, Palette, display, medium, regular, semibold};
use super::widgets::{self, Icon, Style, icon};
use crate::assets;
use crate::i18n::{tr, trf};
use crate::setup::{self, Mode, NAME, Options};

type Outcome = Arc<Mutex<Option<Result<Option<PathBuf>, String>>>>;

enum Step {
    Ask,
    Working { started: f64, result: Outcome },
    Done(Option<PathBuf>),
    Failed(String),
}

pub struct SetupApp {
    mode: Mode,
    step: Step,
    opts: Options,
    remove_data: bool,
    open_after: bool,
    update: bool,
    mark: egui::TextureHandle,
    wordmark: egui::TextureHandle,
    applied: Option<Palette>,
    shot: Option<super::Screenshot>,
    /// Windows' options and wording. A macOS preview (for the docs) shows the Windows screen:
    /// on macOS the launcher has no setup, the DMG is drag and drop.
    windows: bool,
}

/// A step to show right away (`--setup-preview`, for screenshots).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preview {
    Ask,
    Working,
    Done,
    Failed,
}

impl SetupApp {
    pub fn new(cc: &eframe::CreationContext, mode: Mode, lang: Option<crate::settings::LangPref>, shot: Option<PathBuf>, preview: Option<Preview>) -> Self {
        let ctx = &cc.egui_ctx;
        theme::install_fonts(ctx);
        crate::i18n::set(lang.unwrap_or_else(|| crate::settings::Settings::load().language));
        let tex = |name: &str, png: &[u8]| {
            let (size, rgba) = assets::decode_png(png).unwrap_or(([1, 1], vec![0; 4]));
            ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, &rgba), egui::TextureOptions::LINEAR)
        };
        let step = match preview {
            None | Some(Preview::Ask) => Step::Ask,
            Some(Preview::Working) => Step::Working { started: f64::INFINITY, result: Arc::default() },
            Some(Preview::Done) => Step::Done(None),
            Some(Preview::Failed) => Step::Failed("couldn't copy the launcher (Access is denied. (os error 5))".into()),
        };
        Self {
            mode,
            step,
            opts: Options { everyone: false, desktop_shortcut: true },
            remove_data: false,
            open_after: true,
            update: setup::installed_program().is_some(),
            mark: tex("mark", assets::MARK_DARK),
            wordmark: tex("wordmark", assets::WORDMARK_DARK),
            applied: None,
            shot: shot.map(super::Screenshot::new),
            windows: cfg!(windows) || (cfg!(target_os = "macos") && preview.is_some()),
        }
    }

    fn start(&mut self, now: f64) {
        let result: Outcome = Arc::default();
        let (mode, opts, remove_data, out) = (self.mode, self.opts, self.remove_data, result.clone());
        std::thread::spawn(move || {
            let r = match mode {
                Mode::Install => setup::install(opts).map(Some),
                Mode::Uninstall => setup::uninstall(remove_data).map(|()| None),
            };
            if let Ok(mut o) = out.lock() {
                *o = Some(r);
            }
        });
        self.step = Step::Working { started: now, result };
    }
}

impl eframe::App for SetupApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark = ctx.input(|i| i.raw.system_theme).is_none_or(|t| t == egui::Theme::Dark);
        let p = if dark { Palette::dark() } else { Palette::light() };
        if self.applied != Some(p) {
            theme::apply(ctx, &p);
            self.applied = Some(p);
        }
        let now = ctx.input(|i| i.time);
        if let Step::Working { started, result } = &self.step {
            // Let the progress show for a moment, even when the copy is instant.
            let finished = result.lock().ok().and_then(|mut r| if now - *started > 1.2 { r.take() } else { None });
            match finished {
                Some(Ok(path)) => self.step = Step::Done(path),
                Some(Err(e)) => {
                    log::error!("setup: {e}");
                    self.step = Step::Failed(e);
                }
                None => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            }
        }
        if let Some(shot) = &mut self.shot {
            shot.step(ctx, true);
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let p = self.applied.unwrap_or_else(Palette::dark);
        let full = ui.ctx().content_rect();
        ui.painter().rect_filled(full, 0.0, p.bg);
        let brand = Rect::from_min_size(full.min, vec2(290.0, full.height()));
        self.brand_panel(ui, brand);

        // Window controls and the drag strip (the window has no title bar).
        let strip = Rect::from_min_max(pos2(brand.right(), full.top()), pos2(full.right(), full.top() + 40.0));
        let drag = ui.interact(strip, egui::Id::unique("setup-drag"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let brand_drag = ui.interact(brand, egui::Id::unique("setup-brand-drag"), Sense::drag());
        if brand_drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let close = Rect::from_center_size(pos2(full.right() - 24.0, full.top() + 22.0), vec2(28.0, 28.0));
        let min = close.translate(vec2(-34.0, 0.0));
        if window_button(ui, &p, close, "close", true).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if window_button(ui, &p, min, "min", false).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        let content = Rect::from_min_max(pos2(brand.right() + 44.0, full.top() + 56.0), pos2(full.right() - 44.0, full.bottom() - 32.0));
        let mut ui = ui.new_child(UiBuilder::new().max_rect(content));
        let now = ui.input(|i| i.time);
        match (&self.step, self.mode) {
            (Step::Ask, Mode::Install) => self.ask_install(&mut ui, &p, now),
            (Step::Ask, Mode::Uninstall) => self.ask_uninstall(&mut ui, &p, now),
            (Step::Working { .. }, _) => self.working(&mut ui, &p),
            (Step::Done(path), _) => {
                let path = path.clone();
                self.done(&mut ui, &p, path);
            }
            (Step::Failed(e), _) => {
                let e = e.clone();
                self.failed(&mut ui, &p, &e, now);
            }
        }
    }
}

impl SetupApp {
    fn brand_panel(&self, ui: &mut Ui, rect: Rect) {
        let d = Palette::dark();
        let painter = ui.painter_at(rect);
        widgets::gradient_rect(&painter, rect.expand(20.0), 0.0, theme::mix(theme::INK, theme::LIME, 0.10), theme::INK);
        widgets::hex_pattern(&painter, rect, theme::alpha(theme::LIME, 40), 26.0);
        // A soft glow behind the mark.
        let c = pos2(rect.center().x, rect.top() + 190.0);
        for (r, a) in [(120.0, 10), (84.0, 14), (56.0, 18)] {
            painter.circle_filled(c, r, theme::alpha(theme::LIME, a));
        }
        let mark = Rect::from_center_size(c, vec2(96.0, 96.0));
        egui::Image::new((self.mark.id(), mark.size())).paint_at(ui, mark);
        let wm = &self.wordmark;
        let h = 30.0;
        let w = h * wm.size()[0] as f32 / wm.size()[1] as f32;
        let wr = Rect::from_center_size(pos2(c.x, c.y + 96.0), vec2(w, h));
        egui::Image::new((wm.id(), wr.size())).paint_at(ui, wr);
        let job = widgets::spaced("Launcher", semibold(11.5), d.muted, 4.0);
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(pos2(c.x - galley.size().x / 2.0, wr.bottom() + 8.0), galley, d.muted);

        let v = format!("v{}", env!("CARGO_PKG_VERSION"));
        let g = ui.painter().layout_no_wrap(v, medium(12.0), d.text);
        let chip = Rect::from_center_size(pos2(c.x, rect.bottom() - 70.0), g.size() + vec2(22.0, 10.0));
        painter.rect(chip, CornerRadius::same(11), Color32::from_white_alpha(12), Stroke::new(1.0, Color32::from_white_alpha(22)), egui::StrokeKind::Inside);
        painter.galley(chip.center() - g.size() / 2.0, g, d.text);
        painter.text(pos2(c.x, rect.bottom() - 36.0), Align2::CENTER_CENTER, "DraftCraft Engineering Ventures", regular(11.5), d.faint);
    }

    fn ask_install(&mut self, ui: &mut Ui, p: &Palette, now: f64) {
        let title = if self.update { trf("Update {}", NAME) } else { trf("Install {}", NAME) };
        ui.label(RichText::new(title).font(display(27.0)).color(p.text));
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(tr("Install, update and open the DraftCraft apps, all in one place.")).font(regular(14.5)).color(p.muted)).wrap());
        // Windows has two more options: everything sits a little closer.
        let gap = if self.windows { 0.55 } else { 1.0 };
        ui.add_space(22.0 * gap);
        for (k, text) in [
            (Icon::Download, tr("Installs the right version for this computer")),
            (Icon::Refresh, tr("Keeps your apps up to date")),
            (Icon::Play, tr("Opens them with one click")),
        ] {
            feature(ui, p, k, text);
        }
        ui.add_space(18.0 * gap);

        let dir = match (self.windows && !cfg!(windows), self.opts.everyone) {
            (true, false) => r"%LOCALAPPDATA%\Programs\D.E.V. Launcher".to_string(),
            (true, true) => r"C:\Program Files\D.E.V. Launcher".to_string(),
            (false, _) => super::library::short_path(&setup::target_dir(self.opts)),
        };
        let folder = |ui: &mut Ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
                icon(ui.painter(), r, Icon::Folder, p.faint);
                ui.add(egui::Label::new(RichText::new(dir.as_str()).font(regular(12.5)).color(p.faint)).truncate());
            });
        };
        if self.windows {
            // Where it goes sits under the choice that decides it.
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Install for")).font(medium(13.5)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let opts = [(false, tr("Only me").to_string()), (true, tr("Everyone").to_string())];
                    widgets::segmented(ui, p, &mut self.opts.everyone, &opts);
                });
            });
            ui.add_space(-4.0);
            folder(ui);
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Shortcut on the desktop")).font(medium(13.5)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::toggle(ui, p, &mut self.opts.desktop_shortcut);
                });
            });
            if self.opts.everyone {
                ui.label(RichText::new(tr("Installing for everyone asks for administrator permission.")).font(regular(12.0)).color(p.warn));
            }
        } else {
            folder(ui);
        }

        footer(ui, |ui| {
            let label = if self.update { tr("Update") } else { tr("Install") };
            if widgets::button(ui, p, Style::Primary, Some(Icon::Download), label, 44.0, true).clicked() {
                self.start(now);
            }
            let linux = cfg!(target_os = "linux") && !self.windows;
            let other = if linux { tr("Run without installing") } else { tr("Cancel") };
            if widgets::button(ui, p, Style::Ghost, None, other, 44.0, true).clicked() {
                if linux {
                    setup::decline_appimage_setup();
                    if let Some(a) = std::env::var_os("APPIMAGE") {
                        let _ = setup::launch_installed(&PathBuf::from(a));
                    }
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn ask_uninstall(&mut self, ui: &mut Ui, p: &Palette, now: f64) {
        ui.label(RichText::new(trf("Uninstall {}?", NAME)).font(display(27.0)).color(p.text));
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(tr("The apps you installed with it stay on this computer.")).font(regular(14.5)).color(p.muted)).wrap());
        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(tr("Also remove settings and downloads")).font(medium(13.5)).color(p.text));
                // The docs preview shows Windows' layout from a Mac: Windows' folder too.
                let data = if self.windows && !cfg!(windows) { r"%APPDATA%\D.E.V. Launcher".to_string() } else { super::library::short_path(&crate::paths::config_dir()) };
                ui.label(RichText::new(data).font(regular(12.0)).color(p.faint));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::toggle(ui, p, &mut self.remove_data);
            });
        });
        footer(ui, |ui| {
            if widgets::button(ui, p, Style::Danger, Some(Icon::Trash), tr("Uninstall"), 44.0, true).clicked() {
                self.start(now);
            }
            if widgets::button(ui, p, Style::Ghost, None, tr("Cancel"), 44.0, true).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn working(&self, ui: &mut Ui, p: &Palette) {
        let (title, sub) = match self.mode {
            Mode::Install => (if self.update { trf("Updating to {}", env!("CARGO_PKG_VERSION")) } else { trf("Installing {}", NAME) }, tr("Copying files and adding shortcuts…")),
            Mode::Uninstall => (trf("Removing {}", NAME), tr("Removing files and shortcuts…")),
        };
        ui.add_space(70.0);
        ui.label(RichText::new(title).font(display(24.0)).color(p.text));
        ui.add_space(4.0);
        ui.label(RichText::new(sub).font(regular(14.0)).color(p.muted));
        ui.add_space(26.0);
        widgets::progress(ui, p, None, ui.available_width(), 8.0);
    }

    fn done(&mut self, ui: &mut Ui, p: &Palette, path: Option<PathBuf>) {
        ui.add_space(40.0);
        let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 40.0, p.accent_soft);
        ui.painter().circle_filled(r.center(), 30.0, p.accent);
        icon(ui.painter(), Rect::from_center_size(r.center(), vec2(30.0, 30.0)), Icon::Check, p.on_accent);
        ui.add_space(18.0);
        match self.mode {
            Mode::Install => {
                ui.label(RichText::new(trf("{} is installed", NAME)).font(display(26.0)).color(p.text));
                ui.add_space(4.0);
                let where_ = if self.windows { tr("You'll find it in the Start menu.") } else { tr("You'll find it in your applications menu.") };
                ui.label(RichText::new(where_).font(regular(14.0)).color(p.muted));
                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    widgets::toggle(ui, p, &mut self.open_after);
                    ui.label(RichText::new(trf("Open {} now", NAME)).font(medium(13.5)).color(p.text));
                });
                footer(ui, |ui| {
                    if widgets::button(ui, p, Style::Primary, None, tr("Finish"), 44.0, true).clicked() {
                        if self.open_after
                            && let Some(path) = &path
                                && let Err(e) = setup::launch_installed(path) {
                                    log::error!("{e}");
                                }
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            }
            Mode::Uninstall => {
                ui.label(RichText::new(trf("{} has been uninstalled", NAME)).font(display(26.0)).color(p.text));
                footer(ui, |ui| {
                    if widgets::button(ui, p, Style::Primary, None, tr("Close"), 44.0, true).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            }
        }
    }

    fn failed(&mut self, ui: &mut Ui, p: &Palette, error: &str, now: f64) {
        ui.add_space(40.0);
        let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 36.0, theme::alpha(p.danger, 36));
        icon(ui.painter(), Rect::from_center_size(r.center(), vec2(32.0, 32.0)), Icon::Alert, p.danger);
        ui.add_space(18.0);
        let title = match self.mode {
            Mode::Install => tr("The installation didn't finish"),
            Mode::Uninstall => tr("The uninstall didn't finish"),
        };
        ui.label(RichText::new(title).font(display(24.0)).color(p.text));
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(error).font(regular(13.5)).color(p.muted)).wrap());
        footer(ui, |ui| {
            if widgets::button(ui, p, Style::Primary, Some(Icon::Refresh), tr("Retry"), 44.0, true).clicked() {
                self.start(now);
            }
            if widgets::button(ui, p, Style::Ghost, None, tr("Close"), 44.0, true).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

/// Buttons along the bottom right of the content.
fn footer(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let max = ui.max_rect();
    let rect = Rect::from_min_max(pos2(max.left(), max.bottom() - 48.0), max.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)));
    child.spacing_mut().item_spacing.x = 10.0;
    add(&mut child);
}

fn feature(ui: &mut Ui, p: &Palette, k: Icon, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(9), p.accent_soft);
        icon(ui.painter(), r.shrink(8.0), k, p.accent_text);
        ui.add_space(4.0);
        ui.label(RichText::new(text).font(medium(14.0)).color(p.text));
    });
    ui.add_space(4.0);
}

fn window_button(ui: &mut Ui, p: &Palette, rect: Rect, id: &str, close: bool) -> egui::Response {
    let resp = ui.interact(rect, egui::Id::unique(("setup-window", id)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), if close { theme::alpha(p.danger, 50) } else { p.raised });
    }
    let c = if resp.hovered() && close { p.danger } else { p.muted };
    if close {
        icon(ui.painter(), rect.shrink(8.0), Icon::Close, c);
    } else {
        ui.painter().line_segment([pos2(rect.left() + 9.0, rect.center().y), pos2(rect.right() - 9.0, rect.center().y)], Stroke::new(1.5, c));
    }
    let _ = Margin::ZERO;
    resp
}
