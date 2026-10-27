// Frame pass: animates over the baked landscape and tone maps for the display.

@group(0) @binding(1) var baked: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

fn is_mat(m: f32, code: f32) -> bool {
    return abs(m - code) < 0.1;
}

fn clouds(p: vec2f, base: vec3f) -> vec3f {
    let t = nightness(p.x);
    let day = daylight(p.x);
    // High wind carries the cloud deck night-ward.
    let q = vec2f(p.x * 1.4 - P.time * 0.0035, p.y * 5.5);
    let n = fbm(q, 5);
    let band = smoothstep(0.5, 0.62, p.y) * smoothstep(0.98, 0.78, p.y);
    let dens = smoothstep(0.52, 0.78, n) * band;
    let under = smoothstep(0.9, 0.5, p.y);
    let lit = vec3f(1.6, 0.55, 0.18) * day * under + vec3f(0.12, 0.05, 0.22) * (1.0 - t) * 0.5;
    let shadow = band3(vec3f(0.12, 0.03, 0.03), vec3f(0.05, 0.02, 0.08), vec3f(0.004, 0.005, 0.012), t);
    let cc = mix(shadow, lit, smoothstep(0.55, 0.85, n));
    return mix(base, cc, dens * 0.85);
}

fn storm(p: vec2f, base: vec3f) -> vec3f {
    let dx = p.x - P.storm_x;
    let cell = exp(-dx * dx * 14.0);
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
    let rain = column * streak * smoothstep(0.56, 0.45, p.y) * cell;
    col += vec3f(0.08, 0.09, 0.16) * rain;
    // Lightning lights the cloud from inside and drops one forked bolt.
    if (P.lightning > 0.0) {
        let gd = length(vec2f(dx * 1.4, p.y - 0.64));
        col += vec3f(0.9, 0.8, 1.7) * P.lightning * exp(-gd * 13.0) * (0.25 + 1.2 * deck) * cell;
        let bx = P.storm_x + 0.025 * (fbm(vec2f(p.y * 30.0, floor(P.time * 0.05)), 4) - 0.5)
            + 0.04 * (0.55 - p.y);
        let bolt = smoothstep(0.0016, 0.0, abs(p.x - bx)) * step(p.y, 0.56);
        col += vec3f(12.0, 12.0, 20.0) * bolt * P.lightning;
    }
    return col;
}

fn smoke(p: vec2f, base: vec3f) -> vec3f {
    let rise = p.y - VOLCANO_TOP;
    if (rise < -0.01) {
        return base;
    }
    let r = max(rise, 0.0);
    let xc = VOLCANO_X + pow(r, 1.6) * 0.9;
    let w = 0.02 + r * 0.32;
    let across = (p.x - xc) / w;
    let n = fbm(vec2f(across * 1.3, p.y * 9.0 - P.time * 0.03), 5);
    let dens = smoothstep(1.0, 0.0, abs(across)) * smoothstep(0.25, 0.65, n) * smoothstep(0.62, 0.3, r)
        * smoothstep(-0.01, 0.01, rise);
    let glow = vec3f(3.0, 0.8, 0.15) * exp(-r * 25.0);
    let ash = vec3f(0.05, 0.035, 0.035) + glow;
    return mix(base, ash, dens * 0.92);
}

fn stars(p: vec2f) -> vec3f {
    let t = nightness(p.x);
    if (t < 0.55) {
        return vec3f(0.0);
    }
    let g = p * 420.0;
    let cell = floor(g);
    let h = hash21(cell);
    if (h < 0.965) {
        return vec3f(0.0);
    }
    let off = hash22(cell + 3.1);
    let d = length(fract(g) - off);
    let tw = 0.6 + 0.4 * sin(P.time * (0.4 + 2.0 * h) + h * 50.0);
    let s = exp(-d * d * 60.0) * tw * smoothstep(0.55, 0.9, t) * smoothstep(0.35, 0.6, p.y);
    return vec3f(0.8, 0.85, 1.2) * s * (0.6 + 6.0 * (h - 0.965) * 20.0);
}

