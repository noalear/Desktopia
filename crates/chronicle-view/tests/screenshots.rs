//! Renders every page of the window, to check the look: from a demo history,
//! or from the chronicle folder in `CHRONICLE` when it is set.
//! `SHOTS=dir cargo test -p chronicle-view --test screenshots -- --ignored`

use chronicle_view::{ChronicleApp, Page, Source};
use egui_kittest::Harness;
use vesper_chronicle::{Reader, Ref, demo};

#[test]
#[ignore = "needs a GPU or software Vulkan; writes PNG files"]
fn render_every_page() {
    let dir = std::path::PathBuf::from(std::env::var("SHOTS").unwrap_or_else(|_| "target/shots".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let h = match std::env::var("CHRONICLE") {
        Ok(folder) => Reader::new(folder).history,
        Err(_) => demo::history(4.0, 7),
    };
    let species = h.species.values().filter(|s| s.alive()).max_by_key(|s| (!s.children.is_empty(), s.peak)).map(|s| s.id);
    let creature = h.notables.values().max_by(|a, b| a.score.total_cmp(&b.score)).map(|n| n.id);
    let group = h.groups.values().max_by_key(|g| (g.notable, g.peak)).map(|g| g.id);
    let mut pages: Vec<(String, Page)> = vec![
        ("1-now".into(), Page::Now),
        ("2-tree".into(), Page::Tree),
        ("3-annals".into(), Page::Annals),
        ("4-species".into(), Page::Species),
        ("5-lives".into(), Page::Lives),
        ("6-peoples".into(), Page::Peoples),
    ];
    pages.extend(species.map(|id| ("7-species-page".into(), Page::Detail(Ref::Species(id)))));
    pages.extend(creature.map(|id| ("8-creature-page".into(), Page::Detail(Ref::Creature(id)))));
    pages.extend(group.map(|id| ("9-group-page".into(), Page::Detail(Ref::Group(id)))));
    // Every species with a portrait, to check them all.
    pages.extend(h.likenesses.keys().map(|id| (format!("portrait-{}", id.0), Page::Detail(Ref::Species(*id)))));
    for (name, page) in pages {
        let h = h.clone();
        let mut harness = Harness::builder().with_size([1360.0, 880.0]).wgpu().build_eframe(move |cc| {
            let mut app = ChronicleApp::new(cc, Source::Fixed(Box::new(h)));
            app.open(page);
            app
        });
        harness.run_steps(6);
        let image = harness.render().expect("render");
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
}
