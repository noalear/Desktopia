// Shared by the bake and frame passes. World units are monitor heights:
// x runs 0..aspect from the day edge (left) to the night edge (right),
// h runs 0 at the bottom of the screen to 1 at the top.

struct Params {
    world_size: vec2f,  // whole span in pixels
    view_origin: vec2f, // this output's top-left in world pixels
    view_size: vec2f,   // this output's size in world pixels
    time: f32,          // seconds, wrapped
    band_shift: f32,    // seasonal slide of the twilight band, world units
    exposure: f32,
    mode: u32,          // 0 = SDR (shader encodes sRGB), 1 = HDR scRGB
    paper_white: f32,   // HDR only: paper white in scRGB units (nits / 80)
    peak: f32,          // HDR only: peak brightness in scRGB units
    lightning: f32,     // 0..1 current flash strength
    storm_x: f32,       // world x of the storm cell
    storm: f32,         // 0 clear .. 1 full storm
    volcano: f32,       // 0 quiet, ~0.3 rumbling, 1 erupting
    river_phase: f32,   // the river's slow drift, radians
    river_flood: f32,   // 0 normal .. 1 twice as wide
    lightning_x: f32,   // where the bolt falls
    lava: f32,          // how far lava has run each way from the crater
    ash: f32,           // 0..1 ash in the sky
    cold: f32,          // cold fall off the glacier: strength
    cold_x: f32,        //   and its front
    dust: f32,          // dust storm off the day side: strength
    dust_x: f32,        //   and its centre
    wind_low: f32,      // ground wind, world units/s, negative is day-ward
    wind_high: f32,     // high wind, positive is night-ward
    cloud_drift: f32,   // how far the high wind has carried the clouds
};

@group(0) @binding(0) var<uniform> P: Params;

struct VOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VOut {
    var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    let c = corners[i];
    var o: VOut;
    o.pos = vec4f(c, 0.0, 1.0);
    o.uv = c * vec2f(0.5, -0.5) + 0.5;
    return o;
}

// World position (x, h) for a uv on this output.
fn world_pos(uv: vec2f) -> vec2f {
    let px = P.view_origin + uv * P.view_size;
    let w = px / P.world_size.y;
    return vec2f(w.x, 1.0 - w.y);
}

// ---------------------------------------------------------------- noise

