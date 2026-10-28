//! The window's frame: header, navigation, and keeping up with the files.

use crate::pages::{self, PageState};
use crate::portrait::Portraits;
use crate::theme::*;
use crate::tree::TreeState;
use crate::widgets::{Nav, ago, when};
use eframe::egui::{self, CornerRadius, RichText, Sense, Stroke, Ui, vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vesper_chronicle::demo::DemoWorld;
use vesper_chronicle::{Reader, Ref};

/// Where the history comes from.
pub enum Source {
    /// A chronicle folder the wallpaper writes.
    Folder(PathBuf),
    /// A made-up world, for trying the window before the simulation exists.
    Demo,
    /// A fixed history, for tests.
    Fixed(Box<vesper_chronicle::History>),
}

/// How often to look for new lines from the wallpaper.
const POLL: Duration = Duration::from_secs(2);
/// How often the demo world moves on by one census.
const DEMO_STEP: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Now,
    Tree,
    Annals,
    Species,
    Lives,
    Peoples,
    Detail(Ref),
}

impl Page {
    const MAIN: [(Page, &'static str, &'static str); 6] = [
        (Page::Now, "Now", "What is happening"),
        (Page::Tree, "Tree of life", "Every species and where it came from"),
        (Page::Annals, "Annals", "The written history"),
        (Page::Species, "Species", "The field guide"),
        (Page::Lives, "Lives", "Creatures worth remembering"),
        (Page::Peoples, "Peoples", "Clans, wars and plagues"),
    ];
}

pub struct ChronicleApp {
    reader: Reader,
    demo: Option<(DemoWorld, Instant)>,
    folder: Option<PathBuf>,
    page: Page,
    back: Vec<Page>,
    last_poll: Instant,
    last_change: Instant,
    state: PageState,
    tree: TreeState,
    portraits: Portraits,
}

impl ChronicleApp {
    pub fn new(cc: &eframe::CreationContext, source: Source) -> Self {
        install(&cc.egui_ctx);
        let (reader, demo, folder) = match source {
            Source::Folder(dir) => (Reader::new(&dir), None, Some(dir)),
            Source::Demo => {
                let (reader, world) = demo();
                (reader, Some(world), None)
            }
            Source::Fixed(h) => (Reader::fixed(*h), None, None),
        };
        Self {
            reader,
            demo,
            folder,
            page: Page::Now,
            back: Vec::new(),
            last_poll: Instant::now(),
            last_change: Instant::now(),
            state: PageState::default(),
            tree: TreeState::default(),
            portraits: Portraits::new(cc.wgpu_render_state.as_ref()),
        }
    }

    /// Opens a page directly, for tests and screenshots.
    pub fn open(&mut self, page: Page) {
        self.page = page;
    }

    /// Swaps the empty real world for the sample one.
    fn show_demo(&mut self) {
        let (reader, world) = demo();
        self.reader = reader;
        self.demo = Some(world);
        self.page = Page::Now;
        self.back.clear();
    }

    fn keep_up(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some((world, last)) = &mut self.demo {
            if now.duration_since(*last) >= DEMO_STEP {
                *last = now;
                let wall_base = self.reader.history.wall - self.reader.history.now;
                for (tick, fact) in world.step() {
                    self.reader.history.apply(tick, wall_base + tick, &fact);
                }
                self.last_change = now;
                ctx.request_repaint();
            }
            ctx.request_repaint_after(DEMO_STEP);
            return;
        }
        if now.duration_since(self.last_poll) >= POLL {
            self.last_poll = now;
            if self.reader.poll() {
                if self.reader.take_renewed() {
                    // A new world: nothing on the old pages exists any more.
                    self.page = Page::Now;
                    self.back.clear();
                    self.state = PageState::default();
                    self.tree = TreeState::default();
                }
                self.last_change = now;
                ctx.request_repaint();
            }
        }
        ctx.request_repaint_after(POLL);
    }

