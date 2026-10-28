//! The pages other than the tree.

use crate::portrait::{Portrait, Portraits};
use crate::theme::*;
use crate::widgets::*;
use eframe::egui::{self, Color32, RichText, Ui, vec2};
use std::collections::BTreeSet;
use vesper_chronicle::history::{CauseKind, Species, plural};
use vesper_chronicle::names::{self, capital, describe, place, region, span};
use vesper_chronicle::{CreatureId, GroupId, History, Kind, Kingdom, Ref, SpeciesId, StrainId};

pub struct PageState {
    pub kinds: BTreeSet<Kind>,
    /// Least weight shown in the annals.
    pub weight: u8,
    pub search: String,
    pub shown: usize,
    pub species_filter: SpeciesFilter,
    pub species_sort: SpeciesSort,
    pub living_only: bool,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            kinds: Kind::ALL.into_iter().collect(),
            weight: 2,
            search: String::new(),
            shown: 150,
            species_filter: SpeciesFilter::Living,
            species_sort: SpeciesSort::Numerous,
            living_only: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpeciesFilter {
    Living,
    Extinct,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpeciesSort {
    Numerous,
    Newest,
    Oldest,
}

/// Returns true if the user asked to see a sample world.
pub fn empty(ui: &mut Ui, waiting: bool) -> bool {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.3);
        ui.label(RichText::new("Nothing has happened yet").font(serif_bold(30.0)).color(TEXT));
        ui.add_space(8.0);
        let text = if waiting {
            "The chronicle fills in as soon as life on Vesper begins. Leave this open and it will catch up on its own."
        } else {
            "There is no history to show."
        };
        ui.label(RichText::new(text).font(serif(17.0)).color(DIM));
        ui.add_space(20.0);
        ui.button(RichText::new("Show a sample world meanwhile").size(15.0))
            .on_hover_text("A made-up history, so you can see what the chronicle will look like")
            .clicked()
    })
    .inner
}

fn heading(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).font(serif_bold(34.0)).color(TEXT));
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).font(italic(17.0)).color(DIM));
    }
    ui.add_space(14.0);
}

fn species_name(h: &History, id: SpeciesId) -> String {
    h.species.get(&id).map_or("unknown".into(), |s| s.common.clone())
}

fn species_link(ui: &mut Ui, nav: &mut Nav, h: &History, id: SpeciesId) {
    let color = h.species.get(&id).map_or(DIM, |s| species_color(&s.traits));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        swatch(ui, color);
        link(ui, nav, species_name(h, id), Ref::Species(id));
    });
}

fn group_name(h: &History, id: GroupId) -> String {
    h.groups.get(&id).map_or("a lost group".into(), |g| g.name.clone())
}

// ---------------------------------------------------------------- Now

pub fn now(ui: &mut Ui, nav: &mut Nav, h: &History) {
    let title = match h.era().and_then(|e| h.species.get(&e.species)) {
        Some(s) => format!("The Age of the {}", names::title_case(&plural(&s.common))),
        None => "Vesper".to_string(),
    };
    let extinct = h.species.len() - h.living_species();
    heading(
        ui,
        &title,
        &format!(
            "Day {} since life began. {} species alive, {} gone forever.",
            day_of(h.now),
            h.living_species(),
            extinct
        ),
    );

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
        tile(ui, &thousands(h.population.last() as u64), "creatures alive", TEAL);
        tile(ui, &thousands(h.plants.last() as u64), "plants alive", ACID);
        tile(ui, &h.living_species().to_string(), "living species", AMBER);
        tile(ui, &extinct.to_string(), "extinct species", CRIMSON);
        tile(ui, &thousands(h.deaths), "deaths recorded", BONE);
        tile(ui, &h.wars.len().to_string(), "wars fought", EMBER);
    });
    ui.add_space(14.0);

    let total: f32 = h.matter.iter().sum();
    if total > 0.0 {
        section(ui, "Where the world's matter is");
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 14.0), egui::Sense::hover());
        let mut x = rect.left();
        for (share, color) in h.matter.iter().zip([TEAL, BONE.gamma_multiply(0.7), Color32::from_rgb(120, 82, 56)]) {
            let w = rect.width() * share / total;
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, rect.height()));
            ui.painter().rect_filled(r, egui::CornerRadius::same(3), color);
            x += w;
        }
        let pct = |v: f32| (v / total * 100.0).round();
        resp.on_hover_text("Matter is never made or lost on Vesper. Every body is borrowed from the soil.");
        ui.label(
            RichText::new(format!(
                "{}% alive, {}% in remains, {}% in the soil and air, waiting.",
                pct(h.matter[0]),
                pct(h.matter[1]),
                pct(h.matter[2])
            ))
            .small()
            .color(DIM),
        );
        ui.add_space(10.0);
    }

    section(ui, "Living things over time");
    chart(
        ui,
        vec2(ui.available_width().min(1100.0), 150.0),
        h.now,
        &[(&h.plants, ACID, "plants"), (&h.population, TEAL, "creatures")],
    );
    ui.add_space(16.0);

    columns2(
        ui,
        0.44,
        nav,
        |ui, nav| happening(ui, nav, h),
        |ui, nav| {
            section(ui, "Latest in the annals");
            let mut n = 0;
            for e in h.entries.iter().rev().filter(|e| e.weight >= 2) {
                entry(ui, nav, h, e, true);
                n += 1;
                if n >= 10 {
                    break;
                }
            }
        },
    );
}

