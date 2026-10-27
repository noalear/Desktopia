//! Rooted life. Each kind follows one of the growth habits on the plant
//! reference sheet, with its proportions drawn fresh per plant. Plants are
//! posed once; the wind bends them on the GPU through each segment's sway.

use std::f32::consts::PI;

use vesper_render::{Look, Membrane, Seg, Tissue};
use vesper_terrain as terrain;

use crate::{Rng, add, lerp2, mul, order, perspective, polar};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlantKind {
    /// Translucent light-film fans on stalks, turned to the sun.
    Fan,
    /// Mineral chimneys crusted around hot springs.
    VentTubes,
    /// Stilted river-bank thicket on arching roots.
    Thicket,
    /// Tall arching stems hung with amber seed pods.
    PodArch,
    /// Low rosette of veined leaves.
    Rosette,
    /// A hinged trap of two heavy lobes.
    Trap,
    /// Night bloom with a glowing heart.
    GlowBloom,
    /// Cluster of translucent water-storing bulbs.
    Bulbs,
    /// Branching lace skeleton.
    Lace,
    /// Floating mat on the river, roots trailing.
    Mat,
}

pub struct Plant {
    pub kind: PlantKind,
    pub segs: Vec<Seg>,
    pub membranes: Vec<Membrane>,
    pub look: Look,
    pub z: f32,
    pub alive: f32,
    pub seed: u32,
}

struct Grow<'a> {
    root: [f32; 2],
    s: f32,
    height: f32,
    flex: f32,
    segs: &'a mut Vec<Seg>,
    mems: &'a mut Vec<Membrane>,
}

impl Grow<'_> {
    fn w(&self, l: [f32; 2]) -> [f32; 2] {
        add(self.root, mul(l, self.s))
    }

    /// Bend at a local point: grows with the square of its height.
    fn sway(&self, l: [f32; 2]) -> f32 {
        let h = (l[1] / self.height.max(1e-5)).max(0.0);
        self.flex * h * h * self.s
    }

    fn seg(&mut self, a: [f32; 2], b: [f32; 2], ra: f32, rb: f32, tissue: Tissue, side: f32) {
        self.segs.push(Seg {
            a: self.w(a),
            b: self.w(b),
            ra: ra * self.s,
            rb: rb * self.s,
            tissue,
            damage: 0.0,
            side,
            sway: self.sway(b),
        });
    }

    fn chain(&mut self, pts: &[[f32; 2]], r0: f32, r1: f32, tissue: Tissue, side: f32) {
        let n = (pts.len() - 1) as f32;
        for i in 0..pts.len() - 1 {
            let ra = r0 + (r1 - r0) * i as f32 / n;
            let rb = r0 + (r1 - r0) * (i + 1) as f32 / n;
            self.seg(pts[i], pts[i + 1], ra, rb, tissue, side);
        }
    }

    fn mem(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], tissue: Tissue, side: f32) {
        self.mems.push(Membrane { a: self.w(a), b: self.w(b), c: self.w(c), tissue, side });
    }

    /// A curved stem from `from`, leaving at `angle` and curling by `curl`.
    fn stem(&self, from: [f32; 2], angle: f32, length: f32, curl: f32, n: usize) -> Vec<[f32; 2]> {
        let mut pts = vec![from];
        let mut a = angle;
        for _ in 0..n {
            pts.push(add(*pts.last().unwrap(), polar(a, length / n as f32)));
            a += curl / n as f32;
        }
        pts
    }

    /// A fan of film: a wedge of triangles from `apex` spanning `a0..a1`.
    fn fan(&mut self, apex: [f32; 2], a0: f32, a1: f32, r: f32, wedges: usize, tissue: Tissue, side: f32) {
        for k in 0..wedges {
            let t0 = a0 + (a1 - a0) * k as f32 / wedges as f32;
            let t1 = a0 + (a1 - a0) * (k + 1) as f32 / wedges as f32;
            let rr = |t: f32| r * (0.85 + 0.15 * (t * 7.0).sin());
            self.mem(apex, add(apex, polar(t0, rr(t0))), add(apex, polar(t1, rr(t1))), tissue, side);
        }
    }
}

