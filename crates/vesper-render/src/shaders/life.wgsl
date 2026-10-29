// Life pass: creatures and plants as instanced tissue, drawn into the HDR
// scene. Each segment is a tapered tube shaded as if round, so overlapping
// tubes meet like real limbs; membranes are vein-lit sheets; glow cells add
// light. Output is premultiplied alpha, and glow writes alpha 0 to add.

const T_FRAME = 0;
const T_MUSCLE = 1;
const T_FILM = 2;
const T_GUT = 3;
const T_FLOAT = 4;
const T_SENSE = 5;
const T_GLOW = 6;
const T_ARMOR = 7;
const T_SPINE = 8;
const T_STORE = 9;
const T_BROOD = 10;
const T_MOUTH = 11;

// Depth units per world unit of body thickness. Small enough that an
// organism's own layers never reach the next organism's z.
const THICK = 0.0004;

struct Inst {
    @location(0) a: vec2f,
    @location(1) b: vec2f,
    @location(2) c: vec2f,
    @location(3) r: vec2f,
    @location(4) sway: vec4f,  // wind bend at a, b, c; clip height
    @location(5) zinfo: vec4f, // z, near/far offset, pattern u at a and b
    @location(6) color: vec4f, // albedo, translucency
    @location(7) accent: vec4f,// pattern colour, pattern kind
    @location(8) glow: vec4f,  // glow colour, freckles
    @location(9) mat: vec4f,   // tissue, seed, damage, alive
    @location(10) look: vec4f, // pattern scale, sheen, fringe, contrast
    @location(11) extra: vec4f,// countershade, iridescence, signal, signal pattern
    @location(12) state: vec4f,// infection load, visible, hue (+2 symbiont), bleeding
};

struct LOut {
    @builtin(position) pos: vec4f,
    @location(0) p: vec2f,
    @location(1) @interpolate(flat) ab: vec4f,
    @location(2) @interpolate(flat) cr: vec4f,
    @location(3) @interpolate(flat) sway: vec4f,
    @location(4) @interpolate(flat) zinfo: vec4f,
    @location(5) @interpolate(flat) color: vec4f,
    @location(6) @interpolate(flat) accent: vec4f,
    @location(7) @interpolate(flat) glow: vec4f,
    @location(8) @interpolate(flat) mat: vec4f,
    @location(9) @interpolate(flat) look: vec4f,
    @location(10) @interpolate(flat) extra: vec4f,
    @location(11) bary: vec3f,
    @location(12) @interpolate(flat) state: vec4f,
};

struct FOut {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
};

fn px_world() -> f32 {
    return 1.0 / P.world_size.y;
}

fn to_clip(w: vec2f, z: f32) -> vec4f {
    let px = vec2f(w.x, 1.0 - w.y) * P.world_size.y;
    let uv = (px - P.view_origin) / P.view_size;
    return vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, z, 1.0);
}

// Ground wind blows day-ward (toward -x) in gusts: harder under a storm,
// in a dust storm, and at the head of a cold fall pouring off the ice.
fn wind(x: f32) -> f32 {
    let t = P.time;
    let gust = vnoise(vec2f(x * 2.5 + t * 0.12, t * 0.07));
    let ds = x - P.storm_x;
    let dd = (x - P.dust_x) / 0.4;
    let dc = (x - P.cold_x - 0.05) / 0.2;
    let local = 1.0 + 1.5 * exp(-ds * ds * 8.0) * P.storm + 2.0 * exp(-dd * dd) * P.dust + 1.5 * exp(-dc * dc) * P.cold;
    // The sim's calm ground wind is -0.004 units/s; this is that as 1.
    let base = P.wind_low / 0.004;
    let flutter = 0.18 * sin(t * 1.7 + x * 53.0) * (0.5 + gust) * min(abs(base), 2.0);
    return base * (0.25 + 0.75 * gust * gust) * local + flutter;
}

fn bend(q: vec2f, amount: f32) -> vec2f {
    if (amount == 0.0) {
        return q;
    }
    let w = wind(q.x) * amount;
    // Tips swing on an arc, dipping a little as they lean.
    return q + vec2f(w, -abs(w) * 0.3);
}

fn pass_through(o: ptr<function, LOut>, inst: Inst, a: vec2f, b: vec2f, c: vec2f) {
    (*o).ab = vec4f(a, b);
    (*o).cr = vec4f(c, inst.r);
    (*o).sway = inst.sway;
    (*o).zinfo = inst.zinfo;
    (*o).color = inst.color;
    (*o).accent = inst.accent;
    (*o).glow = inst.glow;
    (*o).mat = inst.mat;
    (*o).look = inst.look;
    (*o).extra = inst.extra;
    (*o).state = inst.state;
}

