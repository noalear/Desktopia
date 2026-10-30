//! Drawing a segment in its form. The simulation knows a segment as one
//! tapered rod; here it becomes what its form says: a ringed limb, an eye on
//! a stalk, a talon, a pincer, antlers, a plume, a fin, a blade, a row of
//! plates, a bristling tuft or a coil, as ordinary capsules and sheets the
//! renderer already draws.
//!
//! Everything is in screen-world units, between the segment's laid-out base
//! `a` and tip `b`, so physics and animation never see any of it.

use crate::body::Segment;
use crate::genome::{Form, Tissue};
use crate::pose::{Membrane, Pose, Seg};
use std::f32::consts::{PI, TAU};

/// Pieces an organism may draw before further segments fall back to plain
/// tubes, so an elaborate body can't swamp a frame.
const BUDGET: usize = 260;
/// One pixel of a 4K monitor, in world units (monitor heights). Detail
/// finer than a few pixels isn't drawn: a hair or barb per two or three
/// pixels at most, so a world of small bodies costs little more than it did
/// as plain tubes, while a close-up shows every barb.
const PIXEL: f32 = 1.0 / 2160.0;

/// What one segment is drawn with.
pub(crate) struct Piece {
    pub a: [f32; 2],
    pub b: [f32; 2],
    /// Radius at base and tip.
    pub ra: f32,
    pub rb: f32,
    pub damage: f32,
    /// Wind weight at the base and at the tip.
    pub sway: [f32; 2],
    /// +1 when the body is seen as laid out, -1 when seen mirrored, so
    /// curls keep their side of the body.
    pub hand: f32,
    /// Something grows on from its tip, so it must still end there.
    pub continues: bool,
    /// Something grows from its middle, so it must not bow far.
    pub carries: bool,
}

struct Pen<'a> {
    out: &'a mut Pose,
    tissue: Tissue,
    damage: f32,
    side: i8,
    sway: [f32; 2],
}

impl Pen<'_> {
    /// A capsule from `p` to `q`, which lie `_t` and `u` of the way from the
    /// segment's base to its tip, for the wind.
    fn tube(&mut self, p: [f32; 2], q: [f32; 2], rp: f32, rq: f32, _t: f32, u: f32) {
        self.out.segs.push(Seg {
            a: p,
            b: q,
            ra: rp.max(0.0),
            rb: rq.max(0.0),
            tissue: self.tissue,
            damage: self.damage,
            side: self.side,
            sway: self.sway[0] + (self.sway[1] - self.sway[0]) * u,
        });
    }

    fn sheet(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2]) {
        self.out.membranes.push(Membrane { a, b, c, tissue: self.tissue, sway: self.sway[1] });
    }
}

fn add(p: [f32; 2], q: [f32; 2]) -> [f32; 2] {
    [p[0] + q[0], p[1] + q[1]]
}

fn mul(p: [f32; 2], k: f32) -> [f32; 2] {
    [p[0] * k, p[1] * k]
}

fn rot(d: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
}

/// A fixed number 0..1 for the `j`th detail of segment `i`, so hairs and
/// barbs don't jump about between frames.
fn jitter(i: usize, j: usize) -> f32 {
    crate::rng::hash01((i as u64) << 20 ^ j as u64 ^ 0x5EED_F0E5)
}

