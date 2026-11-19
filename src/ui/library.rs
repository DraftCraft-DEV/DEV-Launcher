//! The Library page: one app, big — what it is, its state, the one thing to do next, then the
//! release notes, details and versions.

use egui::text::LayoutJob;
use egui::{Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, TextFormat, Ui, UiBuilder, pos2, vec2};

use super::theme::{Palette, alpha, display, medium, mix, regular, semibold};
use super::widgets::{self, Icon, Style, icon};
use super::{AppEntry, Launcher, UiAction, stage_label};
use crate::detect::Installed;
use crate::i18n::{self, tr, trf};
use crate::install::{self, Plan, Stage};
use crate::jobs::{Action, Status};
use crate::net::Release;

/// Page content width: wide enough for two columns, never a full 4K line.
const MAX_WIDTH: f32 = 980.0;

impl Launcher {
    pub(super) fn library(&mut self, ui: &mut Ui) {
        let Some(idx) = (self.selected < self.apps.len()).then_some(self.selected) else {
            return;
        };
        let ctx = ui.ctx().clone();
        let tex = self.textures.app(&ctx, &self.apps[idx].def.id);
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            page_column(ui, |ui| {
                self.hero(ui, idx, tex.as_ref(), &mut actions);
                ui.add_space(18.0);
                self.body(ui, idx, &mut actions);
                ui.add_space(28.0);
            });
        });
        self.actions.extend(actions);
    }

    fn hero(&mut self, ui: &mut Ui, idx: usize, tex: Option<&egui::TextureHandle>, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let a = &self.apps[idx];
        let accent = a.def.accent_rgb().map_or(p.info, |[r, g, b]| Color32::from_rgb(r, g, b));
        let width = ui.available_width();
        let job = self.job_for(&a.def.id).map(|j| (j.id, j.action, j.version.clone(), j.status()));
        let height = if job.is_some() { 262.0 } else { 226.0 };
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let painter = ui.painter();

        // Background: the app's colour fading into the card, the brand's hexagons on the right.
        let base = if p.dark { mix(p.card, accent, 0.30) } else { mix(p.card, accent, 0.16) };
        widgets::gradient_rect(painter, rect, 18.0, base, p.card);
        widgets::hex_pattern(painter, rect.shrink(2.0), alpha(p.accent_text, if p.dark { 34 } else { 46 }), 22.0);
        painter.rect_stroke(rect, CornerRadius::same(18), Stroke::new(1.0, p.stroke), egui::StrokeKind::Inside);

        let inner = rect.shrink2(vec2(28.0, 26.0));
        let icon_rect = Rect::from_min_size(inner.min, vec2(112.0, 112.0));
        // A soft shadow under the icon.
        painter.rect_filled(icon_rect.translate(vec2(0.0, 6.0)).expand(2.0), CornerRadius::same(28), Color32::from_black_alpha(if p.dark { 70 } else { 22 }));
        match tex {
            Some(t) => {
                egui::Image::new((t.id(), icon_rect.size())).corner_radius(CornerRadius::same(26)).paint_at(ui, icon_rect);
            }
            None => {
                painter.rect_filled(icon_rect, CornerRadius::same(26), accent);
                painter.text(icon_rect.center(), Align2::CENTER_CENTER, a.def.name.chars().next().unwrap_or('?'), display(48.0), Color32::WHITE);
            }
        }

        let text_rect = Rect::from_min_max(pos2(icon_rect.right() + 28.0, inner.top()), inner.max);
        let platform = self.platform.clone();
        let pre = self.settings.prereleases;
        let installed = a.primary().cloned();
        let update = a.update(&platform, pre).cloned();
        let latest = a.latest(&platform, pre).cloned();
        // Children placed over the painted card; `new_child` leaves the page cursor alone.
        let mut col = ui.new_child(UiBuilder::new().max_rect(text_rect));
        {
            let ui = &mut col;
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add_space(4.0);
            ui.label(RichText::new(&a.def.name).font(display(34.0)).color(p.text));
            ui.label(RichText::new(a.def.tagline(i18n::code())).font(regular(15.0)).color(p.muted));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                match (&installed, &update) {
                    _ if !a.detected => {
                        widgets::chip(ui, &p, None, tr("Checking…"));
                    }
                    (Some(i), Some(u)) => {
                        widgets::chip(ui, &p, Some(p.warn), &trf("v{} available", u.version_label()));
                        if let Some(v) = &i.version {
                            widgets::chip(ui, &p, None, &trf("Version {}", v));
                        }
                    }
                    (Some(i), None) => {
                        let label = if a.running { tr("Running") } else { tr("Installed") };
                        widgets::chip(ui, &p, Some(p.accent_text), label);
                        if let Some(v) = &i.version {
                            widgets::chip(ui, &p, None, &trf("Version {}", v));
                        }
                    }
                    (None, _) => {
                        widgets::chip(ui, &p, Some(p.faint), tr("Not installed"));
                        if let Some(l) = &latest {
                            widgets::chip(ui, &p, None, &trf("Version {}", l.version_label()));
                        }
                    }
                }
                for c in a.def.categories.iter().take(3) {
                    widgets::chip(ui, &p, None, c);
                }
            });
            ui.add_space(16.0);
            match &job {
                Some((id, action, version, Status::Running(stage))) => self.progress_panel(ui, &a.def.name, *id, *action, version, stage, actions),
                _ => self.action_row(ui, idx, installed.as_ref(), update.as_ref(), latest.as_ref(), actions),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn progress_panel(&self, ui: &mut Ui, name: &str, id: u64, action: Action, version: &str, stage: &Stage, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let fill = if p.dark { Color32::from_black_alpha(70) } else { Color32::from_white_alpha(170) };
        egui::Frame::new().fill(fill).corner_radius(CornerRadius::same(14)).inner_margin(Margin::symmetric(18, 14)).stroke(Stroke::new(1.0, p.stroke)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = match action {
                Action::Install => trf("Installing {}", format!("{name} {version}")),
                Action::Update => trf("Updating to {}", version),
                Action::Uninstall => trf("Removing {}", name),
            };
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(title).font(semibold(14.5)));
                    ui.label(RichText::new(stage_detail(stage, name)).font(regular(12.5)).color(p.muted));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let cancellable = !matches!(stage, Stage::Installing | Stage::Authorizing) && action != Action::Uninstall;
                    if widgets::button(ui, &p, Style::Secondary, Some(Icon::Close), tr("Cancel"), 32.0, cancellable).clicked() {
                        actions.push(UiAction::Cancel(id));
                    }
                });
            });
            ui.add_space(8.0);
            let fraction = match stage {
                Stage::Downloading(t) if t.total > 0 => Some(t.done as f32 / t.total as f32),
                _ => None,
            };
            widgets::progress(ui, &p, fraction, ui.available_width(), 6.0);
        });
    }

    fn action_row(&self, ui: &mut Ui, idx: usize, installed: Option<&Installed>, update: Option<&Release>, latest: Option<&Release>, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let a = &self.apps[idx];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let h = 44.0;
            if a.def.coming_soon {
                widgets::button(ui, &p, Style::Secondary, Some(Icon::Hexagon), tr("More apps are on the way"), h, false);
                return;
            }
            if !a.detected {
                widgets::button(ui, &p, Style::Secondary, None, tr("Checking…"), h, false);
                return;
            }
            match (installed, update) {
                (Some(_), Some(u)) => {
                    if widgets::button(ui, &p, Style::Primary, Some(Icon::Download), &trf("Update to {}", u.version_label()), h, true).clicked() {
                        actions.push(UiAction::Update(idx));
                    }
                    if widgets::button(ui, &p, Style::Secondary, Some(Icon::Play), tr("Open"), h, true).clicked() {
                        actions.push(UiAction::Launch(idx));
                    }
                }
                (Some(_), None) => {
                    if widgets::button(ui, &p, Style::Primary, Some(Icon::Play), tr("Open"), h, true).clicked() {
                        actions.push(UiAction::Launch(idx));
                    }
                    if a.running && self.restart_pending(&a.def.id) && widgets::button(ui, &p, Style::Secondary, Some(Icon::Refresh), &trf("Restart {}", &a.def.name), h, true).clicked() {
                        actions.push(UiAction::Restart(idx));
                    }
                }
                (None, _) => match latest {
                    Some(rel) => {
                        let plans = install::install_plans(&a.def, &self.platform, rel, &self.settings);
                        if plans.is_empty() {
                            widgets::button(ui, &p, Style::Secondary, None, tr("Not available for this computer"), h, false);
                        } else {
                            let choice = self.format_choice.get(&a.def.id).copied().unwrap_or(0).min(plans.len() - 1);
                            let plan = &plans[choice];
                            if widgets::button(ui, &p, Style::Primary, Some(Icon::Download), tr("Install"), h, true).clicked() {
                                actions.push(UiAction::Install { app: idx, plan: plan.clone(), release: rel.clone() });
                            }
                            if plans.len() > 1 {
                                self.format_combo(ui, &a.def.id, &plans, choice);
                            }
                            let size = widgets::bytes(plan.asset.size);
                            ui.label(RichText::new(format!("{} · {size}", plan.target.kind().label())).font(regular(12.5)).color(p.muted));
                        }
                    }
                    None => match &a.releases {
                        None => {
                            widgets::button(ui, &p, Style::Secondary, None, tr("Checking…"), h, false);
                        }
                        Some(Err(e)) => {
                            if widgets::button(ui, &p, Style::Secondary, Some(Icon::Refresh), tr("Retry"), h, true).clicked() {
                                actions.push(UiAction::CheckUpdates);
                            }
                            ui.label(RichText::new(tr("Couldn't check for updates")).font(medium(13.0)).color(p.danger)).on_hover_text(e);
                        }
                        Some(Ok(_)) => {
                            widgets::button(ui, &p, Style::Secondary, None, tr("Not available for this computer"), h, false);
                        }
                    },
                },
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.more_menu(ui, idx, installed, actions);
                if let Some(i) = installed {
                    if widgets::icon_button(ui, &p, Icon::Folder, 40.0, tr("Show in file manager")).clicked() {
                        actions.push(UiAction::Reveal(i.location()));
                    }
                }
            });
        });
    }

    /// A finished update left the old version running.
    fn restart_pending(&self, app_id: &str) -> bool {
        self.jobs.iter().rev().find(|j| j.app == app_id).is_some_and(|j| matches!(j.status(), Status::Done(Some(o)) if o.restart_needed))
    }

    fn format_combo(&self, ui: &mut Ui, app_id: &str, plans: &[Plan], choice: usize) {
        let mut sel = choice;
        egui::ComboBox::from_id_salt(("format", app_id))
            .selected_text(RichText::new(plans[choice].target.describe()).font(medium(13.0)))
            .height(300.0)
            .show_ui(ui, |ui| {
                ui.label(RichText::new(tr("Install as")).font(semibold(11.5)).color(self.palette.faint));
                for (i, plan) in plans.iter().enumerate() {
                    ui.selectable_value(&mut sel, i, plan.target.describe());
                }
            })
            .response
            .on_hover_text(tr("Install as"));
        if sel != choice {
            // `format_choice` lives on self, which is borrowed immutably here; stash it in egui memory.
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("format-choice", app_id)), sel));
        }
    }

    fn more_menu(&self, ui: &mut Ui, idx: usize, installed: Option<&Installed>, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let a = &self.apps[idx];
        let resp = widgets::icon_button(ui, &p, Icon::More, 40.0, tr("More"));
        let releases_url = a.def.releases_url();
        let homepage = a.def.homepage.clone();
        let installed_index = installed.and_then(|i| a.installs.iter().position(|x| x == i));
        egui::Popup::menu(&resp).align(egui::RectAlign::BOTTOM_END).width(230.0).show(|ui| {
            ui.set_min_width(220.0);
            if menu_item(ui, &p, Icon::Refresh, tr("Check for updates"), p.text).clicked() {
                actions.push(UiAction::CheckUpdates);
                ui.close();
            }
            if menu_item(ui, &p, Icon::Tag, tr("Release page"), p.text).clicked() {
                actions.push(UiAction::OpenUrl(releases_url.clone()));
                ui.close();
            }
            if let Some(h) = &homepage {
                if menu_item(ui, &p, Icon::External, tr("Website"), p.text).clicked() {
                    actions.push(UiAction::OpenUrl(h.clone()));
                    ui.close();
                }
            }
            if let Some(i) = installed {
                if menu_item(ui, &p, Icon::Folder, tr("Show in file manager"), p.text).clicked() {
                    actions.push(UiAction::Reveal(i.location()));
                    ui.close();
                }
            }
            if let Some(ii) = installed_index {
                ui.separator();
                if menu_item(ui, &p, Icon::Trash, tr("Uninstall…"), p.danger).clicked() {
                    actions.push(UiAction::AskUninstall(idx, ii));
                    ui.close();
                }
            }
        });
    }

    fn body(&mut self, ui: &mut Ui, idx: usize, actions: &mut Vec<UiAction>) {
        // Pick up a format chosen in the combo box last frame.
        let id = self.apps[idx].def.id.clone();
        if let Some(sel) = ui.ctx().data_mut(|d| d.remove_temp::<usize>(egui::Id::unique(("format-choice", id.as_str())))) {
            self.format_choice.insert(id, sel);
        }
        let width = ui.available_width();
        let two = width > 760.0;
        let gap = 18.0;
        if two {
            let left = ((width - gap) * 0.6).floor();
            let right = width - gap - left;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.vertical(|ui| {
                    ui.set_width(left);
                    self.left_column(ui, idx, actions);
                });
                ui.vertical(|ui| {
                    ui.set_width(right);
                    self.right_column(ui, idx, actions);
                });
            });
        } else {
            self.left_column(ui, idx, actions);
            ui.add_space(gap);
            self.right_column(ui, idx, actions);
        }
    }

    fn left_column(&self, ui: &mut Ui, idx: usize, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let a = &self.apps[idx];
        ui.spacing_mut().item_spacing.y = 18.0;
        let description = a.def.description(i18n::code());
        if !description.is_empty() {
            widgets::card(&p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                card_title(ui, &p, tr("About"));
                ui.add(egui::Label::new(RichText::new(description).font(regular(14.0)).color(p.muted)).wrap());
            });
        }
        let shown = a.update(&self.platform, self.settings.prereleases).or_else(|| a.latest(&self.platform, self.settings.prereleases));
        widgets::card(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match (shown, &a.releases) {
                (Some(rel), _) => {
                    ui.horizontal(|ui| {
                        card_title(ui, &p, &trf("What's new in {}", rel.version_label()));
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            let r = ui.add(egui::Label::new(RichText::new(rel.date()).font(regular(12.5)).color(p.faint)));
                            let _ = r;
                        });
                    });
                    let body = rel.body.trim();
                    if body.is_empty() {
                        ui.label(RichText::new(tr("No release notes.")).color(p.faint));
                    } else {
                        markdown(ui, &p, body, 40);
                    }
                    ui.add_space(4.0);
                    if link(ui, &p, tr("Release page")).clicked() {
                        actions.push(UiAction::OpenUrl(rel.html_url.clone()));
                    }
                }
                (None, Some(Err(e))) => {
                    card_title(ui, &p, tr("Couldn't check for updates"));
                    ui.add(egui::Label::new(RichText::new(e).font(regular(13.0)).color(p.muted)).wrap());
                    ui.add_space(4.0);
                    if widgets::button(ui, &p, Style::Secondary, Some(Icon::Refresh), tr("Retry"), 0.0, true).clicked() {
                        actions.push(UiAction::CheckUpdates);
                    }
                }
                (None, _) => {
                    card_title(ui, &p, tr("Versions"));
                    ui.label(RichText::new(if a.releases.is_none() { tr("Checking…") } else { tr("Not available for this computer") }).color(p.faint));
                }
            }
        });
    }

    fn right_column(&self, ui: &mut Ui, idx: usize, actions: &mut Vec<UiAction>) {
        let p = self.palette;
        let a = &self.apps[idx];
        ui.spacing_mut().item_spacing.y = 18.0;
        let pre = self.settings.prereleases;
        let installed = a.primary();
        let latest = a.latest(&self.platform, pre);

        // Details.
        widgets::card(&p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            card_title(ui, &p, tr("Details"));
            ui.spacing_mut().item_spacing.y = 10.0;
            if let Some(i) = installed {
                let v = i.version.as_ref().map_or_else(|| tr("unknown").to_string(), ToString::to_string);
                widgets::detail_row(ui, &p, Icon::Check, tr("Installed version"), &v);
            }
            if let Some(l) = latest {
                widgets::detail_row(ui, &p, Icon::Tag, tr("Latest version"), &l.version_label());
                widgets::detail_row(ui, &p, Icon::Calendar, tr("Released"), l.date());
            }
            let plan = match (installed, latest) {
                (Some(i), Some(l)) => install::update_plan(&a.def, &self.platform, l, i).ok(),
                (None, Some(l)) => {
                    let plans = install::install_plans(&a.def, &self.platform, l, &self.settings);
                    let c = self.format_choice.get(&a.def.id).copied().unwrap_or(0);
                    plans.into_iter().nth(c)
                }
                _ => None,
            };
            let format = match (installed, &plan) {
                (Some(i), _) => Some(i.method.label()),
                (None, Some(pl)) => Some(pl.target.kind().label().to_string()),
                _ => None,
            };
            if let Some(f) = format {
                widgets::detail_row(ui, &p, Icon::Package, tr("Format"), &f);
            }
            if let Some(pl) = &plan {
                widgets::detail_row(ui, &p, Icon::Download, tr("Download size"), &widgets::bytes(pl.asset.size));
            }
            if let Some(i) = installed {
                let loc = short_path(&i.path);
                let r = widgets::detail_row(ui, &p, Icon::Folder, tr("Location"), &loc).on_hover_text(i.path.display().to_string());
                if r.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    actions.push(UiAction::Reveal(i.location()));
                }
            }
            widgets::detail_row(ui, &p, Icon::Cpu, tr("This computer"), &self.platform.label());
        });

        // Versions.
        let releases = a.usable_releases(&self.platform, pre);
        if !releases.is_empty() {
            widgets::card(&p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                card_title(ui, &p, tr("Versions"));
                ui.spacing_mut().item_spacing.y = 2.0;
                let busy = self.job_for(&a.def.id).is_some();
                for (n, rel) in releases.iter().take(6).enumerate() {
                    let is_installed = installed.and_then(|i| i.version.as_ref()).is_some_and(|v| rel.version().as_ref() == Some(v));
                    if let Some(act) = version_row(ui, &p, idx, a, rel, n == 0, is_installed, busy, installed, &self.platform, &self.settings, self.format_choice.get(&a.def.id).copied().unwrap_or(0)) {
                        actions.push(act);
                    }
                }
            });
        }

        // Other copies.
        if a.installs.len() > 1 {
            widgets::card(&p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                card_title(ui, &p, tr("Also installed"));
                for (n, i) in a.installs.iter().enumerate().skip(1) {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let v = i.version.as_ref().map_or_else(|| tr("unknown").to_string(), ToString::to_string);
                            ui.label(RichText::new(format!("{} · {}", i.method.label(), v)).font(medium(13.0)));
                            ui.add(egui::Label::new(RichText::new(short_path(&i.path)).font(regular(12.0)).color(p.faint)).truncate());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::icon_button(ui, &p, Icon::Trash, 30.0, tr("Uninstall…")).clicked() {
                                actions.push(UiAction::AskUninstall(idx, n));
                            }
                            if widgets::icon_button(ui, &p, Icon::Folder, 30.0, tr("Show in file manager")).clicked() {
                                actions.push(UiAction::Reveal(i.location()));
                            }
                        });
                    });
                }
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn version_row(
    ui: &mut Ui,
    p: &Palette,
    idx: usize,
    a: &AppEntry,
    rel: &Release,
    newest: bool,
    is_installed: bool,
    busy: bool,
    installed: Option<&Installed>,
    platform: &crate::platform::Platform,
    settings: &crate::settings::Settings,
    choice: usize,
) -> Option<UiAction> {
    let mut out = None;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let hovered = resp.hovered() || ui.rect_contains_pointer(rect);
    if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(8), p.raised);
    }
    let painter = ui.painter();
    let y = rect.center().y;
    let name = painter.text(pos2(rect.left() + 10.0, y), Align2::LEFT_CENTER, rel.version_label(), semibold(13.5), p.text);
    let mut x = name.right() + 8.0;
    let mut tag = |text: &str, color: Color32| {
        let g = painter.layout_no_wrap(text.to_string(), medium(11.0), color);
        let r = Rect::from_min_size(pos2(x, y - 9.0), vec2(g.size().x + 12.0, 18.0));
        painter.rect_filled(r, CornerRadius::same(9), alpha(color, 30));
        painter.galley(pos2(r.left() + 6.0, y - g.size().y / 2.0), g, color);
        x = r.right() + 6.0;
    };
    if newest {
        tag(tr("Latest"), p.accent_text);
    }
    if rel.prerelease {
        tag(tr("Pre-release"), p.warn);
    }
    if is_installed {
        tag(tr("Installed"), p.info);
    }
    if hovered && !is_installed && !busy {
        let plan = match installed {
            Some(i) => install::update_plan(&a.def, platform, rel, i).ok(),
            None => install::install_plans(&a.def, platform, rel, settings).into_iter().nth(choice),
        };
        if let Some(plan) = plan {
            let btn_rect = Rect::from_min_max(pos2(rect.right() - 180.0, rect.top() + 5.0), pos2(rect.right() - 6.0, rect.bottom() - 5.0));
            let mut child = ui.new_child(UiBuilder::new().max_rect(btn_rect).layout(Layout::right_to_left(Align::Center)));
            if widgets::button(&mut child, p, Style::Ghost, Some(Icon::Download), tr("Install this version"), 30.0, true).clicked() {
                out = Some(UiAction::Install { app: idx, plan, release: rel.clone() });
            }
            return out;
        }
    }
    ui.painter().text(pos2(rect.right() - 10.0, y), Align2::RIGHT_CENTER, rel.date(), regular(12.5), p.faint);
    out
}

