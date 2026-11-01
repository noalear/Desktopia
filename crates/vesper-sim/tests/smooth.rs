//! Bodies drawn between ticks move smoothly: they glide from one snapshot to
//! the next and keep striding, rather than jumping five times a second.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;
use vesper_sim::Sim;
use vesper_sim::frame::{CreatureView, Frame};
use vesper_sim::playback::Playback;
use vesper_sim::pose;

fn grown(ticks: usize) -> (Sim, Arc<Frame>) {
    let mut sim = Sim::new(5, 1_800_000_000.0);
    for _ in 0..ticks {
        sim.step();
    }
    let f = Arc::new(sim.frame());
    (sim, f)
}

fn by_id(f: &Frame) -> HashMap<u64, &CreatureView> {
    f.creatures.iter().map(|c| (c.id, c)).collect()
}

fn phase_gap(x: f32, y: f32) -> f32 {
    let d = (x - y).rem_euclid(TAU);
    d.min(TAU - d)
}

#[test]
fn blends_land_on_both_snapshots_and_keep_striding() {
    let (mut sim, a) = grown(25);
    sim.step();
    let b = sim.frame();
    let dt = (b.time - a.time) as f32;
    let before = by_id(&a);
    let mut striding = 0;
    for nb in &b.creatures {
        let Some(na) = before.get(&nb.id) else { continue };
        let start = pose::blend(na, nb, 0.0, dt);
        let end = pose::blend(na, nb, 1.0, dt);
        for i in 0..3 {
            assert!((start.pos[i] - na.pos[i]).abs() < 1e-5);
            assert!((end.pos[i] - nb.pos[i]).abs() < 1e-5);
        }
        assert!(phase_gap(start.gait, na.gait) < 1e-3);
        assert!(phase_gap(end.gait, nb.gait) < 1e-3);
        // Striding steadily forward, the phase only ever moves forward.
        if na.stride > 0.3 && nb.stride > 0.3 {
            striding += 1;
            let mut was = start.gait;
            let mut ran = 0.0;
            for k in 1..=20 {
                let g = pose::blend(na, nb, k as f32 / 20.0, dt).gait;
                let step = (g - was).rem_euclid(TAU);
                assert!(step < TAU * 0.5, "stride ran backwards");
                ran += step;
                was = g;
            }
            let want = (na.stride + nb.stride) * 0.5 * TAU * dt;
            assert!((ran - want).abs() <= TAU * 0.5 + 1e-3, "strode {ran:.2} rad where {want:.2} was due");
        }
    }
    assert!(striding > 0, "nobody was striding to check");
}

/// Snapshots arriving a little late, unevenly, as on a busy machine, are
/// still played back on an even clock, and every body glides along its path
/// with no frame frozen and none jumping ahead.
#[test]
fn drawn_motion_is_even_through_uneven_snapshots() {
    let (mut sim, first) = grown(25);
    let mut playback = Playback::new();
    playback.push(first, 0.0);
    let mut tick = 1u64;
    let mut shown = Vec::new();
    let mut paths: HashMap<u64, Vec<([f32; 2], f32, u64)>> = HashMap::new();
    let frame = 1.0 / 30.0;
    let mut now = 0.0f64;
    while now < 6.0 {
        // Each snapshot is due every 0.2 s and lands up to 50 ms late.
        let due = tick as f64 * 0.2;
        let late = 0.05 * ((tick * 7919 % 11) as f64 / 10.0);
        if now >= due + late {
            sim.step();
            playback.push(Arc::new(sim.frame()), due + late);
            tick += 1;
        }
        let s = playback.show(now).unwrap();
        shown.push(s.a.time + (s.b.time - s.a.time) * s.t as f64);
        let before = by_id(&s.a);
        for b in &s.b.creatures {
            let Some(a) = before.get(&b.id) else { continue };
            let c = pose::blend(a, b, s.t, s.gap());
            // Where it is, and how far it goes in a frame at this tick's pace.
            let d = ((b.pos[0] - a.pos[0]).powi(2) + (b.pos[1] - a.pos[1]).powi(2)).sqrt();
            paths.entry(b.id).or_default().push(([c.pos[0], c.pos[1]], d * frame as f32 / s.gap().max(1e-3), s.a.tick));
        }
        now += frame;
    }
    // Skip the first second, while the clock settles.
    for w in shown.windows(2).skip(30) {
        let step = w[1] - w[0];
        assert!((step - frame).abs() < frame * 0.25, "a frame showed {step:.4} s of the world, not {frame:.4}");
    }
    let mut moving = 0;
    for path in paths.values() {
        for w in path.windows(2).skip(30) {
            let (a, b) = (w[0].0, w[1].0);
            let step = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let (slow, pace) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
            // Between the same two snapshots it must keep going; across one
            // it may have turned back.
            if slow > 1e-5 && w[0].2 == w[1].2 {
                moving += 1;
                assert!(step > slow * 0.5, "a moving body stood still");
            }
            assert!(step < pace * 1.6 + 1e-6, "a body jumped {step:.5} in a frame where {pace:.5} was its pace");
        }
    }
    assert!(moving > 1000, "too little moved to tell");
}
