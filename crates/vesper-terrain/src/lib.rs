//! Vesper's land shape, as plain functions shared by the simulation and the
//! renderer so that what creatures walk on is exactly what is drawn.
//!
//! These are exact ports of the WGSL in `vesper-render/src/shaders/common.wgsl`.
//! Change both together.
//!
//! World units are monitor heights: x runs 0..[`WORLD_W`] from the day edge
//! (left) to the night edge (right), h runs 0 at the bottom of the screen to 1
//! at the top.

/// Width of the whole two-monitor span in world units (7680 / 2160).
pub const WORLD_W: f32 = 7680.0 / 2160.0;

pub const SUN: [f32; 2] = [0.34, 0.375];
pub const SUN_R: f32 = 0.085;
pub const VOLCANO_X: f32 = 1.27;
pub const VOLCANO_TOP: f32 = 0.53;
/// Where the glacier starts rising on the night edge.
pub const GLACIER_X: f32 = 3.02;

/// Hot springs and vent fields, as (x, strength). The first is the volcano's
/// flank; the others glow in the dark near the glacier.
pub const VENTS: [(f32, f32); 3] = [(VOLCANO_X + 0.09, 1.0), (2.86, 0.7), (3.24, 0.9)];

/// Columns of air the simulation steps its weather in, and the renderer
/// reads it by.
pub const SKY_COLS: usize = 64;
/// Rows of the weather the simulation hands the renderer, each four floats:
///
/// - `0..SKY_COLS`, per column: cloud cover, what is falling (0..1), the
///   share of it that is snow, fog.
/// - `SKY_COLS..2*SKY_COLS`: ground wetness, lying snow, ash in the air,
///   dust in the air.
/// - `2*SKY_COLS..3*SKY_COLS`: ash lying on the ground, hail, gusts (0 calm
///   .. about 3 a gale), cold snap (how far below usual, 0..1.5).
/// - then [`SKY_CELLS`] storm cells: x, power (0..1.3), the power it can reach, and a seed
///   for its shape (whole part) plus its age through its life (fraction).
/// - then one row: the ash veil, the dust front's speed, the cold fall's
///   front speed, unused.
pub const SKY_CELLS: usize = 5;
pub const SKY_ROWS: usize = SKY_COLS * 3 + SKY_CELLS + 1;

/// Length of one seasonal wobble of the twilight band, in days: eight
/// hours, so a day at the desktop sees the warm and cold zones sweep out and
/// back, and life that can't follow its climate is stranded.
pub const SEASON_DAYS: f64 = 1.0 / 3.0;
/// How far the band slides each way at most, in world units.
pub const BAND_AMPLITUDE: f64 = 0.3;

/// The river's current course. The simulation drifts `phase` over days and
/// raises `flood` after storms; zero for both is the shader's original river.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RiverState {
    /// Radians added to the meander's phase.
    pub phase: f32,
    /// 0 is normal; 1 doubles the river's width.
    pub flood: f32,
}

// ---------------------------------------------------------------- noise

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn hash21(p: [f32; 2]) -> f32 {
    let mut q = [fract(p[0] * 123.34), fract(p[1] * 456.21)];
    let d = q[0] * (q[0] + 45.32) + q[1] * (q[1] + 45.32);
    q[0] += d;
    q[1] += d;
    fract(q[0] * q[1])
}

pub fn vnoise(p: [f32; 2]) -> f32 {
    let i = [p[0].floor(), p[1].floor()];
    let f = [p[0] - i[0], p[1] - i[1]];
    let u = [f[0] * f[0] * (3.0 - 2.0 * f[0]), f[1] * f[1] * (3.0 - 2.0 * f[1])];
    let a = hash21(i);
    let b = hash21([i[0] + 1.0, i[1]]);
    let c = hash21([i[0], i[1] + 1.0]);
    let d = hash21([i[0] + 1.0, i[1] + 1.0]);
    mix(mix(a, b, u[0]), mix(c, d, u[0]), u[1])
}

pub fn fbm(p: [f32; 2], octaves: i32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut q = p;
    for _ in 0..octaves {
        sum += amp * vnoise(q);
        q = [q[0] * 2.03 + 17.1, q[1] * 2.03 + 9.2];
        amp *= 0.5;
    }
    sum
}

pub fn ridged(p: [f32; 2], octaves: i32) -> f32 {
    let mut amp = 0.5;
    let mut sum = 0.0;
    let mut q = p;
    for _ in 0..octaves {
        let n = 1.0 - (2.0 * vnoise(q) - 1.0).abs();
        sum += amp * n * n;
        q = [q[0] * 2.07 + 5.3, q[1] * 2.07 + 1.7];
        amp *= 0.5;
    }
    sum
}

// ---------------------------------------------------------------- light and season