fn happening(ui: &mut Ui, nav: &mut Nav, h: &History) {
    section(ui, "Happening now");
    let mut any = false;
    for (i, w) in h.wars.iter().enumerate().filter(|(_, w)| w.ended.is_none()) {
        any = true;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("WAR").font(bold(10.5)).color(EMBER));
                link_styled(ui, nav, RichText::new(capital(&w.name)).font(serif_bold(17.0)).color(TEXT), Ref::War(i));
            });
            ui.horizontal_wrapped(|ui| {
                link(ui, nav, capital(&group_name(h, w.a)), Ref::Group(w.a));
                ui.label(RichText::new("against").color(DIM));
                link(ui, nav, group_name(h, w.b), Ref::Group(w.b));
            });
            ui.label(
                RichText::new(format!("{} dead in {} so far, in {}.", w.deaths, span(h.now - w.began), place(w.x)))
                    .small()
                    .color(DIM),
            );
        });
        ui.add_space(6.0);
    }
    for o in h.outbreaks.iter().filter(|o| o.ended.is_none()) {
        let Some(s) = h.strains.get(&o.strain) else { continue };
        any = true;
        let harmful = s.traits.virulence > 0.0;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if harmful { "PLAGUE" } else { "PARTNER" }).font(bold(10.5)).color(ACID));
                link_styled(
                    ui,
                    nav,
                    RichText::new(capital(&s.name)).font(serif_bold(17.0)).color(TEXT),
                    Ref::Strain(s.id),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} infected among the", s.infected)).color(DIM));
                link(ui, nav, plural(&species_name(h, o.host)), Ref::Species(o.host));
            });
            if harmful {
                ui.label(
                    RichText::new(format!("{} dead since it began {}.", o.deaths, ago(h.now - o.began)))
                        .small()
                        .color(DIM),
                );
            }
        });
        ui.add_space(6.0);
    }
    let recent: Vec<_> = h
        .entries
        .iter()
        .rev()
        .take_while(|e| h.now - e.tick < 6.0 * 3600.0)
        .filter(|e| e.kind == Kind::Famine)
        .collect();
    for e in recent {
        any = true;
        card(ui, |ui| {
            ui.label(RichText::new("FAMINE").font(bold(10.5)).color(AMBER));
            ui.label(RichText::new(&e.title).font(serif_bold(17.0)));
            paragraph(ui, nav, h, &e.text, serif(15.0), DIM);
        });
        ui.add_space(6.0);
    }
    if !any {
        ui.label(RichText::new("No wars, plagues or famines right now. A quiet time.").font(italic(15.5)).color(DIM));
    }

    ui.add_space(10.0);
    section(ui, "Most numerous");
    let mut living: Vec<&Species> =
        h.species.values().filter(|s| s.alive() && s.traits.kingdom == Kingdom::Motile).collect();
    living.sort_by_key(|s| std::cmp::Reverse(s.count));
    for s in living.iter().take(6) {
        ui.horizontal(|ui| {
            species_link(ui, nav, h, s.id);
            ui.label(RichText::new(thousands(s.count as u64)).small().color(DIM));
        });
    }

    ui.add_space(10.0);
    section(ui, "Biggest peoples");
    let mut groups: Vec<_> = h.groups.values().filter(|g| g.gone.is_none() && g.notable).collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.members));
    for g in groups.iter().take(5) {
        ui.horizontal(|ui| {
            signals(ui, &g.signals);
            link(ui, nav, capital(&g.name), Ref::Group(g.id));
            ui.label(
                RichText::new(format!("{} {}", g.members, plural(&species_name(h, g.species)))).small().color(DIM),
            );
        });
    }
}