/// Centre the page in a column at most [`MAX_WIDTH`] wide.
fn page_column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let full = ui.available_width();
    let width = (full - 64.0).min(MAX_WIDTH);
    let side = ((full - width) / 2.0).max(24.0);
    egui::Frame::new().inner_margin(Margin { left: side as i8, right: 0, top: 40, bottom: 0 }).show(ui, |ui| {
        ui.set_width(width);
        add(ui);
    });
}

pub(super) fn card_title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(semibold(15.5)).color(p.text));
    ui.add_space(2.0);
}

fn link(ui: &mut Ui, p: &Palette, text: &str) -> egui::Response {
    let resp = ui.horizontal(|ui| {
        let r = ui.add(egui::Label::new(RichText::new(text).font(medium(13.0)).color(p.accent_text)).sense(Sense::click()));
        let (ir, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
        icon(ui.painter(), ir, Icon::External, p.accent_text);
        r
    });
    resp.inner.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn menu_item(ui: &mut Ui, p: &Palette, icon_kind: Icon, label: &str, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(200.0), 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), p.raised_hover);
    }
    icon(ui.painter(), Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(15.0, 15.0)), icon_kind, if color == p.text { p.muted } else { color });
    ui.painter().text(pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, label, medium(13.5), color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// What a stage means, with the numbers.
fn stage_detail(stage: &Stage, name: &str) -> String {
    match stage {
        Stage::Downloading(t) if t.total > 0 => {
            let mut s = trf("{} of {}", widgets::bytes(t.done)).replacen("{}", &widgets::bytes(t.total), 1);
            if t.speed > 1.0 {
                s.push_str(&format!(" · {}/s", widgets::bytes(t.speed as u64)));
                let left = (t.total.saturating_sub(t.done)) as f64 / t.speed;
                if left.is_finite() && left >= 1.0 {
                    s.push_str(" · ");
                    s.push_str(&trf("{} left", duration(left)));
                }
            }
            s
        }
        _ => stage_label(stage, name),
    }
}

fn duration(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

/// `~/Applications/DraftCraft.app`
pub(super) fn short_path(path: &std::path::Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    path.strip_prefix(&home).map_or_else(|_| path.display().to_string(), |r| format!("~/{}", r.display()))
}

/// GitHub release notes, enough of them: headings, bullets, **bold**, `code`, [links](…).
fn markdown(ui: &mut Ui, p: &Palette, text: &str, max_lines: usize) {
    ui.spacing_mut().item_spacing.y = 6.0;
    let mut lines = 0;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with("<!--") {
            continue;
        }
        lines += 1;
        if lines > max_lines {
            ui.label(RichText::new("…").color(p.faint));
            break;
        }
        let trimmed = line.trim_start();
        if let Some(h) = trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("#### ")) {
            ui.add_space(2.0);
            ui.label(RichText::new(clean_inline(h)).font(semibold(13.5)).color(p.text));
        } else if let Some(h) = trimmed.strip_prefix("## ").or_else(|| trimmed.strip_prefix("# ")) {
            ui.add_space(4.0);
            ui.label(RichText::new(clean_inline(h)).font(semibold(14.5)).color(p.text));
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
            let indent = (line.len() - trimmed.len()) as f32 * 4.0;
            ui.horizontal_top(|ui| {
                ui.add_space(4.0 + indent);
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
                ui.painter().circle_filled(pos2(r.center().x, r.top() + 9.5), 2.5, p.accent_text);
                ui.add(egui::Label::new(inline_job(item, p, ui.available_width())).wrap());
            });
        } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
            ui.separator();
        } else {
            ui.add(egui::Label::new(inline_job(trimmed, p, ui.available_width())).wrap());
        }
    }
}