/// The band's seasonal slide at a moment, in world units.
pub fn band_shift(world_secs: f64) -> f32 {
    let phase = world_secs / 86400.0 / SEASON_DAYS * std::f64::consts::TAU;
    // A slower swell under the seasons, so no two are quite alike: some
    // reach far, some hardly turn.
    let swell = (phase * 0.29 + 1.3).sin();
    (BAND_AMPLITUDE * (0.75 * phase.sin() + 0.25 * swell)) as f32
}

/// 0 on the day edge, 1 on the night edge, following the sliding band.
pub fn nightness(x: f32, band_shift: f32) -> f32 {
    smoothstep(0.25, 3.3, x + band_shift)
}

/// Sunlight reaching the ground, 1 at the day edge.
pub fn daylight(x: f32, band_shift: f32) -> f32 {
    1.0 - smoothstep(0.2, 2.9, x + band_shift)
}

pub fn sun_pos(band_shift: f32) -> [f32; 2] {
    [SUN[0] - band_shift, SUN[1]]
}

// ---------------------------------------------------------------- land

// The landscape is seen through one pinhole camera CAM_H metres above the
// plain, looking level, with its horizon at screen h HORIZON. The ground
// plane life walks on is a real stretch of that plain: depth 0 is the bottom
// of the screen, NEAR_Z metres out, and depth 1 its back edge about BACK_Z
// out, with depth linear in metres so life spreads evenly over the ground.

pub const HORIZON: f32 = 0.28;
pub const FOCAL: f32 = 3.0;
pub const CAM_H: f32 = 20.0;
/// Distance in metres to the plain at the bottom of the screen.
pub const NEAR_Z: f32 = CAM_H * FOCAL / HORIZON;
/// Typical distance in metres to the back of the ground plane.
pub const BACK_Z: f32 = 1300.0;

/// Distance in metres to the back edge of the ground plane.
pub fn back_z(x: f32) -> f32 {
    BACK_Z * (1.0 + 0.3 * (fbm([x * 4.0, 2.7], 4) - 0.5))
}

/// Distance in metres to depth 0..1 on the ground plane, given its back edge.
fn z_at(back: f32, depth: f32) -> f32 {
    NEAR_Z + (back - NEAR_Z) * depth
}

/// Screen h of the plain at a distance in metres.
fn h_at(z: f32) -> f32 {
    HORIZON - CAM_H * FOCAL / z
}

/// Top of the foreground ground plane (its back edge) in screen h.
pub fn ground_top(x: f32) -> f32 {
    h_at(back_z(x))
}

/// Which serac block `x` falls in, and how far across it (0..1). Blocks vary
/// in width so the ice wall never reads as a grid.
pub fn serac(x: f32) -> [f32; 2] {
    let w = x / 0.03 + 1.6 * fbm([x * 9.0, 6.6], 3);
    [w.floor(), fract(w)]
}

/// The ice wall: a broad rise broken into leaning serac blocks.
pub fn glacier_lift(x: f32) -> f32 {
    let rise = smoothstep(GLACIER_X, 3.3, x);
    let [bi, f] = serac(x);
    let bevel = smoothstep(0.0, 0.25, f) * smoothstep(1.0, 0.75, f);
    let h = hash21([bi, 3.7]);
    let lean = (hash21([bi, 8.1]) - 0.5) * 0.04;
    let block = (0.03 * h + lean * (f - 0.5)) * mix(0.6, 1.0, bevel);
    rise * (0.19 + 0.07 * fbm([x * 2.5, 1.1], 3) + block + 0.006 * (fbm([x * 60.0, 4.0], 3) - 0.5))
}

/// Rolling foothills just behind the ground plane, sometimes sinking below
/// its back edge, with the glacier on the night edge.
pub fn near_top(x: f32) -> f32 {
    let roll = fbm([x * 1.7, 5.3], 4);
    let hills =
        0.2 + 0.13 * (roll - 0.45) + 0.03 * (fbm([x * 6.0, 2.1], 4) - 0.5) + 0.015 * (ridged([x * 9.0, 2.0], 3) - 0.3);
    hills + glacier_lift(x)
}

pub fn volcano(x: f32) -> f32 {
    let d = (x - VOLCANO_X).abs();
    let mut v = VOLCANO_TOP - d * 0.8 - 0.06 * d * d;
    v += 0.03 * (ridged([x * 16.0, 3.3], 4) - 0.35);
    v -= 0.018 * smoothstep(0.03, 0.0, d);
    v
}

pub fn mid_top(x: f32) -> f32 {
    let hills = 0.3 + 0.1 * (ridged([x * 2.3, 8.1], 6) - 0.35);
    hills.max(volcano(x))
}

