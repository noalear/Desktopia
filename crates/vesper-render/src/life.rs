//! Living things. Creatures and plants arrive as world-space tissue segments
//! and membranes (the shape the simulation's pose step emits) and are drawn as
//! instanced primitives into the HDR scene, over the landscape.
//!
//! Colour never comes from a sprite or a fixed palette per species: each
//! organism carries sixteen look genes that the simulation inherits and
//! mutates without interpreting, and this module maps them onto the art rules
//! (saturated jewel tones, deep values, no pastels).

/// What a piece of body is made of. Mirrors the simulation's tissue table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tissue {
    Frame,
    Muscle,
    LightFilm,
    Gut,
    Float,
    Sense,
    Glow,
    Armor,
    Spine,
    Store,
    Brood,
    Mouth,
}

impl Tissue {
    /// Organs seen through the skin rather than forming it.
    fn is_inner(self) -> bool {
        matches!(self, Tissue::Gut | Tissue::Store | Tissue::Brood)
    }
}

/// One tapered tube of tissue, in world units (monitor heights; x runs from
/// the day edge, y up from the bottom of the screen).
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub ra: f32,
    pub rb: f32,
    pub tissue: Tissue,
    /// 0 whole .. 1 torn to shreds.
    pub damage: f32,
    /// Where in the body's thickness it sits: -1 far side, 0 midline, +1 near side.
    pub side: f32,
    /// How far the `b` end bends in the wind, in world units. Zero for creatures.
    pub sway: f32,
}

/// A thin sheet stretched between three points: fins, wings, webs, leaf films.
/// `a` is where it attaches; the edge from `b` to `c` is free.
#[derive(Clone, Copy, Debug)]
pub struct Membrane {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub c: [f32; 2],
    pub tissue: Tissue,
    pub side: f32,
}

/// Sixteen heritable appearance genes, each 0..1. Genes 0..3 are the ones
/// creatures perceive in each other, so they are the most visible:
/// 0 dominant hue, 1 value (dark to light), 2 pattern contrast, 3 glow hue.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub genes: [f32; 16],
}

/// One creature or plant as it should appear this frame.
#[derive(Clone, Copy, Debug)]
pub struct Organism<'a> {
    pub segs: &'a [Seg],
    pub membranes: &'a [Membrane],
    pub look: &'a Look,
    /// Draw order, 0 nearest the viewer .. 1 farthest. Also sets aerial haze.
    pub z: f32,
    /// 1 living, falling to 0 as a corpse decays.
    pub alive: f32,
    /// Brain-driven glow flash, 0..1, and which of 16 patterns it flashes in.
    pub signal: f32,
    pub symbol: u8,
    /// Glowing fluid leaking from wounds, 0..1.
    pub bleeding: f32,
    pub infection: Option<Infection>,
    /// Hide everything below this height (a burrower half in the ground).
    pub clip_y: Option<f32>,
    /// The point on the floor under it, where its shadow falls. `None`
    /// draws no shadow (a portrait with no ground, say).
    pub ground: Option<[f32; 2]>,
    /// Stable per-individual number, so patterns don't swim between frames.
    pub seed: u32,
}

/// Something living in or on an organism.
#[derive(Clone, Copy, Debug)]
pub struct Infection {
    /// 0..1 how far it has grown.
    pub load: f32,
    /// 0 a microbe (sores and a sickly tint) .. 1 a parasite you can see inside.
    pub visible: f32,
    /// 0..1 its own colour, around the same ring as look gene 0.
    pub hue: f32,
    /// A partner that feeds its host rather than a disease.
    pub symbiont: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Prim {
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    r: [f32; 2],
    /// Wind bend at a, b, c; clip height.
    sway: [f32; 4],
    /// z, near/far offset (world units), pattern coordinate at a and b.
    zinfo: [f32; 4],
    /// Albedo, translucency.
    color: [f32; 4],
    /// Pattern colour, pattern kind.
    accent: [f32; 4],
    /// Glow colour, freckle density.
    glow: [f32; 4],
    /// Tissue, seed, damage, alive.
    mat: [f32; 4],
    /// Pattern scale, sheen, fringe, pattern contrast.
    look: [f32; 4],
    /// Countershading, iridescence, signal, signal pattern.
    extra: [f32; 4],
    /// Infection load, how visible, hue (+2 for a symbiont), bleeding.
    state: [f32; 4],
}

pub(crate) const PRIM_ATTRIBS: [wgpu::VertexAttribute; 13] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2,
    4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
    8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4,
    12 => Float32x4,
];