impl Plant {
    pub fn grow(kind: PlantKind, rng: &mut Rng, x: f32, depth: f32, look: Look) -> Self {
        let s = perspective(depth);
        let root = [x, terrain::floor_h(x, depth) + 0.002];
        let (mut segs, mut membranes) = (Vec::new(), Vec::new());
        let mut g = Grow { root, s, height: 0.05, flex: 0.004, segs: &mut segs, mems: &mut membranes };
        match kind {
            PlantKind::Fan => {
                let h = rng.range(0.03, 0.08);
                g.height = h;
                g.flex = h * 0.1;
                let stalks = rng.int(1, 4);
                for k in 0..stalks {
                    let lean = (k as f32 - (stalks - 1) as f32 * 0.5) * 0.25 + rng.normal() * 0.08;
                    let hk = h * rng.range(0.5, 1.0);
                    let pts = g.stem([0.0, 0.0], PI * 0.5 + lean, hk, -lean * 0.4, 4);
                    g.chain(&pts, h * 0.035, h * 0.018, Tissue::Frame, 0.0);
                    let top = *pts.last().unwrap();
                    let r = hk * rng.range(0.45, 0.8);
                    // The cup faces the sun on the day edge.
                    let tilt = 0.25 + lean * 0.5;
                    g.fan(top, PI * 0.1 + tilt, PI * 0.9 + tilt, r, 5, Tissue::LightFilm, 0.1 * k as f32);
                }
            }
            PlantKind::VentTubes => {
                let h = rng.range(0.025, 0.06);
                g.height = h;
                g.flex = 0.0;
                let tubes = rng.int(3, 7);
                for k in 0..tubes {
                    let xo = (k as f32 - tubes as f32 * 0.5) * h * 0.16 + rng.normal() * h * 0.03;
                    let hk = h * rng.range(0.35, 1.0);
                    let r = h * rng.range(0.07, 0.11);
                    let lean = rng.normal() * 0.12;
                    let top = [xo + lean * hk, hk];
                    g.seg([xo, 0.0], top, r * 1.2, r, Tissue::Armor, rng.range(-0.5, 0.5));
                    g.seg(top, add(top, [0.0, r * 0.15]), r * 0.75, r * 0.75, Tissue::Glow, 0.6);
                }
                // Crust heaped at the base.
                for _ in 0..rng.int(4, 8) {
                    let p = [rng.normal() * h * 0.35, rng.range(0.0, h * 0.1)];
                    let r = h * rng.range(0.05, 0.13);
                    g.seg(p, add(p, [r * 0.3, 0.0]), r, r * 0.8, Tissue::Armor, rng.range(0.3, 0.9));
                }
            }
            PlantKind::Thicket => {
                let h = rng.range(0.04, 0.09);
                g.height = h;
                g.flex = h * 0.06;
                let base = [0.0, h * 0.28];
                // Prop roots arching down to the mud.
                for k in 0..rng.int(4, 7) {
                    let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                    let foot = [side * h * rng.range(0.15, 0.45), 0.0];
                    let mid = add(lerp2(base, foot, 0.5), [side * h * 0.1, h * 0.08]);
                    g.chain(&[base, mid, foot], h * 0.025, h * 0.012, Tissue::Frame, rng.range(-0.6, 0.6));
                }
                for k in 0..rng.int(3, 6) {
                    let a = PI * 0.5 + (k as f32 - 2.0) * 0.22 + rng.normal() * 0.1;
                    let pts = g.stem(base, a, h * rng.range(0.5, 0.75), rng.normal() * 0.4, 3);
                    g.chain(&pts, h * 0.022, h * 0.01, Tissue::Frame, 0.0);
                    let tip = *pts.last().unwrap();
                    for j in 0..3 {
                        let aa = a + (j as f32 - 1.0) * 0.9;
                        let l = h * rng.range(0.15, 0.25);
                        let leaf_tip = add(tip, polar(aa, l));
                        let w = polar(aa + PI * 0.5, l * 0.25);
                        g.mem(tip, add(lerp2(tip, leaf_tip, 0.5), w), leaf_tip, Tissue::LightFilm, 0.2);
                        g.mem(tip, leaf_tip, add(lerp2(tip, leaf_tip, 0.5), mul(w, -1.0)), Tissue::LightFilm, 0.2);
                    }
                }
            }
            PlantKind::PodArch => {
                let h = rng.range(0.05, 0.1);
                g.height = h;
                g.flex = h * 0.15;
                for k in 0..rng.int(3, 6) {
                    let a = PI * 0.5 + rng.normal() * 0.2 + (k as f32 - 2.0) * 0.08;
                    let curl = -rng.range(1.2, 2.4) * if rng.chance(0.5) { 1.0 } else { -1.0 };
                    let pts = g.stem([0.0, 0.0], a, h * rng.range(0.7, 1.1), curl, 7);
                    g.chain(&pts, h * 0.018, h * 0.006, Tissue::Frame, 0.0);
                    for p in pts.iter().skip(3).step_by(1) {
                        if rng.chance(0.6) {
                            let hang = add(*p, [0.0, -h * 0.07]);
                            g.seg(*p, hang, h * 0.004, h * 0.004, Tissue::Frame, 0.1);
                            let pod = h * rng.range(0.025, 0.04);
                            g.seg(hang, add(hang, [0.0, -pod * 1.4]), pod * 0.6, pod, Tissue::Float, 0.2);
                        }
                    }
                }
            }
            PlantKind::Rosette => {
                let h = rng.range(0.012, 0.025);
                g.height = h;
                g.flex = h * 0.2;
                let leaves = rng.int(5, 9);
                for k in 0..leaves {
                    let a = PI * (0.08 + 0.84 * k as f32 / (leaves - 1) as f32);
                    let l = h * rng.range(1.0, 1.8);
                    let tip = add([0.0, h * 0.1], [a.cos() * l, a.sin() * l * 0.45]);
                    let w = polar(a + PI * 0.5, l * 0.3);
                    let mid = lerp2([0.0, 0.0], tip, 0.55);
                    let side = if k % 2 == 0 { 0.3 } else { -0.3 };
                    g.mem([0.0, 0.0], add(mid, w), tip, Tissue::LightFilm, side);
                    g.mem([0.0, 0.0], tip, add(mid, mul(w, -0.6)), Tissue::LightFilm, side);
                    g.seg([0.0, 0.0], tip, h * 0.04, h * 0.01, Tissue::Muscle, side + 0.05);
                }
            }
            PlantKind::Trap => {
                let h = rng.range(0.02, 0.04);
                g.height = h;
                g.flex = h * 0.03;
                let gape = rng.range(0.3, 0.8);
                for side in [-1.0f32, 1.0] {
                    let a = PI * 0.5 + side * gape;
                    let pts = g.stem([0.0, h * 0.1], a, h, -side * 1.6, 5);
                    for i in 0..pts.len() - 1 {
                        let t = i as f32 / (pts.len() - 1) as f32;
                        let r = h * 0.2 * (PI * (t * 0.85 + 0.1)).sin();
                        g.seg(
                            pts[i],
                            pts[i + 1],
                            r,
                            h * 0.2 * (PI * ((t + 0.2) * 0.85 + 0.1)).sin(),
                            Tissue::Muscle,
                            side * 0.5,
                        );
                    }
                    // Teeth along the rim.
                    for p in pts.iter().skip(2) {
                        g.seg(*p, add(*p, polar(a - side * 1.8, h * 0.12)), h * 0.015, 0.0, Tissue::Spine, side * 0.6);
                    }
                }
                g.seg([0.0, 0.0], [0.0, h * 0.12], h * 0.12, h * 0.1, Tissue::Gut, 0.0);
                g.seg([0.0, 0.0], [0.0, h * 0.12], h * 0.14, h * 0.12, Tissue::Muscle, -0.1);
            }
            PlantKind::GlowBloom => {
                let h = rng.range(0.025, 0.055);
                g.height = h;
                g.flex = h * 0.1;
                let pts = g.stem([0.0, 0.0], PI * 0.5 + rng.normal() * 0.1, h * 0.55, rng.normal() * 0.3, 3);
                g.chain(&pts, h * 0.03, h * 0.02, Tissue::Frame, 0.0);
                let top = *pts.last().unwrap();
                let petals = rng.int(4, 7);
                for k in 0..petals {
                    let a = PI * (0.12 + 0.76 * k as f32 / (petals - 1) as f32);
                    let l = h * rng.range(0.4, 0.6);
                    let tip = add(top, polar(a, l));
                    let w = polar(a + PI * 0.5, l * 0.35);
                    g.mem(top, add(lerp2(top, tip, 0.6), w), tip, Tissue::LightFilm, (k as f32 - 2.0) * 0.1);
                    g.mem(top, tip, add(lerp2(top, tip, 0.6), mul(w, -1.0)), Tissue::LightFilm, (k as f32 - 2.0) * 0.1);
                }
                g.seg(top, add(top, [0.0, h * 0.08]), h * 0.07, h * 0.04, Tissue::Glow, 0.4);
                // Small buds around it.
                for _ in 0..rng.int(1, 3) {
                    let bx = rng.normal() * h * 0.5;
                    let bh = h * rng.range(0.15, 0.3);
                    g.seg([bx, 0.0], [bx, bh], h * 0.012, h * 0.01, Tissue::Frame, 0.2);
                    g.seg([bx, bh], [bx, bh], h * 0.03, h * 0.03, Tissue::Glow, 0.3);
                }
            }
            PlantKind::Bulbs => {
                let h = rng.range(0.015, 0.035);
                g.height = h;
                g.flex = h * 0.03;
                for k in 0..rng.int(4, 9) {
                    let xo = rng.normal() * h * 0.45;
                    let a = PI * 0.5 + xo / h * 0.8;
                    let l = h * rng.range(0.6, 1.0);
                    let base = [xo * 0.4, 0.0];
                    let tip = add(base, polar(a, l));
                    let r = h * rng.range(0.18, 0.28);
                    g.seg(add(base, polar(a, r * 0.8)), tip, r, r * 0.25, Tissue::Float, (k % 3) as f32 * 0.2 - 0.2);
                    g.seg(
                        add(base, polar(a, r * 0.6)),
                        add(base, polar(a, l * 0.5)),
                        r * 0.35,
                        r * 0.15,
                        Tissue::Store,
                        0.0,
                    );
                }
                for k in 0..4 {
                    let a = -0.4 - 0.6 * k as f32;
                    g.chain(&[[0.0, 0.0], polar(a, h * 0.3)], h * 0.03, h * 0.01, Tissue::Frame, -0.3);
                }
            }
            PlantKind::Lace => {
                let h = rng.range(0.03, 0.07);
                g.height = h;
                g.flex = h * 0.05;
                let mut stack = vec![([0.0f32, 0.0f32], PI * 0.5, h * 0.35, h * 0.03, 0usize)];
                while let Some((p, a, l, r, depth)) = stack.pop() {
                    let tip = add(p, polar(a, l));
                    g.seg(p, tip, r, r * 0.7, Tissue::Frame, 0.0);
                    if depth < 4 {
                        let spread = rng.range(0.3, 0.7);
                        stack.push((tip, a + spread, l * 0.72, r * 0.7, depth + 1));
                        stack.push((tip, a - spread * 0.8, l * 0.75, r * 0.7, depth + 1));
                    } else {
                        let w = polar(a + PI * 0.5, l * 0.6);
                        g.mem(p, add(tip, w), add(tip, mul(w, -1.0)), Tissue::LightFilm, 0.2);
                    }
                }
            }
            PlantKind::Mat => {
                let h = rng.range(0.008, 0.015);
                g.height = h;
                g.flex = h * 0.3;
                // Pads seen from low on the water: flat ovals, a few glowing buds.
                let pads = rng.int(3, 7);
                for _ in 0..pads {
                    let xo = rng.normal() * h * 1.5;
                    let w = h * rng.range(0.5, 1.0);
                    let y = rng.range(-h * 0.1, h * 0.1);
                    g.seg([xo - w, y], [xo + w, y], h * 0.13, h * 0.15, Tissue::LightFilm, rng.range(-0.5, 0.5));
                    if rng.chance(0.4) {
                        let bud = [xo + rng.normal() * w * 0.3, y + h * 0.2];
                        g.seg(bud, bud, h * 0.12, h * 0.12, Tissue::Glow, 0.6);
                    }
                }
            }
        }
        Self { kind, segs, membranes, look, z: order(depth), alive: 1.0, seed: rng.next_u64() as u32 }
    }
}