// Drifting spores and motes. Bioluminescent on the night side.
fn motes(p: vec2f, scale: f32, size: f32, speed: f32, seed: f32) -> vec3f {
    let q = (p + vec2f(P.time * speed * 0.4, P.time * speed)) / scale;
    let cell = floor(q);
    let h = hash21(cell + seed);
    if (h < 0.9) {
        return vec3f(0.0);
    }
    let ph = hash22(cell + seed + 5.0);
    let wob = vec2f(sin(P.time * 0.3 + ph.x * 6.28), cos(P.time * 0.23 + ph.y * 6.28)) * 0.15;
    let d = length((fract(q) - (0.25 + ph * 0.5) - wob) * scale);
    let core = exp(-(d * d) / (size * size));
    let t = nightness(p.x);
    let warm = vec3f(1.2, 0.55, 0.2) * 0.35;
    var cool = vec3f(0.2, 1.6, 1.8);
    if (h > 0.95) {
        cool = vec3f(1.8, 0.3, 1.6);
    }
    let pulse = 0.6 + 0.4 * sin(P.time * (0.5 + h) + h * 40.0);
    return mix(warm, cool * 1.6 * pulse, smoothstep(0.35, 0.8, t)) * core;
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
    } else if (is_mat(m, 0.5)) {
        // Flowing water: ripples travel day-ward, glinting with sun or glow.
        let flow = vec2f(p.x * 70.0 + P.time * 0.35, (p.y - rv.x) * 400.0);
        let rip = fbm(flow, 3);
        let glint = smoothstep(0.62, 0.8, rip);
        let tint = mix(vec3f(3.0, 1.2, 0.3) * day, vec3f(0.2, 1.2, 1.8), t);
        col += tint * glint * 0.35 + col * (rip - 0.5) * 0.4;
    } else if (is_mat(m, 0.25)) {
        col *= 0.75 + 0.35 * sin(P.time * 0.6 + p.y * 90.0 - p.x * 30.0);
    } else if (is_mat(m, 0.75)) {
        let n = fbm(vec2f(p.x * 14.0, p.y * 20.0 + P.time * 0.02), 3);
        col *= 0.85 + 0.3 * n;
        // Meltwater streaks down the ice face.
        let fx = p.x * 140.0;
        let fall = step(0.96, hash21(vec2f(floor(fx), 1.0)))
            * smoothstep(0.25, 0.0, abs(fract(fx) - 0.5))
            * smoothstep(0.3, 1.0, fract(p.y * 40.0 + P.time * 0.6 + hash21(vec2f(floor(fx), 2.0))));
        col += vec3f(0.2, 0.8, 1.4) * fall * 0.25;
    } else if (t > 0.5) {
        // Glow speckles on night-side ground, slowly breathing.
        let gq = p * 160.0;
        let g = floor(gq);
        let h = hash21(g + 0.37);
        if (h > 0.985) {
            let c = 0.2 + 0.6 * hash22(g);
            let d = length(fract(gq) - c) / 160.0;
            let spot = exp(-d * d / (0.0009 * 0.0009));
            let breathe = 0.5 + 0.5 * sin(P.time * (0.3 + h) + h * 90.0);
            col += mix(vec3f(0.2, 1.5, 1.4), vec3f(1.6, 0.3, 1.5), step(0.993, h)) * spot * breathe * 2.5 * (t - 0.5) * 2.0;
        }
    }

    // Mist where the river boils off near the sun.
    let mist_zone = smoothstep(0.1, 0.35, p.x) * smoothstep(1.0, 0.45, p.x)
        * smoothstep(0.1, 0.0, p.y - rv.x - 0.01) * smoothstep(-0.03, 0.0, p.y - rv.x);
    if (mist_zone > 0.001) {
        let mn = fbm(vec2f(p.x * 14.0 + P.time * 0.01, p.y * 30.0 - P.time * 0.06), 4);
        col = mix(col, vec3f(1.6, 0.75, 0.35), smoothstep(0.4, 0.85, mn) * mist_zone * 0.35);
    }

    col += motes(p, 0.03, 0.0011, 0.0022, 0.0);
    col += motes(p, 0.07, 0.0028, 0.0011, 31.0) * 0.5;

    return finish(col, in.pos.xy);
}
