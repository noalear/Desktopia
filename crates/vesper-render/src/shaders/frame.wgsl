// Frame pass: animates the baked landscape into the linear HDR scene.

@group(0) @binding(1) var baked: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

fn is_mat(m: f32, code: f32) -> bool {
    return abs(m - code) < 0.1;
}

// Cloud density in the high deck at screen point p, 0..1. How much of the
// sky it fills follows the simulation's cloud cover overhead: a few wisps in
// fair weather, broken bands under a passing wave, a closed deck in a wet
// spell.
fn cloud_density(p: vec2f, octaves: i32) -> f32 {
    let cover = sky_air(p.x).x;
    let q = vec2f((p.x - P.cloud_drift) * 1.4, p.y * 5.5);
    let n = fbm(q, octaves) + 0.12 * (fbm(q * 3.3 + 7.0, 3) - 0.5);
    let base = mix(0.62, 0.5, cover);
    let band = smoothstep(base - 0.08, base + 0.06, p.y) * smoothstep(0.98, 0.78, p.y);
    let lo = mix(0.7, 0.3, cover);
    return smoothstep(lo, lo + 0.28, n) * band;
}

// The high cloud deck, lit by the sun from the side: light soaks through
// the thin edges and dies inside the thick cores, the sunward side of each
// billow glows and the far side falls into shade. A heavy deck is grey and
// dark underneath.
fn clouds(p: vec2f, base: vec3f) -> vec3f {
    let cover = sky_air(p.x).x;
    // Overcast dulls the whole sky behind the clouds.
    var sky = mix(base, base * 0.55 + sky_base(vec2f(p.x, 0.6)) * 0.12, cover * 0.7 * smoothstep(0.35, 0.6, p.y));
    let dens = cloud_density(p, 5);
    if (dens < 0.003) {
        return sky;
    }
    let t = nightness(p.x);
    let day = daylight(p.x);
    let to_sun = normalize(sun_pos() - p);
    let d1 = cloud_density(p + to_sun * 0.012, 3);
    let d2 = cloud_density(p + to_sun * 0.035, 3);
    let through = exp(-(d1 * 1.5 + d2 * 1.5 + dens * 0.8) * (1.0 + cover));
    // Forward scattering: edges near the sun's line of sight blaze.
    let near_sun = exp(-distance(p, sun_pos()) * 2.2);
    let sun = vec3f(2.4, 0.95, 0.3) * day * (through * (0.8 + 1.3 * near_sun * (1.0 - dens)));
    // Sky light from above, and the warm glow of the land from below.
    let ambient = sky_base(vec2f(p.x, 0.95)) * 0.9 + sky_base(vec2f(p.x, 0.35)) * 0.25 * (1.0 - dens);
    let night_lit = vec3f(0.06, 0.03, 0.12) * (1.0 - t) * 0.5;
    var cc = sun + ambient + night_lit;
    cc *= (0.55 + 0.45 * (1.0 - dens)) * (1.0 - 0.45 * cover * smoothstep(0.75, 0.55, p.y));
    // Ash from an eruption dulls the deck and thickens it.
    let ash = max(P.ash, sky_ground(p.x).z);
    cc = mix(cc, vec3f(0.04, 0.03, 0.03), ash * 0.5);
    let alpha = clamp((1.0 - exp(-dens * 3.5)) * (0.9 + 0.3 * ash + 0.3 * cover), 0.0, 0.97);
    return mix(sky, cc, alpha);
}

// Height of the base of the storm clouds, just above the far ranges.
const CLOUD_BASE = 0.41;

// How far the high wind has dragged a tower night-ward at height y.
fn shear(y: f32) -> f32 {
    return max(y - CLOUD_BASE, 0.0) * 0.3;
}

