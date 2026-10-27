//! The tree of life: every species as a band through time, as thick as its
//! numbers, branching from the species it came from.

use crate::theme::*;
use crate::widgets::{Nav, thousands, when};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use vesper_chronicle::history::{Species, plural};
use vesper_chronicle::names::{capital, span};
use vesper_chronicle::{History, Kind, Kingdom, Ref, SpeciesId};

pub struct TreeState {
    /// Visible time span as a fraction of the whole, and where it starts.
    zoom: f64,
    start: f64,
    plants: bool,
    creatures: bool,
    extinct: bool,
}

impl Default for TreeState {
    fn default() -> Self {
        Self { zoom: 1.0, start: 0.0, plants: true, creatures: true, extinct: true }
    }
}

/// Species in tree order: each followed by its descendants, oldest branch first.
fn order(h: &History, st: &TreeState) -> Vec<SpeciesId> {
    let keep = |s: &Species| {
        (st.extinct || s.alive())
            && match s.traits.kingdom {
                Kingdom::Rooted => st.plants,
                Kingdom::Motile => st.creatures,
            }
    };
    let mut roots: Vec<&Species> =
        h.species.values().filter(|s| s.parent.is_none_or(|p| !h.species.contains_key(&p))).collect();
    roots.sort_by(|a, b| (a.founding.kingdom == Kingdom::Motile).cmp(&(b.founding.kingdom == Kingdom::Motile)).then(a.arose.total_cmp(&b.arose)));
    let mut out = Vec::new();
    let mut stack: Vec<SpeciesId> = roots.iter().rev().map(|s| s.id).collect();
    while let Some(id) = stack.pop() {
        let Some(s) = h.species.get(&id) else { continue };
        if keep(s) {
            out.push(id);
        }
        let mut kids: Vec<&Species> = s.children.iter().filter_map(|c| h.species.get(c)).collect();
        kids.sort_by(|a, b| b.arose.total_cmp(&a.arose));
        stack.extend(kids.iter().map(|k| k.id));
    }
    out
}

