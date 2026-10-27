//! Small pieces shared by the pages: links, paragraphs, tiles, bars and charts.

use crate::theme::*;
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, RichText, Sense, Stroke, Ui, pos2, vec2};
use vesper_chronicle::history::Series;
use vesper_chronicle::{Entry, History, Piece, Ref};

/// Where the user asked to go this frame.
#[derive(Default)]
pub struct Nav {
    pub go: Option<Ref>,
}

/// "Day 3, 14:22", counting from the world's first day.
pub fn when(tick: f64) -> String {
    let day = (tick / 86_400.0).floor() as u64 + 1;
    let secs = tick.rem_euclid(86_400.0) as u64;
    format!("Day {day}, {:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

pub fn day_of(tick: f64) -> u64 {
    (tick / 86_400.0).floor() as u64 + 1
}

pub fn clock(tick: f64) -> String {
    let secs = tick.rem_euclid(86_400.0) as u64;
    format!("{:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

/// "3 hours ago".
pub fn ago(secs: f64) -> String {
    if secs < 60.0 { "just now".into() } else { format!("{} ago", vesper_chronicle::names::span(secs)) }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A clickable name.
pub fn link(ui: &mut Ui, nav: &mut Nav, text: impl Into<String>, r: Ref) -> egui::Response {
    link_styled(ui, nav, RichText::new(text.into()).color(AMBER), r)
}

pub fn link_styled(ui: &mut Ui, nav: &mut Nav, text: RichText, r: Ref) -> egui::Response {
    let resp = ui.add(egui::Label::new(text).sense(Sense::click()).selectable(false));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        let rect = resp.rect;
        ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, AMBER.gamma_multiply(0.7)));
    }
    if resp.clicked() {
        nav.go = Some(r);
    }
    resp
}

/// Entry text with its names as links, wrapped like a paragraph.
pub fn paragraph(ui: &mut Ui, nav: &mut Nav, h: &History, text: &str, font: egui::FontId, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for piece in h.pieces(text) {
            match piece {
                Piece::Text(t) => {
                    ui.add(egui::Label::new(RichText::new(t).font(font.clone()).color(color)).selectable(false));
                }
                Piece::Link(r, t) => {
                    link_styled(ui, nav, RichText::new(t).font(font.clone()).color(AMBER), r);
                }
            }
        }
    });
}

/// One line of the annals.
pub fn entry(ui: &mut Ui, nav: &mut Nav, h: &History, e: &Entry, show_day: bool) {
    let color = kind_color(e.kind);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (rect, _) = ui.allocate_exact_size(vec2(4.0, 40.0), Sense::hover());
        let alpha = match e.weight {
            3 => 1.0,
            2 => 0.75,
            _ => 0.45,
        };
        ui.painter().rect_filled(rect, CornerRadius::same(2), color.gamma_multiply(alpha));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                let stamp = if show_day { when(e.tick) } else { clock(e.tick) };
                ui.label(RichText::new(stamp).small().color(FAINT))
                    .on_hover_text(ago((h.wall - e.wall).max(0.0)));
                ui.label(RichText::new(e.kind.label().to_uppercase()).font(bold(10.5)).color(color.gamma_multiply(0.9)));
            });
            let size = match e.weight {
                3 => 19.0,
                2 => 17.0,
                _ => 16.0,
            };
            ui.label(RichText::new(&e.title).font(serif_bold(size)).color(TEXT));
            paragraph(ui, nav, h, &e.text, serif(15.5), Color32::from_rgb(208, 202, 222));
        });
    });
    ui.add_space(10.0);
}

/// A big number with a caption.
pub fn tile(ui: &mut Ui, value: &str, caption: &str, color: Color32) {
    egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(16, 12)).show(
        ui,
        |ui| {
            ui.set_min_width(118.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(value).font(serif_bold(28.0)).color(color));
                ui.label(RichText::new(caption).small().color(DIM));
            });
        },
    );
}

/// A rounded card around some content.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title.to_uppercase()).font(bold(11.5)).color(DIM));
    ui.add_space(2.0);
}

/// A labelled 0..1 bar.
pub fn bar(ui: &mut Ui, label: &str, value: f32, note: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 18.0), egui::Label::new(RichText::new(label).color(DIM)));
        let (rect, _) = ui.allocate_exact_size(vec2(160.0, 8.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(4), LINE);
        let mut fill = rect;
        fill.set_width(rect.width() * value.clamp(0.0, 1.0));
        p.rect_filled(fill, CornerRadius::same(4), color);
        ui.label(RichText::new(note).small().color(FAINT));
    });
}