/// Draw groups, drawn in this order with different depth rules.
#[derive(Clone, Copy)]
pub(crate) enum Group {
    /// Shadows on the ground under each body, drawn first so every body
    /// stands over them.
    Shadow = 0,
    /// Skin, shell, limbs: writes depth.
    Body = 1,
    /// Eggs laid on their own (organs inside a body don't show).
    Inner = 2,
    Membrane = 3,
    /// Additive halos around glow cells.
    Glow = 4,
}

pub(crate) const GROUPS: usize = 5;

/// Shader code for a shadow prim, past the tissue codes.
const SHADOW: f32 = 12.0;

/// What the outside of a body is like, the surface texture life.wgsl draws
/// (its SKIN_ codes). Plants' stems are always bark.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Skin {
    Scales = 0,
    Pebbled = 1,
    Wrinkled = 2,
    Chitin = 3,
    Slick = 4,
    Fur = 5,
    Bark = 6,
}

/// A frame's worth of organisms, ready to upload.
#[derive(Default)]
pub struct LifeBatch {
    groups: [Vec<(f32, Prim)>; GROUPS],
}

/// Resolved colours for one organism.
struct Palette {
    base: [f32; 3],
    accent: [f32; 3],
    armor: [f32; 3],
    bone: [f32; 3],
    film: [f32; 3],
    glow: [f32; 3],
    translucency: f32,
    pattern_kind: f32,
    pattern_scale: f32,
    contrast: f32,
    sheen: f32,
    fringe: f32,
    countershade: f32,
    iridescence: f32,
    freckles: f32,
    skin: Skin,
}

impl Look {
    fn palette(&self) -> Palette {
        let g = &self.genes;
        let hue = g[0];
        let sat = 0.62 + 0.33 * g[6];
        // Deep to bright, never chalky: saturation stays high at every value,
        // and most bodies sit in the darker half so glow and rim light carry.
        let value = 0.2 + 0.62 * g[1] * g[1];
        let base = hsv(hue, sat, value);
        let accent_hue = hue + 0.12 + 0.5 * g[9];
        let accent_value = if g[1] > 0.5 { value * 0.35 } else { (value * 1.9).min(0.95) };
        let accent = hsv(accent_hue, sat.min(0.95), accent_value);
        // Shell runs from matte mineral (ochre, slate, bone) to lacquer in the body hue.
        let mineral = mix3(hsv(0.08, 0.45, 0.32), hsv(0.62, 0.35, 0.12), g[11]);
        let armor = mix3(mineral, hsv(hue + 0.03, sat, value * 0.8), g[14]);
        let film = mix3(hsv(0.13, 0.85, 0.55), hsv(hue, sat, 0.45), 0.35 + 0.4 * g[15]);
        Palette {
            base,
            accent,
            armor,
            bone: [0.62, 0.55, 0.42],
            film,
            glow: glow_color(g[3]),
            translucency: (g[10] - 0.15).max(0.0) * 1.05,
            pattern_kind: (g[4] * 5.999).floor() + 1.0,
            // Relative to the body's thickness, so small creatures show it too.
            pattern_scale: 0.9 + 3.0 * g[5],
            contrast: 0.25 + 0.75 * g[2],
            sheen: g[7],
            fringe: ((g[8] - 0.62) / 0.38).max(0.0) * 1.4,
            countershade: g[11],
            iridescence: (g[12] - 0.55).max(0.0) * 2.2,
            freckles: (g[13] - 0.5).max(0.0) * 2.0,
            skin: skin(g),
        }
    }
}

