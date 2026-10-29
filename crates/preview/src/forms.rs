//! A gallery of body forms: founders of every plan and seedlings of every
//! kind, each dressed and mutated differently, then a row of each grown
//! from many generations of mutation, to show the range of shapes the genes
//! can make without growing a whole world.

use crate::{Args, read_rgba8, target, view, view_at};
use vesper_render::{LifeBatch, Look, Organism, OutputConfig, OutputMode, Renderer};
use vesper_sim::body::Body;
use vesper_sim::genome::{Founder, Genome};
use vesper_sim::plant::{PlantGenome, Seedling};
use vesper_sim::pose::{self, Pose};
use vesper_sim::rng::Rng;

const COLS: usize = 6;
const CELL: [u32; 2] = [480, 360];

struct Sitter {
    body: Body,
    look: [f32; 16],
    plant: bool,
    id: u32,
}

pub fn render(renderer: &mut Renderer, args: &Args) {
    let mut rng = Rng::new(args.seed);
    let mut creatures: Vec<Vec<Sitter>> = Vec::new();
    for kind in [Founder::Drifter, Founder::Grazer, Founder::Swimmer, Founder::Crawler] {
        creatures.push((0..COLS).map(|_| creature(Genome::founder(kind, &mut rng), &mut rng, 0)).collect());
    }
    // Lineages left to drift: forms come and go, parts duplicate and change.
    let kinds = [Founder::Drifter, Founder::Grazer, Founder::Swimmer, Founder::Crawler];
    for _ in 0..2 {
        creatures.push((0..COLS).map(|c| creature(Genome::founder(kinds[c % 4], &mut rng), &mut rng, 60)).collect());
    }
    sheet(renderer, args, &creatures, "forms-creatures");

    let mut plants: Vec<Vec<Sitter>> = Vec::new();
    for kind in [Seedling::Fan, Seedling::VentCrust, Seedling::Thicket, Seedling::Mat] {
        plants.push((0..COLS).map(|_| plant(PlantGenome::founder(kind, &mut rng), &mut rng, 0)).collect());
    }
    let kinds = [Seedling::Fan, Seedling::VentCrust, Seedling::Thicket, Seedling::Mat];
    for _ in 0..2 {
        plants.push((0..COLS).map(|c| plant(PlantGenome::founder(kinds[c % 4], &mut rng), &mut rng, 40)).collect());
    }
    sheet(renderer, args, &plants, "forms-plants");
}

fn creature(mut g: Genome, rng: &mut Rng, rounds: usize) -> Sitter {
    for _ in 0..rounds {
        g.mutate(rng, 1.5);
    }
    Sitter { body: Body::develop(&g), look: g.look, plant: false, id: rng.next_u64() as u32 }
}

fn plant(mut g: PlantGenome, rng: &mut Rng, rounds: usize) -> Sitter {
    for _ in 0..rounds {
        g.mutate(rng);
    }
    // Shown as it stands, not floating.
    g.floating = g.floating.min(0.4);
    Sitter { body: g.develop(), look: g.look, plant: true, id: rng.next_u64() as u32 }
}

fn sheet(renderer: &mut Renderer, args: &Args, rows: &[Vec<Sitter>], name: &str) {
    let world_w = 2.0 * crate::MONITOR[0] as f32 / crate::MONITOR[1] as f32;
    let mut img = image::RgbaImage::new(CELL[0] * COLS as u32, CELL[1] * rows.len() as u32);
    let tex = target(renderer.device(), CELL[0], CELL[1], wgpu::TextureFormat::Rgba8Unorm);
    let (mut batch, mut pose) = (LifeBatch::new(), Pose::default());
    let (mut segs, mut membranes) = (Vec::new(), Vec::new());
    // Sitters stand on the plain near the front, where the light is good.
    let stage = [1.6f32, 0.06];
    for (row, sitters) in rows.iter().enumerate() {
        for (col, s) in sitters.iter().enumerate() {
            pose::at_rest(&s.body, stage, 1.0, 1.0, s.plant, &mut pose);
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            crate::acts::bounds(&pose, &mut lo, &mut hi);
            for m in &pose.membranes {
                for q in [m.a, m.b, m.c] {
                    for k in 0..2 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
            }
            // Fit each to its cell: about a sixth of a monitor high.
            let span = (hi[0] - lo[0]).max((hi[1] - lo[1]) * CELL[0] as f32 / CELL[1] as f32).max(1e-6);
            let k = 0.11 / span;
            pose::at_rest(&s.body, stage, k, 1.0, s.plant, &mut pose);
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            crate::acts::bounds(&pose, &mut lo, &mut hi);
            let w = 0.11 * 1.3;
            let h = w * CELL[1] as f32 / CELL[0] as f32;
            let ppu = CELL[0] as f32 / w;
            let (cx, cy) = ((lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5);
            let mut out = renderer.create_output(OutputConfig {
                origin_px: [(cx - w * 0.5) * ppu, (1.0 - cy - h * 0.5) * ppu],
                size_px: CELL,
                format: wgpu::TextureFormat::Rgba8Unorm,
                mode: OutputMode::Sdr,
            });
            batch.clear();
            crate::living::convert(&pose, |_| 0.0, &mut segs, &mut membranes);
            batch.add(&Organism {
                segs: &segs,
                membranes: &membranes,
                look: &Look { genes: s.look },
                z: 0.05,
                alive: 1.0,
                signal: 0.0,
                symbol: 0,
                bleeding: 0.0,
                infection: None,
                clip_y: None,
                ground: None,
                seed: s.id,
            });
            if s.plant {
                renderer.set_plants(&mut batch);
                renderer.set_creatures(&mut LifeBatch::new());
            } else {
                renderer.set_plants(&mut LifeBatch::new());
                renderer.set_creatures(&mut batch);
            }
            let world = view_at(args, [world_w * ppu, ppu], args.time);
            renderer.render(&mut out, &world, &view(&tex));
            let px = read_rgba8(renderer, &tex, CELL[0], CELL[1]);
            let (ox, oy) = (col as u32 * CELL[0], row as u32 * CELL[1]);
            for y in 0..CELL[1] {
                for x in 0..CELL[0] {
                    let i = ((y * CELL[0] + x) * 4) as usize;
                    img.put_pixel(ox + x, oy + y, image::Rgba([px[i], px[i + 1], px[i + 2], 255]));
                }
            }
        }
    }
    let path = args.out.join(format!("{name}-{}.png", args.seed));
    img.save(&path).unwrap();
    eprintln!("{}", path.display());
}
