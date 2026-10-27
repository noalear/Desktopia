// Bake pass: the slow-changing landscape, rendered rarely into an fp16 texture.
// Alpha carries a material code the frame pass animates:
// 0 rock, 0.25 lava, 0.5 water, 0.75 ice, 1 sky.

const M_ROCK = 0.0;
const M_LAVA = 0.25;
const M_WATER = 0.5;
const M_ICE = 0.75;
const M_SKY = 1.0;

fn haze_color(x: f32, h: f32) -> vec3f {
    let p = vec2f(x, h);
    return sky_base(p) + sun_glow(p) * 0.6;
}

// Rock shading for a terrain layer. depth runs 0 (near) to 1 (far).
fn rock(p: vec2f, top: f32, depth: f32, seed: f32) -> vec3f {
    let t = nightness(p.x);
    let day = daylight(p.x);
    let n = fbm(p * 38.0 + seed, 4);
    let strata = 0.5 + 0.5 * sin(p.y * 260.0 + n * 6.0);
    var base = band3(vec3f(0.16, 0.045, 0.018), vec3f(0.06, 0.02, 0.06), vec3f(0.006, 0.008, 0.022), t);
    base *= 0.55 + 0.7 * n + 0.15 * strata;

    // Small mineral crystals in jewel tones, only up close.
    let cq = p * 260.0;
    let cell = floor(cq);
    let cd = length(fract(cq) - (0.25 + 0.5 * hash22(cell + seed + 9.0)));
    let crystal = step(0.988, hash21(cell + seed)) * smoothstep(0.22, 0.08, cd) * (1.0 - depth) * smoothstep(0.4, 0.6, n);
    let jewel = band3(vec3f(0.05, 0.7, 0.55), vec3f(0.6, 0.08, 0.7), vec3f(0.03, 0.3, 0.7), hash21(cell + 2.0));
    base = mix(base, jewel, crystal * 0.6);

    // The low sun rakes along every ridge top.
    let edge = exp(-(top - p.y) * mix(520.0, 160.0, depth));
    // Strong on the far ranges, faint up close, so near layers don't stack
    // into parallel glowing bands.
    let rim = edge * (day * vec3f(2.4, 0.7, 0.12) + (1.0 - day) * vec3f(0.05, 0.02, 0.14))
        * (1.0 - 0.55 * depth) * mix(0.1, 1.0, smoothstep(0.1, 0.7, depth));

    let lit = 0.35 + 0.9 * day * (1.0 - 0.5 * depth);
    var col = base * lit + rim;

    // Aerial haze dissolves distant layers into the horizon.
    col = mix(col, haze_color(p.x, top + 0.02) * 0.7, clamp(depth * depth * 0.7, 0.0, 0.9));
    return col;
}

fn ground(p: vec2f, gt: f32) -> vec4f {
    let t = nightness(p.x);
    let day = daylight(p.x);
    let rv = river(p.x);
    let dist = abs(p.y - rv.x);
    let pan = smoothstep(0.95, 0.4, p.x);
    let wet = pan_wet(p);

    if (dist < rv.y && wet > 0.5) {
        // Water mirrors the sky, with the sun's glitter path on the day side.
        // In the pan it runs shallow and bright over salt.
        let depth_in = (1.0 - dist / max(rv.y, 1e-4)) * (1.0 - pan * 0.8);
        let refl_h = 0.55 + (rv.x - p.y) * 4.0;
        var col = sky_base(vec2f(p.x, refl_h)) * (0.5 + 0.35 * pan) + vec3f(0.0, 0.07, 0.08) * depth_in;
        col += sun_glow(vec2f(p.x, sun_pos().y)) * 0.35 * exp(-abs(p.x - sun_pos().x) * 6.0);
        col += vec3f(0.03, 0.2, 0.35) * t * depth_in * 0.3; // glacier meltwater glow
        return vec4f(col, M_WATER);
    }

    var col = rock(p, gt, 0.0, 0.0);
    // Wet, overgrown banks.
    let bank = smoothstep(rv.y + 0.012, rv.y, dist) * (1.0 - pan);
    col *= 1.0 - bank * 0.55;
    let moss = band3(vec3f(0.03, 0.12, 0.05), vec3f(0.09, 0.03, 0.12), vec3f(0.01, 0.05, 0.1), t);
    col += moss * smoothstep(0.5, 0.8, fbm(p * 120.0, 3)) * bank * (0.3 + day);
    // The salt pan: a cracked white crust where the river has boiled dry.
    let crust = pan * smoothstep(rv.y * 1.35, rv.y * 0.9, dist + fbm(p * 60.0, 2) * 0.008);
    if (crust > 0.0) {
        let q = p * vec2f(45.0, 150.0);
        let cell = floor(q);
        let f = fract(q);
        var d1 = 9.0;
        var d2 = 9.0;
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let o = vec2f(f32(i), f32(j));
                let d = length(f - o - hash22(cell + o));
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
        let crack = smoothstep(0.06, 0.0, d2 - d1);
        var salt = vec3f(0.62, 0.4, 0.24) * (0.3 + 0.6 * day) * (0.85 + 0.25 * fbm(p * 120.0, 2));
        salt = mix(salt, salt * 0.3, crack);
        col = mix(col, salt, crust * 0.8);
    }
    // Ground gets darker toward the bottom edge of the screen.
    col *= 0.45 + 0.55 * smoothstep(0.0, gt, p.y);
    return vec4f(col, M_ROCK);
}

