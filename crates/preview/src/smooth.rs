//! `--smooth SECONDS` with `--sim`: films one walker at 30 frames a second,
//! drawn the old way (the newest tick as soon as it lands) on the left and
//! played back smoothly on the right, from the same world on the same
//! clock. Ticks land up to 30 ms late, as on a busy Windows machine.
//!
//! Writes smooth-NNN.png frames, and prints how far the walker moved on
//! screen in each drawn frame, both ways.

use crate::living::Painter;
use crate::{Args, MONITOR, read_rgba8, target, view, view_at};
use std::sync::Arc;
use vesper_render::{LifeBatch, OutputConfig, OutputMode, Renderer};
use vesper_sim::Sim;
use vesper_sim::frame::Frame;
use vesper_sim::history::Locomotion;
use vesper_sim::playback::{Playback, Shown};
use vesper_sim::pose;

const FPS: f64 = 30.0;
/// Seconds of playback before filming starts, for the clock to settle.
const LEAD: f64 = 1.0;

pub fn render(renderer: &mut Renderer, args: &Args, mut sim: Sim, secs: f64) {
    // The ticks, and when each one lands.
    let mut ticks: Vec<(Arc<Frame>, f64)> = Vec::new();
    let mut k = 0u64;
    while (k as f64) * 0.2 < secs + LEAD + 1.0 {
        let late = 0.03 * ((k * 7919 % 17) as f64 / 16.0);
        ticks.push((Arc::new(sim.frame()), k as f64 * 0.2 + late));
        sim.step();
        k += 1;
    }
    // The grown walker that covers the most ground.
    let first = &ticks[0].0;
    let walker = first
        .creatures
        .iter()
        .filter(|c| {
            c.locomotion == Locomotion::Walk
                && at(first, c.id).1 * MONITOR[1] as f32 > 20.0
                && ticks.iter().all(|(f, _)| f.creatures.iter().any(|d| d.id == c.id))
        })
        .max_by(|a, b| ground(&ticks, a.id).total_cmp(&ground(&ticks, b.id)))
        .map(|c| c.id)
        .expect("no walker lived through the clip");
    let path: Vec<[f32; 2]> = ticks.iter().map(|(f, _)| at(f, walker).0).collect();
    let len = ticks.iter().map(|(f, _)| at(f, walker).1).fold(0.0f32, f32::max);
    eprintln!("filming creature {walker}, {:.0} px long at 4K", len * MONITOR[1] as f32);
    let (lo, hi) = path.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    });
    let cw = (hi[0] - lo[0] + len * 3.0).max(len * 8.0);
    let ch = (hi[1] - lo[1] + len * 2.5).max(cw * 0.5);
    let crop = [(lo[0] + hi[0]) * 0.5 - cw * 0.5, (lo[1] + hi[1]) * 0.5 - ch * 0.45, cw, ch];

    let ppu = MONITOR[1] as f32 * args.scale;
    let world_w = MONITOR[0] as f32 * 2.0 / MONITOR[1] as f32;
    let (w, h) = ((crop[2] * ppu) as u32, (crop[3] * ppu) as u32);
    let world_size = [world_w * ppu, ppu];
    let sdr = wgpu::TextureFormat::Rgba8Unorm;
    let mut out = renderer.create_output(OutputConfig {
        origin_px: [crop[0] * ppu, (1.0 - crop[1] - crop[3]) * ppu],
        size_px: [w, h],
        format: sdr,
        mode: OutputMode::Sdr,
    });
    let tex = target(renderer.device(), w, h, sdr);
    let mut batch = LifeBatch::new();
    let mut old = Painter::default();
    let mut new = Painter::default();
    old.plants(first, &mut batch);
    renderer.set_plants(&mut batch);

    let mut playback = Playback::new();
    let mut landed = 0;
    let mut moved = [Vec::new(), Vec::new()];
    let mut was: [Option<f32>; 2] = [None, None];
    let frames = (secs * FPS) as usize;
    for n in 0..frames + (LEAD * FPS) as usize {
        let now = n as f64 / FPS;
        while landed < ticks.len() && ticks[landed].1 <= now {
            playback.push(ticks[landed].0.clone(), ticks[landed].1);
            landed += 1;
        }
        let newest = ticks[landed.max(1) - 1].0.clone();
        let before = Shown { a: newest.clone(), b: newest, t: 1.0 };
        let after = playback.show(now).unwrap();
        let filming = n >= (LEAD * FPS) as usize;
        let mut img = image::RgbaImage::new(w * 2 + 8, h);
        for (side, (shown, painter)) in [(&before, &mut old), (&after, &mut new)].into_iter().enumerate() {
            let x = drawn_x(shown, walker);
            if filming && let Some(p) = drawn_at(shown, walker) {
                let px = [(p[0] - crop[0]) * ppu + side as f32 * (w + 8) as f32, (crop[1] + crop[3] - p[1]) * ppu];
                println!("track {n} {side} {:.0} {:.0}", px[0], px[1]);
            }
            if let (Some(p), Some(x)) = (was[side], x) {
                if filming {
                    moved[side].push((x - p) * MONITOR[1] as f32);
                }
            }
            was[side] = x;
            batch.clear();
            painter.creatures(shown, now, &mut batch);
            if !filming {
                continue;
            }
            renderer.set_creatures(&mut batch);
            let world = view_at(args, world_size, args.time + now);
            renderer.render(&mut out, &world, &view(&tex));
            let px = read_rgba8(renderer, &tex, w, h);
            for y in 0..h {
                for x in 0..w {
                    let k = ((y * w + x) * 4) as usize;
                    img.put_pixel(x + side as u32 * (w + 8), y, image::Rgba([px[k], px[k + 1], px[k + 2], 255]));
                }
            }
        }
        if filming {
            let f = n - (LEAD * FPS) as usize;
            let path = args.out.join(format!("smooth-{f:03}.png"));
            img.save(&path).unwrap();
            eprintln!("{}", path.display());
        }
    }
    for (name, m) in ["before", "after"].iter().zip(&moved) {
        let still = m.iter().filter(|d| d.abs() < 0.05).count();
        let mean = m.iter().map(|d| d.abs()).sum::<f32>() / m.len().max(1) as f32;
        let worst = m.iter().map(|d| d.abs()).fold(0.0f32, f32::max);
        println!(
            "{name}: {} frames, {still} without moving, mean {mean:.2} px a frame, biggest jump {worst:.2} px (4K pixels)",
            m.len()
        );
        println!("  {}", m.iter().map(|d| format!("{d:.1}")).collect::<Vec<_>>().join(" "));
    }
}

/// Where a creature stands on screen in a snapshot, and its length.
fn at(f: &Frame, id: u64) -> ([f32; 2], f32) {
    let c = f.creatures.iter().find(|c| c.id == id).unwrap();
    let (p, shrink, _) = pose::project(c.pos);
    (p, c.scale * c.body.extent * shrink)
}

/// How far along x a creature goes over the ticks, all told.
fn ground(ticks: &[(Arc<Frame>, f64)], id: u64) -> f32 {
    let xs: Vec<f32> = ticks.iter().map(|(f, _)| at(f, id).0[0]).collect();
    xs.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>()
}

/// Where the creature is drawn along x in what is shown.
fn drawn_x(s: &Shown, id: u64) -> Option<f32> {
    drawn_at(s, id).map(|p| p[0])
}

/// Where the creature stands on screen in what is shown.
fn drawn_at(s: &Shown, id: u64) -> Option<[f32; 2]> {
    let b = s.b.creatures.iter().find(|c| c.id == id)?;
    let c = match s.a.creatures.iter().find(|c| c.id == id) {
        Some(a) => pose::blend(a, b, s.t, s.gap()),
        None => b.clone(),
    };
    Some(pose::project(c.pos).0)
}