// ---------------------------------------------------------------- Annals

pub fn annals(ui: &mut Ui, nav: &mut Nav, h: &History, st: &mut PageState) {
    heading(ui, "Annals", "Everything the chronicle has written, newest first.");
    ui.horizontal_wrapped(|ui| {
        for k in Kind::ALL {
            let on = st.kinds.contains(&k);
            if pill(ui, on, k.label(), kind_color(k)) {
                if on {
                    st.kinds.remove(&k);
                } else {
                    st.kinds.insert(k);
                }
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (w, label) in [(3, "Only the turning points"), (2, "Notable events"), (1, "Everything")] {
            if pill(ui, st.weight == w, label, AMBER) {
                st.weight = w;
            }
        }
        ui.add_space(12.0);
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("Search names and words").desired_width(220.0));
    });
    ui.add_space(14.0);

    let needle = st.search.to_lowercase();
    let mut day = 0;
    let mut shown = 0;
    let mut more = false;
    for e in h.entries.iter().rev() {
        if e.weight < st.weight || !st.kinds.contains(&e.kind) {
            continue;
        }
        if !needle.is_empty()
            && !e.title.to_lowercase().contains(&needle)
            && !h.plain(&e.text).to_lowercase().contains(&needle)
        {
            continue;
        }
        if shown >= st.shown {
            more = true;
            break;
        }
        let d = day_of(e.tick);
        if d != day {
            day = d;
            ui.add_space(8.0);
            ui.label(RichText::new(format!("Day {d}")).font(serif_bold(22.0)).color(AMBER));
            ui.add_space(6.0);
        }
        entry(ui, nav, h, e, false);
        shown += 1;
    }
    if shown == 0 {
        ui.label(RichText::new("Nothing matches.").font(italic(16.0)).color(DIM));
    }
    if more && ui.button("Show older").clicked() {
        st.shown += 150;
    }
}

// ---------------------------------------------------------------- Species

pub fn species_list(ui: &mut Ui, nav: &mut Nav, h: &History, st: &mut PageState, portraits: &mut Portraits) {
    heading(ui, "Species", "A field guide to every kind of life Vesper has made.");
    ui.horizontal(|ui| {
        for (f, label) in
            [(SpeciesFilter::Living, "Living"), (SpeciesFilter::Extinct, "Extinct"), (SpeciesFilter::All, "All")]
        {
            if pill(ui, st.species_filter == f, label, TEAL) {
                st.species_filter = f;
            }
        }
        ui.add_space(16.0);
        for (s, label) in
            [(SpeciesSort::Numerous, "Most numerous"), (SpeciesSort::Newest, "Newest"), (SpeciesSort::Oldest, "Oldest")]
        {
            if pill(ui, st.species_sort == s, label, AMBER) {
                st.species_sort = s;
            }
        }
    });
    ui.add_space(12.0);
    let mut list: Vec<&Species> = h
        .species
        .values()
        .filter(|s| crate::tree::remembered(h, s))
        .filter(|s| match st.species_filter {
            SpeciesFilter::Living => s.alive(),
            SpeciesFilter::Extinct => !s.alive(),
            SpeciesFilter::All => true,
        })
        .collect();
    match st.species_sort {
        SpeciesSort::Numerous => list.sort_by_key(|s| std::cmp::Reverse(if s.alive() { s.count } else { s.peak })),
        SpeciesSort::Newest => list.sort_by(|a, b| b.arose.total_cmp(&a.arose)),
        SpeciesSort::Oldest => list.sort_by(|a, b| a.arose.total_cmp(&b.arose)),
    }
    for kingdom in [Kingdom::Motile, Kingdom::Rooted] {
        let rows: Vec<&&Species> = list.iter().filter(|s| s.traits.kingdom == kingdom).collect();
        if rows.is_empty() {
            continue;
        }
        section(ui, if kingdom == Kingdom::Motile { "Creatures" } else { "Plants" });
        egui::Grid::new(format!("species-{kingdom:?}")).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
            for s in rows {
                let thumb = ui.scope(|ui| portrait(ui, portraits, h, s, vec2(84.0, 48.0), true)).response;
                if thumb.interact(egui::Sense::click()).clicked() {
                    nav.go = Some(Ref::Species(s.id));
                }
                ui.horizontal(|ui| {
                    swatch(ui, species_color(&s.traits));
                    link_styled(
                        ui,
                        nav,
                        RichText::new(capital(&s.common)).font(serif_bold(16.0)).color(AMBER),
                        Ref::Species(s.id),
                    );
                });
                ui.label(RichText::new(s.latin()).font(italic(14.0)).color(DIM));
                ui.label(RichText::new(format!("{:?}", s.traits.locomotion).to_lowercase()).small().color(DIM));
                match s.extinct {
                    None => ui.label(format!("{} alive", thousands(s.count as u64))),
                    Some(t) => ui.label(
                        RichText::new(format!("extinct after {}", span(t - s.arose)))
                            .color(CRIMSON.gamma_multiply(0.8)),
                    ),
                };
                ui.label(RichText::new(format!("since {}", when(s.arose))).small().color(FAINT));
                ui.end_row();
            }
        });
        ui.add_space(12.0);
    }
}