/// Draws segment `s` (index `i`) as its form.
pub(crate) fn draw(out: &mut Pose, s: &Segment, i: usize, p: &Piece) {
    let mut pen = Pen { out, tissue: shown(s, i, p), damage: p.damage, side: s.side, sway: p.sway };
    let (a, b) = (p.a, p.b);
    let v = [b[0] - a[0], b[1] - a[1]];
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-6 || pen.out.segs.len() + pen.out.membranes.len() > BUDGET {
        pen.tube(a, b, p.ra, p.rb, 1.0, 1.0);
        return;
    }
    let d = [v[0] / len, v[1] / len];
    let n = mul([-d[1], d[0]], p.hand);
    // How many of a repeated detail fit, at most `want`, at one per `each` pixels of length.
    let px = len / PIXEL;
    let fit = |want: usize, each: f32| want.min((px / each) as usize);
    // How far it bows at its middle; held back where limbs grow from it.
    let bow = (s.curl * 0.25).clamp(-0.4, 0.4) * len * if p.carries { 0.3 } else { 1.0 };
    let bow = if bow.abs() < PIXEL * 0.75 { 0.0 } else { bow };
    let at = |t: f32| add(add(a, mul(v, t)), mul(n, bow * (PI * t).sin()));
    let r = |t: f32| p.ra + (p.rb - p.ra) * t;
    let k = s.detail.clamp(0.0, 1.0);
    // Which way a curl turns, mirrored with the body.
    let turn = if s.curl < 0.0 { -1.0 } else { 1.0 } * p.hand;
    let ends = !p.continues;

    match s.form {
        Form::Beads if fit(3, 5.0) >= 2 => {
            let m = fit(3 + (k * 5.0).round() as usize, 5.0);
            for j in 0..m {
                let (t0, t1) = (j as f32 / m as f32, (j + 1) as f32 / m as f32);
                let (q0, q1) = (at(t0), at(t1));
                let rr = r((t0 + t1) * 0.5) * 1.1;
                // Pulled in from each end so the beads pinch where they meet.
                let inset = (rr * 0.6).min(len / m as f32 * 0.3);
                let dq = [q1[0] - q0[0], q1[1] - q0[1]];
                let lq = (dq[0] * dq[0] + dq[1] * dq[1]).sqrt().max(1e-6);
                let e = mul(dq, inset / lq);
                pen.tube(add(q0, e), add(q1, mul(e, -1.0)), r(t0) * 1.1, r(t1) * 1.1, t0, t1);
            }
        }
        Form::Bulb => {
            let big = p.ra.max(p.rb);
            if !ends {
                // Mid-chain, it can only swell.
                bowed(&mut pen, &at, |t| big * (1.0 + 0.4 * k * (PI * t).sin()), bow);
                return;
            }
            let ball = (big.max(len * 0.22) * (0.8 + 0.6 * k)).min(len * 0.45);
            let centre = add(b, mul(d, -ball * 0.7));
            pen.tube(a, centre, big * 0.45, big * 0.35, 0.0, 0.7);
            pen.tube(add(centre, mul(d, -ball * 0.15)), add(centre, mul(d, ball * 0.15)), ball, ball, 0.7, 1.0);
        }
        Form::Hook => {
            let end = bowed(&mut pen, &at, r, bow * 0.3);
            if ends {
                let mut q = b;
                let mut dir = end;
                let mut rr = p.rb.min(p.ra * 0.6);
                let step = len * (0.15 + 0.1 * k);
                let m = fit(3, 6.0).max(1);
                for j in 0..m {
                    dir = rot(dir, turn * (0.55 + 0.35 * k) * 3.0 / m as f32);
                    let q1 = add(q, mul(dir, step * (1.0 - 0.2 * j as f32) * 3.0 / m as f32));
                    let r1 = rr * if j + 1 == m { 0.08 } else { 0.62 };
                    pen.tube(q, q1, rr, r1, 1.0, 1.0);
                    (q, rr) = (q1, r1);
                }
            }
        }
        Form::Pincer if ends => {
            // A swollen palm, then two curved fingers that meet at the tips.
            let palm = at(0.6);
            pen.tube(a, palm, p.ra, p.ra.max(p.rb) * 1.15, 0.0, 0.6);
            let gape = 0.25 + 0.35 * k;
            let fl = len * 0.45;
            for (side, size) in [(1.0, 1.0), (-1.0, 0.85)] {
                let open = rot(d, side * gape * p.hand);
                let mid = add(palm, mul(open, fl * 0.55 * size));
                let close = rot(d, -side * gape * 0.9 * p.hand);
                let tip = add(mid, mul(close, fl * 0.5 * size));
                let rr = p.ra.max(p.rb) * 0.55 * size;
                pen.tube(palm, mid, rr, rr * 0.7, 0.6, 0.85);
                pen.tube(mid, tip, rr * 0.7, rr * 0.08, 0.85, 1.0);
            }
        }
        Form::Fork if ends => {
            let end = bowed(&mut pen, &at, r, bow);
            let prongs: &[f32] = if k > 0.5 { &[-0.55, 0.0, 0.55] } else { &[-0.45, 0.45] };
            for (j, &ang) in prongs.iter().enumerate() {
                let spread = ang * (0.8 + 0.4 * jitter(i, j)) * p.hand;
                let dir = rot(end, spread);
                let pl = len * (0.3 + 0.15 * k) * if ang == 0.0 { 1.2 } else { 1.0 };
                let mid = add(b, mul(dir, pl * 0.5));
                let tip = add(mid, mul(rot(dir, spread * 0.4), pl * 0.5));
                let rr = p.rb * 0.7;
                if px < 10.0 {
                    pen.tube(b, tip, rr, rr * 0.08, 1.0, 1.0);
                    continue;
                }
                pen.tube(b, mid, rr, rr * 0.6, 1.0, 1.0);
                pen.tube(mid, tip, rr * 0.6, rr * 0.08, 1.0, 1.0);
                // Antlers branch again.
                if k > 0.75 && ang != 0.0 && px > 20.0 {
                    let tine = add(mid, mul(rot(dir, -spread * 1.4), pl * 0.35));
                    pen.tube(mid, tine, rr * 0.45, rr * 0.05, 1.0, 1.0);
                }
            }
        }
        Form::Frond => {
            let film = s.tissue == Tissue::LightFilm;
            let shaft = if film { 0.1 } else { 0.5 };
            bowed(&mut pen, &at, |t| r(t) * shaft, bow);
            let m = fit(3 + (k * 7.0).round() as usize, 5.0);
            let wide = p.ra.max(p.rb) * 2.0;
            for j in 0..m {
                let t = (j as f32 + 0.6) / (m as f32 + 0.2);
                let q = at(t);
                let lb = (len * 0.3 * (1.0 - 0.55 * t) * (0.7 + 0.3 * k)).max(wide * 0.6);
                for side in [1.0f32, -1.0] {
                    let dir = rot(d, side * p.hand * (0.8 + 0.2 * jitter(i, j)));
                    let e = add(q, mul(dir, lb));
                    if film {
                        // A leaflet, drawn as a small blade of its own.
                        pen.tube(q, e, lb * 0.12, lb * 0.18, t, t);
                    } else {
                        pen.tube(q, e, (r(t) * 0.22).max(p.ra * 0.08), 0.0, t, t);
                    }
                }
            }
        }
        Form::Fan => {
            let end = bowed(&mut pen, &at, r, bow);
            if !ends {
                return;
            }
            let rays = fit(3 + (k * 4.0).round() as usize, 3.0).max(2);
            let spread = 0.9 + 1.5 * k;
            let lr = len * (0.45 + 0.25 * k);
            let mut prev: Option<[f32; 2]> = None;
            for j in 0..rays {
                let c = j as f32 / (rays - 1) as f32 * 2.0 - 1.0;
                let dir = rot(end, c * spread * 0.5 * p.hand);
                // Longer at the edges, like a fluke or a forked fin.
                let e = add(b, mul(dir, lr * (0.8 + 0.3 * c.abs() + 0.1 * jitter(i, j))));
                pen.tube(b, e, p.rb * 0.35, p.rb * 0.04, 1.0, 1.0);
                if let Some(q) = prev {
                    pen.sheet(b, q, e);
                }
                prev = Some(e);
            }
        }
        Form::Paddle if s.tissue == Tissue::LightFilm => {
            // Light film is drawn as a leaf blade already: a broad one.
            let w = p.ra.max(p.rb) * (0.8 + 0.8 * k);
            bowed(&mut pen, &at, |t| w * (0.6 + 0.4 * t), bow);
        }
        Form::Paddle => {
            let w = p.ra.max(p.rb) * (1.2 + 1.8 * k);
            bowed(&mut pen, &at, |t| r(t) * 0.35, bow);
            for side in [1.0, -1.0] {
                let q1 = add(at(0.35), mul(n, side * w));
                let q2 = add(at(0.7), mul(n, side * w * 0.8));
                pen.sheet(a, q1, q2);
                pen.sheet(a, q2, b);
            }
        }
        Form::Plates => {
            let m = fit(3 + (k * 4.0).round() as usize, 5.0).max(1);
            // A ridge along the back, then the plates over it.
            let lift = mul(n, p.ra.max(p.rb) * 0.12);
            for j in 0..m {
                // Each overlaps the next a little, like shingles.
                let t0 = j as f32 / m as f32;
                let t1 = (((j + 1) as f32 + 0.35) / m as f32).min(1.0);
                pen.tube(add(at(t0), lift), add(at(t1), lift), r(t0) * 1.18, r(t1) * 0.95, t0, t1);
            }
        }
        Form::Tuft => {
            // Light film bristles into needles along a bare twig.
            let film = s.tissue == Tissue::LightFilm;
            bowed(&mut pen, &at, |t| r(t) * if film { 0.15 } else { 1.0 }, bow);
            let m = fit(4 + (k * 10.0).round() as usize, 2.5);
            let puff = s.tissue == Tissue::Float && ends;
            for j in 0..m {
                let h = jitter(i, j);
                if puff {
                    // A seed head: plumes in every direction from the tip.
                    let dir = rot(d, TAU * (j as f32 + h * 0.5) / m as f32);
                    pen.tube(b, add(b, mul(dir, len * (0.45 + 0.3 * h))), p.rb * 0.12, p.rb * 0.25, 1.0, 1.0);
                    continue;
                }
                let t = (j as f32 + 0.5) / m as f32;
                let side = if j % 2 == 0 { 1.0 } else { -1.0 };
                let q = add(at(t), mul(n, side * r(t) * 0.6));
                let dir = rot(d, side * p.hand * (0.7 + 0.6 * h));
                let lb = if film { len * 0.3 } else { r(t) + len * 0.18 } * (0.6 + 0.8 * jitter(i, j + 64));
                let q = if film { at(t) } else { q };
                pen.tube(q, add(q, mul(dir, lb)), (r(t) * 0.14).max(p.ra * 0.05), 0.0, t, t);
            }
        }
        Form::Coil if ends => {
            // Curls ever tighter toward the tip.
            let m = fit(6 + (k * 6.0).round() as usize, 3.0).max(2);
            let total = turn * (2.0 + 4.0 * k);
            let weights: f32 = (1..=m).map(|j| j as f32).sum();
            let mut q = a;
            let mut dir = d;
            let piece = len / m as f32;
            for j in 0..m {
                let t0 = j as f32 / m as f32;
                let t1 = (j + 1) as f32 / m as f32;
                let q1 = add(q, mul(dir, piece * (1.0 - 0.4 * t0)));
                pen.tube(q, q1, r(t0), r(t1), t0, t1);
                q = q1;
                dir = rot(dir, total * (j + 1) as f32 / weights);
            }
        }
        _ => {
            // Tubes, and tip forms on a part that carries on into another.
            bowed(&mut pen, &at, r, bow);
        }
    }
}

