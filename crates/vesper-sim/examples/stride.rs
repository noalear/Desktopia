//! Logs one walker's power and progress tick by tick.
use vesper_sim::Sim;
use vesper_sim::history::Locomotion;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    let id = sim.world().creatures.iter().find(|c| c.abil.locomotion == Some(Locomotion::Walk)).unwrap().id.0;
    let mut last: Option<([f32; 3], f32)> = None;
    for t in 0..150 {
        sim.step();
        let Some(c) = sim.world().creatures.iter().find(|c| c.id.0 == id) else { break };
        if let Some((p, h)) = last {
            let d = [c.pos[0] - p[0], c.pos[1] - p[1]];
            let along = (d[0] * h.cos() + d[1] * h.sin()) / 0.2 / c.length();
            let w = sim.world();
            let near = w
                .plants
                .iter()
                .map(|p| ((p.x - c.pos[0]).powi(2) + (p.depth - c.pos[1]).powi(2)).sqrt())
                .fold(f32::MAX, f32::min);
            print!("near plant {:.3} ({:.1} len) energy {:.2} ", near, near / c.length(), c.energy / c.abil.energy_cap);
            println!(
                "{t:>3} power {:+.2} along {:+.2} len/s heading {:+.2} turn {:+.2} stride {:+.2} back {:+.2} action {:?} clock {:.2}",
                c.drive[0], along, c.heading, c.heading - h, c.stride, c.back, c.action, c.rig.clock
            );
        }
        last = Some((c.pos, c.heading));
    }
}
