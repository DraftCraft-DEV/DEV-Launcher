//! The Downloads and Settings pages.

use egui::{Align, Align2, CornerRadius, Layout, Margin, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};

use super::library::{card_title, short_path};
use super::theme::{Palette, display, medium, regular, semibold};
use super::widgets::{self, Icon, Style, icon};
use super::{Launcher, UiAction, stage_label};
use crate::i18n::{tr, trf};
use crate::install::Stage;
use crate::jobs::{Action, Status};
use crate::platform::Os;
use crate::settings::{LangPref, LinuxFormat, ThemePref, WinScope};

const MAX_WIDTH: f32 = 760.0;

fn page(ui: &mut Ui, p: &Palette, title: &str, right: impl FnOnce(&mut Ui), add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let full = ui.available_width();
        let width = (full - 64.0).min(MAX_WIDTH);
        let side = ((full - width) / 2.0).max(24.0);
        egui::Frame::new().inner_margin(Margin { left: side as i8, right: 0, top: 40, bottom: 28 }).show(ui, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(display(26.0)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), right);
            });
            ui.add_space(14.0);
            add(ui);
        });
    });
}

impl Launcher {
    pub(super) fn downloads(&mut self, ui: &mut Ui) {
        let p = self.palette;
        let mut actions = Vec::new();
        let ctx = ui.ctx().clone();
        let icons: Vec<_> = self.jobs.iter().map(|j| self.textures.app(&ctx, &j.app)).collect();
        let any_finished = self.jobs.iter().any(|j| !j.is_running());
        let mut clear = false;
        page(
            ui,
            &p,
            tr("Downloads"),
            |ui| {
                if any_finished && widgets::button(ui, &p, Style::Ghost, Some(Icon::Check), tr("Clear finished"), 32.0, true).clicked() {
                    clear = true;
                }
            },
            |ui| {
                if self.jobs.is_empty() {
                    empty_state(ui, &p);
                    return;
                }
                ui.spacing_mut().item_spacing.y = 10.0;
                for (job, tex) in self.jobs.iter().zip(&icons).rev() {
                    let status = job.status();
                    widgets::card(&p).inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (ir, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                            match tex {
                                Some(t) => {
                                    egui::Image::new((t.id(), ir.size())).corner_radius(CornerRadius::same(10)).paint_at(ui, ir);
                                }
                                None => {
                                    ui.painter().rect_filled(ir, CornerRadius::same(10), p.raised_hover);
                                }
                            }
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                let title = match job.action {
                                    Action::Install => trf("Installing {}", format!("{} {}", job.app_name, job.version)),
                                    Action::Update => format!("{} · {}", job.app_name, trf("Updating to {}", &job.version)),
                                    Action::Uninstall => trf("Removing {}", &job.app_name),
                                };
                                ui.label(RichText::new(title).font(semibold(14.0)));
                                let (line, color) = match &status {
                                    Status::Running(Stage::Downloading(t)) if t.total > 0 => {
                                        (trf("{} of {}", widgets::bytes(t.done)).replacen("{}", &widgets::bytes(t.total), 1), p.muted)
                                    }
                                    Status::Running(s) => (stage_label(s, &job.app_name), p.muted),
                                    Status::Done(_) => (tr("Done").to_string(), p.accent_text),
                                    Status::Failed(e) => (format!("{} · {e}", tr("Failed")), p.danger),
                                    Status::Cancelled => (tr("Cancelled").to_string(), p.faint),
                                };
                                ui.add(egui::Label::new(RichText::new(line).font(regular(12.5)).color(color)).wrap());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &status {
                                Status::Running(stage) => {
                                    let ok = !matches!(stage, Stage::Installing | Stage::Authorizing) && job.action != Action::Uninstall;
                                    if widgets::button(ui, &p, Style::Secondary, Some(Icon::Close), tr("Cancel"), 30.0, ok).clicked() {
                                        actions.push(UiAction::Cancel(job.id));
                                    }
                                }
                                Status::Done(_) => {
                                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                                    icon(ui.painter(), r, Icon::Check, p.accent_text);
                                }
                                Status::Failed(_) => {
                                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                                    icon(ui.painter(), r, Icon::Alert, p.danger);
                                }
                                Status::Cancelled => {}
                            });
                        });
                        if let Status::Running(stage) = &status {
                            ui.add_space(8.0);
                            let f = match stage {
                                Stage::Downloading(t) if t.total > 0 => Some(t.done as f32 / t.total as f32),
                                _ => None,
                            };
                            ui.push_id(job.id, |ui| widgets::progress(ui, &p, f, ui.available_width(), 5.0));
                        }
                    });
                }
            },
        );
        if clear {
            actions.push(UiAction::ClearFinished);
        }
        self.actions.extend(actions);
    }

    pub(super) fn settings_page(&mut self, ui: &mut Ui) {
        let p = self.palette;
        let mut changed = false;
        let mut actions = Vec::new();
        let os = self.platform.os;
        let s = &mut self.settings;
        let mut theme_touched = false;
        page(
            ui,
            &p,
            tr("Settings"),
            |_| {},
            |ui| {
                ui.spacing_mut().item_spacing.y = 16.0;
                section(ui, &p, tr("Appearance"), |ui| {
                    row(ui, &p, tr("Theme"), None, |ui| {
                        let opts = [(ThemePref::System, tr("System").to_string()), (ThemePref::Dark, tr("Dark").to_string()), (ThemePref::Light, tr("Light").to_string())];
                        if widgets::segmented(ui, &p, &mut s.theme, &opts) {
                            changed = true;
                            theme_touched = true;
                        }
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("Language"), None, |ui| {
                        let opts = [(LangPref::Auto, tr("Automatic").to_string()), (LangPref::English, "English".to_string()), (LangPref::Italian, "Italiano".to_string())];
                        changed |= widgets::segmented(ui, &p, &mut s.language, &opts);
                    });
                });
                section(ui, &p, tr("Updates"), |ui| {
                    row(ui, &p, tr("Check for updates when the launcher starts"), None, |ui| {
                        changed |= widgets::toggle(ui, &p, &mut s.check_on_start).changed();
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("Install app updates automatically"), Some(tr("Only while the app is closed.")), |ui| {
                        changed |= widgets::toggle(ui, &p, &mut s.auto_update).changed();
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("Offer pre-release versions"), None, |ui| {
                        changed |= widgets::toggle(ui, &p, &mut s.prereleases).changed();
                    });
                    ui.add_space(2.0);
                    if widgets::button(ui, &p, Style::Secondary, Some(Icon::Refresh), tr("Check now"), 32.0, true).clicked() {
                        actions.push(UiAction::CheckUpdates);
                    }
                });
                section(ui, &p, tr("Installation"), |ui| {
                    match os {
                        Os::MacOs => row(ui, &p, tr("Install apps in"), None, |ui| {
                            let opts = [(false, tr("Applications (all users)").to_string()), (true, tr("My Applications folder").to_string())];
                            changed |= widgets::segmented(ui, &p, &mut s.mac_user_applications, &opts);
                        }),
                        Os::Windows => row(ui, &p, tr("Install apps for"), Some(tr("Installing for everyone asks for administrator permission.")), |ui| {
                            let opts = [(WinScope::User, tr("Only me").to_string()), (WinScope::Machine, tr("Everyone").to_string())];
                            changed |= widgets::segmented(ui, &p, &mut s.windows_scope, &opts);
                        }),
                        Os::Linux => row(ui, &p, tr("Package format"), Some(tr("System packages need your password; AppImages don't.")), |ui| {
                            let opts = [(LinuxFormat::Auto, tr("Automatic").to_string()), (LinuxFormat::AppImage, "AppImage".to_string()), (LinuxFormat::Native, tr("System package (.deb, .rpm, Arch)").to_string())];
                            changed |= widgets::segmented(ui, &p, &mut s.linux_format, &opts);
                        }),
                    }
                    divider(ui, &p);
                    row(ui, &p, tr("Close the launcher when an app starts"), None, |ui| {
                        changed |= widgets::toggle(ui, &p, &mut s.close_on_launch).changed();
                    });
                });
                section(ui, &p, tr("About"), |ui| {
                    row(ui, &p, "D.E.V. Launcher", Some("DraftCraft Engineering Ventures"), |ui| {
                        ui.label(RichText::new(trf("Version {}", env!("CARGO_PKG_VERSION"))).font(medium(13.0)).color(p.muted));
                    });
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        if widgets::button(ui, &p, Style::Secondary, Some(Icon::Folder), tr("Open data folder"), 32.0, true).clicked() {
                            actions.push(UiAction::Reveal(crate::paths::data_dir()));
                        }
                        if widgets::button(ui, &p, Style::Secondary, Some(Icon::Info), tr("Open log"), 32.0, true).clicked() {
                            actions.push(UiAction::Reveal(crate::paths::log_file()));
                        }
                        if widgets::button(ui, &p, Style::Ghost, Some(Icon::Trash), tr("Clear download cache"), 32.0, true).clicked() {
                            actions.push(UiAction::ClearCache);
                        }
                    });
                    ui.label(RichText::new(short_path(&crate::paths::data_dir())).font(regular(12.0)).color(p.faint));
                });
            },
        );
        if theme_touched {
            self.theme_override = None;
        }
        if changed {
            actions.push(UiAction::SettingsChanged);
        }
        self.actions.extend(actions);
    }
}