    fn header(&mut self, ui: &mut Ui) {
        let h = &self.reader.history;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("VESPER").font(serif_bold(22.0)).color(AMBER).extra_letter_spacing(4.0));
            ui.label(RichText::new("CHRONICLE").font(bold(11.0)).color(DIM).extra_letter_spacing(3.0));
            ui.add_space(18.0);
            if !h.species.is_empty() {
                ui.label(RichText::new(when(h.now)).font(serif(16.0)).color(TEXT));
                if let Some(era) = h.era() {
                    let name = h.species.get(&era.species).map(|s| vesper_chronicle::history::plural(&s.common));
                    if let Some(name) = name {
                        ui.label(RichText::new("·").color(FAINT));
                        ui.label(
                            RichText::new(format!("the Age of the {}", vesper_chronicle::names::title_case(&name)))
                                .font(italic(16.0))
                                .color(MAGENTA.gamma_multiply(0.9)),
                        );
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let (dot, text, tip) = if self.demo.is_some() {
                    (
                        AMBER,
                        "Sample world".to_string(),
                        "A made-up history to show the chronicle before the real world runs".to_string(),
                    )
                } else if h.species.is_empty() {
                    (FAINT, "Waiting for the world".to_string(), self.folder_text())
                } else {
                    let quiet = self.last_change.elapsed().as_secs_f64();
                    (TEAL, "Live".to_string(), format!("Last news {}. {}", ago(quiet), self.folder_text()))
                };
                ui.label(RichText::new(text).small().color(DIM)).on_hover_text(tip);
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, dot);
            });
        });
    }

    fn folder_text(&self) -> String {
        self.folder.as_ref().map_or_else(String::new, |f| format!("Reading {}", f.display()))
    }

    fn nav(&mut self, ui: &mut Ui) {
        ui.add_space(14.0);
        for (page, name, hint) in Page::MAIN {
            let selected =
                self.page == page || (self.back.first() == Some(&page) && matches!(self.page, Page::Detail(_)));
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let p = ui.painter();
            if selected {
                p.rect_filled(rect, CornerRadius::same(8), CARD);
                p.rect_filled(
                    egui::Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0)),
                    CornerRadius::same(2),
                    AMBER,
                );
            } else if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(8), CARD.gamma_multiply(0.6));
            }
            p.text(
                rect.left_center() + vec2(16.0, 0.0),
                egui::Align2::LEFT_CENTER,
                name,
                if selected { bold(14.5) } else { egui::FontId::proportional(14.5) },
                if selected { TEXT } else { DIM },
            );
            if resp.on_hover_text(hint).clicked() {
                self.page = page;
                self.back.clear();
            }
        }
        let h = &self.reader.history;
        if h.species.is_empty() {
            return;
        }
        ui.add_space(18.0);
        ui.painter().hline(ui.available_rect_before_wrap().x_range(), ui.cursor().top(), Stroke::new(1.0, LINE));
        ui.add_space(14.0);
        let extinct = h.species.len() - h.living_species();
        for (n, what) in [
            (h.living_species(), "species alive"),
            (extinct, "extinct"),
            (h.notables.len(), "lives recorded"),
            (h.entries.len(), "entries"),
        ] {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new(n.to_string()).font(bold(13.0)).color(TEXT));
                ui.label(RichText::new(what).small().color(FAINT));
            });
        }
    }
}

impl eframe::App for ChronicleApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.keep_up(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ui, |ui| self.header(ui));
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(188.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| self.nav(ui));

        let mut nav = Nav::default();
        let page = self.page;
        self.portraits.begin_frame();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::same(0))).show(
            ui,
            |ui| {
                let h = &self.reader.history;
                if h.species.is_empty() {
                    if pages::empty(ui, self.folder.is_some()) {
                        self.show_demo();
                    }
                    return;
                }
                if page == Page::Tree {
                    crate::tree::show(ui, &mut nav, h, &mut self.tree);
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink(false).id_salt(page_salt(page)).show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin { left: 32, right: 32, top: 24, bottom: 32 }).show(
                        ui,
                        |ui| {
                            ui.set_max_width(ui.available_width().min(1180.0));
                            if let Page::Detail(_) = page
                                && !self.back.is_empty()
                                && ui.button("← Back").clicked()
                                && let Some(prev) = self.back.pop()
                            {
                                self.page = prev;
                            }
                            match page {
                                Page::Now => pages::now(ui, &mut nav, h),
                                Page::Annals => pages::annals(ui, &mut nav, h, &mut self.state),
                                Page::Species => {
                                    pages::species_list(ui, &mut nav, h, &mut self.state, &mut self.portraits)
                                }
                                Page::Lives => pages::lives(ui, &mut nav, h, &mut self.state),
                                Page::Peoples => pages::peoples(ui, &mut nav, h),
                                Page::Detail(r) => pages::detail(ui, &mut nav, h, r, &mut self.portraits),
                                Page::Tree => {}
                            }
                        },
                    );
                });
            },
        );
        if let Some(r) = nav.go {
            self.back.push(self.page);
            self.page = Page::Detail(r);
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        if ui.input(|i| {
            i.pointer.button_pressed(egui::PointerButton::Extra1) || (!typing && i.key_pressed(egui::Key::Backspace))
        }) && let Some(prev) = self.back.pop()
        {
            self.page = prev;
        }
    }
}

/// A toy world run for a few days, to keep going live from there.
fn demo() -> (Reader, (DemoWorld, Instant)) {
    let mut world = DemoWorld::new(7);
    let mut h = vesper_chronicle::History::default();
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
        - 4.0 * 86_400.0;
    while world.t < 4.0 * 86_400.0 {
        for (tick, fact) in world.step() {
            h.apply(tick, wall + tick, &fact);
        }
    }
    (Reader::fixed(h), (world, Instant::now()))
}

fn page_salt(p: Page) -> String {
    format!("{p:?}")
}
