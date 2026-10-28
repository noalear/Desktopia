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

@group(0) @binding(1) var baked: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

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
    let pad = rmax * (1.0 + inst.look.z) + 2.0 * px_world();
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
    return mix(col, hz * 0.7, clamp(z * z * 0.55, 0.0, 0.6));
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
    let tb = tube(p, i.ab.xy, i.ab.zw, i.cr.z, i.cr.w);
    let px = px_world();
    let u = mix(i.zinfo.z, i.zinfo.w, tb.h);
    // Anything sized by the local radius counts from this segment's own
    // start: the chain's running coordinate is large, and dividing it by a
    // tapering radius makes it race and shimmer.
    let ul = u - i.zinfo.z;

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
    let nz = sqrt(max(1.0 - v * v, 0.0));
    // Skin grain: wrinkles across the body and fine pores, as normal wobble.
    let gq = vec2f(ul, v * tb.r) / max(tb.r * 0.35, px * 3.0);
    let grain = vec2f(vnoise(gq * vec2f(0.6, 2.2) + seed), vnoise(gq * 1.7 + seed + 9.0)) - 0.5;
    let n = normalize(vec3f(tb.nrm * v + vec2f(grain.x, grain.y) * 0.22 * nz, nz));
    let L = light_dir(p);

    // ---- albedo and pattern
    var albedo = i.color.rgb * (0.85 + 0.3 * vnoise(gq * 0.5 + seed * 2.0));
    let kind = i32(i.accent.w + 0.5);
    let q = vec2f(u, v * tb.r) / max(i.look.x, 1e-4);
    let patterned = tissue == T_MUSCLE || (tissue == T_FRAME && tb.r > 0.003);
    if (patterned) {
        albedo = mix(albedo, i.accent.rgb, pattern(kind, q, seed) * i.look.w);
        // Dark backs, pale bellies.
        let up = n.y * 0.5 + 0.5;
        albedo *= mix(1.0, mix(1.5, 0.55, up), i.extra.x);
    }
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
    if (tissue == T_FILM) {
        let vein = smoothstep(0.08, 0.0, abs(fract(ul / max(tb.r * 0.8, 1e-4)) - 0.5) - 0.42);
        albedo = mix(albedo, albedo * 0.35, vein);
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
    let diffuse = max(dot(n, L), 0.0);
    let amb = mix(bounce_light(p.x), sky_light(p.x), n.y * 0.5 + 0.5);
    // Key from the sun, plus the twilight sky wrapped around the front.
    let fill = sky_light(p.x) * (0.6 + 0.9 * nz) + sun * 0.12;
    // Joints sit in their own shadow.
    let ao = 1.0 - 0.3 * (smoothstep(0.2, 0.0, tb.h) + smoothstep(0.8, 1.0, tb.h)) * nz;
    var col = albedo * (sun * diffuse * (0.45 + 0.55 * day) + amb * 1.3 + fill * 0.8) * ao;

    // Warm rim on the edge facing the sun, cool rim from the sky on the night side.
    let edge = pow(1.0 - nz, 2.5);
    let toward = max(dot(normalize(tb.nrm * sign(v + 1e-6)), normalize(L.xy)), 0.0);
    col += edge * toward * sun * (0.6 + 0.6 * day) * mix(vec3f(1.0), albedo * 3.0 + 0.3, 0.35);
    col += edge * max(n.y, 0.0) * vec3f(0.06, 0.14, 0.34) * night * 1.4;

    // Lacquer and wet sheen.
    let sheen = i.look.y;
    if (sheen > 0.0) {
        let H = normalize(L + vec3f(0.0, 0.0, 1.0));
        let spec = pow(max(dot(n, H), 0.0), 12.0 + sheen * 90.0) * sheen;
        col += spec * (sun * 0.9 + sky_light(p.x) * 2.0);
    }

    // Light glowing through thin flesh.
    let trans = i.color.a;
    col += albedo * sun * trans * (1.0 - nz * 0.5) * (0.4 + 0.6 * day) * 0.9;

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

    // ---- hair
    // Thin flesh shows the land behind it, bent a little by the curve and
    // tinted by the body. Bodies stay opaque so overlapping limbs don't double up.
    var see = trans * pow(nz, 1.4) * 0.85;
    if (tissue == T_FLOAT) {
        see = 0.55 + 0.4 * nz;
    }
    if (see > 0.0) {
        let uv = (i.pos.xy + n.xy * tb.r * P.world_size.y * 0.35) / P.view_size;
        let bg = textureSampleLevel(baked, samp, uv, 0.0).rgb;
        let tint = mix(vec3f(1.0), albedo / max(max(albedo.r, max(albedo.g, albedo.b)), 1e-3), 0.6);
        col = mix(col, bg * tint * 0.9 + col * 0.15, see);
    }
    // Marks on and in the body go over the see-through, so they show on
    // glassy creatures too. Sores: angry dark rims around pale weeping centres.
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

    var albedo = i.color.rgb;
    let night = nightness(p.x);
    let day = daylight(p.x);
    let sun = sun_light(p.x);
    // Sheets are lit mostly from behind: sunlight soaks through them.
    var col = albedo * (sun * (0.55 + 0.6 * day) + sky_light(p.x) * 1.5);
    col *= 1.0 - 0.55 * max(vein, cross);
    col += sun * rim * 0.25;
    if (tissue == T_FILM) {
        col += albedo * sun * 0.5 * (1.0 - vein);
    }
    // Night: veins carry a trace of glow.
    col += i.glow.rgb * vein * night * 0.25 * i.mat.w;

    let decay = 1.0 - i.mat.w;
    if (decay > 0.0 && vnoise(p * 900.0 + seed) < decay * 1.2 - 0.3 && vein < 0.5) {
        discard;
    }
    var alpha = cover * clamp(i.color.a + 0.35 * max(vein, rim), 0.0, 0.95);
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