/// Lines over time. Each series is drawn in its colour; the y axis starts at 0.
pub fn chart(ui: &mut Ui, size: egui::Vec2, t_end: f64, lines: &[(&Series, Color32, &str)]) {
    let (resp, p) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect.shrink2(vec2(4.0, 6.0));
    p.rect_filled(resp.rect, CornerRadius::same(8), BG);
    let t0 = lines.iter().filter_map(|l| l.0.points.first().map(|p| p.0)).fold(t_end, f64::min);
    let span = (t_end - t0).max(60.0);
    let max = lines.iter().map(|l| l.0.max()).fold(1.0, f32::max) * 1.1;
    let to_pos = |t: f64, v: f32| {
        pos2(rect.left() + ((t - t0) / span) as f32 * rect.width(), rect.bottom() - v / max * rect.height())
    };
    for i in 1..4 {
        let y = rect.bottom() - rect.height() * i as f32 / 4.0;
        p.hline(rect.x_range(), y, Stroke::new(1.0, LINE.gamma_multiply(0.5)));
    }
    for (series, color, _) in lines {
        let mut pts: Vec<egui::Pos2> = series.points.iter().map(|&(t, v)| to_pos(t, v)).collect();
        if let Some(&(_, v)) = series.points.last() {
            pts.push(to_pos(t_end, v));
        }
        if pts.len() >= 2 {
            // A soft fill under the line, then the line.
            let mut fill = pts.clone();
            fill.push(pos2(pts.last().unwrap().x, rect.bottom()));
            fill.push(pos2(pts[0].x, rect.bottom()));
            let mut mesh = egui::Mesh::default();
            for w in pts.windows(2) {
                let base = mesh.vertices.len() as u32;
                let fade = color.gamma_multiply(0.18);
                mesh.colored_vertex(w[0], fade);
                mesh.colored_vertex(w[1], fade);
                mesh.colored_vertex(pos2(w[1].x, rect.bottom()), Color32::TRANSPARENT);
                mesh.colored_vertex(pos2(w[0].x, rect.bottom()), Color32::TRANSPARENT);
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base, base + 2, base + 3);
            }
            p.add(mesh);
            p.add(egui::Shape::line(pts, Stroke::new(1.8, *color)));
        }
    }
    p.text(
        rect.left_top() + vec2(4.0, 0.0),
        Align2::LEFT_TOP,
        thousands(max as u64),
        egui::FontId::proportional(11.0),
        FAINT,
    );
    // Legend.
    let mut x = rect.right() - 4.0;
    for (_, color, name) in lines.iter().rev() {
        let r = p.text(pos2(x, rect.top()), Align2::RIGHT_TOP, *name, egui::FontId::proportional(11.5), *color);
        x = r.left() - 14.0;
    }
    if let Some(hover) = resp.hover_pos() {
        let t = t0 + ((hover.x - rect.left()) / rect.width()) as f64 * span;
        p.vline(hover.x, rect.y_range(), Stroke::new(1.0, DIM.gamma_multiply(0.5)));
        let text: Vec<String> =
            lines.iter().map(|(s, _, name)| format!("{name}: {}", thousands(s.at(t) as u64))).collect();
        resp.on_hover_text(format!("{}\n{}", when(t), text.join("\n")));
    }
}

/// A row of toggle pills. Returns true if any changed.
pub fn pill(ui: &mut Ui, on: bool, text: &str, color: Color32) -> bool {
    let (fill, fg) = if on { (color.gamma_multiply(0.22), color) } else { (Color32::TRANSPARENT, FAINT) };
    let button = egui::Button::new(RichText::new(text).color(fg).size(12.5))
        .fill(fill)
        .stroke(Stroke::new(1.0, if on { color.gamma_multiply(0.6) } else { LINE }))
        .corner_radius(CornerRadius::same(12));
    ui.add(button).clicked()
}

/// Small coloured squares for a group's four light signals.
pub fn signals(ui: &mut Ui, s: &[u8; 4]) {
    let (rect, resp) = ui.allocate_exact_size(vec2(4.0 * 14.0, 12.0), Sense::hover());
    for (i, b) in s.iter().enumerate() {
        let r = egui::Rect::from_min_size(rect.min + vec2(i as f32 * 14.0, 0.0), vec2(11.0, 11.0));
        ui.painter().rect_filled(r, CornerRadius::same(3), signal_color(*b));
    }
    resp.on_hover_text("Its light signals for danger, food, mating and threat");
}

/// Two columns side by side, `split` of the width on the left, with text that
/// wraps normally (egui's own columns justify it).
pub fn columns2(
    ui: &mut Ui,
    split: f32,
    nav: &mut Nav,
    left: impl FnOnce(&mut Ui, &mut Nav),
    right: impl FnOnce(&mut Ui, &mut Nav),
) {
    let gap = 28.0;
    let w = ui.available_width();
    let lw = ((w - gap) * split).floor();
    let rw = w - gap - lw;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let top_down = egui::Layout::top_down(egui::Align::Min);
        ui.allocate_ui_with_layout(vec2(lw, 0.0), top_down, |ui| {
            ui.set_width(lw);
            ui.spacing_mut().item_spacing.x = 8.0;
            left(ui, nav)
        });
        ui.allocate_ui_with_layout(vec2(rw, 0.0), top_down, |ui| {
            ui.set_width(rw);
            ui.spacing_mut().item_spacing.x = 8.0;
            right(ui, nav)
        });
    });
}

/// "1 scar", "3 scars".
pub fn count(n: u32, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {}", vesper_chronicle::history::plural(one)) }
}

pub fn swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, color);
}