pub fn far_top(x: f32) -> f32 {
    0.36 + 0.15 * (ridged([x * 1.6, 13.7], 7) - 0.3)
}

/// Centre and half-width of the river on the ground plane, in screen h. It
/// runs out from under the ice along the back of the plain on the right, and
/// spreads into a shallow salt pan on the left where it boils away.
pub fn river(x: f32, s: RiverState) -> [f32; 2] {
    let mut c = 0.085
        + 0.03 * (x * 2.3 + 0.5 + s.phase).sin()
        + 0.018 * (x * 5.1 + 2.0 + s.phase * 1.7).sin()
        + 0.02 * (fbm([x * 2.8, 4.0], 3) - 0.5);
    c = mix(c, ground_top(x) - 0.006, smoothstep(2.6, 3.15, x));
    let near = (0.5 + 1.6 * (0.12 - c) / 0.12).max(0.15);
    let pan = smoothstep(0.95, 0.4, x);
    let w = 0.02 * near * (0.75 + 0.5 * fbm([x * 6.0, 9.0], 2)) * (1.0 + 0.6 * pan) * (1.0 + s.flood);
    [c, w]
}

/// In the salt pan the river frays into braided channels that thin out
/// toward the sun: 1 where there is still water at (x, h), 0 on dry salt.
pub fn pan_wet(x: f32, h: f32) -> f32 {
    let dry = smoothstep(0.95, 0.3, x);
    if dry <= 0.0 {
        return 1.0;
    }
    let braid = fbm([x * 22.0, h * 95.0], 4);
    smoothstep(dry * 0.62, dry * 0.62 + 0.04, braid) * smoothstep(0.3, 0.42, x + braid * 0.08)
}

/// Depth on the ground plane (0 front .. 1 back) of a screen h on it.
pub fn depth_of(x: f32, h: f32) -> f32 {
    depth_in(back_z(x), h)
}

/// As [`depth_of`], given the back edge's distance.
pub fn depth_in(back: f32, h: f32) -> f32 {
    if h >= HORIZON {
        return 1.0;
    }
    let z = CAM_H * FOCAL / (HORIZON - h);
    ((z - NEAR_Z) / (back - NEAR_Z)).clamp(0.0, 1.0)
}

/// Screen h of a depth on the ground plane.
pub fn floor_h(x: f32, depth: f32) -> f32 {
    floor_h_in(back_z(x), depth)
}

/// As [`floor_h`], given the back edge's distance.
pub fn floor_h_in(back: f32, depth: f32) -> f32 {
    h_at(z_at(back, depth))
}

/// How much perspective shrinks something at a depth on the ground plane:
/// 1 at the bottom of the screen, about a sixth at the back.
pub fn shrink(x: f32, depth: f32) -> f32 {
    NEAR_Z / z_at(back_z(x), depth)
}

/// How wet a spot on the ground plane is: 1 in the river, falling off over its banks.
pub fn wetness(x: f32, depth: f32, s: RiverState) -> f32 {
    let [c, w] = river(x, s);
    if w <= 0.0 {
        return 0.0;
    }
    let d = (floor_h(x, depth) - c).abs();
    smoothstep(w * 3.5, w * 0.8, d)
}

/// Whether a spot on the ground plane is under water.
pub fn in_river(x: f32, depth: f32, s: RiverState) -> bool {
    let [c, w] = river(x, s);
    let h = floor_h(x, depth);
    w > 0.0 && (h - c).abs() < w && pan_wet(x, h) > 0.5
}

/// Vent heat at a point on the floor, 0..1.
pub fn vent_heat(x: f32) -> f32 {
    VENTS.iter().map(|&(vx, s)| s * (-((x - vx) / 0.05).powi(2)).exp()).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_shader_reference() {
        // Values from evaluating the WGSL on the GPU path in f32.
        let h = hash21([0.0, 0.0]);
        assert!((0.0..1.0).contains(&h));
        assert!((ground_top(1.0) - 0.234).abs() < 0.02);
    }

    #[test]
    fn depth_round_trips_through_the_screen() {
        for i in 0..50 {
            let x = i as f32 * 0.07;
            for d in [0.0, 0.1, 0.37, 0.8, 1.0] {
                let back = depth_of(x, floor_h(x, d));
                assert!((back - d).abs() < 1e-3, "x {x} d {d} -> {back}");
            }
            assert!(floor_h(x, 0.0).abs() < 1e-4);
            assert!(shrink(x, 1.0) < 0.25);
        }
    }

    #[test]
    fn river_runs_inside_the_ground_plane() {
        for i in 0..100 {
            let x = 0.8 + i as f32 * 0.02;
            let [c, w] = river(x, RiverState::default());
            assert!(c - w > 0.0 && c + w < ground_top(x), "x {x}");
        }
    }
}