/// Shaggy bodies are furred; the rest take their skin from a blend of shell
/// and colour genes, so it is inherited and drifts as they mutate. Glossy
/// lineages lean to scales, shell and slick skin, matte ones to beaded and
/// wrinkled hide.
fn skin(g: &[f32; 16]) -> Skin {
    if g[8] > 0.62 {
        return Skin::Fur;
    }
    let x = (g[11] * 3.1 + g[14] * 1.7 + g[4] * 0.9).fract();
    let glossy = [Skin::Scales, Skin::Chitin, Skin::Slick, Skin::Scales];
    let matte = [Skin::Pebbled, Skin::Wrinkled, Skin::Pebbled, Skin::Scales];
    let set = if g[7] > 0.5 { glossy } else { matte };
    set[((x * 4.0) as usize).min(3)]
}

fn glow_color(g: f32) -> [f32; 3] {
    // The night-side glow set from the world bible: cyan, acid green,
    // magenta, amber, and a deep violet-blue, blended around a ring.
    const SET: [[f32; 3]; 5] = [[0.1, 1.6, 2.0], [0.9, 2.0, 0.15], [2.2, 0.25, 1.6], [2.4, 1.0, 0.15], [0.7, 0.4, 2.4]];
    let x = g.fract() * 5.0;
    let i = x.floor() as usize;
    mix3(SET[i % 5], SET[(i + 1) % 5], x - x.floor())
}