fn section(ui: &mut Ui, p: &Palette, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            widgets::section_label(ui, p, title);
        });
        widgets::card(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            add(ui);
        });
    });
}

fn row(ui: &mut Ui, p: &Palette, label: &str, hint: Option<&str>, control: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let label_w = (width - 300.0).max(220.0);
    let top = ui.cursor().min;
    let draw_label = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add(egui::Label::new(RichText::new(label).font(medium(14.0)).color(p.text)).wrap());
        if let Some(h) = hint {
            ui.add(egui::Label::new(RichText::new(h).font(regular(12.5)).color(p.faint)).wrap());
        }
    };
    // Measure the label, then centre it and the control on one line.
    let mut probe = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(top, vec2(label_w, 400.0))).invisible());
    draw_label(&mut probe);
    let lh = probe.min_rect().height();
    let h = lh.max(32.0);
    let mut left = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(pos2(top.x, top.y + (h - lh) / 2.0), vec2(label_w, lh))));
    draw_label(&mut left);
    let right_rect = Rect::from_min_max(pos2(top.x + label_w, top.y), pos2(top.x + width, top.y + h));
    let mut right = ui.new_child(UiBuilder::new().max_rect(right_rect).layout(Layout::right_to_left(Align::Center)));
    control(&mut right);
    ui.allocate_rect(Rect::from_min_size(top, vec2(width, h)), Sense::hover());
}

fn divider(ui: &mut Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, p.stroke);
}

fn empty_state(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 300.0), Sense::hover());
    let c = pos2(rect.center().x, rect.top() + 110.0);
    ui.painter().circle_filled(c, 44.0, p.raised);
    icon(ui.painter(), Rect::from_center_size(c, vec2(36.0, 36.0)), Icon::Inbox, p.faint);
    ui.painter().text(pos2(c.x, c.y + 76.0), Align2::CENTER_CENTER, tr("Nothing downloaded yet"), semibold(16.0), p.text);
    ui.painter().text(pos2(c.x, c.y + 100.0), Align2::CENTER_CENTER, tr("Installs and updates show up here."), regular(13.5), p.muted);
    let _ = card_title;
}
