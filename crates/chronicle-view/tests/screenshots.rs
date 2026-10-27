//! Renders every page of the window from a demo history, to check the look.
//! `SHOTS=dir cargo test -p chronicle-view --test screenshots -- --ignored`

use chronicle_view::{ChronicleApp, Page, Source};
use egui_kittest::Harness;
use vesper_chronicle::{Ref, demo};

#[test]
#[ignore = "needs a GPU or software Vulkan; writes PNG files"]
fn render_every_page() {
    let dir = std::path::PathBuf::from(std::env::var("SHOTS").unwrap_or_else(|_| "target/shots".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let h = demo::history(4.0, 7);
    let species = h.species.values().filter(|s| s.alive() && !s.children.is_empty()).max_by_key(|s| s.peak).unwrap().id;
    let creature = h.notables.values().max_by(|a, b| a.score.total_cmp(&b.score)).unwrap().id;
    let group = h.groups.values().filter(|g| g.notable).max_by_key(|g| g.peak).unwrap().id;
    let pages = [
        ("1-now", Page::Now),
        ("2-tree", Page::Tree),
        ("3-annals", Page::Annals),
        ("4-species", Page::Species),
        ("5-lives", Page::Lives),
        ("6-peoples", Page::Peoples),
        ("7-species-page", Page::Detail(Ref::Species(species))),
        ("8-creature-page", Page::Detail(Ref::Creature(creature))),
        ("9-group-page", Page::Detail(Ref::Group(group))),
    ];
    for (name, page) in pages {
        let h = h.clone();
        let mut harness = Harness::builder()
            .with_size([1360.0, 880.0])
            .wgpu()
            .build_eframe(move |cc| {
                let mut app = ChronicleApp::new(&cc.egui_ctx, Source::Fixed(h));
                app.open(page);
                app
            });
        harness.run_steps(4);
        let image = harness.render().expect("render");
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
}
