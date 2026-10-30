//! Runs a world on its own background thread, so the host only has to pick
//! up frames and facts. In real time the thread does a few milliseconds of
//! work every 200 ms and sleeps the rest; it can also run faster than real
//! time, up to as fast as the machine allows.

use crate::frame::Frame;
use crate::history::Fact;
use crate::save;
use crate::sim::Sim;
use crate::world::TICK;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Options {
    /// Where the world is saved; None never saves.
    pub save_path: Option<PathBuf>,
    pub save_every: Duration,
    /// Sim seconds to run quickly at the start, to catch up on time the PC was off.
    pub catch_up: f64,
    /// Runs on the sim thread before anything else, e.g. to lower its priority.
    pub on_start: Option<Box<dyn FnOnce() + Send>>,
    /// Takes the facts on the sim thread instead of `Runner::drain_history`.
    pub recorder: Option<Box<dyn Recorder>>,
}

impl Default for Options {
    fn default() -> Self {
        Self { save_path: None, save_every: Duration::from_secs(600), catch_up: 0.0, on_start: None, recorder: None }
    }
}

/// Keeps the world's facts as they happen, on the sim thread, e.g. its chronicle.
pub trait Recorder: Send {
    fn record(&mut self, facts: Vec<(f64, Fact)>);
    /// The world has just been saved with every fact recorded so far.
    fn saved(&mut self);
    /// A new frame of the world, drawn or not, after its facts were recorded.
    fn frame(&mut self, _frame: &Frame) {}
}

/// Sim seconds per real second that means as fast as it can go.
pub const FLAT_OUT: f32 = f32::INFINITY;
/// Frames are made at most this often, however fast the world runs.
const FRAME_MIN: f32 = 1.0 / 30.0;
/// The share of each frame's time spent stepping when the world can't keep
/// up with its speed; the rest is left for frames, saves and the machine.
const DUTY: f32 = 0.9;
/// Above this share of one core, ticks are shared out among the helpers,
/// until the shared work falls below the second share.
const SHARE: (f32, f32) = (0.6, 0.15);

#[derive(Default)]
struct Control {
    paused: bool,
    quit: bool,
    save_now: bool,
    /// Sim seconds per real second, or FLAT_OUT.
    speed: f32,
    /// Extra threads the world may use when one isn't enough for its speed.
    helpers: usize,
}

/// The two most recent frames and when the newer one was made, so the
/// renderer can draw smoothly between ticks.
#[derive(Clone)]
pub struct Frames {
    pub prev: Arc<Frame>,
    pub next: Arc<Frame>,
    pub made: Instant,
    /// Real seconds until the frame after `next` is due.
    pub span: f32,
}

impl Frames {
    /// How far to blend from `prev` to `next` right now, 0..1.
    pub fn alpha(&self) -> f32 {
        (self.made.elapsed().as_secs_f32() / self.span).clamp(0.0, 1.0)
    }
}

struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
    frames: Mutex<Frames>,
    history: Mutex<Vec<(f64, Fact)>>,
    /// Sim time, and whether it's still catching up.
    status: Mutex<(f64, bool)>,
    /// Sim seconds per real second over the last few seconds.
    rate: Mutex<f32>,
}

pub struct Runner {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Runner {
    pub fn start(mut sim: Sim, mut opts: Options) -> Self {
        let first = Arc::new(Frame::of(sim.world()));
        let shared = Arc::new(Shared {
            control: Mutex::new(Control { speed: 1.0, ..Default::default() }),
            wake: Condvar::new(),
            frames: Mutex::new(Frames { prev: first.clone(), next: first, made: Instant::now(), span: TICK }),
            history: Mutex::new(Vec::new()),
            status: Mutex::new((sim.time(), opts.catch_up > 0.0)),
            rate: Mutex::new(1.0),
        });
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("vesper-sim".into())
            .spawn(move || {
                if let Some(f) = opts.on_start.take() {
                    f();
                }
                run(&mut sim, &s, &mut opts);
            })
            .expect("can't start the simulation thread");
        Self { shared, thread: Some(thread) }
    }

    pub fn frames(&self) -> Frames {
        self.shared.frames.lock().unwrap().clone()
    }

    /// Facts since the last call, for the chronicle.
    pub fn drain_history(&self) -> Vec<(f64, Fact)> {
        std::mem::take(&mut *self.shared.history.lock().unwrap())
    }

    /// Sim seconds since the world's birth, and whether it is still catching up.
    pub fn status(&self) -> (f64, bool) {
        *self.shared.status.lock().unwrap()
    }

    /// Stops time, e.g. while a game needs the machine. Nothing runs while paused.
    pub fn pause(&self, paused: bool) {
        self.shared.control.lock().unwrap().paused = paused;
        self.shared.wake.notify_all();
    }

    /// Sim seconds per real second: 1 is real time, FLAT_OUT as fast as the
    /// thread is given time for.
    pub fn set_speed(&self, speed: f32) {
        self.shared.control.lock().unwrap().speed = speed.max(0.0);
        self.shared.wake.notify_all();
    }

    /// How many more threads may share the work when one can't keep up.
    pub fn set_helpers(&self, n: usize) {
        self.shared.control.lock().unwrap().helpers = n;
    }

    /// How fast the world has really been running lately, in sim seconds per
    /// real second; below the speed asked for when the machine can't keep up.
    pub fn rate(&self) -> f32 {
        *self.shared.rate.lock().unwrap()
    }

