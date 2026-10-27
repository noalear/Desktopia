//! Colours, fonts and the look of the window: a field guide read by the light of
//! Vesper's twilight.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::sync::Arc;
use vesper_chronicle::{Kind, Traits};

pub const BG: Color32 = Color32::from_rgb(11, 9, 18);
pub const PANEL: Color32 = Color32::from_rgb(17, 14, 27);
pub const CARD: Color32 = Color32::from_rgb(26, 22, 40);
pub const LINE: Color32 = Color32::from_rgb(50, 43, 72);
pub const TEXT: Color32 = Color32::from_rgb(232, 227, 242);
pub const DIM: Color32 = Color32::from_rgb(150, 142, 172);
pub const FAINT: Color32 = Color32::from_rgb(92, 85, 116);
pub const AMBER: Color32 = Color32::from_rgb(244, 172, 72);
pub const TEAL: Color32 = Color32::from_rgb(64, 200, 188);
pub const CRIMSON: Color32 = Color32::from_rgb(222, 62, 84);
pub const VIOLET: Color32 = Color32::from_rgb(156, 114, 236);
pub const ACID: Color32 = Color32::from_rgb(164, 224, 66);
pub const CYAN: Color32 = Color32::from_rgb(78, 202, 244);
pub const MAGENTA: Color32 = Color32::from_rgb(226, 88, 204);
pub const BONE: Color32 = Color32::from_rgb(226, 214, 190);
pub const GOLD: Color32 = Color32::from_rgb(252, 214, 96);
pub const EMBER: Color32 = Color32::from_rgb(240, 104, 50);

pub fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Origin => TEAL,
        Kind::Extinction => CRIMSON,
        Kind::Death => BONE,
        Kind::War => EMBER,
        Kind::Plague => ACID,
        Kind::Famine => AMBER,
        Kind::Society => VIOLET,
        Kind::First => GOLD,
        Kind::Nature => CYAN,
        Kind::Era => MAGENTA,
    }
}

/// HSV to colour, all in 0..1.
pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let f = |u: f32| ((u + m) * 255.0).round() as u8;
    Color32::from_rgb(f(r), f(g), f(b))
}

/// A species' colour, from its most visible look gene.
pub fn species_color(t: &Traits) -> Color32 {
    hsv(t.look[0], 0.62, 0.92)
}

/// A light signal byte as a colour swatch.
pub fn signal_color(b: u8) -> Color32 {
    hsv(b as f32 / 255.0, 0.75, 1.0)
}

pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}
pub fn serif_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif-bold".into()))
}
pub fn italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("italic".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "Spectral", include_bytes!("../fonts/Spectral-Regular.ttf"));
    add(&mut fonts, "Spectral-SemiBold", include_bytes!("../fonts/Spectral-SemiBold.ttf"));
    add(&mut fonts, "Spectral-Italic", include_bytes!("../fonts/Spectral-Italic.ttf"));
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str| {
        let mut v = vec![first.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("Inter"));
    fonts.families.insert(FontFamily::Name("bold".into()), family("Inter-SemiBold"));
    fonts.families.insert(FontFamily::Name("serif".into()), family("Spectral"));
    fonts.families.insert(FontFamily::Name("serif-bold".into()), family("Spectral-SemiBold"));
    fonts.families.insert(FontFamily::Name("italic".into()), family("Spectral-Italic"));
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Heading, serif_bold(26.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = PANEL;
        v.window_fill = CARD;
        v.extreme_bg_color = BG;
        v.faint_bg_color = CARD;
        v.code_bg_color = CARD;
        v.hyperlink_color = AMBER;
        v.selection.bg_fill = AMBER.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, AMBER);
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_corner_radius = CornerRadius::same(10);
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        for (state, fill) in [
            (&mut w.inactive, CARD),
            (&mut w.hovered, Color32::from_rgb(40, 34, 62)),
            (&mut w.active, Color32::from_rgb(54, 46, 84)),
            (&mut w.open, Color32::from_rgb(40, 34, 62)),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.corner_radius = CornerRadius::same(6);
            state.bg_stroke = Stroke::new(1.0, LINE);
            state.fg_stroke = Stroke::new(1.0, TEXT);
        }
        w.hovered.bg_stroke = Stroke::new(1.0, AMBER.gamma_multiply(0.6));
    });
}
