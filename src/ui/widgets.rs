//! Drawn widgets: icons, buttons, chips, switches, progress bars, cards.

use std::f32::consts::{PI, TAU};

use egui::epaint::{Mesh, PathShape, Shadow, Vertex, WHITE_UV};
use egui::{Color32, CornerRadius, Margin, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use super::theme::{Palette, alpha, medium, mix, regular, semibold};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Play,
    Download,
    Refresh,
    More,
    Folder,
    Trash,
    Gear,
    Library,
    Inbox,
    Check,
    Close,
    External,
    Alert,
    Info,
    Hexagon,
    Tag,
    Cpu,
    Calendar,
    Package,
}

/// Paint `icon` inside `rect` (drawn on a 16×16 grid).
pub fn icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0 * s, 8.0 * s);
    let p = |x: f32, y: f32| o + vec2(x * s, y * s);
    let w = (1.5 * s).max(1.2);
    let st = Stroke::new(w, color);
    let line = |pts: &[Pos2]| {
        painter.add(PathShape::line(pts.to_vec(), st));
    };
    match icon {
        Icon::Play => {
            painter.add(Shape::convex_polygon(vec![p(4.5, 2.5), p(13.5, 8.0), p(4.5, 13.5)], color, Stroke::NONE));
        }
        Icon::Download => {
            line(&[p(8.0, 2.0), p(8.0, 10.5)]);
            line(&[p(4.5, 7.0), p(8.0, 10.5), p(11.5, 7.0)]);
            line(&[p(3.0, 13.5), p(13.0, 13.5)]);
        }
        Icon::Refresh => {
            let c = p(8.0, 8.0);
            let r = 5.2 * s;
            let pts: Vec<Pos2> = (0..=20).map(|i| {
                let a = -PI * 0.15 + (i as f32 / 20.0) * PI * 1.55;
                c + vec2(a.cos(), a.sin()) * r
            }).collect();
            let end = pts[0];
            line(&pts);
            line(&[end + vec2(-3.2 * s, -0.6 * s), end, end + vec2(0.4 * s, -3.2 * s)]);
        }
        Icon::More => {
            for x in [3.5, 8.0, 12.5] {
                painter.circle_filled(p(x, 8.0), 1.4 * s, color);
            }
        }
        Icon::Folder => {
            line(&[p(2.0, 4.0), p(6.0, 4.0), p(7.5, 5.5), p(14.0, 5.5), p(14.0, 13.0), p(2.0, 13.0), p(2.0, 4.0)]);
        }
        Icon::Trash => {
            line(&[p(2.5, 4.5), p(13.5, 4.5)]);
            line(&[p(6.0, 4.5), p(6.0, 2.5), p(10.0, 2.5), p(10.0, 4.5)]);
            line(&[p(4.0, 4.5), p(4.8, 14.0), p(11.2, 14.0), p(12.0, 4.5)]);
            line(&[p(6.8, 7.0), p(6.8, 11.5)]);
            line(&[p(9.2, 7.0), p(9.2, 11.5)]);
        }
        Icon::Gear => {
            let c = p(8.0, 8.0);
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                let d = vec2(a.cos(), a.sin());
                painter.line_segment([c + d * 4.6 * s, c + d * 6.8 * s], Stroke::new(2.2 * s, color));
            }
            painter.circle_stroke(c, 4.6 * s, st);
            painter.circle_stroke(c, 1.8 * s, st);
        }
        Icon::Library => {
            for (x, y) in [(2.0, 2.0), (9.0, 2.0), (2.0, 9.0), (9.0, 9.0)] {
                painter.rect_stroke(Rect::from_min_size(p(x, y), vec2(5.0 * s, 5.0 * s)), CornerRadius::same((1.5 * s) as u8), st, StrokeKind::Middle);
            }
        }
        Icon::Inbox => {
            line(&[p(8.0, 1.5), p(8.0, 8.5)]);
            line(&[p(5.2, 5.8), p(8.0, 8.5), p(10.8, 5.8)]);
            line(&[p(2.0, 9.5), p(5.0, 9.5), p(6.0, 11.5), p(10.0, 11.5), p(11.0, 9.5), p(14.0, 9.5), p(14.0, 14.0), p(2.0, 14.0), p(2.0, 9.5)]);
        }
        Icon::Check => line(&[p(3.0, 8.5), p(6.5, 12.0), p(13.0, 4.5)]),
        Icon::Close => {
            line(&[p(4.0, 4.0), p(12.0, 12.0)]);
            line(&[p(12.0, 4.0), p(4.0, 12.0)]);
        }
        Icon::External => {
            line(&[p(7.0, 3.0), p(3.0, 3.0), p(3.0, 13.0), p(13.0, 13.0), p(13.0, 9.0)]);
            line(&[p(9.0, 3.0), p(13.0, 3.0), p(13.0, 7.0)]);
            line(&[p(13.0, 3.0), p(7.5, 8.5)]);
        }
        Icon::Alert => {
            line(&[p(8.0, 2.0), p(14.5, 13.5), p(1.5, 13.5), p(8.0, 2.0)]);
            line(&[p(8.0, 6.0), p(8.0, 9.5)]);
            painter.circle_filled(p(8.0, 11.6), 0.9 * s, color);
        }
        Icon::Info => {
            painter.circle_stroke(p(8.0, 8.0), 6.2 * s, st);
            line(&[p(8.0, 7.2), p(8.0, 11.5)]);
            painter.circle_filled(p(8.0, 4.9), 0.9 * s, color);
        }
        Icon::Hexagon => {
            let pts: Vec<Pos2> = (0..=6).map(|i| {
                let a = (i as f32 / 6.0) * TAU + PI / 6.0;
                p(8.0, 8.0) + vec2(a.cos(), a.sin()) * 6.5 * s
            }).collect();
            line(&pts);
        }
        Icon::Tag => {
            line(&[p(2.5, 2.5), p(8.0, 2.5), p(14.0, 8.5), p(8.5, 14.0), p(2.5, 8.0), p(2.5, 2.5)]);
            painter.circle_filled(p(5.5, 5.5), 1.0 * s, color);
        }
        Icon::Cpu => {
            painter.rect_stroke(Rect::from_min_max(p(4.0, 4.0), p(12.0, 12.0)), CornerRadius::same((1.5 * s) as u8), st, StrokeKind::Middle);
            for t in [6.5, 9.5] {
                line(&[p(t, 1.5), p(t, 4.0)]);
                line(&[p(t, 12.0), p(t, 14.5)]);
                line(&[p(1.5, t), p(4.0, t)]);
                line(&[p(12.0, t), p(14.5, t)]);
            }
        }
        Icon::Calendar => {
            painter.rect_stroke(Rect::from_min_max(p(2.0, 3.5), p(14.0, 14.0)), CornerRadius::same((1.5 * s) as u8), st, StrokeKind::Middle);
            line(&[p(2.0, 7.0), p(14.0, 7.0)]);
            line(&[p(5.5, 2.0), p(5.5, 5.0)]);
            line(&[p(10.5, 2.0), p(10.5, 5.0)]);
        }
        Icon::Package => {
            line(&[p(8.0, 1.8), p(14.0, 5.0), p(14.0, 11.0), p(8.0, 14.2), p(2.0, 11.0), p(2.0, 5.0), p(8.0, 1.8)]);
            line(&[p(2.0, 5.0), p(8.0, 8.2), p(14.0, 5.0)]);
            line(&[p(8.0, 8.2), p(8.0, 14.2)]);
        }
    }
}