/// A species as it looks, `width` points wide, or a note saying why not.
fn portrait(ui: &mut Ui, portraits: &mut Portraits, h: &History, s: &Species, size: egui::Vec2, small: bool) {
    let ppp = ui.ctx().pixels_per_point();
    let px = [(size.x * ppp).round() as u32, (size.y * ppp).round() as u32];
    let shown = match h.likenesses.get(&s.id) {
        Some(l) => portraits.get(ui.ctx(), s.id, l, px),
        None => Portrait::None,
    };
    match shown {
        Portrait::Ready(tex) => {
            ui.add(egui::Image::new(&tex).fit_to_exact_size(size).corner_radius(if small { 4 } else { 8 }));
        }
        other => {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(rect, if small { 4 } else { 8 }, CARD);
            if !small {
                let note = match (other, s.alive(), h.likenesses.contains_key(&s.id)) {
                    (Portrait::Pending, ..) => "Painting…",
                    (_, _, true) => "This window can't paint pictures here.",
                    (_, true, false) => "Not drawn yet. Species are drawn a few minutes after they appear.",
                    (_, false, false) => "It died out before anyone drew it.",
                };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, note, italic(15.0), DIM);
            }
        }
    }
}

fn species_page(ui: &mut Ui, nav: &mut Nav, h: &History, s: &Species, portraits: &mut Portraits) {
    let color = species_color(&s.traits);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 34.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 7.0, color);
        ui.label(RichText::new(capital(&s.common)).font(serif_bold(34.0)));
    });
    ui.label(RichText::new(s.latin()).font(italic(18.0)).color(DIM));
    let status = match s.extinct {
        None => format!(
            "Alive: {} now, {} at most. Arose {} in {}.",
            thousands(s.count as u64),
            thousands(s.peak as u64),
            when(s.arose),
            region(s.founding.zone)
        ),
        Some(t) => format!(
            "Extinct since {}, after {}. There were {} at most.",
            when(t),
            span(t - s.arose),
            thousands(s.peak as u64)
        ),
    };
    ui.label(RichText::new(status).color(if s.alive() { TEAL } else { CRIMSON }));
    ui.add_space(12.0);

    columns2(
        ui,
        0.5,
        nav,
        |ui, _| {
            ui.label(RichText::new(describe(&s.traits)).font(serif(17.0)).color(TEXT));
            ui.add_space(12.0);
            section(ui, "Body and habits");
            let t = &s.traits;
            let size_word = ["speck", "small", "middling", "large", "huge"][names::size_class(t.size)];
            bar(ui, "Size", (t.size / 0.2).sqrt(), size_word, color);
            bar(ui, "Speed", (t.speed / 0.3).sqrt(), "", color);
            bar(
                ui,
                "Diet",
                t.diet,
                if t.diet > 0.6 {
                    "meat"
                } else if t.diet > 0.35 {
                    "both"
                } else {
                    "plants or light"
                },
                color,
            );
            bar(ui, "Light-feeding", t.photosynthesis, "", color);
            bar(ui, "Armor", t.armor, "", color);
            bar(ui, "Spines", t.spines, "", color);
            bar(ui, "Poison", t.toxin, "", color);
            bar(ui, "Glow", t.glow, "", color);
            if t.kingdom == Kingdom::Motile {
                bar(ui, "Sociality", t.sociality, "", color);
                bar(ui, "Brain", (t.neurons as f32 / 400.0).sqrt(), &format!("{} neurons", t.neurons), color);
            }
            bar(ui, "Lifespan", (t.lifespan / 86_400.0).sqrt(), &span(t.lifespan as f64), color);
            bar(
                ui,
                "Lives where",
                t.zone,
                if t.zone < 0.33 {
                    "day edge"
                } else if t.zone > 0.66 {
                    "night edge"
                } else {
                    "twilight"
                },
                color,
            );
        },
        |ui, nav| {
            let w = ui.available_width();
            portrait(ui, portraits, h, s, vec2(w, (w * 0.56).round()), false);
            if let Some(l) = h.likenesses.get(&s.id) {
                let b = l.bounds();
                let what = if l.plant { "One" } else { "A grown one" };
                // World units are monitor heights; Kyle's monitors are 2160 pixels tall.
                let px = ((b[2] - b[0]).max(b[3] - b[1]) * 2160.0).round().max(1.0);
                ui.label(
                    RichText::new(format!(
                        "{what}, as seen {} in {}. On a 4K screen it is about {px} pixels across.",
                        when(l.tick),
                        place((b[0] + b[2]) / 2.0)
                    ))
                    .small()
                    .color(DIM),
                );
            }
            ui.add_space(10.0);
            section(ui, "Numbers");
            chart(
                ui,
                vec2(ui.available_width(), 130.0),
                s.extinct.unwrap_or(h.now),
                &[(&s.population, color, "alive")],
            );
            ui.add_space(8.0);
            section(ui, "Lineage");
            let line = h.lineage(s.id);
            if line.is_empty() {
                ui.label(RichText::new("One of the first life on Vesper.").font(italic(15.0)).color(DIM));
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (i, a) in line.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("›").color(FAINT));
                        }
                        let alive = h.species.get(a).is_some_and(|a| a.alive());
                        let text = RichText::new(species_name(h, *a)).color(if alive {
                            AMBER
                        } else {
                            AMBER.gamma_multiply(0.55)
                        });
                        link_styled(ui, nav, text, Ref::Species(*a));
                    }
                    ui.label(RichText::new("›").color(FAINT));
                    ui.label(RichText::new(&s.common).color(TEXT));
                });
            }
            if !s.children.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("Gave rise to").small().color(DIM));
                // Inline, so the row wraps between names but never inside one.
                ui.horizontal_wrapped(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    for c in &s.children {
                        let name = species_name(h, *c);
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let width = ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), font, AMBER).size().x) + 24.0;
                        if width > ui.available_width() {
                            ui.end_row();
                        }
                        swatch(ui, h.species.get(c).map_or(DIM, |s| species_color(&s.traits)));
                        link(ui, nav, name, Ref::Species(*c));
                        ui.add_space(6.0);
                    }
                });
            }
            let top = |m: &std::collections::BTreeMap<SpeciesId, u32>| {
                let mut v: Vec<(SpeciesId, u32)> = m.iter().map(|(k, v)| (*k, *v)).collect();
                v.sort_by_key(|x| std::cmp::Reverse(x.1));
                v.truncate(5);
                v
            };
            for (title, list) in [("Eats", top(&s.prey)), ("Eaten by", top(&s.predators))] {
                if list.is_empty() {
                    continue;
                }
                section(ui, title);
                for (id, n) in list {
                    ui.horizontal(|ui| {
                        species_link(ui, nav, h, id);
                        ui.label(RichText::new(format!("{n} kills")).small().color(FAINT));
                    });
                }
            }
            if !s.deaths.is_empty() {
                section(ui, "How they die");
                let total: u32 = s.deaths.values().sum();
                let mut d: Vec<(CauseKind, u32)> = s.deaths.iter().map(|(k, v)| (*k, *v)).collect();
                d.sort_by_key(|x| std::cmp::Reverse(x.1));
                for (k, n) in d.into_iter().take(6) {
                    bar(ui, k.label(), n as f32 / total as f32, &n.to_string(), CRIMSON.gamma_multiply(0.8));
                }
            }
        },
    );

    let members: Vec<_> = h.notables.values().filter(|n| n.species == s.id).collect();
    if !members.is_empty() {
        section(ui, "Remembered members");
        ui.horizontal_wrapped(|ui| {
            for n in members.iter().rev().take(24) {
                link(ui, nav, format!("{} {}", n.name, n.title), Ref::Creature(n.id));
                ui.label(RichText::new("·").color(FAINT));
            }
        });
    }
    let groups: Vec<_> = h.groups.values().filter(|g| g.species == s.id && g.notable).collect();
    if !groups.is_empty() {
        section(ui, "Peoples");
        ui.horizontal_wrapped(|ui| {
            for g in groups {
                signals(ui, &g.signals);
                link(ui, nav, capital(&g.name), Ref::Group(g.id));
                ui.add_space(8.0);
            }
        });
    }
    mentions(ui, nav, h, Ref::Species(s.id));
}

