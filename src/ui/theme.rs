//! The launcher's look, built from the D.E.V. brand: ink `#14171B`, lime `#B6EE2B` (`#6FA800`
//! on light backgrounds), bone `#F4F1EA` and the slate greys of the wordmark.

use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::sync::Arc;

use crate::assets;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// The window behind everything.
    pub bg: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    /// Hovered rows, secondary buttons.
    pub raised: Color32,
    pub raised_hover: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    /// Lime: primary buttons, progress, the selected item.
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    /// Text on a lime fill.
    pub on_accent: Color32,
    /// Lime for text and small marks, readable on this background.
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub warn: Color32,
    pub danger: Color32,
    pub info: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const INK: Color32 = hex(0x14171B);
pub const LIME: Color32 = hex(0xB6EE2B);
pub const BONE: Color32 = hex(0xF4F1EA);

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: hex(0x0E1013),
            sidebar: INK,
            card: hex(0x181C21),
            raised: hex(0x1F242A),
            raised_hover: hex(0x272D34),
            stroke: hex(0x262B32),
            text: BONE,
            muted: hex(0x9AA1A9),
            faint: hex(0x6B747F),
            accent: LIME,
            accent_hover: hex(0xC7F55C),
            accent_pressed: hex(0x9FD61A),
            on_accent: INK,
            accent_text: LIME,
            accent_soft: Color32::from_rgba_unmultiplied(0xB6, 0xEE, 0x2B, 28),
            warn: hex(0xF2B33D),
            danger: hex(0xFF6B5E),
            info: hex(0x3EA6E8),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: BONE,
            sidebar: hex(0xEBE7DD),
            card: hex(0xFFFFFF),
            raised: hex(0xF1EEE6),
            raised_hover: hex(0xE6E2D7),
            stroke: hex(0xDDD8CC),
            text: INK,
            muted: hex(0x5B626B),
            faint: hex(0x8A9099),
            accent: LIME,
            accent_hover: hex(0xC2F248),
            accent_pressed: hex(0xA4DA1C),
            on_accent: INK,
            accent_text: hex(0x5E8F00),
            accent_soft: Color32::from_rgba_unmultiplied(0x6F, 0xA8, 0x00, 30),
            warn: hex(0xB7791F),
            danger: hex(0xD9443A),
            info: hex(0x2B7FB8),
        }
    }
}

/// Font families beyond egui's two.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("inter", assets::INTER_REGULAR),
        ("inter-medium", assets::INTER_MEDIUM),
        ("inter-semibold", assets::INTER_SEMIBOLD),
        ("montserrat-bold", assets::MONTSERRAT_BOLD),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay behind ours for symbols and emoji.
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, family("inter"));
    fonts.families.insert(FontFamily::Name("medium".into()), family("inter-medium"));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("inter-semibold"));
    fonts.families.insert(FontFamily::Name("display".into()), family("montserrat-bold"));
    ctx.set_fonts(fonts);
}

/// Make egui's own widgets (combo boxes, scroll bars, menus, text fields) match the palette.
pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut visuals = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.card;
    visuals.extreme_bg_color = if p.dark { hex(0x0B0D10) } else { hex(0xFFFFFF) };
    visuals.faint_bg_color = p.raised;
    visuals.window_stroke = Stroke::new(1.0, p.stroke);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }) };
    visuals.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if p.dark { 110 } else { 35 }) };
    visuals.hyperlink_color = p.accent_text;
    visuals.selection.bg_fill = p.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0, p.accent_text);
    visuals.override_text_color = Some(p.text);
    let w = &mut visuals.widgets;
    for (state, fill, stroke) in [
        (&mut w.noninteractive, p.card, p.stroke),
        (&mut w.inactive, p.raised, p.stroke),
        (&mut w.hovered, p.raised_hover, p.faint),
        (&mut w.active, p.raised_hover, p.accent_text),
        (&mut w.open, p.raised_hover, p.stroke),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.corner_radius = CornerRadius::same(8);
        state.fg_stroke = Stroke::new(1.0, p.text);
    }
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.muted);
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.stroke);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, medium(14.0)),
            (TextStyle::Heading, semibold(20.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = true;
        style.spacing.menu_margin = egui::Margin::same(6);
        style.interaction.selectable_labels = false;
    });
}

/// `a` faded to `alpha` (0–255).
pub fn alpha(c: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Linear mix of two colours.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(ar, br), l(ag, bg), l(ab, bb), l(aa, ba))
}
