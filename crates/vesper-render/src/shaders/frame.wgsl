// Frame pass: animates the baked landscape into the linear HDR scene.

@group(0) @binding(1) var baked: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

fn is_mat(m: f32, code: f32) -> bool {
    return abs(m - code) < 0.1;
}

fn clouds(p: vec2f, base: vec3f) -> vec3f {
    let t = nightness(p.x);
    let day = daylight(p.x);
    // High wind carries the cloud deck night-ward.
    let q = vec2f((p.x - P.cloud_drift) * 1.4, p.y * 5.5);
    let n = fbm(q, 5);
    let band = smoothstep(0.5, 0.62, p.y) * smoothstep(0.98, 0.78, p.y);
    let dens = smoothstep(0.52, 0.78, n) * band;
    let under = smoothstep(0.9, 0.5, p.y);
    let lit = vec3f(1.6, 0.55, 0.18) * day * under + vec3f(0.12, 0.05, 0.22) * (1.0 - t) * 0.5;
    let shadow = band3(vec3f(0.12, 0.03, 0.03), vec3f(0.05, 0.02, 0.08), vec3f(0.004, 0.005, 0.012), t);
    // Ash from an eruption dulls the deck and thickens it.
    let cc = mix(mix(shadow, lit, smoothstep(0.55, 0.85, n)), shadow * 0.6, P.ash * 0.5);
    return mix(base, cc, min(dens * (0.85 + 0.3 * P.ash), 0.95));
}

fn storm(p: vec2f, base: vec3f) -> vec3f {
    let dx = p.x - P.storm_x;
    let cell = exp(-dx * dx * 14.0) * P.storm;
    if (cell < 0.0005) {
        return base;
    }
    var col = base;
    // A dark cloud tower with a flat base.
    let n = fbm(vec2f(p.x * 3.0 - P.time * 0.004, p.y * 7.0), 5);
    let tower = smoothstep(0.98, 0.6, p.y + dx * dx * 6.0);
    let deck = smoothstep(0.5, 0.6, p.y) * tower * smoothstep(0.3, 0.55, n) * cell;
    col = mix(col, vec3f(0.01, 0.008, 0.025), deck * 0.95);
    // Rain curtains below it.
    let rp = vec2f((p.x + p.y * 0.18) * 520.0, p.y * 22.0 + P.time * 1.6);
    let column = step(0.72, hash21(vec2f(floor(rp.x), 3.0)));
    let streak = smoothstep(0.75, 1.0, fract(rp.y + hash21(vec2f(floor(rp.x), 9.0)) * 7.0));
    let rain = column * streak * smoothstep(0.56, 0.45, p.y) * smoothstep(0.25, 0.6, cell);
    col += vec3f(0.08, 0.09, 0.16) * rain;
    // Lightning lights the cloud from inside and drops one forked bolt.
    if (P.lightning > 0.0) {
        let gd = length(vec2f((p.x - P.lightning_x) * 1.4, p.y - 0.64));
        col += vec3f(0.9, 0.8, 1.7) * P.lightning * exp(-gd * 13.0) * (0.25 + 1.2 * deck) * cell;
        let bx = P.lightning_x + 0.025 * (fbm(vec2f(p.y * 30.0, floor(P.time * 0.05)), 4) - 0.5)
            + 0.04 * (0.55 - p.y);
        let bolt = smoothstep(0.0016, 0.0, abs(p.x - bx)) * step(p.y, 0.56) * step(0.5, P.storm);
        col += vec3f(12.0, 12.0, 20.0) * bolt * P.lightning;
    }
    return col;
}

// The volcano's plume follows its mood: a thin steam wisp when quiet, a
// darkening column as it rumbles, a thick ash tower with a lava fountain
// when it erupts.
fn smoke(p: vec2f, base: vec3f) -> vec3f {
    let v = P.volcano;
    let rise = p.y - VOLCANO_TOP;
    if (rise < -0.01) {
        return base;
    }
    let r = max(rise, 0.0);
    let height = mix(0.22, 0.62, smoothstep(0.0, 0.8, v));
    let xc = VOLCANO_X + pow(r, 1.6) * mix(1.3, 0.7, v);
    let w = mix(0.008, 0.03, v) + r * mix(0.18, 0.4, v);
    let across = (p.x - xc) / w;
    let n = fbm(vec2f(across * 1.3, p.y * 9.0 - P.time * mix(0.02, 0.06, v)), 5);
    let dens = smoothstep(1.0, 0.0, abs(across)) * smoothstep(mix(0.45, 0.25, v), 0.65, n) * smoothstep(height, height * 0.4, r)
        * smoothstep(-0.01, 0.01, rise);
    let t = nightness(p.x);
    let steam = mix(vec3f(0.55, 0.32, 0.3), vec3f(0.12, 0.05, 0.12), t) * (0.5 + 0.5 * n);
    let ash = vec3f(0.035, 0.028, 0.03) * (0.6 + 0.8 * n);
    // Only the foot of the plume is lit by the crater.
    let glow = vec3f(3.0, 0.8, 0.15) * exp(-r * mix(70.0, 35.0, v)) * (0.08 + 1.2 * v);
    var col = mix(base, mix(steam, ash, smoothstep(0.1, 0.5, v)) + glow, dens * mix(0.45, 0.95, smoothstep(0.0, 0.5, v)));
    // Lava fountain: incandescent spray thrown up from the crater.
    if (v > 0.5 && r < 0.1) {
        let q = vec2f((p.x - VOLCANO_X) * 70.0, r * 40.0 - P.time * 3.0);
        let spray = smoothstep(0.62, 0.8, fbm(q, 3)) * smoothstep(0.012 + r * 0.6, 0.0, abs(p.x - VOLCANO_X))
            * smoothstep(0.1 * v, 0.0, r);
        col += vec3f(10.0, 3.0, 0.5) * spray * (v - 0.5) * 2.0;
    }
    return col;
}