fn mentions(ui: &mut Ui, nav: &mut Nav, h: &History, r: Ref) {
    let list: Vec<_> = h.entries_about(r).take(30).collect();
    if list.is_empty() {
        return;
    }
    ui.add_space(10.0);
    section(ui, "In the annals");
    for e in list {
        entry(ui, nav, h, e, true);
    }
}

// ---------------------------------------------------------------- Lives

pub fn lives(ui: &mut Ui, nav: &mut Nav, h: &History, st: &mut PageState) {
    heading(ui, "Lives", "Creatures that stood out: great hunters, founders, leaders, survivors.");
    ui.horizontal(|ui| {
        if pill(ui, !st.living_only, "Everyone remembered", AMBER) {
            st.living_only = false;
        }
        if pill(ui, st.living_only, "Still alive", TEAL) {
            st.living_only = true;
        }
    });
    ui.add_space(12.0);
    let mut list: Vec<_> = h.notables.values().filter(|n| !st.living_only || n.died.is_none()).collect();
    list.sort_by(|a, b| b.score.total_cmp(&a.score));
    let width = ui.available_width();
    let per_row = ((width / 360.0).floor() as usize).max(1);
    for row in list.chunks(per_row).take(40) {
        ui.columns(per_row, |cols| {
            for (ui, n) in cols.iter_mut().zip(row) {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        link_styled(
                            ui,
                            nav,
                            RichText::new(&n.name).font(serif_bold(19.0)).color(AMBER),
                            Ref::Creature(n.id),
                        );
                        ui.label(RichText::new(&n.title).font(italic(16.0)).color(DIM));
                    });
                    species_link(ui, nav, h, n.species);
                    let status = match n.died {
                        Some(_) => RichText::new(format!("Died at {}", span(n.life.age as f64))).color(FAINT),
                        None => RichText::new(format!("Alive, {} old", span(n.life.age as f64))).color(TEAL),
                    };
                    ui.label(status.small());
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · {}",
                            count(n.life.kills, "kill"),
                            n.life.offspring.to_string() + " offspring",
                            count(n.life.scars, "scar")
                        ))
                        .small()
                        .color(DIM),
                    );
                });
            }
        });
        ui.add_space(8.0);
    }
}