// One storm cell as (colour, cover): a tower of round billows that climbs
// as the cell grows, spreads a fibrous anvil night-ward on the high wind
// once mature, and loses its crisp billows as it dies until only the anvil
// is left. The billows are shaded as rough spheres, lit from behind and to
// the side by the low sun: dark faces with blazing sunward rims, tops that
// catch the light long after the ground has gone dark, and a heavy, rain
// dark base. Lightning lights it from within.
fn tower(p: vec2f, c: vec4f) -> vec4f {
    let power = c.y;
    if (power < 0.03) {
        return vec4f(0.0);
    }
    let seed = floor(c.w);
    let age = fract(c.w);
    let grown = smoothstep(0.0, 0.45, age);
    let dying = smoothstep(0.7, 1.0, age);
    // Its width and height follow the strength it reached, so a dying tower
    // dissolves where it stands instead of sinking.
    let peak = max(c.z, power);
    let w = 0.07 + 0.08 * peak;
    // It builds upward: a small heap of cloud first, then turret on turret.
    let full = 0.08 + 0.34 * min(peak, 1.25);
    let built = 0.12 + 0.88 * grown;
    let height = full * built;
    let top = CLOUD_BASE + height;
    let spread = smoothstep(0.25, 0.75, age) * smoothstep(0.35, 0.8, peak) * (0.12 + 0.45 * peak) * (1.0 + 0.5 * dying);
    let dx = p.x - c.x - shear(min(p.y, top));
    if (p.y < CLOUD_BASE - 0.025 || p.y > top + w * 1.3 || dx < -w * 1.9 - spread * 0.4 || dx > w * 1.9 + spread) {
        return vec4f(0.0);
    }
    let drift = vec2f(-P.time * 0.0004, P.time * 0.0003);
    let fine = fbm(p * vec2f(60.0, 70.0) + seed * 3.1 + drift, 4) - 0.5;
    let finer = fbm(p * 210.0 + seed + drift * 3.0, 2) - 0.5;

    // The billows. Where they overlap the nearer surface mostly wins, blended
    // softly so the creases between them stay soft.
    var wsum = 0.0;
    var nsum = vec3f(0.0);
    var cover = 0.0;
    for (var i = 0; i < 22; i++) {
        let fi = f32(i);
        let h = pcg2(vec2f(seed, fi * 7.0 + 1.0));
        let h2 = pcg2(vec2f(seed + 1500.0, fi * 13.0 + 3.0));
        let f = (fi + 0.5) / 22.0;
        if (f > built + 0.02) {
            break;
        }
        let crown = smoothstep(built - 0.4, built, f);
        // A broad, flat foot, a narrower column and a swelling crown.
        let foot = smoothstep(0.2, 0.0, f);
        let boil = 0.9 + 0.1 * sin(P.time * (0.015 + 0.02 * h2.y) + fi * 2.3 + seed);
        let r = w * (0.45 + 0.3 * h.x + 0.3 * crown + 0.2 * foot) * (1.0 - 0.4 * dying) * boil
            * (0.55 + 0.45 * smoothstep(0.0, 0.5, built)) * smoothstep(built + 0.03, built - 0.1, f);
        let y = CLOUD_BASE + full * (0.04 + 0.88 * f) - r * 0.2;
        let x = c.x + shear(y) + (h.y - 0.5) * w * (0.6 + 0.9 * crown + 0.3 * foot);
        let q = (p - vec2f(x, y)) / vec2f(r * (1.0 + 0.2 * foot), r * (1.0 - 0.2 * foot));
        let d = length(q) + (fine * 0.3 + finer * 0.08) * (1.0 + 1.5 * dying);
        if (d >= 1.0) {
            continue;
        }
        let zf = sqrt(max(1.0 - dot(q, q), 0.0));
        let depth = ((h2.x - 0.5) * w + zf * r) / w;
        cover = max(cover, smoothstep(1.0, 0.75 - 0.4 * dying, d));
        let wt = exp(depth * 6.0) * smoothstep(1.0, 0.85, d);
        wsum += wt;
        nsum += normalize(vec3f(q, zf + 0.25)) * wt;
    }
    var nrm = normalize(nsum + vec3f(0.0, 0.0, 1e-4));
    // Smaller billows on the big ones.
    let e = 0.004;
    let bq = mat2x2f(0.8, 0.6, -0.6, 0.8) * p * 48.0 + seed * 1.7 + drift * 2.0;
    let b0 = fbm(bq, 3);
    let bump = vec2f(fbm(bq + vec2f(e * 48.0, 0.0), 3) - b0, fbm(bq + vec2f(0.0, e * 48.0), 3) - b0) / e;
    nrm = normalize(nrm - vec3f(bump * 0.007, 0.0));
    // Flat, ragged underneath where the rising air first condenses.
    cover *= smoothstep(CLOUD_BASE - 0.02, CLOUD_BASE + 0.008, p.y + fine * 0.03);

    // The anvil: a thin sheet at the top, reaching far downwind.
    var anvil = 0.0;
    if (spread > 0.01) {
        let along = p.x - c.x - shear(top);
        let reach = select(w * 1.1 + spread * 0.25, w + spread, along > 0.0);
        let fib = fbm(vec2f(p.x * 7.0 - P.time * 0.0006 + seed, p.y * 60.0), 4);
        let thick = (0.03 + 0.045 * peak) * (1.0 - 0.7 * smoothstep(0.0, reach, abs(along))) * (1.0 - 0.4 * dying);
        let ay = top - thick * 0.5 + 0.01 * (fib - 0.5);
        anvil = smoothstep(reach, reach * 0.5, abs(along) + 0.05 * (fib - 0.5))
            * smoothstep(thick, thick * 0.3, abs(p.y - ay))
            * smoothstep(0.25, 0.6, fib + 0.15) * (1.0 - 0.5 * dying);
        nrm = normalize(mix(nrm, vec3f(0.0, sign(p.y - ay) * 0.7, 0.7), anvil * (1.0 - cover)));
    }
    let alpha = max(cover, anvil * 0.9);
    if (alpha < 0.002) {
        return vec4f(0.0);
    }

    let rel = clamp((p.y - CLOUD_BASE) / 0.5, 0.0, 1.0);
    let to_sun = normalize(sun_pos() - p);
    let l = normalize(vec3f(to_sun, -0.6));
    let wrap = clamp((dot(nrm, l) + 0.4) / 1.4, 0.0, 1.0);
    let rim = pow(1.0 - nrm.z, 3.0) * max(dot(nrm.xy, to_sun), 0.0);
    let near_sun = exp(-distance(p, sun_pos()) * 1.8);
    // The tops stay in the sun well past where the ground below has lost it.
    let sunlit = daylight(p.x - 0.7 * rel);
    let sun = vec3f(2.3, 1.0, 0.42) * sunlit * sunlit * (wrap * (0.3 + 0.7 * rel) + rim * (0.7 + 3.0 * near_sun));
    let sky_up = sky_base(vec2f(p.x, 0.92)) * (0.55 + 0.45 * nrm.y);
    let bounce = sky_base(vec2f(p.x, 0.3)) * 0.2 * (0.5 - 0.5 * nrm.y);
    let night = vec3f(0.012, 0.014, 0.03) * (0.6 + 0.4 * nrm.y);
    // Darker underneath the bigger it is: a young heap is pale all over.
    let deep = smoothstep(0.1, 0.45, age) * smoothstep(0.2, 0.8, peak);
    var col = (sun + sky_up + bounce + night) * mix(1.0, mix(0.3, 1.0, smoothstep(0.0, 0.45, rel)), deep);
    // Heavy with rain at the base.
    col *= 1.0 - 0.5 * smoothstep(0.06, 0.0, p.y - CLOUD_BASE) * min(power, 1.0) * (1.0 - dying);
    // Lightning glows through the body of the cloud.
    let gd = length(vec2f((p.x - P.lightning_x) * 1.2, (p.y - CLOUD_BASE - 0.08) * 0.8));
    col += vec3f(0.85, 0.8, 1.6) * P.lightning * exp(-gd * 16.0) * (0.4 + 0.8 * (1.0 - nrm.z));
    return vec4f(col, clamp(alpha * 1.05, 0.0, 1.0) * smoothstep(1.0, 0.85, age));
}