/// The tissue a segment shows on the outside. Gut, fat and brood are
/// hidden inside the body, unless they are the body: a gut that limbs grow
/// from is a belly under skin, a fat hump or a brood pouch bulges out in
/// skin, and feathery gills hang out in the air.
fn shown(s: &Segment, i: usize, p: &Piece) -> Tissue {
    match (s.tissue, s.form) {
        (Tissue::Gut, Form::Frond | Form::Tuft) => Tissue::Muscle,
        (Tissue::Store, Form::Bulb) | (Tissue::Brood, Form::Bulb | Form::Paddle) => Tissue::Frame,
        (Tissue::Gut | Tissue::Store | Tissue::Brood, _) if i == 0 || p.continues || p.carries => Tissue::Frame,
        (t, _) => t,
    }
}

/// Draws the segment's (bowed) length as a tube, and returns the direction
/// at its tip.
fn bowed(pen: &mut Pen, at: &impl Fn(f32) -> [f32; 2], r: impl Fn(f32) -> f32, bow: f32) -> [f32; 2] {
    let b = at(1.0);
    let pieces = if bow.abs() > 0.0 { 3 } else { 1 };
    let mut q = at(0.0);
    for j in 0..pieces {
        let (t0, t1) = (j as f32 / pieces as f32, (j + 1) as f32 / pieces as f32);
        let q1 = at(t1);
        pen.tube(q, q1, r(t0), r(t1), t0, t1);
        q = q1;
    }
    let before = at(if pieces == 1 { 0.0 } else { 0.9 });
    let v = [b[0] - before[0], b[1] - before[1]];
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::Body;
    use crate::genome::{Founder, Genome};
    use crate::rng::Rng;

    /// A founder whose parts all take one form, drawn big enough for full detail.
    fn drawn(form: Form, seed: u64) -> (Body, Pose) {
        let mut rng = Rng::new(seed);
        let mut g = Genome::founder(Founder::Grazer, &mut rng);
        for m in g.plan.modules.iter_mut() {
            m.form = form;
            m.curl = 0.8;
            m.detail = 0.9;
        }
        g.plan.trunk.form = form;
        g.plan.trunk.tail_form = form;
        g.plan.trunk.head_form = form;
        let body = Body::develop(&g);
        let mut pose = Pose::default();
        crate::pose::at_rest(&body, [1.0, 0.3], 0.1, -1.0, false, &mut pose);
        (body, pose)
    }

    #[test]
    fn every_form_draws_whole_and_finite() {
        for (k, &form) in Form::ALL.iter().enumerate() {
            let (body, pose) = drawn(form, k as u64);
            assert!(pose.segs.len() >= body.segments.len(), "{form:?} drew too little");
            assert!(pose.segs.len() + pose.membranes.len() <= BUDGET + 40, "{form:?} drew too much");
            for s in &pose.segs {
                assert!(s.a.iter().chain(&s.b).all(|v| v.is_finite()) && s.ra >= 0.0 && s.rb >= 0.0);
            }
        }
    }

    #[test]
    fn forms_stay_joined_to_what_grows_from_them() {
        for (k, &form) in Form::ALL.iter().enumerate() {
            let (body, pose) = drawn(form, 10 + k as u64);
            // Where each segment's base lands, laid out as `at_rest` lays it.
            let m = crate::motion::standing(&body, 0.1, -1.0);
            let mut base = vec![[0.0f32; 2]; body.segments.len()];
            for p in &m.parts {
                let s = &body.segments[p.seg as usize];
                let half = s.length * m.scale * 0.5;
                let (sa, ca) = p.angle.sin_cos();
                base[p.seg as usize] = [1.0 + (p.u - ca * half) * -1.0, 0.3 + (p.alt - sa * half)];
            }
            // Wherever a part carries on from another's tip, some piece of
            // the parent still reaches that point.
            for (i, s) in body.segments.iter().enumerate() {
                if s.parent < 0 || s.along < 0.9 {
                    continue;
                }
                let at = base[i];
                let near = pose.segs.iter().any(|q| {
                    let d = |p: [f32; 2]| ((p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2)).sqrt();
                    d(q.a).min(d(q.b)) < q.ra.max(q.rb).max(0.002) * 2.0 + 0.01 * 0.1
                });
                assert!(near, "{form:?}: part {i} came loose from its parent");
            }
        }
    }

    #[test]
    fn small_bodies_draw_little_detail() {
        let (body, big) = drawn(Form::Tuft, 3);
        let mut small = Pose::default();
        crate::pose::at_rest(&body, [1.0, 0.3], 0.002, 1.0, false, &mut small);
        assert!(small.segs.len() * 3 < big.segs.len(), "{} vs {}", small.segs.len(), big.segs.len());
    }
}