// How frosted the ground is: behind the cold fall's front, back toward the ice.
fn frost(p: vec2f) -> f32 {
    if (P.cold < 0.01) {
        return 0.0;
    }
    let dx = p.x - P.cold_x + 0.08 * (fbm(vec2f(min(p.y, ground_top(p.x)) * 30.0, p.x * 3.0), 3) - 0.5);
    return P.cold * smoothstep(0.0, 0.18, dx) * exp(-max(dx - 0.3, 0.0) * 1.5);
}

// Light from the crater on the rock and air around it.
fn crater_glow(p: vec2f) -> vec3f {
    let d = length(vec2f((p.x - VOLCANO_X) * 2.5, VOLCANO_TOP - 0.012 - p.y));
    return vec3f(4.0, 1.0, 0.15) * exp(-d * 45.0) * (0.06 + 1.2 * P.volcano);
}

// Two scattered layers on an integer hash, thinning and clumping with a
// slow noise so no pattern repeats across the sky.
fn stars(p: vec2f) -> vec3f {
    let t = nightness(p.x);
    if (t < 0.55) {
        return vec3f(0.0);
    }
    let fade = smoothstep(0.55, 0.9, t) * smoothstep(0.35, 0.6, p.y);
    let clump = fbm(p * 3.0 + 11.0, 3);
    var acc = vec3f(0.0);
    for (var layer = 0; layer < 2; layer++) {
        let fl = f32(layer);
        let scale = mix(240.0, 620.0, fl);
        let g = p * scale + fl * 37.0;
        let cell = floor(g);
        let r = pcg2(cell + fl * 7919.0);
        let density = mix(0.035, 0.09, fl) * smoothstep(0.25, 0.75, clump) * 1.6;
        if (r.x > density) {
            continue;
        }
        let off = pcg2(cell + 31.0 + fl * 101.0);
        let d = length(fract(g) - (0.2 + 0.6 * off));
        let tw = 0.65 + 0.35 * sin(P.time * (0.3 + 1.5 * off.x) + off.y * 60.0);
        let size = mix(55.0, 120.0, fl);
        let bright = mix(1.6, 0.5, fl) * (0.3 + 1.4 * r.y * r.y);
        let tint = mix(vec3f(1.0, 0.8, 0.7), vec3f(0.7, 0.85, 1.25), off.x);
        acc += tint * exp(-d * d * size) * tw * bright;
    }
    return acc * fade;
}