fn creature_page(ui: &mut Ui, nav: &mut Nav, h: &History, id: CreatureId) {
    let Some(n) = h.notables.get(&id) else {
        ui.label(RichText::new(h.creature_name(id)).font(serif_bold(34.0)));
        ui.label(RichText::new("Its biography was not kept; only its name survives.").font(italic(16.0)).color(DIM));
        mentions(ui, nav, h, Ref::Creature(id));
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(&n.name).font(serif_bold(34.0)));
        ui.label(RichText::new(&n.title).font(italic(22.0)).color(DIM));
    });
    species_link(ui, nav, h, n.species);
    ui.add_space(10.0);
    paragraph(ui, nav, h, &h.biography(n), serif(17.5), TEXT);
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
        tile(ui, &span(n.life.age as f64), if n.died.is_some() { "lived" } else { "old" }, TEAL);
        tile(ui, &n.life.kills.to_string(), "kills", EMBER);
        tile(ui, &n.life.offspring.to_string(), "offspring", ACID);
        tile(ui, &n.life.scars.to_string(), "wounds survived", BONE);
        tile(ui, &n.life.kindness.to_string(), "kindnesses", VIOLET);
        tile(ui, &n.life.generation.to_string(), "generation", AMBER);
    });
    let children: Vec<_> = h.notables.values().filter(|c| c.life.parent == Some(id)).collect();
    if !children.is_empty() {
        section(ui, "Remembered children");
        ui.horizontal_wrapped(|ui| {
            for c in children {
                link(ui, nav, format!("{} {}", c.name, c.title), Ref::Creature(c.id));
            }
        });
    }
    mentions(ui, nav, h, Ref::Creature(id));
}

