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
    let rim = edge * (day * vec3f(2.4, 0.7, 0.12) + (1.0 - day) * vec3f(0.05, 0.02, 0.14)) * (1.0 - 0.55 * depth);

    let lit = 0.35 + 0.9 * day * (1.0 - 0.5 * depth);
    var col = base * lit + rim;

    // Aerial haze dissolves distant layers into the horizon.
    col = mix(col, haze_color(p.x, top + 0.02) * 0.7, clamp(depth * depth * 0.7, 0.0, 0.9));
    return col;
}

// Alien plants along the top edge of the ground plane. Returns colour + coverage.
fn plants(p: vec2f, gt: f32) -> vec4f {
    let cell_w = 0.0042;
    let cx = floor(p.x / cell_w);
    let r = hash21(vec2f(cx, 7.0));
    let clump = fbm(vec2f(cx * 0.03, 1.0), 2);
    if (r < 0.75 - clump * 0.6) {
        return vec4f(0.0);
    }
    let height = 0.006 + 0.03 * hash21(vec2f(cx, 3.0)) * (0.4 + clump);
    let y = (p.y - gt) / height;
    if (y < 0.0 || y > 1.15) {
        return vec4f(0.0);
    }
    let sway = 0.22 * sin(y * 3.0 + r * 6.28) * y;
    let local = (fract(p.x / cell_w) - 0.5 - sway) * cell_w;
    let half_w = cell_w * 0.12 * (1.0 - y * 0.75);
    let stalk = smoothstep(half_w, half_w * 0.4, abs(local)) * step(y, 1.0);
    let pod_r = 0.0007 + 0.0009 * r;
    let pod_q = vec2f(local, (y - 0.97) * height * 0.35);
    let bulb = smoothstep(pod_r, pod_r * 0.5, length(pod_q)) * step(0.5, hash21(vec2f(cx, 5.0)));
    let cover = max(stalk, bulb);
    if (cover <= 0.0) {
        return vec4f(0.0);
    }

    let t = nightness(p.x);
    let day = daylight(p.x);
    let kind = hash21(vec2f(cx, 11.0));
    var body = vec3f(0.04, 0.22, 0.12);
    var glow = vec3f(0.2, 1.6, 0.9);
    if (kind > 0.66) {
        body = vec3f(0.2, 0.04, 0.22);
        glow = vec3f(1.8, 0.3, 2.2);
    } else if (kind > 0.33) {
        body = vec3f(0.3, 0.1, 0.02);
        glow = vec3f(2.4, 0.9, 0.2);
    }
    var col = body * (0.25 + 0.9 * day) + vec3f(2.0, 0.7, 0.15) * day * smoothstep(0.6, 1.0, y) * 0.5;
    col += glow * bulb * (0.15 + 1.6 * t);
    return vec4f(col, cover);
}

fn ground(p: vec2f, gt: f32) -> vec4f {
    let t = nightness(p.x);
    let day = daylight(p.x);
    let rv = river(p.x);
    let dist = abs(p.y - rv.x);

    if (dist < rv.y) {
        // Water mirrors the sky, with the sun's glitter path on the day side.
        let depth_in = 1.0 - dist / max(rv.y, 1e-4);
        let refl_h = 0.55 + (rv.x - p.y) * 4.0;
        let taper = smoothstep(0.0, 0.004, rv.y);
    var col = sky_base(vec2f(p.x, refl_h)) * 0.5 * taper + vec3f(0.0, 0.07, 0.08) * depth_in;
        col += sun_glow(vec2f(p.x, sun_pos().y)) * 0.35 * exp(-abs(p.x - sun_pos().x) * 6.0);
        col += vec3f(0.05, 0.35, 0.6) * t * depth_in * 0.4; // glacier meltwater glow
        return vec4f(col, M_WATER);
    }

    var col = rock(p, gt, 0.0, 0.0);
    // Wet, overgrown banks.
    let bank = smoothstep(rv.y + 0.012, rv.y, dist) * smoothstep(0.0, 0.004, rv.y);
    col *= 1.0 - bank * 0.55;
    let moss = band3(vec3f(0.03, 0.12, 0.05), vec3f(0.09, 0.03, 0.12), vec3f(0.01, 0.05, 0.1), t);
    col += moss * smoothstep(0.5, 0.8, fbm(p * 120.0, 3)) * bank * (0.3 + day);
    // Ground gets darker toward the bottom edge of the screen.
    col *= 0.45 + 0.55 * smoothstep(0.0, gt, p.y);
    return vec4f(col, M_ROCK);
}

fn glacier(p: vec2f, top: f32) -> vec4f {
    let below = top - p.y;
    let n = fbm(vec2f(p.x * 7.0, p.y * 12.0), 4);
    let crevasse = smoothstep(0.03, 0.0, abs(fract(p.x * 11.0 + n * 2.0) - 0.5)) * smoothstep(0.0, 0.06, below)
        * step(0.45, hash21(vec2f(floor(p.x * 11.0 + n * 2.0), 4.0)));
    // Shade from a smoothed crest so the jagged top doesn't streak the face.
    let crest = 0.24 + 0.24 * smoothstep(3.02, 3.32, p.x);
    let depth_glow = exp(-max(crest - p.y, 0.0) * 9.0);
    var col = mix(vec3f(0.003, 0.01, 0.035), vec3f(0.02, 0.14, 0.34), depth_glow) * (0.6 + 0.8 * n);
    col += vec3f(0.05, 0.5, 1.2) * crevasse * (0.3 + n) * 0.5;
    col += vec3f(0.5, 1.2, 2.0) * exp(-below * 260.0) * 0.7;
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
        lava = max(lava, smoothstep(w, w * 0.3, abs(p.x - xl)) * smoothstep(0.16 + fk * 0.02, 0.0, below));
    }
    let crater = exp(-length(vec2f((p.x - VOLCANO_X) * 2.5, below)) * 45.0);
    col += vec3f(9.0, 2.2, 0.3) * lava + vec3f(4.0, 1.0, 0.15) * crater;
    return vec4f(col, select(M_ROCK, M_LAVA, lava > 0.25));
}

@fragment
fn fs_bake(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    let x = p.x;

    let gt = ground_top(x);
    if (p.y < gt) {
        return ground(p, gt);
    }

    let pl = plants(p, gt);
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
    if (pl.a > 0.0) {
        return vec4f(mix(behind.rgb, pl.rgb, pl.a), select(behind.a, M_ROCK, pl.a > 0.5));
    }
    return behind;
}
