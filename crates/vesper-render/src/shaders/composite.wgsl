// Composite pass: the finished scene, with spores drifting in front of
// everything, tone mapped for the display.

@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
// Bloom at a quarter and a sixteenth of the screen (see bloom.wgsl).
@group(0) @binding(3) var bloom_near: texture_2d<f32>;
@group(0) @binding(4) var bloom_wide: texture_2d<f32>;

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
    let churn = fbm(vec2f(p.x * 9.0 + P.ground_drift * 3.0, p.y * 12.0 - P.time * 0.012), 4);
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

// How strongly the bloom shows.
const BLOOM = 0.12;

// ---------------------------------------------------------------- near weather
// What is falling right in front of the viewer, in a few layers from near
// (big, fast) to far (fine, slow), each moving along its own path so the
// layers never line up into a pattern.

// The light falling things catch at p: the sky's, and the sun's from behind.
fn air_light(p: vec2f) -> vec3f {
    return sky_base(vec2f(p.x, 0.6)) * 0.7 + sun_glow(p) * 0.25 + vec3f(0.012, 0.014, 0.026);
}

// How far a layer has fallen, wrapped every WRAP cells of height `cell` so
// it stays small enough to keep full precision however long the clock runs;
// the cells' hashes wrap with it, so the wrap never shows.
const WRAP = 512.0;

fn fall_offset(vy: f32, cell: f32) -> f32 {
    return (vy * P.time) % (cell * WRAP);
}

// One layer of streaks, 0..1 at p. Drops move along (s * vy, -vy), so in
// coordinates (x - s y, y + vy t) each one stands still; cells there are
// tall and thin, one drop in some of them.
fn streaks(p: vec2f, scale: f32, vy: f32, s: f32, density: f32, width: f32, seed: f32) -> f32 {
    let g = vec2f(p.x - s * p.y, p.y + fall_offset(vy, scale * 9.0)) / vec2f(scale, scale * 9.0);
    let cell = vec2f(floor(g.x), floor(g.y) % WRAP);
    let h = pcg2(cell + seed);
    if (h.x > density) {
        return 0.0;
    }
    let f = fract(g);
    let along = smoothstep(0.0, 0.35, f.y) * smoothstep(1.0, 0.75, f.y);
    let d = abs(f.x - 0.2 - 0.6 * h.y) * scale;
    // Never thinner than a pixel and a bit, or the drops shimmer into moire;
    // a drop thinner than that is drawn fainter instead.
    let wd = max(width, 1.3 / P.world_size.y);
    return smoothstep(wd, wd * 0.2, d) * along * (0.5 + 0.5 * fract(h.x * 91.0)) * width / wd;
}

// One layer of drifting specks (snow, ash), each wobbling on its way down.
// The wind carries the layer sideways by `span` screen units per unit of
// `P.fall_drift`, which the host integrates from the ground wind. The layer
// repeats every `span` across (whole cells), so the drift's wrap at 1 never
// shows. Leaning the cells with the local wind instead, as rain does,
// stretched and folded the flakes into streaks wherever the wind changed
// across the screen or over time.
fn specks(p: vec2f, scale: f32, vy: f32, span: f32, density: f32, size: f32, seed: f32, stir: f32) -> f32 {
    let n = round(span / scale);
    let g = vec2f(p.x / scale - P.fall_drift * n, (p.y + fall_offset(vy, scale)) / scale);
    let fx = floor(g.x);
    let cell = vec2f(fx - n * floor(fx / n), floor(g.y) % WRAP);
    let h = pcg2(cell + seed);
    if (h.x > density) {
        return 0.0;
    }
    // Gusts toss the flakes about more, but never out of their cells.
    let wob = vec2f(sin(P.time * (0.7 + h.y) + h.x * 40.0), cos(P.time * (0.5 + h.x) + h.y * 30.0)) * (0.13 + 0.05 * stir);
    let d = length((fract(g) - 0.3 - 0.4 * pcg2(cell + seed + 9.0) - wob) * scale);
    let r = size * (0.5 + h.y);
    let rd = max(r, 0.8 / P.world_size.y);
    return exp(-d * d / (rd * rd)) * r * r / (rd * rd);
}

fn falling(p: vec2f, base: vec3f) -> vec3f {
    let air = sky_air(p.x);
    let more = sky_more(p.x);
    let ash = sky_ground(p.x).z;
    let hail = more.y;
    if (air.y < 0.02 && hail < 0.02 && ash < 0.05) {
        return base;
    }
    let fall = smoothstep(0.0, 0.6, air.y);
    let snow = air.z;
    // Rain leans with the ground wind. The lean is the same everywhere: one
    // that followed the local gusts would bend the drops' grid across the
    // screen and smear them.
    let s = clamp(-P.wind_low * 55.0, -0.9, 0.9);
    let stir = clamp(more.z, 0.0, 2.0) * 0.5;
    let light = air_light(p);
    var col = base;
    // Thinner against the sky, where there is less behind it to catch light.
    let rain = fall * (1.0 - snow) * (0.45 + 0.55 * smoothstep(0.7, 0.35, p.y));
    if (rain > 0.01) {
        var r = streaks(p, 0.011, 1.7, s, rain * 0.45, 0.0009, 1.0) * 0.8;
        r += streaks(p, 0.0065, 1.15, s * 0.9, rain * 0.55, 0.0006, 37.0) * 0.6;
        r += streaks(p, 0.0038, 0.75, s * 0.8, rain * 0.65, 0.0004, 91.0) * 0.4;
        // A downpour also greys out everything behind it.
        col = mix(col, light * 0.6, rain * 0.25);
        col = col * (1.0 - 0.2 * min(r, 1.0)) + light * r * 0.9;
    }
    let flakes = fall * snow;
    if (flakes > 0.01) {
        var f = specks(p, 0.03, 0.09, 14.6, flakes * 0.5, 0.0018, 3.0, stir);
        f += specks(p, 0.017, 0.065, 9.4, flakes * 0.6, 0.0012, 17.0, stir) * 0.8;
        f += specks(p, 0.009, 0.045, 5.8, flakes * 0.7, 0.0007, 59.0, stir) * 0.6;
        col = mix(col, light * 0.7, flakes * 0.3);
        col += (light * 1.6 + vec3f(0.02, 0.025, 0.05)) * f;
    }
    if (hail > 0.02) {
        let h = streaks(p, 0.012, 2.4, s * 0.5, hail * 0.4, 0.0016, 211.0);
        col += (light * 2.0 + vec3f(0.08)) * h;
    }
    if (ash > 0.05) {
        let a = specks(p, 0.022, 0.03, 6.8, ash * 0.5, 0.0013, 71.0, stir) + specks(p, 0.012, 0.022, 4.0, ash * 0.6, 0.0008, 5.0, stir);
        col = mix(col, vec3f(0.03, 0.026, 0.025) + light * 0.15, min(a, 1.0) * 0.8);
    }
    return col;
}