@fragment
fn fs_frame(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    let t = nightness(p.x);
    let day = daylight(p.x);

    // Heat shimmer over the scorched horizon.
    var uv = in.uv;
    let shimmer = day * day * smoothstep(0.45, 0.3, p.y) * smoothstep(0.12, 0.26, p.y);
    if (shimmer > 0.001) {
        let n = vnoise(vec2f(p.x * 90.0, p.y * 140.0 - P.time * 1.8)) - 0.5;
        uv.x += n * 0.0012 * shimmer * P.world_size.y / P.view_size.x;
        uv.y += n * 0.0006 * shimmer * P.world_size.y / P.view_size.y;
    }
    let b = textureSampleLevel(baked, samp, uv, 0.0);
    var col = b.rgb;
    let m = b.a;

    let rv = river(p.x);
    if (is_mat(m, 1.0)) {
        col += stars(p);
        col = clouds(p, col);
        col = smoke(p, col);
        col = storm(p, col);
        // Snow blowing off the ice wall on the night wind.
        let above_ice = p.y - near_top(p.x);
        if (p.x > 3.0 && above_ice < 0.05) {
            let drift = fbm(vec2f(p.x * 50.0 - P.time * 0.25, p.y * 140.0), 4);
            col += vec3f(0.25, 0.35, 0.6) * smoothstep(0.55, 0.8, drift) * exp(-above_ice * 70.0)
                * smoothstep(3.0, 3.2, p.x) * 0.5;
        }
    } else if (is_mat(m, 0.5)) {
        // Flowing water: ripples travel day-ward, glinting with sun or glow.
        let flow = vec2f(p.x * 70.0 + P.time * 0.35, (p.y - rv.x) * 400.0);
        let rip = fbm(flow, 3);
        let glint = smoothstep(0.62, 0.8, rip);
        let tint = mix(vec3f(3.0, 1.2, 0.3) * day, vec3f(0.2, 1.2, 1.8), t);
        col += tint * glint * 0.35 + col * (rip - 0.5) * 0.4;
    } else if (is_mat(m, 0.25)) {
        // Flows glow as far as the lava has run, brightest at the front,
        // cooling as the eruption dies and crusting over when it is quiet.
        let below = volcano(p.x) - p.y;
        let run = max(abs(p.x - VOLCANO_X), below * 0.8);
        let reach = smoothstep(P.lava + 0.01, P.lava - 0.025, run) * smoothstep(0.0, 0.03, P.lava);
        let front = exp(-pow((run - P.lava) / 0.012, 2.0)) * reach;
        let heat = mix(0.03, 1.0, reach * (0.45 + 0.55 * smoothstep(0.0, 0.4, P.volcano))) + front * 0.8;
        col *= heat * (0.75 + 0.35 * sin(P.time * 0.6 + p.y * 90.0 - p.x * 30.0));
    } else if (is_mat(m, 0.75)) {
        let n = fbm(vec2f(p.x * 14.0, p.y * 20.0 + P.time * 0.02), 3);
        col *= 0.85 + 0.3 * n;
        // A few meltwater threads falling from the lowest ledges.
        let fx = p.x * 90.0;
        let col_id = floor(fx);
        let foot = p.y - ground_top(p.x);
        let fall = step(0.93, hash21(vec2f(col_id, 1.0)))
            * smoothstep(0.2, 0.0, abs(fract(fx) - 0.5))
            * smoothstep(0.06, 0.0, foot)
            * (0.5 + 0.5 * smoothstep(0.3, 1.0, fract(p.y * 60.0 + P.time * 0.8 + hash21(vec2f(col_id, 2.0)))));
        col += vec3f(0.2, 0.7, 1.1) * fall * 0.35;
    } else if (t > 0.5 && frost(p) < 0.05) {
        // Glow speckles on night-side ground, slowly breathing.
        let gq = p * 160.0;
        let g = floor(gq);
        let h = hash21(g + 0.37);
        if (h > 0.992) {
            let c = 0.2 + 0.6 * hash22(g);
            let d = length(fract(gq) - c) / 160.0;
            let spot = exp(-d * d / (0.0009 * 0.0009));
            let breathe = 0.5 + 0.5 * sin(P.time * (0.3 + h) + h * 90.0);
            col += mix(vec3f(0.2, 1.5, 1.4), vec3f(1.6, 0.3, 1.5), step(0.993, h)) * spot * breathe * 1.2 * (t - 0.5) * 2.0;
        }
    }

    col += crater_glow(p);

    // Hoarfrost where a cold fall has passed over the plain and the water.
    let fr = frost(p);
    if (fr > 0.002 && (is_mat(m, 0.0) || is_mat(m, 0.5)) && p.y < ground_top(p.x) + 0.004) {
        // A cool bloom over the ground, gathered in hollows, with glints.
        let n = fbm(p * vec2f(40.0, 110.0), 3);
        let rime = band3(vec3f(1.2, 0.85, 0.8), vec3f(0.5, 0.45, 0.75), vec3f(0.06, 0.08, 0.16), t);
        col = mix(col, rime * (0.5 + 0.5 * n), fr * smoothstep(0.3, 0.7, n) * 0.4);
        let g = floor(p * 900.0);
        let glint = step(0.996, hash21(g)) * (0.5 + 0.5 * sin(P.time * 2.0 + hash21(g + 3.0) * 40.0));
        col += rime * glint * fr * 1.5;
    }

    // Mist where the river boils off near the sun.
    let mist_zone = smoothstep(0.1, 0.35, p.x) * smoothstep(1.0, 0.45, p.x)
        * smoothstep(0.1, 0.0, p.y - rv.x - 0.01) * smoothstep(-0.03, 0.0, p.y - rv.x);
    if (mist_zone > 0.001) {
        let mn = fbm(vec2f(p.x * 14.0 + P.time * 0.01, p.y * 30.0 - P.time * 0.06), 4);
        col = mix(col, vec3f(1.6, 0.75, 0.35), smoothstep(0.4, 0.85, mn) * mist_zone * 0.35);
    }

    return vec4f(col, 1.0);
}