fn storm(p: vec2f, base: vec3f) -> vec3f {
    var col = base;
    for (var k = 0; k < SKY_CELLS; k++) {
        let tw = tower(p, sky_cell(k));
        col = mix(col, tw.rgb, tw.a);
    }
    return col;
}

// A zigzag across y, one random kink every `seg`.
fn zig(y: f32, seed: f32, seg: f32, amp: f32) -> f32 {
    let s = y / seg;
    let i = floor(s);
    let a = pcg2(vec2f(i, seed)).x - 0.5;
    let b = pcg2(vec2f(i + 1.0, seed)).x - 0.5;
    return mix(a, b, fract(s)) * amp;
}

fn bolt_path(y: f32, seed: f32) -> f32 {
    return zig(y, seed, 0.02, 0.026) + zig(y, seed + 7.0, 0.006, 0.007) + zig(y, seed + 13.0, 0.0018, 0.0022);
}

// A bolt from the cloud base to the far land: a jagged channel with forks
// that die out partway down, and the air glowing around it. Each flash
// takes a new path.
fn bolt(p: vec2f, base: vec3f) -> vec3f {
    if (P.lightning < 0.05) {
        return base;
    }
    let fork = floor(P.time * 0.37);
    let top = CLOUD_BASE + 0.05;
    let floor_y = HORIZON + 0.015 + 0.02 * pcg2(vec2f(fork, 3.0)).x;
    let in_y = step(floor_y, p.y) * step(p.y, top);
    let x0 = P.lightning_x + bolt_path(p.y, fork);
    let d = abs(p.x - x0);
    var core = smoothstep(0.0011, 0.0, d) * in_y;
    for (var b = 0; b < 3; b++) {
        let hb = pcg2(vec2f(fork, f32(b) + 40.0));
        let start = top - (0.015 + hb.x * 0.07);
        let len = 0.025 + 0.05 * hb.y;
        let down = start - p.y;
        if (down > 0.0 && down < len) {
            let dir = select(-1.0, 1.0, fract(hb.x * 7.0) > 0.5);
            let bx = P.lightning_x + bolt_path(start, fork) + dir * down * (0.3 + 0.6 * hb.y)
                + bolt_path(p.y, fork + 50.0 + f32(b)) * 0.5;
            core = max(core, smoothstep(0.0008, 0.0, abs(p.x - bx)) * (1.0 - down / len) * 0.7);
        }
    }
    let glow = exp(-d * 80.0) * in_y * 0.5;
    return base + (vec3f(10.0, 10.0, 18.0) * core + vec3f(0.5, 0.45, 0.95) * glow) * P.lightning;
}