/// A rounded-rect polygon (convex) with `segments` per corner.
fn rounded_points(rect: Rect, r: f32, segments: usize) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -PI / 2.0),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), PI / 2.0),
        (pos2(rect.left() + r, rect.top() + r), PI),
    ];
    let mut pts = Vec::with_capacity(4 * (segments + 1));
    for (c, start) in corners {
        for i in 0..=segments {
            let a = start + (i as f32 / segments as f32) * PI / 2.0;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts
}

/// A rounded rectangle filled with a diagonal gradient from `from` (top left) to `to`.
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    let pts = rounded_points(rect, radius, 10);
    let mut mesh = Mesh::default();
    let color_at = |q: Pos2| {
        let t = ((q.x - rect.left()) / rect.width() * 0.75 + (q.y - rect.top()) / rect.height() * 0.25).clamp(0.0, 1.0);
        mix(from, to, t)
    };
    mesh.vertices.push(Vertex { pos: rect.center(), uv: WHITE_UV, color: color_at(rect.center()) });
    for q in &pts {
        mesh.vertices.push(Vertex { pos: *q, uv: WHITE_UV, color: color_at(*q) });
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

/// The brand's hexagon lattice, faint, filling `rect` and fading to the left.
pub fn hex_pattern(painter: &Painter, rect: Rect, color: Color32, size: f32) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let w = 3f32.sqrt() * size;
    let h = 1.5 * size;
    let rows = (rect.height() / h).ceil() as i32 + 2;
    let cols = (rect.width() / w).ceil() as i32 + 2;
    for row in -1..rows {
        for col in -1..cols {
            let cx = rect.left() + col as f32 * w + if row % 2 == 0 { 0.0 } else { w / 2.0 };
            let cy = rect.top() + row as f32 * h;
            let fade = ((cx - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let a = (color.a() as f32 * fade * fade) as u8;
            if a < 2 {
                continue;
            }
            let c = alpha(color, a);
            let pts: Vec<Pos2> = (0..6).map(|i| {
                let ang = i as f32 / 6.0 * TAU + PI / 6.0;
                pos2(cx, cy) + vec2(ang.cos(), ang.sin()) * (size * 0.86)
            }).collect();
            painter.add(PathShape::closed_line(pts, Stroke::new(1.0, c)));
        }
    }
}

/// Card background frame.
pub fn card(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.stroke))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .shadow(Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(if p.dark { 40 } else { 12 }) })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

/// A button with an optional icon. `height` 0 means the default (34).
pub fn button(ui: &mut Ui, p: &Palette, style: Style, icon_kind: Option<Icon>, label: &str, height: f32, enabled: bool) -> Response {
    let height = if height == 0.0 { 34.0 } else { height };
    let font = if height >= 42.0 { semibold(15.0) } else { medium(13.5) };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::WHITE);
    let icon_w = if icon_kind.is_some() { height * 0.42 + if label.is_empty() { 0.0 } else { 8.0 } } else { 0.0 };
    let pad = if label.is_empty() { (height - height * 0.42) / 2.0 } else { height * 0.5 };
    let width = (galley.size().x + icon_w + pad * 2.0).max(if label.is_empty() { height } else { 0.0 });
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();
        let (fill, fg, stroke) = match style {
            Style::Primary => (
                if pressed { p.accent_pressed } else if hovered { p.accent_hover } else { p.accent },
                p.on_accent,
                Stroke::NONE,
            ),
            Style::Secondary => (if hovered { p.raised_hover } else { p.raised }, p.text, Stroke::new(1.0, p.stroke)),
            Style::Ghost => (if hovered { p.raised } else { Color32::TRANSPARENT }, if hovered { p.text } else { p.muted }, Stroke::NONE),
            Style::Danger => (if hovered { alpha(p.danger, 40) } else { alpha(p.danger, 22) }, p.danger, Stroke::new(1.0, alpha(p.danger, 70))),
        };
        let (fill, fg) = if enabled { (fill, fg) } else { (alpha(p.raised, 160), p.faint) };
        let r = CornerRadius::same((height * 0.28) as u8);
        ui.painter().rect(rect, r, fill, stroke, StrokeKind::Inside);
        let mut x = rect.left() + pad;
        if let Some(k) = icon_kind {
            let sz = height * 0.42;
            let ir = Rect::from_center_size(pos2(x + sz / 2.0, rect.center().y), vec2(sz, sz));
            icon(ui.painter(), ir, k, fg);
            x += icon_w;
        }
        if !label.is_empty() {
            let pos = pos2(x, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(pos, galley, fg);
        }
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A square icon-only button.
pub fn icon_button(ui: &mut Ui, p: &Palette, icon_kind: Icon, size: f32, tip: &str) -> Response {
    button(ui, p, Style::Secondary, Some(icon_kind), "", size, true).on_hover_text(tip)
}

/// A small rounded label with a coloured dot.
pub fn chip(ui: &mut Ui, p: &Palette, dot: Option<Color32>, text: &str) -> Response {
    let font = medium(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, p.text);
    let dot_w = if dot.is_some() { 14.0 } else { 0.0 };
    let size = vec2(galley.size().x + 20.0 + dot_w, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let fill = if p.dark { Color32::from_white_alpha(14) } else { Color32::from_black_alpha(10) };
    ui.painter().rect(rect, CornerRadius::same(12), fill, Stroke::new(1.0, if p.dark { Color32::from_white_alpha(16) } else { Color32::from_black_alpha(14) }), StrokeKind::Inside);
    let mut x = rect.left() + 10.0;
    if let Some(c) = dot {
        ui.painter().circle_filled(pos2(x + 3.5, rect.center().y), 3.5, c);
        x += dot_w;
    }
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, p.text);
    resp
}

/// An iOS-style switch.
pub fn toggle(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let size = vec2(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let track = mix(if p.dark { p.raised_hover } else { p.stroke }, p.accent, t);
    ui.painter().rect_filled(rect, CornerRadius::same(11), track);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    let knob = mix(if p.dark { p.muted } else { Color32::WHITE }, p.on_accent, t);
    ui.painter().circle_filled(pos2(x, rect.center().y), 8.0, knob);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A segmented control; returns true when the choice changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, p: &Palette, value: &mut T, options: &[(T, String)]) -> bool {
    let font = medium(13.0);
    let galleys: Vec<_> = options.iter().map(|(_, l)| ui.painter().layout_no_wrap(l.clone(), font.clone(), p.text)).collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 28.0).collect();
    let total = widths.iter().sum::<f32>() + 6.0;
    let (rect, base) = ui.allocate_exact_size(vec2(total, 32.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), p.raised);
    let mut x = rect.left() + 3.0;
    let mut changed = false;
    for (i, ((val, _), galley)) in options.iter().zip(galleys).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(widths[i], 26.0));
        let resp = ui.interact(r, base.id.with(("seg", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let selected = *value == *val;
        if selected {
            ui.painter().rect(r, CornerRadius::same(7), if p.dark { p.raised_hover } else { p.card }, Stroke::new(1.0, p.stroke), StrokeKind::Inside);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(7), alpha(p.raised_hover, 140));
        }
        let color = if selected { p.text } else { p.muted };
        ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
        if resp.clicked() && !selected {
            *value = *val;
            changed = true;
        }
        x += widths[i];
    }
    changed
}

/// A progress bar; `None` is indeterminate (a moving stripe).
pub fn progress(ui: &mut Ui, p: &Palette, fraction: Option<f32>, width: f32, height: f32) {
    let (rect, bar) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let r = CornerRadius::same((height / 2.0) as u8);
    ui.painter().rect_filled(rect, r, if p.dark { Color32::from_white_alpha(18) } else { Color32::from_black_alpha(16) });
    match fraction {
        Some(f) => {
            let f = ui.ctx().animate_value_with_time(bar.id.with("progress"), f.clamp(0.0, 1.0), 0.25);
            if f > 0.0 {
                let fill = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(height), height));
                ui.painter().rect_filled(fill, r, p.accent);
            }
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.3;
            let x = rect.left() - seg + ((t * 0.7).fract()) * (rect.width() + seg);
            let fill = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + seg).min(rect.right()), rect.bottom()));
            if fill.width() > 0.0 {
                ui.painter().rect_filled(fill, r, p.accent);
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Small caps section label: "APPS".
pub fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    let job = spaced(text, semibold(11.0), p.faint, 1.2);
    ui.label(job);
}

/// A layout job with letter spacing.
pub fn spaced(text: &str, font: egui::FontId, color: Color32, spacing: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(&text.to_uppercase(), 0.0, egui::TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    job
}

/// A row in a details list: icon, label, value.
pub fn detail_row(ui: &mut Ui, p: &Palette, icon_kind: Icon, label: &str, value: &str) -> Response {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
        icon(ui.painter(), r, icon_kind, p.faint);
        ui.add_space(2.0);
        ui.label(egui::RichText::new(label).font(regular(13.0)).color(p.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(value).font(medium(13.0)).color(p.text)).truncate())
        })
        .inner
    })
    .inner
}

/// "75.4 MB"
pub fn bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.2} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.0} KB", n / 1e3)
    } else {
        format!("{n} B")
    }
}