fn hash21(p: vec2f) -> f32 {
    var q = fract(p * vec2f(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

// Integer hash (pcg2d) for patterns that must never visibly repeat.
fn pcg2(p: vec2f) -> vec2f {
    var v = vec2u(vec2i(p) + vec2i(100000));
    v = v * 1664525u + 1013904223u;
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2u(16u));
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2u(16u));
    return vec2f(v) / 4294967295.0;
}

fn hash22(p: vec2f) -> vec2f {
    let n = hash21(p);
    return vec2f(n, hash21(p + n + 17.17));
}

fn vnoise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2f(1.0, 0.0));
    let c = hash21(i + vec2f(0.0, 1.0));
    let d = hash21(i + vec2f(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2f, octaves: i32) -> f32 {
    var amp = 0.5;
    var sum = 0.0;
    var q = p;
    for (var i = 0; i < octaves; i++) {
        sum += amp * vnoise(q);
        q = q * 2.03 + vec2f(17.1, 9.2);
        amp *= 0.5;
    }
    return sum;
}

// Sharp-crested noise for mountain ridges.
fn ridged(p: vec2f, octaves: i32) -> f32 {
    var amp = 0.5;
    var sum = 0.0;
    var q = p;
    for (var i = 0; i < octaves; i++) {
        let n = 1.0 - abs(2.0 * vnoise(q) - 1.0);
        sum += amp * n * n;
        q = q * 2.07 + vec2f(5.3, 1.7);
        amp *= 0.5;
    }
    return sum;
}

// ---------------------------------------------------------------- world layout

// 0 on the day edge, 1 on the night edge, following the sliding band.
fn nightness(x: f32) -> f32 {
    return smoothstep(0.25, 3.3, x + P.band_shift);
}

fn daylight(x: f32) -> f32 {
    return 1.0 - smoothstep(0.2, 2.9, x + P.band_shift);
}

const SUN = vec2f(0.34, 0.375);
const SUN_R = 0.085;
const VOLCANO_X = 1.27;
const VOLCANO_TOP = 0.53;

// Typical distance in metres to the back of the ground plane life walks on.
const BACK_Z = 1300.0;

// Distance in metres to the back edge of the ground plane.
fn back_z(x: f32) -> f32 {
    return BACK_Z * (1.0 + 0.3 * (fbm(vec2f(x * 4.0, 2.7), 4) - 0.5));
}

// Top of the foreground ground plane (its back edge) in screen h.
fn ground_top(x: f32) -> f32 {
    return HORIZON - CAM_H * FOCAL / back_z(x);
}

// ---------------------------------------------------------------- camera

// Everything behind the plain is a real landscape seen through one pinhole
// camera, CAM_H metres above the plain, looking level across the band. Its
// optical centre is the middle of the span, so the two monitors share one
// perspective. World positions behind the plain are metres: x across the
// band (0 under the centre), y up from the plain, z away from the eye.
const CAM_X = 1.7777778;
const HORIZON = 0.28;
const FOCAL = 3.0;
const CAM_H = 20.0;

fn view_ray(p: vec2f) -> vec3f {
    return normalize(vec3f(p.x - CAM_X, p.y - HORIZON, FOCAL));
}

// Distance (z) to the point of the plain under screen point p.
fn plain_z(h: f32) -> f32 {
    return CAM_H * FOCAL / max(HORIZON - h, 1e-4);
}

// The plain's surface point (x, z) in metres under screen point p.
fn plain_point(p: vec2f) -> vec2f {
    let z = plain_z(p.y);
    return vec2f((p.x - CAM_X) * z / FOCAL, z);
}

// Screen x of a world point.
fn screen_x(xz: vec2f) -> f32 {
    return CAM_X + xz.x * FOCAL / max(xz.y, 1.0);
}

// Screen uv on this output of a world screen position.
fn uv_of(p: vec2f) -> vec2f {
    let px = vec2f(p.x, 1.0 - p.y) * P.world_size.y;
    return (px - P.view_origin) / P.view_size;
}

// Skyline of the far ranges, in screen h. The far mountains are shaped so
// their crests follow it.
fn far_top(x: f32) -> f32 {
    return 0.36 + 0.15 * (ridged(vec2f(x * 1.6, 13.7), 7) - 0.3);
}

// Centre and half-width of the river on the ground plane. It runs out from
// under the ice along the back of the plain on the right, and spreads into a
// shallow salt pan on the left where it boils away (see pan_wet).
fn river(x: f32) -> vec2f {
    var c = 0.085 + 0.03 * sin(x * 2.3 + 0.5 + P.river_phase) + 0.018 * sin(x * 5.1 + 2.0 + P.river_phase * 1.7)
        + 0.02 * (fbm(vec2f(x * 2.8, 4.0), 3) - 0.5);
    c = mix(c, ground_top(x) - 0.006, smoothstep(2.6, 3.15, x));
    let near = max(0.5 + 1.6 * (0.12 - c) / 0.12, 0.15);
    let pan = smoothstep(0.95, 0.4, x);
    let w = 0.02 * near * (0.75 + 0.5 * fbm(vec2f(x * 6.0, 9.0), 2)) * (1.0 + 0.6 * pan) * (1.0 + P.river_flood);
    return vec2f(c, w);
}

// In the salt pan the river frays into braided channels that thin out
// toward the sun. 1 where there is still water, 0 on dry salt.
fn pan_wet(p: vec2f) -> f32 {
    let dry = smoothstep(0.95, 0.3, p.x);
    if (dry <= 0.0) {
        return 1.0;
    }
    let braid = fbm(vec2f(p.x * 22.0, p.y * 95.0), 4);
    return smoothstep(dry * 0.62, dry * 0.62 + 0.04, braid) * smoothstep(0.3, 0.42, p.x + braid * 0.08);
}

// ---------------------------------------------------------------- sky

fn band3(day: vec3f, twi: vec3f, night: vec3f, t: f32) -> vec3f {
    if (t < 0.5) {
        return mix(day, twi, t * 2.0);
    }
    return mix(twi, night, (t - 0.5) * 2.0);
}

fn sun_pos() -> vec2f {
    return SUN - vec2f(P.band_shift, 0.0);
}

// Sky gradient without the sun.
fn sky_base(p: vec2f) -> vec3f {
    let t = nightness(p.x);
    let elev = clamp((p.y - 0.33) / 0.67, 0.0, 1.0);
    let low = band3(vec3f(1.5, 0.42, 0.07), vec3f(0.55, 0.07, 0.3), vec3f(0.02, 0.035, 0.11), t);
    let high = band3(vec3f(0.32, 0.03, 0.05), vec3f(0.05, 0.012, 0.13), vec3f(0.001, 0.002, 0.008), t);
    return mix(low, high, pow(elev, 0.55));
}

fn sun_glow(p: vec2f) -> vec3f {
    let d = distance(p, sun_pos());
    return exp(-d * 7.0) * vec3f(2.6, 0.9, 0.18) + exp(-d * 2.2) * vec3f(0.35, 0.08, 0.015);
}

fn sky_color(p: vec2f) -> vec3f {
    let col = sky_base(p) + sun_glow(p);
    let disc = smoothstep(SUN_R, SUN_R - 0.0025, distance(p, sun_pos()));
    return mix(col, vec3f(42.0, 24.0, 9.0), disc);
}

// ---------------------------------------------------------------- tone mapping

fn aces1(x: f32) -> f32 {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Matches aces1 below the knee so both monitors agree in the midtones,
// then rolls off smoothly to the display's peak (m, relative to paper white).
fn hdr_curve(x: f32, m: f32) -> f32 {
    let k = 0.8;
    if (m <= 1.0) {
        return min(aces1(x), m);
    }
    if (x <= k) {
        return aces1(x);
    }
    let y0 = aces1(k);
    let slope = (aces1(k + 0.01) - aces1(k - 0.01)) / 0.02;
    let room = m - y0;
    return y0 + room * (1.0 - exp(-(x - k) * slope / room));
}

fn srgb_encode(c: vec3f) -> vec3f {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3f(0.0031308));
}

fn finish(scene: vec3f, frag: vec2f) -> vec4f {
    let c = max(scene * P.exposure, vec3f(0.0));
    if (P.mode == 1u) {
        let m = P.peak / P.paper_white;
        let tm = vec3f(hdr_curve(c.r, m), hdr_curve(c.g, m), hdr_curve(c.b, m));
        return vec4f(tm * P.paper_white, 1.0);
    }
    let tm = vec3f(aces1(c.r), aces1(c.g), aces1(c.b));
    let dither = (hash21(frag + fract(P.time) * 13.0) - 0.5) / 255.0;
    return vec4f(srgb_encode(tm) + dither, 1.0);
}