    pub fn save_now(&self) {
        self.shared.control.lock().unwrap().save_now = true;
        self.shared.wake.notify_all();
    }

    /// Saves and stops the thread.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.control.lock().unwrap().quit = true;
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn publish(sim: &mut Sim, s: &Shared, recorder: &mut Option<Box<dyn Recorder>>, span: f32) {
    let frame = Arc::new(Frame::of(sim.world()));
    {
        let mut f = s.frames.lock().unwrap();
        f.prev = std::mem::replace(&mut f.next, frame.clone());
        f.made = Instant::now();
        f.span = span;
    }
    hand_on(sim, s, recorder);
    if let Some(r) = recorder {
        r.frame(&frame);
    }
}

/// Passes the latest facts to the recorder, or keeps them for the host.
fn hand_on(sim: &mut Sim, s: &Shared, recorder: &mut Option<Box<dyn Recorder>>) {
    let facts = sim.drain_history();
    if facts.is_empty() {
        return;
    }
    match recorder {
        Some(r) => r.record(facts),
        None => s.history.lock().unwrap().extend(facts),
    }
}

fn run(sim: &mut Sim, s: &Shared, opts: &mut Options) {
    let mut recorder = opts.recorder.take();
    let save_path = opts.save_path.clone();
    let save = |sim: &mut Sim, recorder: &mut Option<Box<dyn Recorder>>| {
        hand_on(sim, s, recorder);
        let Some(p) = &save_path else { return };
        match save::save(sim, p) {
            Ok(()) => {
                if let Some(r) = recorder {
                    r.saved();
                }
            }
            Err(e) => eprintln!("vesper-sim: saving failed: {e}"),
        }
    };
    // Catch up quickly, yielding often so the machine stays responsive.
    let mut behind = opts.catch_up;
    sim.set_coarse(behind > 0.0);
    while behind > 0.0 {
        if s.control.lock().unwrap().quit {
            sim.set_coarse(false);
            save(sim, &mut recorder);
            return;
        }
        let ran = sim.advance(behind.min(20.0), Duration::from_millis(15));
        behind -= ran;
        *s.status.lock().unwrap() = (sim.time(), true);
        // Keep facts flowing but don't build frames nobody is looking at.
        hand_on(sim, s, &mut recorder);
        std::thread::sleep(Duration::from_millis(5));
    }
    sim.set_coarse(false);
    publish(sim, s, &mut recorder, TICK);
    let mut last_save = Instant::now();
    let mut next = Instant::now();
    // Ticks due but not yet run.
    let mut owed = 0.0f64;
    // Recent real seconds per tick, for deciding when to share the work.
    let mut tick_cost = 0.0f32;
    let mut sharing = false;
    let mut measured = (Instant::now(), sim.time());
    loop {
        let (paused, quit, save_now, speed, helpers) = {
            let mut c = s.control.lock().unwrap();
            let r = (c.paused, c.quit, c.save_now, c.speed, c.helpers);
            c.save_now = false;
            r
        };
        if quit {
            break;
        }
        if save_now || last_save.elapsed() >= opts.save_every {
            save(sim, &mut recorder);
            last_save = Instant::now();
        }
        if paused || speed <= 0.0 {
            let c = s.control.lock().unwrap();
            let _ = s.wake.wait_timeout(c, Duration::from_millis(500));
            next = Instant::now();
            owed = 0.0;
            measured = (Instant::now(), sim.time());
            continue;
        }
        // A frame per tick up to 30 a second, then several ticks per frame.
        let span = (TICK / speed).max(FRAME_MIN);
        let flat_out = speed.is_infinite();
        if !flat_out {
            owed = (owed + (speed * span / TICK) as f64).min((speed * span / TICK) as f64 * 2.0 + 1.0);
        }
        // One thread is plenty in real time; more only when it can't keep up.
        let need = if flat_out { f32::INFINITY } else { speed * tick_cost / TICK };
        sharing = helpers > 0 && if sharing { need > SHARE.1 } else { need > SHARE.0 };
        sim.set_helpers(if sharing { helpers } else { 0 });
        let start = Instant::now();
        let budget = Duration::from_secs_f32(span * DUTY);
        while (flat_out || owed >= 1.0) && start.elapsed() < budget {
            let t = Instant::now();
            sim.step();
            let cost = t.elapsed().as_secs_f32();
            tick_cost = if tick_cost == 0.0 { cost } else { tick_cost + (cost - tick_cost) * 0.05 };
            owed -= 1.0;
        }
        // What couldn't be run in time is let go rather than piled up.
        owed = owed.clamp(0.0, 1.0);
        publish(sim, s, &mut recorder, span);
        *s.status.lock().unwrap() = (sim.time(), false);
        let (since, from) = measured;
        if since.elapsed() >= Duration::from_secs(3) {
            *s.rate.lock().unwrap() = ((sim.time() - from) / since.elapsed().as_secs_f64()) as f32;
            measured = (Instant::now(), sim.time());
        }
        next += Duration::from_secs_f32(span);
        let now = Instant::now();
        if next > now {
            let c = s.control.lock().unwrap();
            let _ = s.wake.wait_timeout(c, next - now);
        } else {
            // Fell behind (the PC was busy or asleep): don't try to make it up.
            next = now;
        }
    }
    save(sim, &mut recorder);
}