// The ice wall on the night edge: a mosaic of fractured blocks and facets,
// banded with old snow and dust, cracked deep blue, with meltwater caves at
// the toe. Kept dark: it should glow faintly, not outshine the life.
fn glacier(p: vec2f, top: f32) -> vec4f {
    let x = p.x;
    let below = top - p.y;
    let sb = serac(x);
    let ci = sb.x;
    let f = sb.y;
    let n = fbm(vec2f(x * 30.0, p.y * 60.0), 4);

    // Broken facets: an uneven cell mosaic, each cell tilted to the sky by its own amount.
    let q = vec2f(x * 40.0, p.y * 30.0) + vec2f(fbm(p * 20.0, 2), fbm(p * 20.0 + 3.0, 2)) * 1.5;
    let cell = floor(q);
    let fq = fract(q);
    var d1 = 9.0;
    var d2 = 9.0;
    var id = vec2f(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2f(f32(i), f32(j));
            let c = o + hash22(cell + o);
            let d = length(fq - c);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                id = cell + o;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    let facet = hash21(id + ci * 0.37);
    let seam = smoothstep(0.06, 0.0, d2 - d1);

    let deep = vec3f(0.003, 0.012, 0.04);
    let mid = vec3f(0.018, 0.08, 0.2);
    let lit = vec3f(0.09, 0.22, 0.42);
    let toplight = exp(-below * 14.0);
    var col = mix(deep, mid, 0.25 + 0.55 * facet + 0.2 * (n - 0.5));
    col = mix(col, lit, toplight * (0.3 + 0.4 * facet));
    col = mix(col, deep, seam * 0.3);

    // Annual layers, warped by the flow, with the odd darker dust band.
    let ty = p.y + (x - 3.0) * 0.07 + 0.012 * fbm(vec2f(x * 12.0, p.y * 6.0), 3);
    col *= 0.85 + 0.25 * fbm(vec2f(x * 8.0, ty * 220.0), 3);
    let dust = smoothstep(0.94, 1.0, sin(ty * 90.0 + 1.3)) * smoothstep(0.3, 0.6, fbm(vec2f(x * 20.0, 3.0), 2));
    col = mix(col, vec3f(0.015, 0.012, 0.022), dust * 0.45);

    // Deep cracks where blocks part, running only partway down.
    let edge = min(f, 1.0 - f);
    let crack = smoothstep(0.03, 0.0, edge) * smoothstep(0.0, 0.01, below)
        * smoothstep(0.03 + 0.12 * hash21(vec2f(ci, 4.4)), 0.0, below);
    col = mix(col, vec3f(0.0, 0.008, 0.03), crack * 0.85);
    col += vec3f(0.02, 0.2, 0.4) * crack * 0.12;

    // Snow on every block top.
    col += vec3f(0.35, 0.45, 0.8) * exp(-below * 380.0) * 0.6;

    // Meltwater caves at the toe, lit from inside.
    let foot = p.y - ground_top(x);
    if (hash21(vec2f(ci, 9.9)) > 0.65) {
        let cq = vec2f((f - 0.5) * 0.03, foot);
        let r = 0.01 + 0.006 * hash21(vec2f(ci, 5.5));
        let d = length(cq * vec2f(1.0, 0.8));
        let cave = smoothstep(r, r * 0.8, d) * step(0.0, foot);
        col = mix(col, vec3f(0.0, 0.01, 0.03), cave);
        col += vec3f(0.03, 0.4, 0.6) * smoothstep(r * 0.4, r, d) * cave * 0.4;
    }
    col += vec3f(0.02, 0.2, 0.35) * exp(-max(foot, 0.0) * 70.0) * 0.1;
    return vec4f(col, M_ICE);
}

fn mid_layer(p: vec2f, top: f32) -> vec4f {
    var col = rock(p, top, 0.42, 2.0);
    let v = volcano(p.x);
    if (v < top - 0.0005) {
        return vec4f(col, M_ROCK);
    }
    // Lava channels running down the cone from the crater.
    let below = top - p.y;
    var lava = 0.0;
    for (var k = 0; k < 4; k++) {
        let fk = f32(k);
        let spread = (fk - 1.5) * 0.55;
        let xl = VOLCANO_X + spread * below * 1.1 + 0.008 * sin(below * 70.0 + fk * 2.0);
        let w = 0.0012 + 0.002 * below;
        lava = max(lava, smoothstep(w, w * 0.3, abs(p.x - xl)) * smoothstep(0.34 + fk * 0.03, 0.0, below) * (0.5 + 0.5 * smoothstep(0.2, 0.0, below)));
    }
    col += vec3f(9.0, 2.2, 0.3) * lava;
    // Even the faint edges of a channel are tagged lava, so the frame pass
    // can cool the whole flow when the volcano is quiet.
    return vec4f(col, select(M_ROCK, M_LAVA, lava > 0.02));
}

@fragment
fn fs_bake(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    let x = p.x;

    let gt = ground_top(x);
    if (p.y < gt) {
        return ground(p, gt);
    }

    let nt = near_top(x);
    var behind: vec4f;
    if (p.y < nt) {
        let ice = smoothstep(0.0, 0.14, glacier_lift(x)) * smoothstep(gt + 0.004, gt + 0.02, p.y);
        let r = vec4f(rock(p, nt, 0.15, 1.0), M_ROCK);
        if (ice > 0.0) {
            let g = glacier(p, nt);
            behind = vec4f(mix(r.rgb, g.rgb, ice), select(M_ROCK, M_ICE, ice > 0.5));
        } else {
            behind = r;
        }
    } else {
        let mt = mid_top(x);
        if (p.y < mt) {
            behind = mid_layer(p, mt);
        } else {
            let ft = far_top(x);
            if (p.y < ft) {
                behind = vec4f(rock(p, ft, 0.85, 3.0), M_ROCK);
            } else {
                behind = vec4f(sky_color(p), M_SKY);
            }
        }
    }
    return behind;
}
