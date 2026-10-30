//! Running the world faster than real time.

use std::time::Duration;
use vesper_sim::runner::{FLAT_OUT, Options, Runner};
use vesper_sim::{Sim, save};

/// Sharing a tick over more threads, to run faster, must not change the world.
#[test]
fn helpers_change_nothing() {
    let mut one = Sim::new(11, 1_800_000_000.0);
    let mut many = Sim::new(11, 1_800_000_000.0);
    many.set_helpers(3);
    for _ in 0..120 {
        one.step();
        many.step();
    }
    assert!(one.world().creatures.len() >= 16, "too few bodies to share out");
    assert!(save::to_bytes(&one) == save::to_bytes(&many), "the worlds parted");
}

/// An empty world ticks in well under a millisecond even in a debug build on
/// a busy runner, so only a runner that waits between ticks could fall short.
#[test]
fn flat_out_outruns_real_time() {
    let mut sim = Sim::new(3, 1_800_000_000.0);
    let w = sim.world_mut();
    w.creatures.clear();
    w.eggs.clear();
    w.remains.clear();
    w.plants.clear();
    let runner = Runner::start(sim, Options::default());
    runner.set_speed(FLAT_OUT);
    std::thread::sleep(Duration::from_millis(3500));
    let (time, _) = runner.status();
    let rate = runner.rate();
    runner.stop();
    assert!(rate > 5.0 && time > 17.5, "flat out ran at only {rate:.1}x ({time:.0} s in 3.5 s)");
}
