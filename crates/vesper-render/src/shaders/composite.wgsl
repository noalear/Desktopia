// Composite pass: the finished scene, with spores drifting in front of
// everything, tone mapped for the display.

@group(0) @binding(1) var scene: texture_2d<f32>;

// Drifting spores and motes. Bioluminescent on the night side.
fn motes(p: vec2f, scale: f32, size: f32, speed: f32, seed: f32) -> vec3f {
    let q = (p + vec2f(P.time * speed * 0.4, P.time * speed)) / scale;
    let cell = floor(q);
    let h = hash21(cell + seed);
    if (h < 0.94) {
        return vec3f(0.0);
    }
    let ph = hash22(cell + seed + 5.0);
    let wob = vec2f(sin(P.time * 0.3 + ph.x * 6.28), cos(P.time * 0.23 + ph.y * 6.28)) * 0.15;
    let d = length((fract(q) - (0.25 + ph * 0.5) - wob) * scale);
    let core = exp(-(d * d) / (size * size));
    let t = nightness(p.x);
    let warm = vec3f(1.2, 0.55, 0.2) * 0.35;
    let cool = vec3f(0.25, 0.7, 0.8);
    let pulse = 0.6 + 0.4 * sin(P.time * (0.5 + h) + h * 40.0);
    return mix(warm, cool * 0.6 * pulse, smoothstep(0.35, 0.8, t)) * core;
}

// Weather that hangs in the air, over the land and the life on it alike.
// Each returns a colour and how much it covers.

// A dust storm: a towering wall with a sharp front on its night side and a
// long thinning tail, churning as it rolls.
fn dust_air(p: vec2f) -> vec4f {
    let dx = p.x - P.dust_x;
    if (P.dust < 0.01 || dx < -1.3 || dx > 0.4) {
        return vec4f(0.0);
    }
    let ragged = 0.05 * (fbm(vec2f(p.y * 9.0, P.time * 0.02), 3) - 0.5);
    let along = select(exp(-dx * dx / 0.25), smoothstep(0.24, 0.04, dx + ragged), dx > 0.0);
    let billow = fbm(vec2f(p.x * 6.0 - P.time * 0.0015, P.time * 0.003 + 4.0), 4);
    // Lumpy, cauliflower billows along the top.
    let lumps = fbm(vec2f(p.x * 22.0 - P.time * 0.002, p.y * 10.0 + P.time * 0.006), 4);
    let top = 0.17 + (0.08 + 0.26 * P.dust) * sqrt(along) + 0.07 * (billow - 0.5) + 0.035 * (lumps - 0.5);
    let churn = fbm(vec2f(p.x * 9.0 + P.time * P.wind_low * 3.0, p.y * 12.0 - P.time * 0.012), 4);
    let dens = smoothstep(top, top - 0.035, p.y) * along * P.dust * (0.55 + 0.7 * churn);
    // The sun catches the upper billows; the base is dark and thick.
    let lit = smoothstep(top - 0.16, top, p.y) * daylight(p.x);
    let shade = mix(vec3f(0.16, 0.05, 0.015), vec3f(1.5, 0.62, 0.2), lit) * (0.55 + 0.6 * churn);
    let twi = vec3f(0.3, 0.08, 0.12) * nightness(p.x);
    return vec4f(shade + twi, clamp(dens * 1.2, 0.0, 0.93));
}

// A cold fall: dense air spilling off the glacier and creeping day-ward
// along the plain, with a raised head at its front like any gravity current.
fn cold_air(p: vec2f) -> vec4f {
    let dx = p.x - P.cold_x;
    if (P.cold < 0.01 || dx < -0.12) {
        return vec4f(0.0);
    }
    // Ragged across the plain's depth; above the plain it stands as one bank.
    let floor_y = min(p.y, ground_top(p.x));
    let wob = 0.08 * (fbm(vec2f(floor_y * 30.0, p.x * 3.0), 3) - 0.5) + 0.025 * (fbm(vec2f(p.y * 25.0, P.time * 0.03), 3) - 0.5);
    let edge = smoothstep(-0.05, 0.03, dx + wob);
    let head = exp(-pow((dx - 0.05) / 0.07, 2.0));
    let body = exp(-max(dx - 0.1, 0.0) * 0.9);
    // Long wisps streaming day-ward along the ground.
    let curl = fbm(vec2f(p.x * 9.0 + P.time * 0.006, p.y * 70.0 + P.time * 0.01), 4);
    let top = ground_top(p.x) + P.cold * (0.018 + 0.022 * head) * (0.7 + 0.6 * curl) * body;
    var dens = smoothstep(top, top - 0.02, p.y) * edge * body * P.cold;
    // Thinner toward the front of the plain, where less of it lies between
    // the viewer and the ground.
    dens *= 0.45 + 0.55 * smoothstep(0.0, 0.12, p.y);
    let col = band3(vec3f(1.1, 0.7, 0.6), vec3f(0.45, 0.32, 0.6), vec3f(0.05, 0.08, 0.17), nightness(p.x));
    return vec4f(col * (0.7 + 0.5 * curl), clamp(dens * (0.5 + 0.7 * curl), 0.0, 0.85));
}

// Ash: a brown pall spreading from the crater into a high sheet that the
// high wind drags night-ward, with a thinner haze dimming everything under it.
fn ash_air(p: vec2f) -> vec4f {
    if (P.ash < 0.01) {
        return vec4f(0.0);
    }
    let dx = p.x - VOLCANO_X;
    let spread = select(exp(dx * 4.0), exp(-dx * 0.6), dx > 0.0);
    let n = fbm(vec2f((p.x - P.cloud_drift * 1.3) * 2.2, p.y * 9.0 - P.time * 0.003), 5);
    let sheet = smoothstep(0.5, 0.64, p.y + 0.06 * n) * smoothstep(1.0, 0.72, p.y - 0.05 * n);
    let dens = P.ash * spread * (sheet * (0.3 + 1.1 * n) + 0.22);
    // Lit orange from below near the crater and by the low sun.
    let crater = exp(-length(vec2f(dx * 1.5, p.y - VOLCANO_TOP)) * 9.0) * P.volcano;
    let col = vec3f(0.045, 0.03, 0.028) * (0.5 + 0.9 * n) + vec3f(0.9, 0.22, 0.04) * crater
        + vec3f(0.35, 0.1, 0.03) * daylight(p.x) * smoothstep(0.7, 0.9, n);
    return vec4f(col, clamp(dens * 1.1, 0.0, 0.9));
}

fn over(col: vec3f, layer: vec4f) -> vec3f {
    return mix(col, layer.rgb, layer.a);
}

@fragment
fn fs_composite(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    var col = textureLoad(scene, vec2i(in.pos.xy), 0).rgb;
    col = over(col, ash_air(p));
    col = over(col, cold_air(p));
    col = over(col, dust_air(p));
    col += motes(p, 0.03, 0.0008, 0.0022, 0.0) * 0.6;
    return finish(col, in.pos.xy);
}