// ---------------------------------------------------------------- Peoples

pub fn peoples(ui: &mut Ui, nav: &mut Nav, h: &History) {
    heading(ui, "Peoples", "Clans and colonies, the wars between them, and the sicknesses that move through them.");
    if !h.wars.is_empty() {
        section(ui, "Wars");
        egui::Grid::new("wars").striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
            for (i, w) in h.wars.iter().enumerate().rev() {
                link_styled(ui, nav, RichText::new(capital(&w.name)).font(serif_bold(16.0)).color(AMBER), Ref::War(i));
                ui.horizontal(|ui| {
                    link(ui, nav, capital(&group_name(h, w.a)), Ref::Group(w.a));
                    ui.label(RichText::new("v").color(FAINT));
                    link(ui, nav, capital(&group_name(h, w.b)), Ref::Group(w.b));
                });
                ui.label(format!("{} dead", w.deaths));
                match w.ended {
                    None => ui.label(RichText::new("raging").color(EMBER)),
                    Some(t) => ui
                        .label(RichText::new(format!("{}, {}", span(t - w.began), when(w.began))).small().color(FAINT)),
                };
                ui.end_row();
            }
        });
        ui.add_space(14.0);
    }
    section(ui, "Peoples");
    let mut groups: Vec<_> = h.groups.values().filter(|g| g.notable).collect();
    groups.sort_by_key(|g| (g.gone.is_some(), std::cmp::Reverse(g.members.max(g.peak / 4))));
    egui::Grid::new("groups").striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
        for g in groups {
            signals(ui, &g.signals);
            link_styled(ui, nav, RichText::new(capital(&g.name)).font(serif_bold(16.0)).color(AMBER), Ref::Group(g.id));
            species_link(ui, nav, h, g.species);
            match g.gone {
                None => ui.label(format!("{} members", g.members)),
                Some(t) => ui.label(RichText::new(format!("gone since {}", when(t))).color(FAINT)),
            };
            ui.label(RichText::new(place((g.x[0] + g.x[1]) / 2.0)).small().color(DIM));
            ui.end_row();
        }
    });
    if !h.strains.is_empty() {
        ui.add_space(14.0);
        section(ui, "Sicknesses and partners");
        let mut strains: Vec<_> = h.strains.values().filter(|s| s.peak >= 5).collect();
        strains.sort_by_key(|s| std::cmp::Reverse(s.deaths));
        egui::Grid::new("strains").striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
            for s in strains {
                link_styled(
                    ui,
                    nav,
                    RichText::new(capital(&s.name)).font(serif_bold(16.0)).color(ACID),
                    Ref::Strain(s.id),
                );
                species_link(ui, nav, h, s.host);
                ui.label(format!("{} dead, {} sick at most", s.deaths, s.peak));
                ui.label(RichText::new(if s.gone.is_some() { "gone" } else { "still about" }).small().color(DIM));
                ui.end_row();
            }
        });
    }
}

fn group_page(ui: &mut Ui, nav: &mut Nav, h: &History, id: GroupId) {
    let Some(g) = h.groups.get(&id) else { return };
    ui.horizontal(|ui| {
        signals(ui, &g.signals);
        ui.label(RichText::new(capital(&g.name)).font(serif_bold(34.0)));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("A people of the").color(DIM));
        link(ui, nav, plural(&species_name(h, g.species)), Ref::Species(g.species));
    });
    let status = match g.gone {
        None => format!(
            "{} members, {} at most. Formed {}, ranging over {}.",
            g.members,
            g.peak,
            when(g.formed),
            place((g.x[0] + g.x[1]) / 2.0)
        ),
        Some(t) => format!("Gone since {}, after {}. It numbered {} at most.", when(t), span(t - g.formed), g.peak),
    };
    ui.label(RichText::new(status).color(if g.gone.is_none() { TEAL } else { FAINT }));
    ui.add_space(8.0);
    if let Some(p) = g.parent.filter(|p| h.groups.contains_key(p)) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Broke away from").color(DIM));
            link(ui, nav, group_name(h, p), Ref::Group(p));
        });
    }
    let splits: Vec<_> = h.groups.values().filter(|c| c.parent == Some(id) && c.notable).collect();
    if !splits.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Splinter groups:").color(DIM));
            for c in splits {
                link(ui, nav, capital(&c.name), Ref::Group(c.id));
            }
        });
    }
    if let Some(l) = g.leader.filter(|l| h.names.contains_key(l)) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if g.gone.is_none() { "Led by" } else { "Last led by" }).color(DIM));
            link(ui, nav, h.creature_name(l), Ref::Creature(l));
        });
    }
    ui.label(RichText::new(format!("Killed {} enemies and lost {} in fighting.", g.kills, g.losses)).color(DIM));
    let wars: Vec<_> = h.wars.iter().enumerate().filter(|(_, w)| w.a == id || w.b == id).collect();
    if !wars.is_empty() {
        section(ui, "Wars");
        for (i, w) in wars {
            ui.horizontal(|ui| {
                link(ui, nav, capital(&w.name), Ref::War(i));
                ui.label(RichText::new(format!("{} dead", w.deaths)).small().color(DIM));
            });
        }
    }
    mentions(ui, nav, h, Ref::Group(id));
}