/// Which plants suit a place on the floor, weighted.
fn habit(x: f32, wet: f32, heat: f32) -> &'static [(PlantKind, f32)] {
    if heat > 0.4 {
        return &[(PlantKind::VentTubes, 4.0), (PlantKind::Bulbs, 1.0)];
    }
    if wet > 0.5 {
        return &[(PlantKind::Thicket, 1.0), (PlantKind::Rosette, 2.0), (PlantKind::Fan, 1.0), (PlantKind::Trap, 0.5)];
    }
    let t = terrain::nightness(x, 0.0);
    if t < 0.25 {
        &[(PlantKind::Bulbs, 3.0), (PlantKind::Fan, 1.5), (PlantKind::Lace, 1.0), (PlantKind::Rosette, 0.5)]
    } else if t < 0.6 {
        &[
            (PlantKind::Fan, 3.0),
            (PlantKind::PodArch, 2.0),
            (PlantKind::Rosette, 2.0),
            (PlantKind::Trap, 1.0),
            (PlantKind::Lace, 1.0),
            (PlantKind::GlowBloom, 0.5),
        ]
    } else {
        &[(PlantKind::GlowBloom, 3.0), (PlantKind::Lace, 1.5), (PlantKind::Rosette, 1.0), (PlantKind::PodArch, 0.7)]
    }
}