pub fn show(ui: &mut Ui, nav: &mut Nav, h: &History, st: &mut TreeState) {
    egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 18, bottom: 6 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Tree of life").font(serif_bold(30.0)));
            ui.add_space(12.0);
            ui.label(
                RichText::new("Each band is a species, as thick as its numbers. Scroll to zoom, drag to move through time.")
                    .font(italic(15.0))
                    .color(DIM),
            );
        });
        ui.horizontal(|ui| {
            if crate::widgets::pill(ui, st.creatures, "Creatures", TEAL) {
                st.creatures = !st.creatures;
            }
            if crate::widgets::pill(ui, st.plants, "Plants", ACID) {
                st.plants = !st.plants;
            }
            if crate::widgets::pill(ui, st.extinct, "Show the extinct", CRIMSON) {
                st.extinct = !st.extinct;
            }
            if st.zoom > 1.0 && crate::widgets::pill(ui, false, "See all of time", AMBER) {
                st.zoom = 1.0;
                st.start = 0.0;
            }
        });
    });

    let ids = order(h, st);
    let avail = ui.available_rect_before_wrap().shrink2(vec2(24.0, 8.0));
    let label_w = 210.0;
    let axis_h = 34.0;
    let era_h = 22.0;
    let rows = ids.len().max(1) as f32;
    let row_h = ((avail.height() - axis_h - era_h) / rows).clamp(7.0, 30.0);
    let content_h = rows * row_h + axis_h + era_h;

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(0.0);
        let (resp, p) = ui.allocate_painter(vec2(ui.available_width(), content_h.max(avail.height())), Sense::click_and_drag());
        let full = resp.rect.shrink2(vec2(24.0, 0.0));
        let plot = Rect::from_min_max(pos2(full.left(), full.top() + era_h), pos2(full.right() - label_w, full.top() + era_h + rows * row_h));
        p.rect_filled(Rect::from_min_max(full.min, pos2(plot.right(), plot.bottom())), CornerRadius::same(8), BG);

        // Time mapping, with zoom about the pointer and dragging.
        let total = h.now.max(60.0);
        let span_t = total / st.zoom;
        if let Some(hover) = resp.hover_pos()
            && plot.expand2(vec2(0.0, axis_h)).contains(hover)
        {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.smooth_scroll_delta.x);
            if scroll != 0.0 && ui.input(|i| !i.modifiers.shift) {
                let at = st.start + ((hover.x - plot.left()) / plot.width()) as f64 * span_t;
                st.zoom = (st.zoom * (scroll as f64 * 0.004).exp()).clamp(1.0, 400.0);
                let new_span = total / st.zoom;
                st.start = at - ((hover.x - plot.left()) / plot.width()) as f64 * new_span;
            }
        }
        if resp.dragged() {
            st.start -= (resp.drag_delta().x / plot.width()) as f64 * span_t;
        }
        let span_t = total / st.zoom;
        st.start = st.start.clamp(0.0, (total - span_t).max(0.0));
        let x_of = |t: f64| plot.left() + ((t - st.start) / span_t) as f32 * plot.width();
        let t_of = |x: f32| st.start + ((x - plot.left()) / plot.width()) as f64 * span_t;

        // Eras along the top.
        for (i, e) in h.eras.iter().enumerate() {
            let x0 = x_of(e.began).max(plot.left());
            let x1 = x_of(e.ended.unwrap_or(h.now)).min(plot.right());
            if x1 <= x0 {
                continue;
            }
            let color = h.species.get(&e.species).map_or(MAGENTA, |s| species_color(&s.traits));
            let r = Rect::from_min_max(pos2(x0, full.top() + 3.0), pos2(x1, full.top() + era_h - 5.0));
            p.rect_filled(r, CornerRadius::same(3), color.gamma_multiply(if i % 2 == 0 { 0.22 } else { 0.3 }));
            if let Some(s) = h.species.get(&e.species) {
                let label = format!("Age of the {}", vesper_chronicle::names::title_case(&plural(&s.common)));
                let galley = p.layout_no_wrap(label, FontId::proportional(11.0), color);
                if galley.size().x + 8.0 < r.width() {
                    p.galley(pos2(r.left() + 5.0, r.center().y - galley.size().y / 2.0), galley, color);
                }
            }
        }

        // Day lines and the time axis.
        let tick = nice_step(span_t / (plot.width() as f64 / 110.0).max(1.0));
        let mut t = (st.start / tick).ceil() * tick;
        while t <= st.start + span_t {
            let x = x_of(t);
            let day_start = (t % 86_400.0).abs() < 1.0;
            p.vline(x, plot.y_range(), Stroke::new(1.0, if day_start { LINE } else { LINE.gamma_multiply(0.4) }));
            let label = if tick >= 86_400.0 || day_start { format!("Day {}", (t / 86_400.0).floor() as u64 + 1) } else { when(t)[..].split(", ").nth(1).unwrap_or("").to_string() };
            p.text(pos2(x, plot.bottom() + 6.0), Align2::CENTER_TOP, label, FontId::proportional(11.0), if day_start { DIM } else { FAINT });
            t += tick;
        }

        // Markers for the great events.
        let mut hovered_marker = None;
        for e in h.entries.iter().filter(|e| e.weight >= 3 && matches!(e.kind, Kind::Extinction | Kind::War | Kind::Nature | Kind::First)) {
            let x = x_of(e.tick);
            if x < plot.left() || x > plot.right() {
                continue;
            }
            let y = plot.bottom() + 24.0;
            let c = kind_color(e.kind);
            p.add(Shape::convex_polygon(vec![pos2(x, y - 4.0), pos2(x + 4.0, y + 3.0), pos2(x - 4.0, y + 3.0)], c, Stroke::NONE));
            if resp.hover_pos().is_some_and(|m| (m - pos2(x, y)).length() < 7.0) {
                hovered_marker = Some(e);
            }
        }

        let peak_all = ids.iter().filter_map(|id| h.species.get(id)).map(|s| s.peak).max().unwrap_or(1).max(1) as f32;
        let row_of = |id: SpeciesId| ids.iter().position(|x| *x == id);
        let mut hovered: Option<SpeciesId> = None;
        let mouse = resp.hover_pos();

        // Kingdom divider.
        if let Some(first_motile) = ids.iter().position(|id| h.species.get(id).is_some_and(|s| s.traits.kingdom == Kingdom::Motile))
            && first_motile > 0
        {
            let y = plot.top() + first_motile as f32 * row_h;
            p.hline(plot.x_range(), y, Stroke::new(1.0, LINE));
            p.text(pos2(plot.left() + 6.0, y + 2.0), Align2::LEFT_TOP, "CREATURES", crate::theme::bold(10.0), FAINT);
            p.text(pos2(plot.left() + 6.0, plot.top() + 2.0), Align2::LEFT_TOP, "PLANTS", crate::theme::bold(10.0), FAINT);
        }

        for (row, id) in ids.iter().enumerate() {
            let Some(s) = h.species.get(id) else { continue };
            let yc = plot.top() + (row as f32 + 0.5) * row_h;
            let end = s.extinct.unwrap_or(h.now);
            let color = species_color(&s.traits);
            let alpha = if s.alive() { 1.0 } else { 0.5 };
            let scale = 0.3 + 0.7 * ((1.0 + s.peak as f32).ln() / (1.0 + peak_all).ln());
            let half = |v: f32| (row_h * 0.46) * scale * (0.12 + 0.88 * (v / s.peak.max(1) as f32).sqrt());

            // Branch from the parent.
            if let Some(prow) = s.parent.and_then(row_of) {
                let x = x_of(s.arose);
                if x >= plot.left() && x <= plot.right() {
                    let py = plot.top() + (prow as f32 + 0.5) * row_h;
                    let pc = h.species.get(&s.parent.unwrap()).map_or(color, |p| species_color(&p.traits));
                    p.line_segment([pos2(x, py), pos2(x, yc)], Stroke::new(1.0, pc.gamma_multiply(0.55 * alpha)));
                    p.circle_filled(pos2(x, py), 1.8, pc.gamma_multiply(alpha));
                }
            }

            // The band, sampled along the visible part of its life.
            let t0 = s.arose.max(st.start);
            let t1 = end.min(st.start + span_t);
            if t1 > t0 {
                let steps = (((x_of(t1) - x_of(t0)) / 3.0).ceil() as usize).clamp(1, 600);
                let mut mesh = Mesh::default();
                let fill = color.gamma_multiply(0.85 * alpha);
                let mut prev: Option<(Pos2, Pos2)> = None;
                for i in 0..=steps {
                    let t = t0 + (t1 - t0) * i as f64 / steps as f64;
                    let v = s.population.at(t).max(if t <= s.arose + 60.0 { s.population.points.first().map_or(1.0, |p| p.1) } else { 0.0 });
                    let hh = half(v).max(0.6);
                    let x = x_of(t);
                    let (a, b) = (pos2(x, yc - hh), pos2(x, yc + hh));
                    if let Some((pa, pb)) = prev {
                        let base = mesh.vertices.len() as u32;
                        mesh.colored_vertex(pa, fill);
                        mesh.colored_vertex(a, fill);
                        mesh.colored_vertex(b, fill);
                        mesh.colored_vertex(pb, fill);
                        mesh.add_triangle(base, base + 1, base + 2);
                        mesh.add_triangle(base, base + 2, base + 3);
                    }
                    prev = Some((a, b));
                }
                p.add(mesh);
                if let Some(t) = s.extinct
                    && t <= st.start + span_t
                {
                    let x = x_of(t);
                    p.line_segment([pos2(x, yc - row_h * 0.3), pos2(x, yc + row_h * 0.3)], Stroke::new(1.5, CRIMSON.gamma_multiply(0.8)));
                }
                if let Some(m) = mouse
                    && (m.y - yc).abs() <= row_h * 0.5
                    && m.x >= x_of(t0) - 3.0
                    && m.x <= x_of(t1) + 3.0
                {
                    hovered = Some(*id);
                }
            }

            // Name on the right for the living; beside the end for the gone.
            let size = (row_h * 0.75).clamp(8.5, 13.0);
            if s.alive() {
                if row_h >= 8.0 {
                    let name = capital(&s.common);
                    p.text(pos2(plot.right() + 10.0, yc), Align2::LEFT_CENTER, name, FontId::proportional(size), color);
                }
            } else if row_h >= 11.0 {
                let x = x_of(end) + 6.0;
                if x > plot.left() && x < plot.right() - 60.0 {
                    p.text(pos2(x, yc), Align2::LEFT_CENTER, capital(&s.common), FontId::proportional(size * 0.92), FAINT);
                }
            }
            if mouse.is_some_and(|m| m.x > plot.right() && (m.y - yc).abs() <= row_h * 0.5) {
                hovered = Some(*id);
            }
        }

        // "Now" edge.
        if h.now <= st.start + span_t + 1.0 {
            let x = x_of(h.now);
            p.vline(x, plot.y_range(), Stroke::new(1.0, AMBER.gamma_multiply(0.5)));
        }

        if let Some(id) = hovered
            && let Some(s) = h.species.get(&id)
        {
            let row = row_of(id).unwrap_or(0);
            let yc = plot.top() + (row as f32 + 0.5) * row_h;
            p.rect_stroke(
                Rect::from_min_max(pos2(x_of(s.arose.max(st.start)) - 2.0, yc - row_h * 0.5), pos2(plot.right() + label_w, yc + row_h * 0.5)),
                CornerRadius::same(4),
                Stroke::new(1.0, species_color(&s.traits).gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let at = mouse.map(|m| t_of(m.x)).unwrap_or(h.now).clamp(s.arose, s.extinct.unwrap_or(h.now));
            let status = match s.extinct {
                Some(t) => format!("Extinct {} after {}", when(t), span(t - s.arose)),
                None => format!("{} alive now", thousands(s.count as u64)),
            };
            let parent = s.parent.and_then(|p| h.species.get(&p)).map(|p| format!("\nFrom the {}", plural(&p.common))).unwrap_or_default();
            resp.clone().on_hover_text(format!(
                "{}\n{}\nArose {}\n{status}\n{} at {} · peak {}{parent}",
                capital(&s.common),
                s.latin(),
                when(s.arose),
                thousands(s.population.at(at) as u64),
                when(at),
                thousands(s.peak as u64)
            ));
            if resp.clicked() {
                nav.go = Some(Ref::Species(id));
            }
        } else if let Some(e) = hovered_marker {
            resp.clone().on_hover_text(format!("{}\n{}\n{}", when(e.tick), e.title, h.plain(&e.text)));
        }
        if resp.double_clicked() {
            st.zoom = 1.0;
            st.start = 0.0;
        }
        let _ = Color32::TRANSPARENT;
    });
}

/// A round time step near `raw` seconds.
fn nice_step(raw: f64) -> f64 {
    const STEPS: [f64; 10] = [600.0, 1800.0, 3600.0, 7200.0, 3.0 * 3600.0, 6.0 * 3600.0, 12.0 * 3600.0, 86_400.0, 2.0 * 86_400.0, 7.0 * 86_400.0];
    STEPS.iter().copied().find(|s| *s >= raw).unwrap_or(30.0 * 86_400.0)
}