fn war_page(ui: &mut Ui, nav: &mut Nav, h: &History, i: usize) {
    let Some(w) = h.wars.get(i) else { return };
    ui.label(RichText::new(capital(&w.name)).font(serif_bold(34.0)));
    ui.horizontal(|ui| {
        link(ui, nav, capital(&group_name(h, w.a)), Ref::Group(w.a));
        ui.label(RichText::new("against").color(DIM));
        link(ui, nav, group_name(h, w.b), Ref::Group(w.b));
    });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
        tile(ui, &w.deaths.to_string(), "dead", CRIMSON);
        tile(ui, &w.clashes.to_string(), "clashes", EMBER);
        let lasted = w.ended.unwrap_or(h.now) - w.began;
        tile(ui, &span(lasted), if w.ended.is_some() { "long" } else { "so far" }, AMBER);
    });
    ui.add_space(6.0);
    ui.label(RichText::new(format!("Began {} in {}.", when(w.began), place(w.x))).color(DIM));
    if let Some(o) = &w.outcome {
        paragraph(ui, nav, h, &format!("It ended in {o}."), serif(16.0), TEXT);
    }
    mentions(ui, nav, h, Ref::War(i));
}

fn strain_page(ui: &mut Ui, nav: &mut Nav, h: &History, id: StrainId) {
    let Some(s) = h.strains.get(&id) else { return };
    ui.label(RichText::new(capital(&s.name)).font(serif_bold(34.0)));
    let what = if s.traits.virulence < 0.0 {
        "A partner that lives inside its host and feeds it"
    } else if s.traits.visible > 0.5 {
        "A parasite you can see growing inside its host"
    } else {
        "A sickness that spreads by touch"
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{what}, in the")).font(italic(17.0)).color(DIM));
        link(ui, nav, plural(&species_name(h, s.host)), Ref::Species(s.host));
    });
    ui.add_space(8.0);
    let t = &s.traits;
    bar(ui, "Contagion", t.contagion, "", ACID);
    bar(ui, "Harm", t.virulence.max(0.0), "", CRIMSON);
    bar(ui, "Mind control", t.manipulation, "steers its host", VIOLET);
    bar(ui, "Bursts out", t.bursts, "", EMBER);
    bar(ui, "Brings light", t.photosynthetic, "feeds its host", GOLD);
    ui.add_space(6.0);
    ui.label(format!("{} dead in all. {} sick at its height. Arose {}.", s.deaths, s.peak, when(s.arose)));
    if let Some(p) = s.parent {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Descended from").color(DIM));
            link(ui, nav, h.strains.get(&p).map_or("an older strain".into(), |p| capital(&p.name)), Ref::Strain(p));
        });
    }
    mentions(ui, nav, h, Ref::Strain(id));
}

pub fn detail(ui: &mut Ui, nav: &mut Nav, h: &History, r: Ref, portraits: &mut Portraits) {
    ui.add_space(8.0);
    match r {
        Ref::Species(id) => match h.species.get(&id) {
            Some(s) => species_page(ui, nav, h, s, portraits),
            None => {
                ui.label("Unknown species.");
            }
        },
        Ref::Creature(id) => creature_page(ui, nav, h, id),
        Ref::Group(id) => group_page(ui, nav, h, id),
        Ref::War(i) => war_page(ui, nav, h, i),
        Ref::Strain(id) => strain_page(ui, nav, h, id),
    }
}
