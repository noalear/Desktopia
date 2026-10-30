use vesper_sim::Sim;
use vesper_sim::body::Body;
use vesper_sim::genome::{Founder, Genome};
use vesper_sim::rng::Rng;
use vesper_sim::{pose, save};

const EPOCH: f64 = 1_800_000_000.0;

#[test]
fn matter_is_conserved() {
    let mut sim = Sim::new(7, EPOCH);
    let start = sim.world().matter_total;
    // Stop just short of the periodic rebalance so real leaks would show.
    for _ in 0..1499 {
        sim.step();
    }
    let now = sim.world().matter_now();
    assert!((now - start).abs() / start < 1e-4, "matter drifted from {start} to {now}");
}

#[test]
fn a_loaded_world_carries_on_exactly() {
    let mut a = Sim::new(11, EPOCH);
    for _ in 0..400 {
        a.step();
    }
    let bytes = save::to_bytes(&a);
    let mut b = save::from_bytes(&bytes).expect("loads");
    for _ in 0..400 {
        a.step();
        b.step();
    }
    let (wa, wb) = (a.world(), b.world());
    assert_eq!(wa.time, wb.time);
    assert_eq!(wa.creatures.len(), wb.creatures.len());
    assert_eq!(wa.plants.len(), wb.plants.len());
    for (x, y) in wa.creatures.iter().zip(&wb.creatures) {
        assert_eq!(x.id, y.id);
        assert_eq!(x.pos, y.pos);
        assert_eq!(x.energy, y.energy);
    }
}

#[test]
fn damaged_saves_are_refused() {
    let sim = Sim::new(3, EPOCH);
    let mut bytes = save::to_bytes(&sim);
    let n = bytes.len();
    bytes[n / 2] ^= 0x55;
    assert!(save::from_bytes(&bytes).is_err());
    assert!(save::from_bytes(b"hello").is_err());
}

#[test]
fn genomes_stay_growable_under_heavy_mutation() {
    let mut rng = Rng::new(5);
    for kind in Founder::ALL {
        let mut g = Genome::founder(kind, &mut rng);
        for _ in 0..2000 {
            g.mutate(&mut rng, 2.5);
            let body = Body::develop(&g);
            assert!(!body.segments.is_empty());
            assert!(body.segments.len() <= vesper_sim::body::MAX_SEGMENTS);
            assert!(g.plan.modules.len() <= vesper_sim::plan::MAX_MODULES);
            for s in &body.segments {
                assert!(s.length.is_finite() && s.width.is_finite() && s.base.iter().all(|v| v.is_finite()));
            }
        }
    }
}

#[test]
fn development_is_deterministic() {
    let mut rng = Rng::new(9);
    let g = Genome::founder(Founder::Grazer, &mut rng);
    assert_eq!(Body::develop(&g), Body::develop(&g));
}

#[test]
fn poses_land_on_screen() {
    let mut sim = Sim::new(13, EPOCH);
    for _ in 0..50 {
        sim.step();
    }
    let frame = sim.frame();
    let mut p = pose::Pose::default();
    let mut segs = 0;
    for c in &frame.creatures {
        pose::creature(c, 1.0, &mut p);
        for s in &p.segs {
            for v in [s.a, s.b] {
                assert!(v[0] > -0.5 && v[0] < 4.1 && v[1] > -0.3 && v[1] < 1.3, "{v:?}");
            }
        }
        segs += p.segs.len();
    }
    for pl in &frame.plants {
        pose::plant(pl, &mut p);
        segs += p.segs.len();
    }
    assert!(segs > 1000);
}

#[test]
fn lava_only_runs_down_the_volcano() {
    // Weather alone, the way the preview's --weather runs it, until an
    // eruption has come and gone.
    let mut sim = Sim::new(3, EPOCH);
    let w = sim.world_mut();
    w.env.next_eruption = w.time + 60.0;
    let (mut last, mut furthest, mut ended) = (0.0f32, 0.0f32, false);
    for _ in 0..4 * 3600 {
        w.time += 1.0;
        w.ground.refresh(w.env.river);
        vesper_sim::world::weather(w, 1.0);
        w.history.clear();
        let e = &w.env;
        if e.eruption > 0.0 {
            assert!(e.lava >= last, "lava ran back up the cone: {last} -> {}", e.lava);
        } else if furthest > 0.0 {
            ended = true;
            break;
        }
        last = e.lava;
        furthest = furthest.max(e.lava);
    }
    assert!(ended && furthest > 0.1, "the eruption ran lava {furthest} and ended: {ended}");
}