/// Strip markdown marks for plain display.
fn clean_inline(s: &str) -> String {
    let mut out = String::new();
    for (text, _, _) in inline_spans(s) {
        out.push_str(&text);
    }
    out
}

/// Split into (text, bold, code) spans; links keep their text.
fn inline_spans(s: &str) -> Vec<(String, bool, bool)> {
    let mut spans = Vec::new();
    let mut cur = String::new();
    let (mut bold, mut code) = (false, false);
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush = |cur: &mut String, spans: &mut Vec<(String, bool, bool)>, bold: bool, code: bool| {
        if !cur.is_empty() {
            spans.push((std::mem::take(cur), bold, code));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            flush(&mut cur, &mut spans, bold, code);
            code = !code;
            i += 1;
        } else if !code && c == '*' && chars.get(i + 1) == Some(&'*') {
            flush(&mut cur, &mut spans, bold, code);
            bold = !bold;
            i += 2;
        } else if !code && c == '[' {
            // [text](url) → text
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close + 2..].find(')') {
                    cur.push_str(&rest[..close]);
                    i += 1 + rest[..close + 2 + end + 1].chars().count();
                    continue;
                }
            }
            cur.push(c);
            i += 1;
        } else {
            cur.push(c);
            i += 1;
        }
    }
    flush(&mut cur, &mut spans, bold, code);
    spans
}

fn inline_job(s: &str, p: &Palette, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    for (text, bold, code) in inline_spans(s) {
        let (font, color, bg) = if code {
            (egui::FontId::monospace(12.5), p.text, p.raised)
        } else if bold {
            (semibold(13.5), p.text, Color32::TRANSPARENT)
        } else {
            (regular(13.5), p.muted, Color32::TRANSPARENT)
        };
        job.append(&text, 0.0, TextFormat { font_id: font, color, background: bg, line_height: Some(19.0), ..Default::default() });
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markdown() {
        let s = inline_spans("Fix **snap** in `LINE`, see [the docs](https://x.y/z).");
        let plain: String = s.iter().map(|(t, _, _)| t.as_str()).collect();
        assert_eq!(plain, "Fix snap in LINE, see the docs.");
        assert!(s.iter().any(|(t, b, _)| t == "snap" && *b));
        assert!(s.iter().any(|(t, _, c)| t == "LINE" && *c));
    }
}