// Rain and snow hanging under the clouds in curtains, far off over the land:
// shafts that fall from the cloud base and lean with the ground wind, rain
// in grey streaked sheets that glow gold when the sun is behind them, snow
// in soft pale veils.
fn curtains(p: vec2f, base: vec3f, floor_y: f32) -> vec3f {
    let ragged = fbm(vec2f(p.x * 14.0 + P.time * 0.002, 2.0), 3);
    let top = CLOUD_BASE + 0.015 + 0.03 * (ragged - 0.5);
    if (p.y < floor_y || p.y > top) {
        return base;
    }
    let down = clamp((top - p.y) / (top - HORIZON), 0.0, 1.5);
    // The rain drifts with the ground wind as it falls.
    let xs = p.x - P.wind_low * 12.0 * down;
    let air = sky_air(xs);
    if (air.y < 0.02) {
        return base;
    }
    let fall = smoothstep(0.0, 0.55, air.y);
    let snow = air.z;
    let shafts = fbm(vec2f(xs * 22.0, P.time * 0.002 + 5.0), 3);
    let streak = fbm(vec2f(xs * 260.0, p.y * mix(5.0, 1.5, snow) - P.time * mix(0.7, 0.12, snow)), 2);
    var dens = fall * smoothstep(0.25, 0.7, shafts + fall * 0.35) * mix(0.55 + 0.45 * streak, 0.85 + 0.15 * streak, snow);
    dens *= smoothstep(top, top - 0.025, p.y);
    let t = nightness(p.x);
    // Rain is a grey veil, darker than the sky behind it and paler than dark
    // land, glowing where the sun is behind it; snow is a paler, softer one.
    let behind = sky_base(vec2f(p.x, max(p.y, 0.42)));
    let grey = vec3f(dot(behind, vec3f(0.3, 0.55, 0.15)));
    let rain = mix(grey, behind, 0.4) * 0.45 + vec3f(0.012, 0.014, 0.024) + sun_glow(p) * 0.15;
    let flakes = mix(grey, behind, 0.3) * 1.1 + vec3f(0.015, 0.018, 0.035);
    return mix(base, mix(rain, flakes, snow), clamp(dens * 0.8, 0.0, 0.8));
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

// How frosted the ground is: behind the cold fall's front, back toward the
// ice, and wherever a cold snap has settled on clear, still ground.
fn frost(p: vec2f) -> f32 {
    let snap = smoothstep(0.25, 0.9, sky_more(p.x).w) * smoothstep(0.25, 0.6, nightness(p.x));
    if (P.cold < 0.01) {
        return snap;
    }
    let dx = p.x - P.cold_x + 0.08 * (fbm(vec2f(min(p.y, ground_top(p.x)) * 30.0, p.x * 3.0), 3) - 0.5);
    return max(snap, P.cold * smoothstep(0.0, 0.18, dx) * exp(-max(dx - 0.3, 0.0) * 1.5));
}

// What the weather has left lying on the land: rain soaking the ground dark,
// puddles in the plain's hollows mirroring whatever is above them, ash
// greying everything downwind of the volcano, and snow drifted over it all.
// Snow and ash keep the light that is already falling there, so they glow
// in the sun and stay dim in the dark.
fn weathered_ground(p: vec2f, base: vec3f, on_plain: bool) -> vec3f {
    let g = sky_ground(p.x);
    let air = sky_air(p.x);
    let ashfall = sky_more(p.x).x;
    if (g.x < 0.4 && air.y < 0.02 && g.y < 0.01 && ashfall < 0.01) {
        return base;
    }
    let t = nightness(p.x);
    var q = p * vec2f(45.0, 110.0);
    if (on_plain) {
        q = plain_point(p) * vec2f(0.012, 0.03);
    }
    let n = fbm(q, 4);
    let lum = dot(base, vec3f(0.3, 0.55, 0.15));
    let frozen = max(smoothstep(0.62, 0.78, t), air.z);
    var col = base;
    let wet = clamp(smoothstep(0.4, 0.9, g.x) + air.y * 0.6, 0.0, 1.0) * (1.0 - frozen) * (1.0 - g.y);
    col *= 1.0 - 0.4 * wet;
    if (on_plain) {
        let pool = smoothstep(0.64, 0.7, n + wet * 0.1) * smoothstep(0.55, 0.85, wet);
        if (pool > 0.001) {
            let shiver = 0.0015 * air.y * sin(P.time * 7.0 + p.y * 900.0 + n * 40.0);
            let mp = vec2f(p.x + shiver, 2.0 * HORIZON - p.y);
            let refl = textureSampleLevel(baked, samp, uv_of(mp), 0.0);
            var rc = refl.rgb;
            if (is_mat(refl.a, 1.0)) {
                rc = clouds(mp, rc);
            }
            col = mix(col, rc * 0.5, pool * 0.85);
        }
    }
    col = mix(col, vec3f(lum * 0.55) * vec3f(1.0, 0.95, 0.9) * (0.7 + 0.6 * n), clamp(ashfall * 0.85, 0.0, 0.85));
    let snow = clamp(smoothstep(0.38, 0.62, n + g.y * 0.6 - 0.3) * g.y, 0.0, 1.0) * select(0.35, 0.95, on_plain);
    let white = mix(vec3f(lum), base, 0.3) * 2.4 + vec3f(0.006, 0.008, 0.018) * t;
    col = mix(col, white * (0.85 + 0.3 * n), snow);
    return col;
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
    let fade = smoothstep(0.55, 0.9, t) * smoothstep(0.35, 0.6, p.y) * (1.0 - planet_cover(p));
    // Thickest along the galaxy's band.
    let clump = fbm(p * 3.0 + 11.0, 3) + 0.3 * galaxy_near(p);
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
    let on_plain = p.y < ground_top(p.x) + 0.002;
    if (is_mat(m, 1.0)) {
        col += stars(p);
        col = clouds(p, col);
        col = smoke(p, col);
        col = curtains(p, col, 0.0);
        col = storm(p, col);
        col = bolt(p, col);
        // Snow blowing off the ice wall on the night wind.
        if (p.x > 2.95) {
            let below = textureSampleLevel(baked, samp, uv_of(p - vec2f(0.0, 0.03)), 0.0).a;
            if (is_mat(below, 0.75)) {
                let drift = fbm(vec2f(p.x * 50.0 - P.time * 0.25, p.y * 140.0), 4);
                col += vec3f(0.25, 0.35, 0.6) * smoothstep(0.55, 0.8, drift) * smoothstep(3.0, 3.2, p.x) * 0.25;
            }
        }
    } else if (is_mat(m, 0.5)) {
        // A rippled mirror. The ripples lie on the water in perspective and
        // drift day-ward with the current: long swells, chop across them,
        // and streaks drawn out along the flow. Each pixel reflects whatever
        // of the land and sky its tilted surface faces, sun glitter included.
        let w = plain_point(p);
        let calm = 1.0 - 0.6 * smoothstep(0.6, 0.3, p.x); // shallow, slow water in the pan
        let flow = P.time * (0.6 + 0.6 * P.river_flood);
        let swell = noised(vec2f(w.x * 0.3 + flow * 0.35, w.y * 0.8));
        let chop = noised(vec2f(w.x * 1.4 + flow * 1.2, w.y * 2.6 + P.time * 0.15) + 7.0);
        let streak = noised(vec2f(w.x * 0.12 + flow * 0.2, w.y * 3.5) + 3.0);
        // Depth of ground one pixel row spans: ripples finer than that would
        // only alias into lines, so they fade out with distance.
        // Across the view a pixel spans far less ground than along it, so
        // ripples stay sharp sideways long after they blur away in depth.
        let dz = w.y * w.y / (CAM_H * FOCAL * P.world_size.y);
        let dx = w.y / (FOCAL * P.world_size.y);
        let fine = smoothstep(0.35, 0.12, dz * 2.6);
        let mid = smoothstep(0.35, 0.12, dz * 0.8);
        let side = smoothstep(0.35, 0.12, dx * 2.6);
        let slope = (swell.yz * vec2f(0.3, 0.8) * 0.05 * vec2f(1.0, mid)
            + chop.yz * vec2f(1.4, 2.6) * 0.012 * vec2f(side, fine)
            + streak.yz * vec2f(0.12, 3.5) * 0.006 * vec2f(side, fine)) * calm * (1.0 + P.river_flood);
        let nrm = normalize(vec3f(-slope.x, 1.0, -slope.y));
        let rd = view_ray(p);
        let rr = reflect(rd, nrm);
        let mp = vec2f(CAM_X + rr.x / rr.z * FOCAL, HORIZON + rr.y / max(rr.z, 0.05) * FOCAL);
        let refl = textureSampleLevel(baked, samp, uv_of(mp), 0.0);
        var rc = refl.rgb;
        if (is_mat(refl.a, 1.0)) {
            rc = clouds(mp, rc);
        }
        // Ripples too fine to see still roughen the mirror: what it shows
        // smears up and down, as reflections on moving water do.
        let smear = (1.0 - fine) * calm * (0.025 + 0.04 * P.river_flood);
        let up = textureSampleLevel(baked, samp, uv_of(mp + vec2f(slope.x * 0.3, smear)), 0.0).rgb;
        let down = textureSampleLevel(baked, samp, uv_of(mp - vec2f(slope.x * 0.3, smear)), 0.0).rgb;
        rc = mix(rc, (up + down) * 0.5, 0.6 * smoothstep(0.0, 0.01, smear));
        let fres = 0.02 + 0.98 * pow(1.0 - clamp(dot(-rd, nrm), 0.0, 1.0), 5.0);
        // Shore pixels are only partly water (see water_code in the bake).
        let cover = clamp((m - 0.4) / 0.19, 0.0, 1.0);
        col = mix(col, rc * 0.78, fres * cover);
        // Foam and scum lines along the banks and in the eddies, drifting.
        let e = river_across(w);
        let foam_n = fbm(vec2f(w.x * 0.5 + flow * 0.5, w.y * 4.0), 3);
        let foam = smoothstep(0.78, 1.0, e) * smoothstep(0.45, 0.75, foam_n) + smoothstep(0.72, 0.85, foam_n) * 0.25;
        let foam_col = band3(vec3f(1.2, 0.7, 0.45), vec3f(0.35, 0.25, 0.45), vec3f(0.04, 0.08, 0.15), t);
        col = mix(col, foam_col, foam * 0.45 * cover * calm);
        // Meltwater from the glacier carries a living glow at night: drifting
        // threads of light, brightest near the ice.
        let threads = smoothstep(0.55, 0.8, fbm(vec2f(w.x * 0.25 + flow * 0.3, w.y * 2.0) + 11.0, 4));
        col += vec3f(0.03, 0.45, 0.7) * (threads * 0.18 + 0.025) * t * t * cover * (0.5 + 0.5 * smoothstep(2.4, 3.2, p.x));
    } else if (is_mat(m, 0.25)) {
        // Flows glow as far as the lava has run, brightest at the front,
        // cooling as the eruption dies and crusting over when it is quiet.
        let run = (m - 0.19) / 0.12 * 0.45;
        let reach = smoothstep(P.lava + 0.01, P.lava - 0.025, run) * smoothstep(0.0, 0.03, P.lava);
        let front = exp(-pow((run - P.lava) / 0.012, 2.0)) * reach;
        let heat = reach * (0.45 + 0.55 * smoothstep(0.0, 0.4, P.volcano)) + front * 0.8;
        col += vec3f(9.0, 2.2, 0.3) * heat * (0.75 + 0.35 * sin(P.time * 0.6 + p.y * 90.0 - p.x * 30.0));
    } else if (is_mat(m, 0.75)) {
        if (m > 0.77) {
            // A meltwater fall: streaks pouring down, foaming white.
            let q = vec2f(p.x * 900.0, p.y * 60.0 + P.time * 1.4);
            let streak = fbm(vec2f(q.x, q.y), 3);
            col *= 0.7 + 0.6 * smoothstep(0.35, 0.75, streak);
            col += vec3f(0.08, 0.14, 0.2) * smoothstep(0.62, 0.8, streak) * (0.4 + t);
        } else {
            // Light shifting slowly inside the ice.
            let n = fbm(vec2f(p.x * 14.0, p.y * 20.0 + P.time * 0.02), 3);
            col *= 0.92 + 0.16 * n;
        }
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

    if (is_mat(m, 0.0)) {
        col = weathered_ground(p, col, on_plain);
    }
    // Rain and lightning over the far land, in front of the ranges.
    if (!is_mat(m, 1.0) && !on_plain) {
        col = curtains(p, col, ground_top(p.x));
        col = bolt(p, col);
    }

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