impl LifeBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        for g in &mut self.groups {
            g.clear();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(Vec::is_empty)
    }

    pub fn add(&mut self, o: &Organism) {
        self.add_plant(o, 0.0, 0.0);
    }

    /// A plant, with how it is faring: `alarm` 0..1, grazed and armed and
    /// warning its neighbours, flushes it a warning crimson at the edges;
    /// `dormant` 0..1, shut down through hard times, greys and dulls it.
    pub fn add_plant(&mut self, o: &Organism, alarm: f32, dormant: f32) {
        if o.segs.is_empty() && o.membranes.is_empty() {
            return;
        }
        // No two individuals are quite alike: a little of their colour is
        // their own, beyond what their genes say.
        let mut look = *o.look;
        let jitter =
            |k: u32| (o.seed.wrapping_mul(2654435761).wrapping_add(k.wrapping_mul(40503)) >> 8) as f32 / 16777216.0;
        look.genes[0] = (look.genes[0] + (jitter(1) - 0.5) * 0.05).rem_euclid(1.0);
        look.genes[1] = (look.genes[1] + (jitter(2) - 0.5) * 0.14).clamp(0.0, 1.0);
        look.genes[6] = (look.genes[6] + (jitter(3) - 0.5) * 0.2).clamp(0.0, 1.0);
        let mut pal = look.palette();
        let dormant = dormant.clamp(0.0, 1.0);
        if dormant > 0.0 {
            let dull = |c: [f32; 3]| {
                let grey = 0.3 * c[0] + 0.55 * c[1] + 0.15 * c[2];
                scale3(mix3(c, [grey * 1.1, grey, grey * 0.85], 0.7 * dormant), 1.0 - 0.35 * dormant)
            };
            pal.base = dull(pal.base);
            pal.film = dull(pal.film);
            pal.accent = dull(pal.accent);
            pal.translucency *= 1.0 - 0.6 * dormant;
            pal.freckles *= 1.0 - dormant;
        }
        let alarm = alarm.clamp(0.0, 1.0);
        if alarm > 0.0 {
            pal.film = mix3(pal.film, [0.45, 0.03, 0.1], 0.55 * alarm);
            pal.accent = mix3(pal.accent, [0.95, 0.12, 0.28], 0.8 * alarm);
            pal.contrast = pal.contrast.max(alarm);
            pal.glow = mix3(pal.glow, [2.4, 0.3, 0.5], alarm);
            pal.freckles = pal.freckles.max(0.6 * alarm);
        }
        let seed = (o.seed % 9973) as f32 * 0.137;
        // Only plants bend in the wind.
        let plant = o.segs.iter().any(|s| s.sway != 0.0);
        let rmax = o.segs.iter().fold(0.0f32, |m, s| m.max(s.ra).max(s.rb)).max(1e-4);
        let clip = o.clip_y.unwrap_or(-10.0);
        let alive = o.alive.clamp(0.0, 1.0);
        let z = o.z.clamp(0.0, 1.0) * 0.98 + 0.01;
        let state = match o.infection {
            Some(inf) => [
                inf.load.clamp(0.0, 1.0),
                inf.visible.clamp(0.0, 1.0),
                inf.hue.rem_euclid(1.0) + if inf.symbiont { 2.0 } else { 0.0 },
                o.bleeding.clamp(0.0, 1.0),
            ],
            None => [0.0, 0.0, 0.0, o.bleeding.clamp(0.0, 1.0)],
        };

        // Pattern coordinates run continuously down connected chains, and the
        // wind bend at a segment's root is whatever its parent's tip does.
        let n = o.segs.len();
        let mut u = vec![[0.0f32; 2]; n];
        let mut sway_a = vec![0.0f32; n];
        for i in 0..n {
            let s = &o.segs[i];
            let parent = (0..i).rev().find(|&j| dist(o.segs[j].b, s.a) < 0.35 * s.ra.max(1e-5));
            let start = parent.map_or(seed * 0.05, |j| u[j][1]);
            sway_a[i] = parent.map_or(0.0, |j| o.segs[j].sway);
            u[i] = [start, start + dist(s.a, s.b)];
        }

        let common = |tissue: Tissue, damage: f32, side: f32| -> Prim {
            let (albedo, translucency, sheen) = match tissue {
                Tissue::Armor => (pal.armor, 0.0, 0.25 + 0.75 * pal.sheen),
                Tissue::Spine => (mix3(pal.bone, pal.armor, 0.4), 0.0, 0.5),
                Tissue::Mouth => (scale3(pal.armor, 0.45), 0.0, 0.7),
                Tissue::Frame => {
                    (mix3(scale3(pal.base, 0.8), pal.armor, 0.35), pal.translucency * 0.5, 0.2 + 0.5 * pal.sheen)
                }
                Tissue::LightFilm => (pal.film, 0.55 + 0.35 * pal.translucency, 0.3),
                Tissue::Float => (mix3(pal.base, [0.5, 0.5, 0.55], 0.12), 0.6, 0.5),
                Tissue::Sense => ([0.01, 0.008, 0.012], 0.0, 1.0),
                Tissue::Glow => (pal.glow, 0.3, 0.4),
                Tissue::Gut => ([0.38, 0.03, 0.04], 0.0, 0.0),
                Tissue::Store => ([0.6, 0.38, 0.05], 0.0, 0.0),
                Tissue::Brood => (mix3(pal.accent, [0.7, 0.5, 0.15], 0.6), 0.0, 0.0),
                Tissue::Muscle => (pal.base, pal.translucency, pal.sheen * 0.5),
            };
            Prim {
                sway: [0.0, 0.0, 0.0, clip],
                zinfo: [z, side * rmax * 1.1, 0.0, 0.0],
                color: [albedo[0], albedo[1], albedo[2], translucency.min(0.92)],
                accent: [pal.accent[0], pal.accent[1], pal.accent[2], pal.pattern_kind],
                glow: [pal.glow[0], pal.glow[1], pal.glow[2], pal.freckles],
                mat: [tissue as u8 as f32, seed, damage.clamp(0.0, 1.0), alive],
                look: [pal.pattern_scale * rmax, sheen, pal.fringe, pal.contrast],
                extra: [pal.countershade, pal.iridescence, o.signal, (o.symbol % 16) as f32],
                state,
                ..Default::default()
            }
        };

        if let Some(g) = o.ground {
            // Its footprint and height on screen, and how high it is off the
            // floor, set the shadow's size and softness (see vs_shadow).
            let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for s in o.segs {
                for q in [s.a, s.b] {
                    x0 = x0.min(q[0]);
                    x1 = x1.max(q[0]);
                    y0 = y0.min(q[1]);
                    y1 = y1.max(q[1]);
                }
            }
            if x0 <= x1 && alive > 0.0 {
                let mut p = common(Tissue::Frame, 0.0, 0.0);
                p.mat[0] = SHADOW;
                p.a = [(x0 + x1) * 0.5, g[1]];
                p.r = [(x1 - x0) * 0.5 + rmax, (y1 - y0).max(rmax)];
                p.c = [(y0 - g[1]).max(0.0), 0.0];
                self.groups[Group::Shadow as usize].push((z, p));
            }
        }

        // Bodies are opaque, so organs inside them never show; an egg laid
        // on its own is nothing but organ.
        let only_inner = o.segs.iter().all(|s| s.tissue.is_inner());
        let skin = if plant { Skin::Bark } else { pal.skin };
        for (i, s) in o.segs.iter().enumerate() {
            let mut p = common(s.tissue, s.damage, s.side);
            p.a = s.a;
            p.b = s.b;
            p.c = [skin as u8 as f32, 0.0];
            p.r = [s.ra.max(0.0), s.rb.max(0.0)];
            p.sway[0] = sway_a[i];
            p.sway[1] = s.sway;
            // The organism's thickest part, so the shader can tell body from limb.
            p.sway[2] = rmax;
            p.zinfo[2] = u[i][0];
            p.zinfo[3] = u[i][1];
            // Only skin and shell grow hair.
            if !matches!(s.tissue, Tissue::Muscle | Tissue::Frame) {
                p.look[2] = 0.0;
            }
            if s.tissue.is_inner() {
                if only_inner && alive > 0.3 {
                    p.color[3] = 1.0;
                    self.groups[Group::Inner as usize].push((z, p));
                }
                continue;
            }
            self.groups[Group::Body as usize].push((z, p));
            if s.tissue == Tissue::Glow && alive > 0.05 {
                let mut halo = p;
                halo.zinfo[1] = rmax * 1.2;
                self.groups[Group::Glow as usize].push((z, halo));
            }
        }

        for m in o.membranes {
            let mut p = common(m.tissue, 0.0, m.side);
            p.a = m.a;
            p.b = m.b;
            p.c = m.c;
            // Leaf films and fins bend with whatever segment ends they hang from.
            let sway_at = |q: [f32; 2]| {
                o.segs
                    .iter()
                    .map(|s| (dist(s.b, q), s.sway))
                    .chain(o.segs.iter().map(|s| (dist(s.a, q), 0.0)))
                    .fold((f32::MAX, 0.0), |best, c| if c.0 < best.0 { c } else { best })
                    .1
            };
            p.sway[0] = sway_at(m.a);
            p.sway[1] = sway_at(m.b);
            p.sway[2] = sway_at(m.c);
            if m.tissue != Tissue::LightFilm {
                p.color = [pal.base[0], pal.base[1], pal.base[2], pal.translucency];
            }
            self.groups[Group::Membrane as usize].push((z, p));
        }
    }

    /// Prims grouped and sorted far to near, plus each group's range.
    pub(crate) fn finish(&mut self) -> (Vec<Prim>, [std::ops::Range<u32>; GROUPS]) {
        let mut out = Vec::new();
        let mut ranges: [std::ops::Range<u32>; GROUPS] = Default::default();
        for (gi, g) in self.groups.iter_mut().enumerate() {
            g.sort_by(|x, y| y.0.total_cmp(&x.0));
            let start = out.len() as u32;
            out.extend(g.iter().map(|(_, p)| *p));
            ranges[gi] = start..out.len() as u32;
        }
        (out, ranges)
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale3(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// HSV to linear RGB. Hue wraps.
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    // Square for a rough sRGB-to-linear, so mid values stay rich, not chalky.
    [(r + m).powi(2), (g + m).powi(2), (b + m).powi(2)]
}