fn pick(rng: &mut Rng, table: &[(PlantKind, f32)]) -> PlantKind {
    let total: f32 = table.iter().map(|t| t.1).sum();
    let mut r = rng.f() * total;
    for &(k, w) in table {
        if r < w {
            return k;
        }
        r -= w;
    }
    table[0].0
}

/// Plants over the floor in clumps. Each kind has a family look per zone, so
/// neighbours share colours the way a population would.
pub fn populate(rng: &mut Rng, river: terrain::RiverState) -> Vec<Plant> {
    let mut family: Vec<(PlantKind, usize, Look)> = Vec::new();
    let mut out = Vec::new();
    let clumps = 110;
    for _ in 0..clumps {
        let x = rng.range(0.3, 3.3);
        let depth = rng.range(0.03, 0.97);
        let heat = terrain::vent_heat(x);
        for _ in 0..rng.int(1, 5) {
            let px = x + rng.normal() * 0.02;
            let pd = (depth + rng.normal() * 0.05).clamp(0.0, 1.0);
            let in_river = terrain::in_river(px, pd, river);
            let wet = terrain::wetness(px, pd, river);
            let kind = if in_river {
                if rng.chance(0.15) { PlantKind::Mat } else { continue }
            } else {
                pick(rng, habit(px, wet, heat))
            };
            let zone = (px * 2.0) as usize;
            let look = match family.iter().find(|f| f.0 == kind && f.1 == zone) {
                Some(f) => rng.kin(&f.2, 0.04),
                None => {
                    let l = rng.look();
                    family.push((kind, zone, l));
                    l
                }
            };
            out.push(Plant::grow(kind, rng, px, pd, look));
        }
    }
    // Night-side glow fields, densest near the vents and the glacier foot.
    for _ in 0..12 {
        let x = rng.range(2.5, 3.35);
        let d = rng.range(0.05, 0.95);
        let look = rng.look();
        out.push(Plant::grow(PlantKind::GlowBloom, rng, x, d, look));
    }
    out
}
