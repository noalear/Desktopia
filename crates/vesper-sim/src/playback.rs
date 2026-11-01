//! Plays the world's snapshots back on a steady clock, so what is drawn
//! moves smoothly however unevenly the snapshots arrive.
//!
//! The simulation makes a snapshot every tick (or every few ticks when it
//! runs fast), but never exactly on time: the sim thread runs at low
//! priority, its ticks take more or less work, and Windows timers are
//! coarse. Drawing the newest snapshot as soon as it lands turns every late
//! or early one into a hitch. Instead the shown time runs a little behind
//! the newest snapshot, at the pace the world is running, and is only nudged
//! gently back into line, so there is nearly always a later snapshot to
//! blend towards.

use crate::frame::Frame;
use std::collections::VecDeque;
use std::sync::Arc;

/// Snapshots kept to blend between.
const KEEP: usize = 6;
/// How far behind the newest snapshot to show, in snapshot gaps. One gap is
/// the least that leaves something to blend towards; the rest is slack for
/// a late one.
const LAG: f64 = 1.6;
/// How quickly the shown time is pulled back into line, per second.
const PULL: f64 = 1.5;
/// Further out of line than this many gaps, it jumps rather than catching up.
const JUMP: f64 = 6.0;

/// Two snapshots and how far between them to draw, 0..1.
#[derive(Clone)]
pub struct Shown {
    pub a: Arc<Frame>,
    pub b: Arc<Frame>,
    pub t: f32,
}

impl Shown {
    /// Sim seconds from `a` to `b`.
    pub fn gap(&self) -> f32 {
        (self.b.time - self.a.time) as f32
    }
}

#[derive(Default)]
pub struct Playback {
    /// Recent snapshots, oldest first, each with the real time it was made.
    frames: VecDeque<(Arc<Frame>, f64)>,
    /// Sim seconds per real second, and per snapshot, lately.
    rate: f64,
    gap: f64,
    /// The sim time being shown, and the real time it was shown at.
    shown: Option<(f64, f64)>,
}

impl Playback {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a snapshot made at real time `made`, seconds on any steady
    /// clock. Snapshots already held are ignored, so it is fine to offer the
    /// same one often, and missing some is fine too.
    pub fn push(&mut self, frame: Arc<Frame>, made: f64) {
        if let Some((last, _)) = self.frames.back() {
            if frame.tick == last.tick {
                return;
            }
            if frame.tick < last.tick {
                // Another world: start over.
                *self = Self::default();
            }
        }
        if let Some((last, at)) = self.frames.back() {
            let gap = frame.time - last.time;
            let real = made - at;
            // A pause (the world stopped for a game, or nothing was drawn)
            // says nothing about how fast it runs.
            if real > 1e-3 && real < 1.0 {
                let rate = gap / real;
                if self.gap <= 0.0 {
                    (self.gap, self.rate) = (gap, rate);
                } else {
                    self.gap += (gap - self.gap) * 0.3;
                    self.rate += (rate - self.rate) * 0.3;
                }
            }
        }
        self.frames.push_back((frame, made));
        while self.frames.len() > KEEP {
            self.frames.pop_front();
        }
    }

    /// What to draw at real time `now`, or None before the first snapshot.
    pub fn show(&mut self, now: f64) -> Option<Shown> {
        let (newest, made) = self.frames.back().cloned()?;
        let oldest = self.frames.front().unwrap().0.clone();
        if self.frames.len() < 2 || self.gap <= 0.0 {
            return Some(Shown { a: newest.clone(), b: newest, t: 1.0 });
        }
        // Where the newest snapshot says the world is by now, less the lag.
        let target = newest.time + (now - made).max(0.0) * self.rate - LAG * self.gap;
        let time = match self.shown {
            Some((was, at)) if now >= at => {
                let dt = now - at;
                let ran = was + dt * self.rate;
                let off = target - ran;
                if off.abs() > JUMP * self.gap {
                    target
                } else {
                    // Never backwards: a world running ahead just slows a little.
                    (ran + off * (dt * PULL).min(1.0)).max(was)
                }
            }
            _ => target,
        };
        let time = time.clamp(oldest.time, newest.time);
        self.shown = Some((time, now));
        let mut a = &oldest;
        let mut b = &newest;
        for ((x, _), (y, _)) in self.frames.iter().zip(self.frames.iter().skip(1)) {
            if y.time >= time {
                (a, b) = (x, y);
                break;
            }
        }
        let span = b.time - a.time;
        let t = if span > 0.0 { ((time - a.time) / span).clamp(0.0, 1.0) as f32 } else { 1.0 };
        Some(Shown { a: a.clone(), b: b.clone(), t })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(tick: u64, time: f64) -> Arc<Frame> {
        Arc::new(Frame { tick, time, ..Default::default() })
    }

    /// Plays snapshots made every 0.2 s, each arriving late by a different
    /// amount up to `jitter`, and returns the sim time shown at 30 frames a second.
    fn play(jitter: f64, secs: f64) -> Vec<f64> {
        let mut p = Playback::new();
        let mut shown = Vec::new();
        let mut tick = 0u64;
        let mut due = 0.0;
        let mut now = 0.0;
        while now < secs {
            while due <= now {
                // A fixed, irregular pattern of lateness.
                let late = jitter * ((tick * 7919 % 13) as f64 / 12.0);
                if due + late <= now {
                    p.push(frame(tick, tick as f64 * 0.2), due + late);
                    tick += 1;
                    due += 0.2;
                } else {
                    break;
                }
            }
            if let Some(s) = p.show(now) {
                shown.push(s.a.time + (s.b.time - s.a.time) * s.t as f64);
            }
            now += 1.0 / 30.0;
        }
        shown
    }

    #[test]
    fn shown_time_runs_steadily_through_late_snapshots() {
        let shown = play(0.06, 30.0);
        // Once settled, every frame moves on by about a frame's worth.
        let steps: Vec<f64> = shown.windows(2).map(|w| w[1] - w[0]).skip(90).collect();
        let frame = 1.0 / 30.0;
        for s in &steps {
            assert!((s - frame).abs() < frame * 0.25, "a frame moved {s:.4} s instead of {frame:.4}");
        }
    }

    #[test]
    fn never_runs_backwards_or_past_the_newest() {
        let shown = play(0.15, 20.0);
        for w in shown.windows(2) {
            assert!(w[1] >= w[0]);
        }
    }

    #[test]
    fn starts_over_for_a_new_world() {
        let mut p = Playback::new();
        p.push(frame(50, 10.0), 0.0);
        p.push(frame(51, 10.2), 0.2);
        p.push(frame(0, 0.0), 0.3);
        let s = p.show(0.3).unwrap();
        assert_eq!(s.b.tick, 0);
    }
}