// A box around the tube, padded for hair and antialiasing.
fn seg_quad(vi: u32, a: vec2f, b: vec2f, pad: f32) -> vec2f {
    let ax = b - a;
    let len = length(ax);
    var dir = vec2f(1.0, 0.0);
    if (len > 1e-7) {
        dir = ax / len;
    }
    let nrm = vec2f(-dir.y, dir.x);
    var corners = array<vec2f, 6>(
        vec2f(0.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(0.0, -1.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0),
    );
    let k = corners[vi];
    return mix(a - dir * pad, b + dir * pad, k.x) + nrm * pad * k.y;
}

@vertex
fn vs_seg(@builtin(vertex_index) vi: u32, inst: Inst) -> LOut {
    let a = bend(inst.a, inst.sway.x);
    let b = bend(inst.b, inst.sway.y);
    let rmax = max(max(inst.r.x, inst.r.y), px_world() * 0.6);
    let pad = rmax * (1.0 + inst.look.z) * select(1.0, 1.2, i32(inst.mat.x + 0.5) == T_FILM) + 2.0 * px_world();
    let w = seg_quad(vi, a, b, pad);
    var o: LOut;
    o.pos = to_clip(w, inst.zinfo.x);
    o.p = w;
    o.bary = vec3f(0.0);
    pass_through(&o, inst, a, b, inst.c);
    return o;
}

@vertex
fn vs_glow(@builtin(vertex_index) vi: u32, inst: Inst) -> LOut {
    let a = bend(inst.a, inst.sway.x);
    let b = bend(inst.b, inst.sway.y);
    let rmax = max(max(inst.r.x, inst.r.y), px_world());
    let w = seg_quad(vi, a, b, rmax * 5.0);
    var o: LOut;
    o.pos = to_clip(w, inst.zinfo.x);
    o.p = w;
    o.bary = vec3f(0.0);
    pass_through(&o, inst, a, b, inst.c);
    return o;
}

@vertex
fn vs_membrane(@builtin(vertex_index) vi: u32, inst: Inst) -> LOut {
    let a = bend(inst.a, inst.sway.x);
    let b = bend(inst.b, inst.sway.y);
    let c = bend(inst.c, inst.sway.z);
    var o: LOut;
    var w = a;
    var bary = vec3f(1.0, 0.0, 0.0);
    if (vi == 1u) {
        w = b;
        bary = vec3f(0.0, 1.0, 0.0);
    } else if (vi >= 2u) {
        // Vertices 3..5 collapse onto c, leaving one triangle.
        w = c;
        bary = vec3f(0.0, 0.0, 1.0);
    }
    o.pos = to_clip(w, inst.zinfo.x);
    o.p = w;
    o.bary = bary;
    pass_through(&o, inst, a, b, c);
    return o;
}

// ---------------------------------------------------------------- shadows

// Shadows on the ground: a soft dark pool at every body's feet, and on the
// day side a long cast shadow stretched away from the low sun, fading and
// blurring with length. Flyers' shadows fall below them, fainter the higher
// they are. a: the floor point under the body's middle; r: half its width
// and its height on screen; c.x: its height off the floor.

// How long a cast shadow is, per unit of body height, and how dark.
fn cast_shadow(x: f32) -> vec2f {
    let day = daylight(x);
    let night = nightness(x);
    let low = smoothstep(0.1, 0.4, night) * smoothstep(0.75, 0.45, night);
    return vec2f(1.4 + 2.6 * low, 0.32 * day + 0.2 * low);
}

// Ground seen from the eye is squashed this much at screen height y.
fn ground_squash(y: f32) -> f32 {
    return max((HORIZON - y) / FOCAL, 0.012) * 1.6;
}

// Shadows fall away from the sun on the day edge: sideways, and a little
// toward the eye. On the ground, before squashing.
const SHADOW_DIR = vec2f(0.894, -0.447);

@vertex
fn vs_shadow(@builtin(vertex_index) vi: u32, inst: Inst) -> LOut {
    let g = inst.a;
    let hw = inst.r.x;
    let hgt = inst.r.y;
    let alt = inst.c.x;
    let cs = cast_shadow(g.x);
    let len = hgt * cs.x * step(0.02, cs.y) + alt * 3.0;
    let sq = ground_squash(g.y);
    // A box around the pool and the shadow, laid along the shadow.
    let back = hw * 1.6;
    let wide = hw * 1.6 + len * 0.25;
    var corners = array<vec2f, 6>(
        vec2f(0.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(0.0, -1.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0),
    );
    let k = corners[vi];
    let side = vec2f(-SHADOW_DIR.y, SHADOW_DIR.x);
    let gd = SHADOW_DIR * mix(-back, len + back, k.x) + side * wide * k.y;
    let w = g + vec2f(gd.x, gd.y * sq) + vec2f(0.0, 2.0 * px_world() * k.y);
    var o: LOut;
    o.pos = to_clip(w, inst.zinfo.x);
    o.p = w;
    o.bary = vec3f(0.0);
    pass_through(&o, inst, inst.a, inst.b, inst.c);
    return o;
}

@fragment
fn fs_shadow(i: LOut) -> FOut {
    let g = i.ab.xy;
    let hw = max(i.cr.z, 1e-5);
    let hgt = max(i.cr.w, 1e-5);
    let alt = i.cr.x;
    let sq = ground_squash(g.y);
    // Back onto the ground, unsquashed.
    let d = vec2f(i.p.x - g.x, (i.p.y - g.y) / sq);
    let lift = exp(-alt / hgt * 1.5);
    // The pool of shade right under it.
    let k = d / (hw * 1.1);
    let contact = exp(-dot(k, k) * 1.8) * 0.45 * lift;
    // The cast shadow, widening and softening as it goes.
    let cs = cast_shadow(g.x);
    let len = hgt * cs.x + alt * 3.0;
    let side = vec2f(-SHADOW_DIR.y, SHADOW_DIR.x);
    let along = (dot(d, SHADOW_DIR) - alt * 3.0 + hw * 0.3) / max(len, 1e-5);
    let f = clamp(along, 0.0, 1.0);
    let spread = hw * (0.7 + 0.8 * f) + len * 0.06 * f;
    let across = dot(d, side) / max(spread, 1e-6);
    let stretch = smoothstep(-0.05, 0.1, along) * smoothstep(1.0, 0.45, along) * exp(-across * across * 1.4)
        * cs.y * mix(1.0, 0.35, f) * mix(1.0, 0.4, 1.0 - lift);
    let a = clamp(1.0 - (1.0 - contact) * (1.0 - stretch), 0.0, 0.7) * (1.0 - 0.5 * i.zinfo.x) * i.mat.w;
    if (a < 0.004) {
        discard;
    }
    var o: FOut;
    o.color = vec4f(0.0, 0.0, 0.0, a);
    o.depth = i.zinfo.x;
    return o;
}

// ---------------------------------------------------------------- shared shading

struct Tube {
    d: f32,   // distance from the axis
    r: f32,   // radius here
    h: f32,   // 0 at a, 1 at b
    v: f32,   // signed across the tube, -1..1 (+ toward the left normal)
    nrm: vec2f,
};

fn tube(p: vec2f, a: vec2f, b: vec2f, ra: f32, rb: f32) -> Tube {
    let pa = p - a;
    let ba = b - a;
    let l2 = max(dot(ba, ba), 1e-12);
    let h = clamp(dot(pa, ba) / l2, 0.0, 1.0);
    let q = pa - ba * h;
    var t: Tube;
    t.h = h;
    t.r = max(mix(ra, rb, h), px_world() * 0.6);
    t.d = length(q);
    let dir = ba / sqrt(l2);
    t.nrm = vec2f(-dir.y, dir.x);
    // Past the end caps, fall back to the radial direction.
    if (h <= 0.0 || h >= 1.0) {
        t.v = t.d / t.r * sign(dot(q, t.nrm) + 1e-6);
        if (t.d > 1e-7) {
            t.nrm = q / t.d * sign(dot(q, t.nrm) + 1e-6);
        }
    } else {
        t.v = dot(q, t.nrm) / t.r;
    }
    return t;
}

// Light from the low sun reaching a body, by zone.
fn sun_light(x: f32) -> vec3f {
    return band3(vec3f(2.4, 1.15, 0.45), vec3f(1.15, 0.38, 0.6), vec3f(0.025, 0.03, 0.08), nightness(x));
}

fn sky_light(x: f32) -> vec3f {
    return band3(vec3f(0.42, 0.2, 0.12), vec3f(0.14, 0.06, 0.24), vec3f(0.012, 0.02, 0.05), nightness(x));
}

fn bounce_light(x: f32) -> vec3f {
    return band3(vec3f(0.3, 0.1, 0.03), vec3f(0.07, 0.02, 0.06), vec3f(0.004, 0.006, 0.014), nightness(x));
}

fn light_dir(p: vec2f) -> vec3f {
    let s = sun_pos();
    return normalize(vec3f(s.x - p.x, (s.y - p.y) * 0.6 + 0.1, 0.45));
}

fn haze(p: vec2f, col: vec3f, z: f32) -> vec3f {
    let hz = sky_base(vec2f(p.x, 0.36)) + sun_glow(vec2f(p.x, 0.36)) * 0.4;
    // Passing cloud shades bodies as it does the ground under them.
    let shaded = col * (1.0 - 0.5 * cloud_shade(p) * daylight(p.x));
    return mix(shaded, hz * 0.7, clamp(z * z * 0.55, 0.0, 0.6));
}

// How much of the accent colour shows at pattern coordinate q (scaled units).
fn pattern(kind: i32, q: vec2f, seed: f32) -> f32 {
    switch kind {
        case 1: {
            // Cross bands, wavering.
            let w = q.x * 6.283 + (vnoise(q * 0.8 + seed) - 0.5) * 3.0;
            return smoothstep(0.2, 0.6, sin(w));
        }
        case 2: {
            // Spots of uneven size.
            let cell = floor(q);
            let c = 0.25 + 0.5 * hash22(cell + seed);
            let rad = 0.12 + 0.22 * hash21(cell + seed + 3.1);
            return step(0.35, hash21(cell + seed + 7.7)) * smoothstep(rad, rad * 0.55, length(fract(q) - c));
        }
        case 3: {
            // Stripes running nose to tail.
            return smoothstep(0.25, 0.7, sin(q.y * 9.0 + vnoise(vec2f(q.x * 0.5, seed)) * 2.0));
        }
        case 4: {
            // Reticulated: pale plates in dark netting.
            let cell = floor(q);
            let f = fract(q);
            var d1 = 9.0;
            var d2 = 9.0;
            for (var j = -1; j <= 1; j++) {
                for (var i = -1; i <= 1; i++) {
                    let o = vec2f(f32(i), f32(j));
                    let c = o + 0.15 + 0.7 * hash22(cell + o + seed);
                    let d = length(f - c);
                    if (d < d1) {
                        d2 = d1;
                        d1 = d;
                    } else if (d < d2) {
                        d2 = d;
                    }
                }
            }
            return smoothstep(0.03, 0.1, d2 - d1);
        }
        case 5: {
            return smoothstep(0.42, 0.62, fbm(q + seed, 3));
        }
        default: {
            // Fine mottling.
            return smoothstep(0.45, 0.7, vnoise(q * 2.0 + seed)) * 0.6;
        }
    }
}

fn depth_of(i: LOut, height: f32) -> f32 {
    return clamp(i.zinfo.x - THICK * (height + i.zinfo.y), 0.0, 1.0);
}

// Discards below a burrower's ground line.
fn clipped(i: LOut, p: vec2f) -> bool {
    return p.y < i.sway.w;
}

// ---------------------------------------------------------------- skin

// What the outside of a body is like (life.rs Skin). Each is drawn as relief
// on the round of the tube: a height and its slope, which tilt the normal so
// light catches every scale and fold, plus a little colour of its own.
const SKIN_SCALES = 0;
const SKIN_PEBBLED = 1;
const SKIN_WRINKLED = 2;
const SKIN_CHITIN = 3;
const SKIN_SLICK = 4;
const SKIN_FUR = 5;
const SKIN_BARK = 6;

struct Skin {
    h: f32,        // relief, 0 in the cracks .. 1 on top
    slope: vec2f,  // its slope along the body and around it
    tint: f32,     // brightness, 1 unchanged
    accent: f32,   // how much of the accent colour shows (bead tips, scale rims)
    gloss: f32,    // multiplies the sheen
    hair: f32,     // 0..1 how much it lights like fur
};

// How much of a feature `period` world units across can be drawn when one
// pixel spans `foot` of it. Detail fades to its average before it would
// alias, the way a mipmap would, and it is judged per direction, so scales
// foreshortened toward a body's edge blur across but stay crisp along it:
// anisotropic filtering for textures that are made rather than sampled.
fn keep(period: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(0.25, 0.7, foot / max(period, 1e-7));
}

// Overlapping scales in staggered rows, the nearer row lying over the next.
// q in scale units; returns the visible scale's height, slope, and id.
fn scale_cell(q: vec2f, seed: f32) -> vec4f {
    var best = 1e9;
    var out = vec4f(0.0, 0.0, 0.0, 0.0);
    let rad = 0.78;
    for (var j = -1; j <= 1; j++) {
        let row = floor(q.y) + f32(j);
        let off = 0.5 * (row - 2.0 * floor(row * 0.5));
        for (var i = -1; i <= 1; i++) {
            let col = floor(q.x - off) + f32(i);
            let c = vec2f(col + off + 0.5, row + 0.5);
            let d = q - c;
            let rr = rad * (0.9 + 0.2 * hash21(c + seed));
            let k = dot(d, d) / (rr * rr);
            // The scale nearest the head that covers q is on top.
            if (k < 1.0 && c.x < best) {
                best = c.x;
                // Each scale is a shallow dome lifting toward its free edge.
                out = vec4f(1.0 - k, -2.0 * d / (rr * rr) + vec2f(0.6, 0.0), hash21(c + seed + 5.3));
            }
        }
    }
    return out;
}

fn skin(kind: i32, s: vec2f, r: f32, foot: vec2f, seed: f32) -> Skin {
    var k: Skin;
    k.h = 0.5;
    k.slope = vec2f(0.0);
    k.tint = 1.0;
    k.accent = 0.0;
    k.gloss = 1.0;
    k.hair = 0.0;
    let fmax = max(foot.x, foot.y);
    switch kind {
        case 0: {
            // Scales, sized to the body, a little wider than long.
            let cs = r * 0.3;
            let q = s / cs * vec2f(1.0, 0.8);
            let sc = scale_cell(q, seed);
            let fade = keep(cs, fmax);
            // A dark line where each scale's edge sits over the next.
            let w = clamp(fmax / cs * 1.5, 0.05, 0.5);
            let rim = smoothstep(0.0, w, sc.x);
            k.h = mix(0.6, sc.x * 0.7 + 0.3 * rim, fade);
            k.slope = sc.yz * vec2f(1.0, 0.8) * 0.22 * fade;
            k.tint = mix(1.0, (0.55 + 0.45 * rim) * (0.88 + 0.24 * sc.w), fade);
            k.accent = smoothstep(0.35, 0.0, sc.x) * rim * 0.35 * fade;
            k.gloss = 1.2;
        }
        case 1: {
            // Beaded hide: round beads packed in a net of creases, some
            // beads in the accent colour.
            let cs = r * 0.22;
            let q = s / cs;
            let cell = floor(q);
            let f = fract(q);
            var d1 = 9.0;
            var d2 = 9.0;
            var c1 = vec2f(0.0);
            var id = 0.0;
            for (var j = -1; j <= 1; j++) {
                for (var i = -1; i <= 1; i++) {
                    let o = vec2f(f32(i), f32(j));
                    let c = o + 0.2 + 0.6 * hash22(cell + o + seed);
                    let d = length(f - c);
                    if (d < d1) {
                        d2 = d1;
                        d1 = d;
                        c1 = c;
                        id = hash21(cell + o + seed + 9.1);
                    } else if (d < d2) {
                        d2 = d;
                    }
                }
            }
            let fade = keep(cs, fmax);
            let w = clamp(fmax / cs * 1.5, 0.04, 0.4);
            let crease = smoothstep(0.0, 0.08 + w, d2 - d1);
            let dome = sqrt(max(1.0 - d1 * d1 * 2.2, 0.0));
            k.h = mix(0.6, dome * crease, fade);
            k.slope = -(f - c1) * 0.6 * crease * fade;
            k.tint = mix(1.0, (0.45 + 0.55 * crease) * (0.85 + 0.3 * id), fade);
            k.accent = step(0.72, id) * crease * 0.8 * fade;
            k.gloss = 0.5;
        }
        case 2: {
            // Thick wrinkled hide: creases running mostly around the body,
            // finer ones crossing them.
            let cs = r * 0.5;
            let a = noised(vec2f(s.x / cs * 2.2, s.y / cs * 0.6) + seed);
            let b = noised(vec2f(s.x / cs * 0.9, s.y / cs * 2.4) + seed + 19.0);
            let ca = 1.0 - abs(2.0 * a.x - 1.0);
            let cb = 1.0 - abs(2.0 * b.x - 1.0);
            let fa = keep(cs * 0.45, foot.x);
            let fb = keep(cs * 0.45, foot.y);
            let crease = max(smoothstep(0.72, 0.96, ca) * fa, smoothstep(0.8, 0.97, cb) * fb * 0.7);
            k.h = 1.0 - crease;
            k.slope = (a.yz * sign(0.5 - a.x) * vec2f(2.2, 0.6) * fa + b.yz * sign(0.5 - b.x) * vec2f(0.9, 2.4) * fb * 0.5) * 0.4;
            k.tint = (1.0 - 0.6 * crease) * (0.9 + 0.2 * a.x);
            k.gloss = 0.35;
        }
        case 3: {
            // Shell: rings of plate with sutures between them, a seam down
            // the back, and fine pitting all over.
            let ring = r * 0.75;
            let t = fract(s.x / ring + seed);
            let fr = keep(ring, foot.x);
            let w = clamp(foot.x / ring * 1.5, 0.03, 0.3);
            let suture = (1.0 - smoothstep(0.0, w, t) * smoothstep(1.0, 1.0 - w * 2.0, t)) * fr;
            let seam = smoothstep(r * 0.06 + foot.y, 0.0, abs(s.y - r * 0.9)) * keep(r * 0.1, foot.y);
            let pq = s / (r * 0.07);
            let pc = floor(pq);
            let pit = step(0.55, hash21(pc + seed)) * smoothstep(0.32, 0.12, length(fract(pq) - 0.5)) * keep(r * 0.07, fmax);
            k.h = 1.0 - max(suture, seam) - pit * 0.4;
            // Each plate bulges a little, its back edge raised over the next.
            k.slope = vec2f((t - 0.5) * 0.35 * fr, 0.0);
            k.tint = (1.0 - 0.55 * max(suture, seam)) * (1.0 - 0.3 * pit);
            k.gloss = 1.4 * (1.0 - 0.6 * pit);
        }
        case 4: {
            // Slick skin: smooth, with scattered glands standing up from it.
            let cs = r * 0.16;
            let q = s / cs;
            let c = floor(q);
            let o = 0.25 + 0.5 * hash22(c + seed);
            let d = (fract(q) - o) / (0.18 + 0.12 * hash21(c + seed + 3.0));
            let has = step(0.45, hash21(c + seed + 7.0)) * keep(cs, fmax);
            let g = max(1.0 - dot(d, d), 0.0) * has;
            let m = smoothstep(0.3, 0.75, fbm(s / (r * 0.5) + seed, 3));
            k.h = 0.5 + 0.5 * g;
            k.slope = -d * g * 0.9;
            k.tint = 0.75 + 0.4 * m + 0.2 * g;
            k.accent = g * 0.5;
            k.gloss = 1.6;
        }
        case 5: {
            // Fur: fine strands laid back along the body, in clumps.
            let cs = r * 0.08;
            let clump = vnoise(vec2f(s.x / (r * 0.3), s.y / (r * 0.15)) + seed);
            let lay = s.y + (clump - 0.5) * r * 0.3;
            let fs = keep(cs, foot.y);
            let strand = vnoise(vec2f(s.x / (cs * 6.0), lay / cs) + seed * 3.0);
            let fine = vnoise(vec2f(s.x / (cs * 3.0), lay / (cs * 0.4)) + seed * 7.0);
            let fine_k = keep(cs * 0.4, foot.y);
            let tuft = smoothstep(0.25, 0.75, clump);
            k.h = mix(0.5, strand, fs) * (0.4 + 0.6 * tuft);
            k.slope = vec2f(0.0, ((strand - 0.5) * fs + (fine - 0.5) * fine_k * 0.6) * 0.8);
            k.tint = mix(0.9, 0.5 + 0.8 * strand, fs) * mix(1.0, 0.7 + 0.6 * fine, fine_k) * (0.8 + 0.3 * tuft);
            k.gloss = 0.25;
            k.hair = 1.0;
        }
        default: {
            // Bark: ridges and furrows running up the stem, split into
            // plates, with pale breathing pores across them.
            let cs = r * 0.4;
            let wob = (vnoise(vec2f(s.x / (r * 1.5), s.y / cs) + seed) - 0.5) * 1.2;
            let ridge = 0.5 + 0.5 * sin(s.y / cs * 6.283 + wob * 3.0);
            let fa = keep(cs, foot.y);
            let split = smoothstep(0.72, 0.82, vnoise(vec2f(s.x / (r * 0.8), s.y / cs * 2.0) + seed + 4.0)) * keep(r * 0.4, fmax);
            let lq = vec2f(s.x / (r * 0.18), s.y / (r * 0.5));
            let lc = floor(lq);
            let lenticel = step(0.8, hash21(lc + seed)) * smoothstep(0.2, 0.05, abs(fract(lq.x) - 0.5)) * smoothstep(0.45, 0.2, abs(fract(lq.y) - 0.5)) * keep(r * 0.18, foot.x);
            let furrow = mix(0.5, ridge, fa);
            k.h = furrow * (1.0 - split * 0.6);
            k.slope = vec2f(0.0, cos(s.y / cs * 6.283 + wob * 3.0) * 0.35 * fa);
            k.tint = (0.55 + 0.5 * furrow) * (1.0 - split * 0.35) + lenticel * 0.35;
            k.gloss = 0.15;
        }
    }
    return k;
}

// ---------------------------------------------------------------- body

// One of sixteen light signals: four shapes on the skin, each flashed in one
// of four rhythms, so kin can tell "danger" from "mate" across the dark.
fn signal_pattern(symbol: i32, u: f32, v: f32, scale: f32, seed: f32) -> f32 {
    let su = u / scale;
    var shape = 0.0;
    switch (symbol % 4) {
        case 0: { shape = smoothstep(0.3, 0.1, abs(fract(su * 0.5) - 0.5)); }               // bands
        case 1: {                                                                            // spots
            let q = vec2f(su, v * 2.0);
            shape = smoothstep(0.32, 0.18, length(fract(q) - 0.5)) * step(0.4, hash21(floor(q) + seed));
        }
        case 2: { shape = smoothstep(0.35, 0.15, abs(v - 0.15)); }                          // flank stripe
        default: { shape = smoothstep(0.25, 0.08, abs(fract(su * 0.5 + abs(v) * 0.6) - 0.5)); } // chevrons
    }
    let t = P.time;
    var beat = 1.0;
    switch (symbol / 4) {
        case 1: { beat = 0.5 + 0.5 * sin(t * 3.0); }                       // slow throb
        case 2: { beat = step(0.5, fract(t * 4.0)); }                       // strobe
        case 3: { beat = 0.5 + 0.5 * sin(su * 1.6 - t * 7.0); }             // ripple down the body
        default: {}
    }
    return shape * beat;
}

@fragment
fn fs_body(i: LOut) -> FOut {
    let p = i.p;
    if (clipped(i, p)) {
        discard;
    }
    let tissue = i32(i.mat.x + 0.5);
    let seed = i.mat.y;
    let damage = i.mat.z;
    let alive = i.mat.w;
    let decay = 1.0 - alive;
    var tb = tube(p, i.ab.xy, i.ab.zw, i.cr.z, i.cr.w);
    let px = px_world();
    let u = mix(i.zinfo.z, i.zinfo.w, tb.h);
    // Anything sized by the local radius counts from this segment's own
    // start: the chain's running coordinate is large, and dividing it by a
    // tapering radius makes it race and shimmer.
    let ul = u - i.zinfo.z;
    let leaf = tissue == T_FILM;
    if (leaf) {
        // A leaf is a blade, not a tube: narrow at the stalk and widest past
        // the middle. Each plant has its own kind: some drawn out to a point,
        // most rounded or blunt, a few notched at the tip; some smooth-edged,
        // some finely toothed.
        let kind = hash21(vec2f(seed, 3.7));
        let hh = pow(tb.h, mix(0.55, 0.9, hash21(vec2f(seed, 5.1))));
        let pointed = pow(sin(3.14159 * hh), 0.7);
        let rounded = pow(max(1.0 - pow(2.0 * hh - 1.0, 2.0), 0.0), mix(0.3, 0.55, kind));
        var blade = mix(pointed, rounded, smoothstep(0.15, 0.45, kind));
        let notch = smoothstep(0.85, 0.9, kind) * smoothstep(0.75, 1.0, tb.h) * smoothstep(0.35, 0.0, abs(tb.v));
        blade *= 1.0 - notch * 0.9;
        let toothed = 0.12 * smoothstep(0.5, 0.8, hash21(vec2f(seed, 8.3)));
        let teeth = 1.0 - toothed * smoothstep(0.4, 1.0, abs(sin(ul / max(tb.r * 0.28, px) + seed * 7.0)));
        let rr = max(tb.r * max(blade * teeth, 0.06) * 1.15, px * 0.6);
        tb.v = tb.v * tb.r / rr;
        tb.r = rr;
    }

    let night = nightness(p.x);
    let day = daylight(p.x);

    // Hair hangs from the underside of shaggy bodies.
    let fringe = i.look.z * tb.r;
    var hair = 0.0;
    var hair_len = 0.0;
    if (fringe > 0.0 && tb.d > tb.r - px) {
        let down = -tb.nrm.y * sign(tb.v);
        let strand = ul / max(tb.r * 0.09, px * 1.5);
        let len = fringe * (0.35 + 0.65 * hash21(vec2f(floor(strand), seed))) * smoothstep(-0.2, 0.6, down);
        hair_len = (tb.d - tb.r) / max(len, 1e-6);
        let wisp = smoothstep(0.5, 0.15, abs(fract(strand) - 0.5));
        hair = step(hair_len, 1.0) * wisp * (1.0 - hair_len * 0.6) * step(0.0, len - px);
    }

    let cover = smoothstep(tb.r + px, tb.r - px, tb.d);
    if (cover <= 0.0 && hair <= 0.0) {
        discard;
    }
    let v = clamp(tb.v, -1.0, 1.0);
    // Leaves are near-flat sheets, folded a little along the midrib.
    let nz = select(sqrt(max(1.0 - v * v, 0.0)), sqrt(max(1.0 - v * v * 0.3, 0.0)), leaf);
    // The skin's own texture, laid on the round of the tube: `arc` runs
    // around it, so the texture crowds toward the silhouette as it would on
    // a real limb. One pixel spans more of it there (`foot`), and keep()
    // fades what is too fine to draw.
    let arc = asin(v) * tb.r;
    let foot = vec2f(px, px / max(nz, 0.08));
    var sk = Skin(0.5, vec2f(0.0), 1.0, 0.0, 1.0, 0.0);
    if (tissue == T_MUSCLE || tissue == T_FRAME) {
        sk = skin(i32(i.cr.x + 0.5), vec2f(ul, arc), tb.r, foot, seed);
    } else if (tissue == T_FLOAT) {
        sk = skin(SKIN_SLICK, vec2f(ul, arc), tb.r, foot, seed);
    }
    // Skin grain: wrinkles across the body and fine pores, as normal wobble.
    let gq = vec2f(ul, v * tb.r) / max(tb.r * 0.35, px * 3.0);
    let grain = vec2f(vnoise(gq * vec2f(0.6, 2.2) + seed), vnoise(gq * 1.7 + seed + 9.0)) - 0.5;
    var bump = vec2f(grain.x, grain.y) * 0.12 * nz;
    // Soft folds ringing fleshy bodies, deep on some kinds and all but gone
    // on others, spaced by the body's own pattern scale.
    if (tissue == T_MUSCLE || tissue == T_GUT || (tissue == T_FRAME && max(i.cr.z, i.cr.w) > 0.45 * i.sway.z)) {
        let kind_h = hash21(vec2f(i.look.w, i.extra.x) * 97.0);
        let ridged = smoothstep(0.35, 0.9, kind_h);
        let fq = ul / max(tb.r * mix(0.45, 0.9, fract(kind_h * 31.0)), px * 2.5);
        let fold = cos(fq * 6.283 + vnoise(vec2f(fq * 0.5, v * 2.0) + seed) * 1.5);
        let along_dir = vec2f(tb.nrm.y, -tb.nrm.x);
        let deep = smoothstep(0.25, 0.75, vnoise(vec2f(fq * 0.37, seed)));
        bump += along_dir * fold * 0.22 * deep * ridged * nz * smoothstep(px * 2.0, px * 6.0, tb.r);
    }
    let along3 = vec3f(tb.nrm.y, -tb.nrm.x, 0.0);
    let round3 = vec3f(tb.nrm * nz, -v);
    let n = normalize(vec3f(tb.nrm * v + bump, nz) - along3 * sk.slope.x - round3 * sk.slope.y);
    let L = light_dir(p);

    // ---- albedo and pattern
    var albedo = i.color.rgb * (0.85 + 0.3 * vnoise(gq * 0.5 + seed * 2.0));
    let kind = i32(i.accent.w + 0.5);
    let q = vec2f(u, v * tb.r) / max(i.look.x, 1e-4);
    let patterned = tissue == T_MUSCLE || (tissue == T_FRAME && max(i.cr.z, i.cr.w) > 0.45 * i.sway.z);
    if (patterned) {
        // Markings too fine for the pixels fade to an even tint, not fizz.
        let marks = mix(0.3, pattern(kind, q, seed), keep(i.look.x * 0.4, max(foot.x, foot.y)));
        albedo = mix(albedo, i.accent.rgb, marks * i.look.w);
        // Dark backs, pale bellies.
        let up = n.y * 0.5 + 0.5;
        albedo *= mix(1.0, mix(1.5, 0.55, up), i.extra.x);
    }
    // The skin's texture lies over its markings.
    albedo = mix(albedo, i.accent.rgb, sk.accent * (0.3 + 0.7 * i.look.w)) * sk.tint;
    if (tissue == T_ARMOR) {
        // Overlapping plates with dark seams and a mineral grain.
        let plate = fract(ul / max(tb.r * 1.3, 1e-4) + seed);
        let seam = smoothstep(0.0, 0.1, plate) * smoothstep(1.0, 0.82, plate);
        albedo *= (0.35 + 0.65 * seam) * (0.75 + 0.5 * fbm(q * 3.0, 2));
        albedo = mix(albedo, i.accent.rgb * 0.6, pattern(kind, q * 0.7, seed) * i.look.w * 0.5);
    }
    if (tissue == T_SPINE) {
        albedo *= 0.75 + 0.5 * tb.h;
    }
    if (tissue == T_FRAME && !patterned) {
        // Thin limbs: dark knuckles at the joints, a paler shin between,
        // banded in the accent colour on some kinds.
        let knuckle = smoothstep(0.14, 0.0, tb.h) + smoothstep(0.86, 1.0, tb.h);
        albedo *= mix(1.15, 0.45, knuckle);
        let band = smoothstep(0.05, 0.0, abs(tb.h - 0.22)) * step(0.5, fract(seed * 13.7));
        albedo = mix(albedo, i.accent.rgb, band * 0.7);
    }
    var veins = 0.0;
    if (leaf) {
        // A midrib, and side veins angling out toward the tip.
        let rib = smoothstep(0.1, 0.03, abs(v));
        let lat = abs(fract(ul / max(i.cr.z * 0.9, 1e-4) - abs(v) * 0.7 + seed) - 0.5);
        veins = max(rib, smoothstep(0.42, 0.48, lat) * smoothstep(0.95, 0.5, abs(v)) * 0.8);
        // Deep at the stalk, lighter toward the tip, the margin edged in
        // the accent colour; blotched with age.
        albedo *= mix(0.6, 1.2, smoothstep(0.0, 0.85, tb.h));
        albedo = mix(albedo, i.accent.rgb, smoothstep(0.75, 0.97, abs(v)) * 0.55 * i.look.w);
        albedo *= 0.8 + 0.4 * vnoise(vec2f(ul, v * tb.r) / max(tb.r * 0.6, 1e-4) + seed * 3.0);
        // The fine grain of its cells, where there are pixels enough to show it.
        let cell = tb.r * 0.07;
        albedo *= 1.0 + (vnoise(vec2f(ul, arc) / cell + seed * 5.0) - 0.5) * 0.35 * keep(cell, max(foot.x, foot.y));
        albedo = mix(albedo, albedo * 0.35, veins * 0.75);
    }

    // ---- what lives in it
    let load = i.state.x;
    let symbiont = i.state.z >= 2.0;
    let ihue = fract(i.state.z);
    let icol = 0.5 + 0.5 * cos(6.283 * (ihue + vec3f(0.0, 0.33, 0.67)));
    // Caps share one coordinate, so marks keep to the tube's middle.
    let mid = smoothstep(0.0, 0.08, tb.h) * smoothstep(1.0, 0.92, tb.h);
    var sores = 0.0;
    var sore_core = 0.0;
    if (load > 0.0 && !symbiont) {
        let sick = load * (1.0 - i.state.y);
        // A microbe drains the colour toward a jaundiced, bruised tint in blotches.
        let blotch = smoothstep(0.35, 0.7, fbm(q * 0.8 + seed * 7.0, 3));
        let jaundice = mix(vec3f(0.32, 0.3, 0.06), icol * 0.25, 0.35);
        albedo = mix(albedo, mix(vec3f(luma(albedo)), jaundice, 0.7), clamp(sick * (0.35 + 0.65 * blotch), 0.0, 0.8));
        // Open sores: pale weeping centres with angry dark rims.
        let sq = vec2f(ul, v * tb.r) / max(tb.r * 0.6, 1e-4);
        let sc = floor(sq);
        let sd = length(fract(sq) - (0.3 + 0.4 * hash22(sc + seed + 13.0)));
        let has = step(1.0 - 0.5 * sick, hash21(sc + seed + 17.0));
        let size = 0.6 + 0.6 * hash21(sc + seed + 23.0);
        let sdn = sd / size + (vnoise(sq * 6.0 + seed) - 0.5) * 0.12;
        sores = has * smoothstep(0.3, 0.2, sdn) * step(0.15, sick) * mid;
        sore_core = sores * smoothstep(0.17, 0.05, sdn);
    }

    // Death: colour drains to bruise and ochre, flesh opens onto bone.
    var bone = 0.0;
    if (decay > 0.0) {
        let grey = vec3f(luma(albedo));
        albedo = mix(albedo, mix(grey, vec3f(0.09, 0.035, 0.05), 0.5), clamp(decay * 1.6, 0.0, 0.85));
        let rot = fbm(q * 1.7 + seed * 3.0, 3);
        if (tissue == T_FRAME || tissue == T_SPINE) {
            bone = smoothstep(0.35, 0.8, decay);
            albedo = mix(albedo, vec3f(0.55, 0.48, 0.36), bone);
        } else if (rot < decay * 1.15 - 0.35) {
            discard;
        }
    }

    // ---- lighting
    let sun = sun_light(p.x);
    // Flesh lets light creep past the terminator, reddened on the way.
    let fleshy = select(0.0, 0.45, patterned || tissue == T_GUT || tissue == T_STORE || leaf);
    let ndl = dot(n, L);
    let diffuse = max((ndl + fleshy) / (1.0 + fleshy), 0.0);
    let scatter = fleshy * smoothstep(-0.45, 0.0, ndl) * smoothstep(0.35, 0.0, ndl);
    let amb = mix(bounce_light(p.x), sky_light(p.x), n.y * 0.5 + 0.5);
    // Key from the sun, plus the twilight sky wrapped around the front.
    let fill = sky_light(p.x) * (0.6 + 0.9 * nz) + sun * 0.12;
    // Joints sit in their own shadow, and so do the skin's cracks.
    let ao = (1.0 - 0.3 * (smoothstep(0.2, 0.0, tb.h) + smoothstep(0.8, 1.0, tb.h)) * nz) * (0.55 + 0.45 * sk.h);
    var col = albedo * (sun * diffuse * (0.45 + 0.55 * day) + amb * 1.3 + fill * 0.8) * ao;
    col += albedo * sun * scatter * vec3f(1.0, 0.35, 0.25) * 0.5;

    // Warm rim on the edge facing the sun, cool rim from the sky on the night side.
    let edge = pow(1.0 - nz, 2.5);
    let toward = max(dot(normalize(tb.nrm * sign(v + 1e-6)), normalize(L.xy)), 0.0);
    col += edge * toward * sun * (0.6 + 0.6 * day) * mix(vec3f(1.0), albedo * 3.0 + 0.3, 0.35);
    col += edge * max(n.y, 0.0) * vec3f(0.06, 0.14, 0.34) * night * 1.4;
    // Fur catches the light in a soft halo around its outline.
    col += sk.hair * pow(1.0 - nz, 1.5) * albedo * (sun * (0.3 + 0.4 * day) + sky_light(p.x)) * 0.9;

    // Lacquer and wet sheen.
    let sheen = i.look.y * sk.gloss;
    if (sheen > 0.0) {
        let H = normalize(L + vec3f(0.0, 0.0, 1.0));
        let spec = pow(max(dot(n, H), 0.0), 12.0 + sheen * 90.0) * sheen;
        col += spec * (sun * 0.9 + sky_light(p.x) * 2.0);
    }

    // Light glowing through thin flesh, mostly toward the edges where it is
    // thinnest. The flesh itself is solid: nothing behind shows through.
    let trans = i.color.a;
    col += albedo * sun * trans * (0.25 + 0.75 * (1.0 - nz)) * (0.4 + 0.6 * day) * 0.7 * (1.0 - veins * 0.6);

    // Oil-film colour shifting across the curve.
    if (i.extra.y > 0.0) {
        let film = 0.5 + 0.5 * cos(6.283 * (nz * 1.4 + seed + vec3f(0.0, 0.33, 0.67)));
        col = mix(col, film * (luma(col) * 1.6 + 0.02), clamp(i.extra.y * edge * 2.0, 0.0, 0.8));
    }

    // ---- light of its own
    let signal = i.extra.z;
    let glowc = i.glow.rgb;
    if (tissue == T_GLOW) {
        let pulse = 0.55 + 0.45 * sin(P.time * 1.3 + seed * 20.0);
        col = albedo * 0.2 + glowc * (0.4 + 1.4 * night + 4.0 * signal) * (0.4 + 0.6 * nz) * pulse * alive;
    }
    if (i.glow.w > 0.0 && patterned) {
        // Freckles of glow along the flanks, stronger in the dark.
        let fq = vec2f(u, v * tb.r) / max(i.look.x * 0.35, 1e-4);
        let cell = floor(fq);
        let fc = 0.3 + 0.4 * hash22(cell + seed + 2.0);
        let dot_ = step(1.0 - 0.35 * i.glow.w, hash21(cell + seed + 4.4)) * smoothstep(0.22, 0.05, length(fract(fq) - fc));
        col += glowc * dot_ * (0.15 + 1.8 * night + 3.0 * signal) * alive;
    }
    if (signal > 0.0 && patterned && alive > 0.5) {
        let sp = signal_pattern(i32(i.extra.w + 0.5), u, v, max(i.look.x * 2.0, 0.008), seed);
        col += glowc * sp * signal * (1.2 + 2.0 * night);
    }
    if (tissue == T_SENSE) {
        // Wet black pits that catch a pin of reflected light.
        col += glowc * 0.05 * night;
    }

    // Wounds weep glowing fluid; damaged tissue darkens.
    if (damage > 0.0) {
        let tear = fbm(q * 2.3 + seed * 5.0, 3);
        if (tear < damage * 0.75 - 0.25 && tb.d > tb.r * 0.4) {
            discard;
        }
        let wound = smoothstep(damage * 0.75 - 0.1, damage * 0.75 - 0.25, tear);
        col = mix(col, col * 0.4, damage * 0.6);
        col += vec3f(0.15, 1.4, 0.9) * wound * (0.5 + night);
    }

    // Decomposers fuzz over a corpse, glowing faintly at night.
    if (decay > 0.3 && bone < 0.5) {
        let mold = smoothstep(0.55, 0.75, fbm(q * 4.0 + seed, 3)) * smoothstep(0.3, 0.7, decay);
        col = mix(col, vec3f(0.12, 0.16, 0.06), mold * 0.7);
        col += vec3f(0.3, 1.2, 0.4) * mold * night * 0.5;
    }

    // Sores: angry dark rims around pale weeping centres.
    if (sores > 0.0) {
        let lit = sun * diffuse * 0.6 + amb * 1.3;
        col = mix(col, mix(vec3f(0.16, 0.02, 0.03), vec3f(0.75, 0.66, 0.38), sore_core) * lit, sores * 0.9);
    }

    // Bleeding: a few open wounds on the flank, each with glowing fluid
    // trickling from it down toward the belly.
    let bleed = i.state.w;
    if (bleed > 0.0 && alive > 0.0) {
        let cs = max(tb.r * 1.6, 1e-4);
        let cu = floor(ul / cs);
        let has = step(1.0 - bleed * 0.5, hash21(vec2f(cu, seed + 31.0)));
        let du = abs(ul - (cu + 0.3 + 0.4 * hash21(vec2f(cu, seed + 37.0))) * cs) / tb.r;
        let torn = vnoise(vec2f(ul, v * tb.r) / max(tb.r * 0.15, 1e-4) + seed) - 0.5;
        let gash = smoothstep(0.26, 0.1, length(vec2f(du * 0.8, (n.y - 0.35) * 2.0)) + torn * 0.15);
        let run = smoothstep(0.1, 0.03, du) * smoothstep(0.45, -0.1, n.y) * step(-0.95, n.y);
        let flow = 0.45 + 0.55 * smoothstep(0.3, 0.9, fract(n.y * 3.0 + P.time * 0.5 + hash21(vec2f(cu, 3.0))));
        let wet = has * max(gash, run * flow) * mid;
        col = mix(col, vec3f(0.1, 0.9, 0.6) * (0.5 + night), wet * min(bleed * 1.3, 1.0));
    }

    // A parasite seen inside: pale coils shifting slowly under the skin, plain
    // through thin flesh and as moving ridges through thick.
    if (load > 0.0 && i.state.y > 0.0 && !symbiont && tissue != T_ARMOR) {
        let wu = ul / max(tb.r * 1.2, 1e-4);
        let path = 0.55 * sin(wu * 2.1 + seed * 9.0 + P.time * 0.25) + 0.25 * sin(wu * 4.7 - P.time * 0.4);
        let worm = smoothstep(0.16, 0.06, abs(v - path)) * step(0.3, fract(wu * 0.12 + seed)) * mid;
        let seen = load * i.state.y * worm;
        let wcol = mix(vec3f(0.85, 0.8, 0.6), icol, 0.4) * (sun * 0.4 + sky_light(p.x) + 0.05);
        col = mix(col, mix(col * 0.55, wcol, 0.4 + 0.6 * i.color.a), seen * 0.85);
        col += icol * seen * night * 0.25;
    }

    // A symbiont: a healthy bloom of its own colour across the back.
    if (symbiont && load > 0.0) {
        let lq = vec2f(ul, v * tb.r) / max(tb.r * 0.4, 1e-4);
        let lichen = smoothstep(0.5, 0.7, fbm(lq + seed * 3.0, 3)) * smoothstep(-0.2, 0.6, n.y) * mid;
        col = mix(col, icol * (amb * 1.3 + sun * diffuse * 0.6), lichen * load * 0.7);
        col += icol * lichen * load * night * 0.6;
    }

    var alpha = cover;
    if (cover < 0.5 && hair > 0.0) {
        let hc = albedo * 0.7 * (amb + sun * (0.25 + 0.4 * day)) + sun * edge * 0.1;
        col = mix(hc, col, cover);
        alpha = max(alpha, hair * 0.85);
    }

    col = haze(p, col, i.zinfo.x);
    var o: FOut;
    o.color = vec4f(col * alpha, alpha);
    o.depth = depth_of(i, nz * tb.r);
    if (alpha < 0.02) {
        discard;
    }
    return o;
}

// ---------------------------------------------------------------- organs

@fragment
fn fs_inner(i: LOut) -> FOut {
    let p = i.p;
    if (clipped(i, p)) {
        discard;
    }
    let tissue = i32(i.mat.x + 0.5);
    let seed = i.mat.y;
    let tb = tube(p, i.ab.xy, i.ab.zw, i.cr.z, i.cr.w);
    let px = px_world();
    let cover = smoothstep(tb.r + px, tb.r - px * 2.0, tb.d);
    if (cover <= 0.0) {
        discard;
    }
    let u = mix(i.zinfo.z, i.zinfo.w, tb.h);
    let v = clamp(tb.v, -1.0, 1.0);
    let nz = sqrt(max(1.0 - v * v, 0.0));
    var albedo = i.color.rgb;
    var shape = 1.0;
    if (tissue == T_GUT) {
        // Slow peristalsis travelling down the gut.
        albedo *= 0.6 + 0.6 * smoothstep(-0.3, 0.8, sin((u - i.zinfo.z) / max(tb.r, 1e-4) * 2.0 - P.time * 0.8));
    } else if (tissue == T_BROOD) {
        // A cluster of eggs, each with a dark embryo.
        let q = vec2f(u - i.zinfo.z, v * tb.r) / max(tb.r * 0.7, 1e-4);
        let cell = floor(q);
        let c = 0.35 + 0.3 * hash22(cell + seed);
        let d = length(fract(q) - c);
        shape = smoothstep(0.42, 0.3, d);
        albedo = mix(albedo, albedo * 0.15, smoothstep(0.16, 0.08, d));
    } else if (tissue == T_STORE) {
        let q = vec2f(u - i.zinfo.z, v * tb.r) / max(tb.r * 0.5, 1e-4);
        albedo *= 0.7 + 0.5 * smoothstep(0.3, 0.6, vnoise(q + seed));
    }
    let sun = sun_light(p.x);
    var col = albedo * (sky_light(p.x) * 1.5 + sun * (0.35 + 0.4 * nz));
    let alpha = cover * shape * i.color.a * (0.45 + 0.55 * nz);
    if (alpha < 0.02) {
        discard;
    }
    col = haze(p, col, i.zinfo.x);
    var o: FOut;
    o.color = vec4f(col * alpha, alpha);
    o.depth = depth_of(i, 0.0);
    return o;
}

// ---------------------------------------------------------------- membranes

@fragment
fn fs_membrane(i: LOut) -> FOut {
    let p = i.p;
    if (clipped(i, p)) {
        discard;
    }
    let tissue = i32(i.mat.x + 0.5);
    let seed = i.mat.y;
    let bc = i.bary;
    // Scalloped free edge between ribs, like a wing or fin.
    let along = bc.z / max(bc.y + bc.z, 1e-5);
    let ribs = 2.0 + floor(hash21(vec2f(seed, 1.0)) * 3.0);
    // Rounded lobes between ribs, with a soft notch where each rib ends.
    let scallop = 0.07 * pow(1.0 - abs(sin(along * ribs * 3.14159)), 4.0);
    let from_free = bc.x;
    let fw = fwidth(from_free);
    let edge_cut = smoothstep(scallop, scallop + fw * 1.5, from_free);
    // Ragged tatters on old membranes.
    let tatter = step(0.82, vnoise(vec2f(along * 40.0, seed))) * step(from_free, 0.25);
    let cover = edge_cut * (1.0 - tatter) * smoothstep(0.0, fw * 1.5, min(bc.y, bc.z) + 0.02);
    if (cover <= 0.0) {
        discard;
    }
    // Veins radiate from the root, with fine cross-veins between.
    let vr = abs(fract(along * ribs) - 0.5);
    let vfw = fwidth(along * ribs);
    let vein = smoothstep(vfw * 1.5 + 0.02, 0.0, 0.5 - vr);
    let cross = smoothstep(0.06, 0.0, abs(fract(from_free * 9.0 + along * 2.0) - 0.5) - 0.44) * 0.5;
    let rim = smoothstep(scallop + 0.06, scallop, from_free);

    // The sheet's own texture: fine striations fanning out along the ribs,
    // creased like skin between them, or on a leaf film the grain of its
    // cells. Each fades as it gets too fine for the pixels it covers.
    let fq = vec2f(along * ribs * 14.0, from_free * 6.0);
    let striae = abs(fract(fq.x) - 0.5) * 2.0;
    let sfade = 1.0 - smoothstep(0.25, 0.7, fwidth(fq.x));
    let wq = vec2f(along * ribs * 5.0, from_free * 9.0) + seed;
    let wrinkle = vnoise(wq);
    let wfade = 1.0 - smoothstep(0.25, 0.7, max(fwidth(wq.x), fwidth(wq.y)));
    var texture = 1.0;
    if (tissue == T_FILM) {
        let cq = vec2f(along * ribs * 22.0, from_free * 30.0) + seed;
        let cfade = 1.0 - smoothstep(0.25, 0.7, max(fwidth(cq.x), fwidth(cq.y)));
        texture = 1.0 + (vnoise(cq) - 0.5) * 0.4 * cfade + (wrinkle - 0.5) * 0.2 * wfade;
    } else {
        texture = 1.0 - 0.3 * smoothstep(0.5, 1.0, striae) * sfade + (wrinkle - 0.5) * 0.35 * wfade;
    }

    var albedo = i.color.rgb * texture;
    let night = nightness(p.x);
    let day = daylight(p.x);
    let sun = sun_light(p.x);
    // Sheets are solid, but thin: lit from the front, and from behind as far
    // as sunlight soaks through them.
    let soak = i.color.a;
    var col = albedo * (sun * (0.35 + 0.4 * day) * (1.0 + soak) + sky_light(p.x) * 1.5);
    col *= 1.0 - 0.55 * max(vein, cross);
    col += albedo * sun * rim * 0.4;
    // Night: veins carry a trace of glow.
    col += i.glow.rgb * vein * night * 0.25 * i.mat.w;

    let decay = 1.0 - i.mat.w;
    if (decay > 0.0 && vnoise(p * 900.0 + seed) < decay * 1.2 - 0.3 && vein < 0.5) {
        discard;
    }
    let alpha = cover;
    col = haze(p, col, i.zinfo.x);
    var o: FOut;
    o.color = vec4f(col * alpha, alpha);
    o.depth = depth_of(i, 0.0);
    return o;
}

// ---------------------------------------------------------------- glow halos

@fragment
fn fs_glow(i: LOut) -> FOut {
    let p = i.p;
    if (clipped(i, p)) {
        discard;
    }
    let seed = i.mat.y;
    let tb = tube(p, i.ab.xy, i.ab.zw, i.cr.z, i.cr.w);
    let night = nightness(p.x);
    let pulse = 0.55 + 0.45 * sin(P.time * 1.3 + seed * 20.0);
    let k = tb.d / max(tb.r, 1e-5);
    let halo = exp(-k * k * 0.35) * 0.35 + exp(-k * 0.9) * 0.15;
    let strength = (0.1 + 0.8 * night + 4.0 * i.extra.z) * pulse * i.mat.w;
    var o: FOut;
    o.color = vec4f(i.glow.rgb * halo * strength, 0.0);
    o.depth = depth_of(i, 0.0);
    return o;
}