// Fog lying on the land: thickening with distance across the plain, and a
// low bank over the land behind it, drifting with the ground wind and
// glowing where the sun is behind it.
fn fog_air(p: vec2f) -> vec4f {
    let fog = sky_air(p.x).w;
    if (fog < 0.01) {
        return vec4f(0.0);
    }
    let gt = ground_top(p.x);
    var depth: f32;
    if (p.y < gt) {
        depth = 1.0 - exp(-plain_z(p.y) / 700.0);
    } else {
        depth = smoothstep(gt + 0.03 + 0.12 * fog, gt - 0.01, p.y);
    }
    let wisp = fbm(vec2f(p.x * 5.0 + P.ground_drift * 2.0, p.y * 28.0 - P.time * 0.002), 4);
    let dens = fog * depth * (0.45 + 0.8 * wisp);
    let col = sky_base(vec2f(p.x, 0.42)) * 0.85 + sun_glow(p) * 0.4 + vec3f(0.01, 0.012, 0.022);
    return vec4f(col, clamp(dens * 1.2, 0.0, 0.92));
}

// Dust raised by the wind across the land: a churning haze, thick near the
// ground and thinning upward, lit hot by the sun.
fn dust_haze(p: vec2f) -> vec4f {
    let dust = sky_ground(p.x).w;
    if (dust < 0.02) {
        return vec4f(0.0);
    }
    let gust = sky_more(p.x).z;
    let low = smoothstep(0.7, 0.15, p.y);
    let churn = fbm(vec2f(p.x * 7.0 + P.ground_drift * 4.0, p.y * 16.0 - P.time * 0.01), 4);
    let day = daylight(p.x);
    let col = mix(vec3f(0.12, 0.05, 0.02), vec3f(1.3, 0.6, 0.22), day) * (0.6 + 0.6 * churn) + sun_glow(p) * 0.3;
    // Gusts lift more of it.
    return vec4f(col, clamp(dust * low * (0.35 + 0.7 * churn) * (0.7 + 0.15 * gust), 0.0, 0.85));
}

// Fine ash hanging everywhere downwind after an eruption, and the veil it
// leaves high up dimming and browning the whole sky.
fn ash_haze(p: vec2f) -> vec4f {
    let ash = sky_ground(p.x).z;
    let veil = sky_misc().x;
    let a = ash * 0.6 + veil * 0.35;
    if (a < 0.01) {
        return vec4f(0.0);
    }
    let n = fbm(vec2f(p.x * 3.0 - P.cloud_drift, p.y * 8.0), 3);
    let col = vec3f(0.06, 0.045, 0.04) * (0.7 + 0.6 * n) + sun_glow(p) * vec3f(0.5, 0.25, 0.1) * 0.4;
    return vec4f(col, clamp(a * (0.6 + 0.5 * n), 0.0, 0.8));
}

// The whole scene flickers blue-white with each flash.
fn flash(p: vec2f, col: vec3f) -> vec3f {
    if (P.lightning < 0.01) {
        return col;
    }
    let near = exp(-abs(p.x - P.lightning_x) * 2.2);
    let f = P.lightning * (0.06 + 0.3 * near);
    return col * (1.0 + f) + vec3f(0.07, 0.07, 0.12) * f;
}

fn over(col: vec3f, layer: vec4f) -> vec3f {
    return mix(col, layer.rgb, layer.a);
}

@fragment
fn fs_composite(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    var col = textureLoad(scene, vec2i(in.pos.xy), 0).rgb;
    col = over(col, fog_air(p));
    col = over(col, ash_haze(p));
    col = over(col, ash_air(p));
    col = over(col, cold_air(p));
    col = over(col, dust_haze(p));
    col = over(col, dust_air(p));
    col = falling(p, col);
    col = flash(p, col);
    col += motes(p, 0.03, 0.0008, 0.0022, 0.0) * 0.6;
    // Bright light spilling into the air around it. (Bloom was gathered
    // after exposure, so take that back out before finish applies it.)
    let glow = textureSampleLevel(bloom_near, samp, in.uv, 0.0).rgb * 0.5 + textureSampleLevel(bloom_wide, samp, in.uv, 0.0).rgb * 0.5;
    col += glow * BLOOM / max(P.exposure, 1e-4);
    return finish(col, in.pos.xy);
}
