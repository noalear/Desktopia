// Bake pass: the slow-changing landscape, rendered rarely into an fp16 texture.
//
// Behind the plain lies a real heightfield seen through the camera in
// common.wgsl: ray marched, lit by the low sun with soft shadows and sky
// light, and fogged by Vesper's thick air. The plain in front is the
// simulation's ground, textured in perspective. Baking happens only when
// the season moves the light, so this pass can afford to be expensive.
//
// Alpha carries a material code the frame pass animates:
// 0 rock, 0.19..0.31 lava (how far down the flow, see lava_code), 0.4..0.59
// water (how much of the pixel, see water_code),
// 0.75 ice, 1 sky.

const M_ROCK = 0.0;
const M_ICE = 0.75;
const M_SKY = 1.0;

const ROT = mat2x2f(0.8, 0.6, -0.6, 0.8);

// Where the volcano stands, in metres, placed so its summit lands on
// VOLCANO_X, VOLCANO_TOP on screen.
const VOL_Z = 14000.0;
const VOL_X = (VOLCANO_X - CAM_X) * VOL_Z / FOCAL;
const VOL_H = CAM_H + (VOLCANO_TOP - HORIZON) * VOL_Z / FOCAL;
const VOL_R = 3600.0;

// Toward the sun. It sits on the horizon on screen; the light is lifted a
// little so slopes facing it catch the light rather than only the crests.
const SUN_DIR = vec3f(-0.86, 0.17, 0.48);

// ---------------------------------------------------------------- noise

// Fbm whose fine detail fades on steep ground, so it reads as eroded rock:
// sharp crests, gullied faces and smooth valley floors. About -1..1.
fn eroded(p: vec2f, octaves: i32) -> f32 {
    var q = p;
    var sum = 0.0;
    var amp = 1.0;
    var d = vec2f(0.0);
    for (var i = 0; i < octaves; i++) {
        let n = noised(q);
        d += n.yz;
        sum += amp * n.x / (1.0 + dot(d, d));
        amp *= 0.5;
        q = ROT * q * 2.0 + vec2f(3.1, 1.7);
    }
    return sum;
}

// Fbm painted onto a surface from the three axes and blended by its
// normal, so cliffs are textured as well as the ground above them.
fn tri_fbm(pos: vec3f, n: vec3f, s: f32, octaves: i32) -> f32 {
    var wt = pow(abs(n), vec3f(4.0));
    wt /= wt.x + wt.y + wt.z;
    return fbm(pos.zy / s, octaves) * wt.x + fbm(pos.xz / s, octaves) * wt.y + fbm(pos.xy / s + 3.0, octaves) * wt.z;
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (a - b) / k, 0.0, 1.0);
    return mix(b, a, h) + k * h * (1.0 - h);
}

// ---------------------------------------------------------------- land

// The land is built in "view" coordinates: screen x, and log distance scaled
// so one unit of each spans the same ground. Mountains then keep the same
// size on screen at every distance, the way the first build's layers did,
// but as one continuous landscape with real slopes.
fn view_coords(xz: vec2f) -> vec2f {
    return vec2f(screen_x(xz), FOCAL * log(max(xz.y, 1.0)));
}

// Height in metres for a screen elevation above the eye's horizon at z.
fn elev_h(e: f32, z: f32) -> f32 {
    return CAM_H + e * z / FOCAL;
}

fn volcano_h(xz: vec2f, octaves: i32) -> f32 {
    let d = xz - vec2f(VOL_X, VOL_Z);
    let r = length(d);
    if (r > VOL_R) {
        return -1000.0;
    }
    let dir = d / max(r, 1.0);
    // Not a perfect cone: the flanks bulge and slump unevenly.
    let reach = VOL_R * (0.78 + 0.28 * fbm(dir * 1.7 + 3.0, 3));
    let s = max(1.0 - r / reach, 0.0);
    var h = VOL_H * pow(s, 2.3);
    // Gullies radiating from the summit, cut deeper lower down, and the
    // lumpy lobes of old lava flows between them.
    let g = ridged(dir * 13.0 + vec2f(r * 0.0011, 0.0), min(octaves, 4));
    h -= 75.0 * g * smoothstep(60.0, 700.0, r) * s;
    h += 22.0 * eroded(xz / 420.0, min(octaves, 7)) * smoothstep(150.0, 1200.0, r);
    // The crater.
    h -= 55.0 * smoothstep(170.0, 40.0, r);
    return h;
}

// The ice is designed at the scale of a plain ending 560 m out and scaled
// up about the eye to sit behind the real one, which keeps its size on screen.
const ICE_K = BACK_Z / 560.0;

// The ice sheet on the night edge. Its snout ends in a low cliff just
// behind the plain on the right, and its surface climbs away into the
// distance as a long, crevassed slope. To the west its margin pulls back
// and thins into the hills. 0 where there is no ice.
fn ice_h(world: vec2f, octaves: i32) -> f32 {
    let sx = screen_x(world);
    let mask = smoothstep(2.5, 3.0, sx);
    if (mask <= 0.0) {
        return 0.0;
    }
    let xz = world / ICE_K;
    let x_edge = (3.05 - CAM_X) * 560.0 / FOCAL;
    let back = max(x_edge - xz.x, 0.0) * 1.4;
    // The snout wanders, bulging and notched where blocks have calved.
    let front = back_z(sx) / ICE_K + 6.0 + back + 45.0 * fbm(vec2f(xz.x * 0.005, 1.3), 3) + 12.0 * fbm(vec2f(xz.x * 0.03, 7.3), 3)
        + 3.0 * fbm(vec2f(xz.x * 0.2, 2.0), 2);
    let into = xz.y - front;
    let cliff = smoothstep(-3.0, 3.0, into);
    let up = max(into, 0.0);
    // Broken seracs along the brink, smoothing into the slope behind.
    let seracs = 7.0 * ridged(vec2f(xz.x * 0.04, xz.y * 0.06), min(octaves, 4)) * exp(-up * 0.008);
    let slope = up * 0.07 * (1.0 - 0.45 * smoothstep(800.0, 6000.0, up));
    let surface = 22.0 + 8.0 * fbm(vec2f(xz.x * 0.01, 4.2), 3) + slope + 0.02 * up * eroded(xz / 900.0, min(octaves, 5))
        + 3.0 * eroded(xz / 120.0, min(octaves, 4));
    return mask * cliff * (surface + seracs) * ICE_K;
}

fn land_h(xz: vec2f, octaves: i32) -> f32 {
    let z = xz.y;
    let u = view_coords(xz);
    let sx = u.x;

    // Rolling foothills just past the plain, rising out of a low basin.
    let roll = eroded(u * vec2f(2.4, 1.8) + 11.0, octaves);
    let edge = back_z(sx);
    var h = z * 0.032 * clamp(0.3 + 0.7 * roll, 0.0, 1.2) * smoothstep(edge * 1.03, edge * 1.7, z);

    // Foothills and mid ranges, a kilometre or two out to the volcano's distance.
    let mid_e = 0.035 + 0.075 * ridged(vec2f(sx * 2.3, 8.1), 3)
        * smoothstep(-0.2, 0.6, eroded(u * vec2f(1.1, 0.9) + 3.0, 3));
    // Real ridges with faces turned toward the eye, not ramps rising with
    // distance, so the light can model them.
    let mid_shape = 0.15 + 0.85 * ridged(u * vec2f(2.6, 2.2) + 7.0, min(octaves, 5)) + 0.25 * eroded(u * vec2f(6.0, 5.0) + 2.0, octaves);
    // (Fading from far below zero, so the blend never lifts the land where they are absent.)
    let mid = mix(-300.0, elev_h(mid_e * mid_shape, z), smoothstep(2200.0, 5500.0, z));
    h = smax(h, mid, 30.0);

    // Day-side mesas: soft rock eroded into flat-topped buttes and terraces.
    let mesa_zone = smoothstep(1.1, 0.6, sx) * smoothstep(2300.0, 4300.0, z);
    if (mesa_zone > 0.0) {
        let tier = z * 0.018;
        let k = h / tier;
        let stepped = (floor(k) + smoothstep(0.55, 0.95, fract(k))) * tier;
        h = mix(h, stepped, mesa_zone * 0.85);
    }

    // The great far ranges, cresting along far_top.
    let far_e = (far_top(sx) - HORIZON)
        * (0.3 + 0.85 * ridged(u * vec2f(2.4, 1.9) + 1.0, min(octaves, 5)) + 0.18 * eroded(u * vec2f(5.2, 3.4) + 1.0, octaves));
    let far = mix(-1000.0, elev_h(far_e, z), smoothstep(22000.0, 34000.0, z));
    h = smax(h, far, 80.0);

    h = smax(h, volcano_h(xz, octaves), 40.0);
    return max(h, 0.0);
}

fn terrain_h(xz: vec2f, octaves: i32) -> f32 {
    return max(land_h(xz, octaves), ice_h(xz, octaves));
}

// ---------------------------------------------------------------- light

fn sun_light(x: f32) -> vec3f {
    let day = daylight(x);
    // Through the twilight the sun is on the horizon, its light reddened by
    // all the air it crosses: it skims the land in rose and magenta,
    // catching the slopes that face it like alpenglow.
    let night = nightness(x);
    let glow = smoothstep(0.12, 0.38, night) * smoothstep(0.85, 0.5, night);
    return vec3f(3.4, 1.45, 0.5) * day * day * 1.15 + vec3f(0.95, 0.24, 0.36) * glow * 0.55;
}

fn sky_light(x: f32) -> vec3f {
    return band3(vec3f(0.45, 0.2, 0.14), vec3f(0.18, 0.07, 0.25), vec3f(0.035, 0.05, 0.11), nightness(x));
}

// Light from the dark half of the sky, on faces turned toward the night:
// cold indigo, which sets off the warm light from the day side.
fn night_sky_light(x: f32, n: vec3f) -> vec3f {
    // From the glacier's glow to the east and the galaxy and ringed planet overhead.
    return vec3f(0.02, 0.05, 0.14) * smoothstep(0.2, 0.6, nightness(x)) * (clamp(0.3 + 0.7 * n.x, 0.0, 1.0) + 0.4 * max(n.y, 0.0));
}

// Soft shadow toward the sun, cast by land within a few kilometres. (The
// far ranges would shade everything at so low a sun; leaving them out keeps
// the light raking across the land instead.)
fn sun_shadow(pos: vec3f, t0: f32) -> f32 {
    let l = normalize(SUN_DIR);
    var res = 1.0;
    var t = t0 * 0.004 + 2.0;
    let reach = 1500.0 + t0 * 0.25;
    for (var i = 0; i < 40; i++) {
        let q = pos + l * t;
        let h = q.y - terrain_h(q.xz, 4);
        res = min(res, 16.0 * h / t);
        if (res < 0.01 || q.y > 6000.0 || t > reach) {
            break;
        }
        t += clamp(h * 0.6, t * 0.02 + 1.0, 2000.0);
    }
    return smoothstep(0.0, 1.0, res);
}

// The air's own colour where a ray at screen p ends: the horizon sky,
// brightest toward the sun.
fn haze_color(p: vec2f) -> vec3f {
    let q = vec2f(p.x, max(p.y, HORIZON + 0.02));
    return sky_base(q) + sun_glow(q) * 0.65;
}

// Thick air: dense near the ground, thinning with height.
fn fog_amount(ro: vec3f, rd: vec3f, t: f32) -> f32 {
    let k = 1.0 / 24000.0;
    let fall = 1.0 / 1800.0;
    let a = k * exp(-ro.y * fall);
    let b = max(abs(rd.y) * t * fall, 1e-4);
    let integral = select((1.0 - exp(-b)) / b, (exp(b) - 1.0) / b, rd.y < 0.0);
    return 1.0 - exp(-a * t * integral);
}

// ---------------------------------------------------------------- surfaces

// Water, with how much of the pixel it covers: 0.4 (a sliver) .. 0.59 (all).
fn water_code(cover: f32) -> f32 {
    return 0.4 + 0.19 * cover;
}

fn lava_code(run: f32) -> f32 {
    return 0.19 + 0.12 * clamp(run / 0.45, 0.0, 1.0);
}

fn shade_land(p: vec2f, ro: vec3f, rd: vec3f, t: f32) -> vec4f {
    let pos = ro + rd * t;
    let xz = pos.xz;
    let sx = p.x;
    let night = nightness(sx);
    let day = daylight(sx);

    let e = max(t * 0.0005, 0.5);
    let hi = 11;
    var n = normalize(vec3f(
        terrain_h(xz - vec2f(e, 0.0), hi) - terrain_h(xz + vec2f(e, 0.0), hi),
        2.0 * e,
        terrain_h(xz - vec2f(0.0, e), hi) - terrain_h(xz + vec2f(0.0, e), hi)));

    let land = land_h(xz, 6);
    let ice = ice_h(xz, 6);
    let on_ice = ice > land + 0.5;
    let vol = volcano_h(xz, 4);
    let on_volcano = vol > land - 5.0 && vol > 0.0;
    // Texture scale follows the pixel's footprint, so rock reads as rock at
    // every distance without shimmering.
    let ts = max(t * 0.0008, 0.5);
    let steep = 1.0 - smoothstep(0.35, 0.8, n.y);
    // A heightfield stretches anything painted from above down its cliffs
    // into streaks, so faces get their own relief: ledges and joints in
    // the rock, broken by the strata. Far off, where whole faces sit at one
    // distance, the same breaks up gullies that would run down the screen
    // in straight columns.
    let facet = max(steep, 0.6 * smoothstep(4000.0, 15000.0, t) * (1.0 - smoothstep(0.85, 0.97, n.y)));
    if (facet > 0.0) {
        let fq = vec2f(xz.x + xz.y * 0.7, pos.y * 1.6) / (ts * 6.0);
        let b = noised(fq);
        let b2 = noised(fq * 2.7 + 9.0);
        let bump = (b.yz + 0.5 * b2.yz) * 0.35 * facet;
        n = normalize(n + vec3f(bump.x, bump.y * 0.6, bump.x * 0.5));
    }
    let flat = smoothstep(0.55, 0.85, n.y);
    let grain = tri_fbm(pos, n, ts * 2.0, 4);

    // Rock: sandstone banded by strata on the day edge, rust, ochre, cream
    // and maroon; purple-grey basalt through the twilight, stained copper
    // green and sulphur in places; slate under the night.
    let warp = fbm(xz * 0.004, 2) * 3.0;
    let strata = fbm(vec2f(pos.y * 0.09 + warp, 0.5), 3);
    let fine_strata = fbm(vec2f(pos.y * 0.5 + warp * 2.0, 3.5), 2);
    var sand_rock = mix(vec3f(0.36, 0.13, 0.055), vec3f(0.58, 0.3, 0.12), strata);
    sand_rock = mix(sand_rock, vec3f(0.62, 0.48, 0.32), smoothstep(0.6, 0.7, fine_strata) * 0.6);
    sand_rock = mix(sand_rock, vec3f(0.22, 0.06, 0.05), smoothstep(0.38, 0.3, fine_strata) * 0.5);
    sand_rock *= 0.75 + 0.5 * grain;
    // Dark varnish streaks hanging down the cliffs, where water once ran.
    let varnish = smoothstep(0.5, 0.75, fbm(vec2f((xz.x + xz.y * 0.7) / (ts * 9.0), pos.y / (ts * 90.0)), 3)) * steep;
    sand_rock = mix(sand_rock, sand_rock * vec3f(0.3, 0.22, 0.22), varnish * 0.75);
    let region = fbm(xz / 2600.0 + 4.0, 3);
    var basalt = vec3f(0.11, 0.075, 0.09);
    basalt = mix(basalt, vec3f(0.05, 0.11, 0.09), smoothstep(0.6, 0.72, region) * 0.8);
    basalt = mix(basalt, vec3f(0.17, 0.12, 0.05), smoothstep(0.34, 0.26, region) * 0.6);
    basalt *= 0.7 + 0.6 * grain;
    let slate = vec3f(0.06, 0.065, 0.085) * (0.7 + 0.6 * grain);
    var alb = band3(sand_rock, basalt, slate, night);
    // Loose dust and scree on gentle ground, darker in the twilight.
    let dust = band3(vec3f(0.52, 0.26, 0.11), vec3f(0.16, 0.09, 0.12), vec3f(0.08, 0.085, 0.11), night);
    alb = mix(alb, dust * (0.85 + 0.3 * grain), flat * 0.7);
    // Scrub: dark alien thickets on the gentle slopes of the twilight
    // hills, violet and teal, thinning toward both edges.
    let scrub_zone = smoothstep(0.2, 0.42, night) * smoothstep(0.85, 0.6, night) * flat;
    if (scrub_zone > 0.0) {
        let clumps = smoothstep(0.45, 0.62, fbm(xz / (ts * 14.0) + 7.0, 4)) * smoothstep(0.35, 0.55, fbm(xz / 900.0, 3));
        let hue = fbm(xz / 1500.0 + 2.0, 2);
        let scrub = mix(vec3f(0.07, 0.02, 0.09), vec3f(0.01, 0.07, 0.065), smoothstep(0.4, 0.6, hue));
        alb = mix(alb, scrub * (0.6 + 0.8 * grain), clumps * scrub_zone * 0.85);
    }
    // Near ground shares the plain's soil, so the two meet without a seam.
    let near = smoothstep(2500.0, 700.0, t) * smoothstep(0.75, 0.95, n.y);
    if (near > 0.0) {
        alb = mix(alb, soil_albedo(xz, night, t / (FOCAL * 2160.0)), near);
    }
    if (on_volcano) {
        // Black tephra, with red oxidised ash high on the cone.
        let r = length(xz - vec2f(VOL_X, VOL_Z)) / VOL_R;
        let tephra = mix(vec3f(0.2, 0.06, 0.035), vec3f(0.05, 0.04, 0.045), smoothstep(0.05, 0.35, r));
        alb = mix(alb, tephra * (0.7 + 0.6 * grain), smoothstep(-5.0, 5.0, vol - land + 5.0));
    }
    // Snow settles on gentle ground on the night side, highest first.
    let snow_line = mix(900.0, 60.0, smoothstep(0.55, 0.85, night));
    let snow = smoothstep(0.62, 0.8, n.y + 0.15 * grain) * smoothstep(snow_line, snow_line + 300.0, pos.y)
        * smoothstep(0.45, 0.7, night);
    alb = mix(alb, vec3f(0.75, 0.8, 0.95), snow);

    let l = normalize(SUN_DIR);
    let dif = max(dot(n, l), 0.0);
    var shadow = 0.0;
    let sun = sun_light(sx);
    if (dif > 0.0 && max(sun.r, sun.g) > 0.01) {
        shadow = sun_shadow(pos + n * 1.0, t);
    }
    let sky = sky_light(sx) * (0.55 + 0.45 * n.y) + night_sky_light(sx, n);
    // Light bounced off the sunlit land, and the sun's glow through the air
    // reaching faces turned away from it.
    let bounce = sun * 0.06 * clamp(0.5 - 0.5 * n.y, 0.0, 1.0);
    let back = vec3f(0.25, 0.08, 0.03) * day * clamp(dot(n, normalize(vec3f(l.x, 0.0, l.z))) * -0.5 + 0.5, 0.0, 1.0) * 0.4;
    var col = alb * (sun * dif * shadow + sky + bounce + back);

    var mat = M_ROCK;
    if (on_ice) {
        let ic = shade_ice(p, pos / ICE_K, n, sun * shadow, night);
        col = ic.rgb;
        mat = ic.a;
    } else if (on_volcano) {
        // Lava channels follow the gullies down from the crater.
        let d = xz - vec2f(VOL_X, VOL_Z);
        let r = length(d);
        let dir = d / max(r, 1.0);
        let wander = 0.25 * (fbm(vec2f(r * 0.004, 1.0), 2) - 0.5);
        let a = fbm(dir * 3.0 + vec2f(wander, 5.0), 2);
        let w = 0.008 + 0.012 * r / VOL_R;
        let channel = smoothstep(w, w * 0.3, abs(a - 0.5)) * smoothstep(VOL_R * 0.7, VOL_R * 0.2, r);
        let lava = channel * smoothstep(150.0, 400.0, r);
        // Baked as the dark crust of old flows; the frame pass makes them
        // glow as far as fresh lava has run.
        col *= 1.0 - 0.35 * lava;
        if (lava > 0.35) {
            mat = lava_code(r / VOL_R * 0.55);
        }
    }

    let f = fog_amount(ro, rd, t);
    col = mix(col, haze_color(p), f);
    return vec4f(col, mat);
}

// The ice cliff and the snowfields behind it, at the design scale (see
// ICE_K). The cliff is old glacier ice: split into leaning slabs and columns
// by fractures that open deep blue, banded by layers of grit laid down over
// ages, and lit from within where it is thin, so the cracks and the foot of
// the wall glow sapphire and cyan in the dark. Meltwater falls pour off the
// brink in a few places (code M_FALL, which the frame pass sets flowing).
// The slope above is wind-packed snow cut by crevasses.
const M_FALL = 0.8;

fn shade_ice(p: vec2f, pos: vec3f, n_in: vec3f, sun: vec3f, night: f32) -> vec4f {
    let face = smoothstep(0.75, 0.45, n_in.y);
    let sky = sky_light(p.x);
    // The twilight sky toward the day lights faces turned toward it.
    let warm = sky_light(2.0) * max(-n_in.x, 0.0) * 0.35;
    let gt = ground_top(p.x);
    // Height up the wall on screen, 0 at its foot.
    let up_s = max(p.y - gt, 0.0);

    // ---- the cliff
    // Split into standing columns and slabs by near-vertical fractures;
    // each column's face is turned its own way, so the light picks the wall
    // out in facets, some clear deep blue, some white with bubbles.
    let wander = 1.6 * fbm(vec2f(pos.y * 0.05, pos.x * 0.01), 3) + 0.3 * fbm(vec2f(pos.y * 0.3, pos.x * 0.05), 2);
    let cq = pos.x * 0.17 + wander;
    let col_id = floor(cq);
    let col_f = fract(cq);
    let r1 = hash21(vec2f(col_id, 5.0));
    let r2 = hash21(vec2f(col_id, 9.0));
    // Columns break across, too, every so often.
    // Breaks run slanted and ragged, never straight across.
    let tq = pos.y * (0.04 + 0.05 * r2) + r1 * 3.0 + (col_f - 0.5) * (r1 - 0.5) * 1.2
        + 0.5 * fbm(vec2f(pos.x * 0.08, col_id), 2);
    let tier = floor(tq);
    let tier_edge = min(fract(tq), 1.0 - fract(tq));
    let slab_id = hash21(vec2f(col_id, tier) + 1.3);
    let lean = (hash22(vec2f(col_id, tier) + 3.0) - 0.5) * vec2f(0.9, 0.6);
    // Faces round off toward the fractures.
    let round = (col_f - 0.5) * 0.7;
    let b = noised(vec2f(pos.x * 0.9, pos.y * 0.5));
    var n = normalize(n_in + face * (vec3f(lean.x + round, lean.y * 0.3, lean.y) + vec3f(b.y, b.z, 0.0) * 0.06));
    let slab_edge = min(col_f, 1.0 - col_f);

    // Ages of layered ice, gently folded, with dark grit between them.
    let fold = fbm(vec2f(pos.x * 0.012, 1.7), 3) * 6.0 + pos.x * 0.02;
    let layer = fbm(vec2f(pos.x * 0.05, (pos.y + fold) * 0.9), 3);
    let bubbles = fbm(vec2f(pos.x * 0.8, pos.y * 0.3) + slab_id * 7.0, 3);
    // Clearer and cloudier ice drift through each slab, so slabs differ
    // without looking like tiles.
    let cloud = fbm(vec2f(pos.x * 0.07, pos.y * 0.05) + 4.0, 3);
    let clarity = slab_id * 0.4 + cloud * 0.5 + bubbles * 0.4;
    var ice = mix(vec3f(0.02, 0.11, 0.3), vec3f(0.45, 0.62, 0.78), smoothstep(0.45, 0.95, clarity));
    ice = mix(ice, vec3f(0.08, 0.28, 0.46), smoothstep(0.55, 0.6, layer) * 0.35);
    let grit = smoothstep(0.68, 0.72, layer) * 0.35 + smoothstep(6.0, 0.5, pos.y) * 0.7;
    ice = mix(ice, vec3f(0.05, 0.04, 0.045), clamp(grit, 0.0, 0.85));
    // The fractures between columns: deep and nearly black.
    let fissure = smoothstep(0.06, 0.0, slab_edge) * smoothstep(1.5, 4.0, pos.y);
    ice = mix(ice, vec3f(0.004, 0.02, 0.05), fissure * 0.85);
    // Fine cross-breaks: thin dark lines with a pale shattered lip above.
    let crack = smoothstep(0.035, 0.0, tier_edge) * step(0.35, hash21(vec2f(col_id, tier) + 7.1)) * smoothstep(2.0, 5.0, pos.y);
    ice = mix(ice, vec3f(0.01, 0.04, 0.09), crack * 0.6);
    let bevel = 0.0;
    // Wet streaks where meltwater runs down from the brink.
    let wet = smoothstep(0.62, 0.8, fbm(vec2f(pos.x * 0.6, pos.y * 0.02), 3));
    ice = mix(ice, ice * 0.6, wet * 0.5);

    let lit = sky * (1.5 + 0.6 * n.y) + warm + sun * max(dot(n, normalize(SUN_DIR)), 0.0) * 1.4
        + night_sky_light(p.x, n) * 2.0;
    // Light carried inside the ice: in the cracks, through the clear blue
    // and at the wet foot of the wall, where the river is born.
    let inner = (smoothstep(0.6, 0.3, clarity) * 0.12 + smoothstep(5.0, 0.0, pos.y) * 0.3)
        * vec3f(0.01, 0.13, 0.3) * (0.25 + 0.75 * night);
    let cliff = ice * lit * (1.0 + bevel * 0.6) + inner;

    // ---- the slope: wind-packed snow, with transverse crevasses opening
    // blue-black, and drifts carved into ridges by the wind.
    let drift = fbm(pos.xz * vec2f(0.02, 0.006), 4);
    let sastrugi = fbm(vec2f(pos.x * 0.15 + drift * 4.0, pos.z * 0.02), 3);
    let gap = abs(fbm(vec2f(pos.x * 0.004 + drift * 0.3, pos.z * 0.025), 3) - 0.5);
    let crevasse = smoothstep(0.02, 0.006, gap) * smoothstep(40.0, 300.0, pos.z - 560.0)
        * smoothstep(0.45, 0.65, fbm(pos.xz * 0.004, 2));
    var snow = vec3f(0.74, 0.82, 0.98) * (0.8 + 0.25 * drift + 0.15 * sastrugi);
    snow = mix(snow, vec3f(0.01, 0.06, 0.16), crevasse * 0.85);
    // Broad swells of wind-scoured, bluer ice between the drifts.
    snow = mix(snow, vec3f(0.22, 0.42, 0.68), smoothstep(0.5, 0.7, fbm(pos.xz * vec2f(0.003, 0.001), 3)) * 0.55);
    let top = snow * (sky * (0.55 + 0.6 * n.y) + warm * 0.4 + sun + night_sky_light(p.x, n) * 1.5)
        + vec3f(0.0, 0.1, 0.2) * crevasse * night * 0.4;

    var col = mix(top, cliff, face);
    var code = M_ICE;

    // Meltwater falls: a few ribbons pouring off the brink, broken into
    // strands and spray.
    for (var k = 0; k < 3; k++) {
        let fx = 3.12 + f32(k) * 0.13 + 0.03 * hash21(vec2f(f32(k), 5.0));
        let width = 0.004 + 0.003 * hash21(vec2f(f32(k), 9.0));
        let strands = 0.5 + 0.5 * fbm(vec2f((p.x - fx) * 1400.0, up_s * 8.0 + f32(k)), 3);
        let fall = smoothstep(width, width * 0.3, abs(p.x - fx)) * face * strands * smoothstep(0.0, 0.006, up_s);
        if (fall > 0.05) {
            let water = vec3f(0.5, 0.62, 0.72) * (sky * 1.8 + sun * 0.5 + 0.015) + vec3f(0.0, 0.05, 0.1) * night;
            col = mix(col, water, fall * 0.75);
            code = M_FALL;
        }
    }

    // A meltwater cave at the foot where the river is born, glowing blue
    // from deep inside.
    let cx = (p.x - 3.36) / 0.03;
    let cy = (p.y - gt) / 0.03;
    let rim = 0.12 * (fbm(vec2f(p.x * 160.0, p.y * 160.0), 3) - 0.5);
    let arch = length(vec2f(cx, max(cy, 0.0))) + rim;
    let cave = smoothstep(1.0, 0.92, arch) * step(-0.2, cy) * face;
    let inside = vec3f(0.0, 0.012, 0.03) + vec3f(0.03, 0.3, 0.5) * smoothstep(1.1, 0.0, length(vec2f(cx * 0.8, cy + 0.4))) * 1.2;
    col = mix(col, inside, cave);
    col += vec3f(0.02, 0.15, 0.25) * smoothstep(1.4, 1.0, arch) * (1.0 - cave) * face * 0.25;
    return vec4f(col, select(code, M_ICE, cave > 0.5));
}

fn march(ro: vec3f, rd: vec3f, t0: f32) -> f32 {
    var t = t0;
    var prev = t0;
    for (var i = 0; i < 600; i++) {
        let pos = ro + rd * t;
        let h = pos.y - terrain_h(pos.xz, 6);
        if (h < 0.00025 * t) {
            // Refine between the last two samples.
            var a = prev;
            var b = t;
            for (var k = 0; k < 6; k++) {
                let m = 0.5 * (a + b);
                let q = ro + rd * m;
                if (q.y - terrain_h(q.xz, 6) < 0.00025 * m) {
                    b = m;
                } else {
                    a = m;
                }
            }
            return b;
        }
        if (t > 120000.0 || (pos.y > 9000.0 && rd.y > 0.0)) {
            break;
        }
        prev = t;
        t += max(h * 0.55, t * 0.004);
    }
    return -1.0;
}

// ---------------------------------------------------------------- the plain

// The plain is level where the simulation's life walks, but it is not
// smooth: sand drifts into low dunes on the day side, stones and boulders lie
// strewn in fields, old lava breaks through as flat slabs in the twilight,
// and the river cuts a shallow bed. It is ray marched like the land behind
// it, so everything on it stands up with its own silhouette and shadow, but
// all of it stays low (PLAIN_TOP) so bodies still stand on the ground.

const PLAIN_TOP = 1.4;

struct Relief {
    h: f32,     // height above the plain, metres (negative in the river bed)
    rock: f32,  // 0 loose ground, 1 stone, 2 bedrock slab
    id: f32,    // which stone, for its colour and what grows on it
    crown: f32, // 0 at a stone's foot .. 1 on its top
    water: f32, // 1 in the river, fading out across the bank
};

fn rot2(a: f32) -> mat2x2f {
    let c = cos(a);
    let s = sin(a);
    return mat2x2f(c, s, -s, c);
}

// At most one stone per cell, kept wholly inside it, so a single lookup
// finds it. `odds` is how many cells hold one. Returns height (in cells),
// id and crown.
fn stone(w: vec2f, size: f32, odds: f32, flat: f32, salt: f32) -> vec3f {
    let q = w / size;
    let c = floor(q);
    let r = pcg2(c + salt);
    if (r.x > odds) {
        return vec3f(0.0);
    }
    let o = pcg2(c + salt + 57.0);
    let stretch = 0.7 + 0.6 * fract(o.y * 7.31);
    let rad = 0.1 + 0.22 * r.y * r.y;
    let reach = rad * max(stretch, 1.0);
    let centre = c + reach + (1.0 - 2.0 * reach) * o;
    var d = rot2(o.x * 6.283) * (q - centre);
    d.x /= stretch;
    // Lumpy, broken outlines rather than discs.
    let lump = vnoise(d / rad * 2.3 + r * 40.0) - 0.5;
    let k = length(d) / rad + 0.22 * lump;
    if (k >= 1.0) {
        return vec3f(0.0);
    }
    let s = 1.0 - k * k;
    // Rounded cobbles; bigger rocks with flatter, split tops.
    // Big ones lean and are broken into a few facets, not loaves.
    let tilt = dot(d / rad, vec2f(cos(o.y * 40.0), sin(o.y * 40.0))) * 0.3 * flat;
    let facet = (vnoise(d / rad * 1.4 + r * 17.0) - 0.5) * 0.35 * flat;
    let h = rad * mix(0.75, 0.6, flat) * pow(s, mix(0.5, 0.33, flat)) * (1.0 + 0.25 * lump + tilt + facet);
    return vec3f(h, r.x * 13.0 + r.y, s);
}

// How stony the ground is here: fields of rock and stretches of clean sand.
fn rockiness(w: vec2f, sx: f32) -> f32 {
    let field = smoothstep(0.38, 0.72, fbm(w / 70.0 + 3.0, 3));
    // More around the volcano and on the night side, little on the dunes.
    let zone = 0.35 + 0.65 * max(exp(-pow((sx - VOLCANO_X) / 0.45, 2.0)), smoothstep(2.0, 3.0, sx));
    return clamp(field * zone + 0.12, 0.0, 1.0);
}

// Height and makeup of the plain at w. `lod` 0 close up, 1 far off, where
// the smallest stones are left to the texture.
fn relief(w: vec2f, lod: f32) -> Relief {
    var r: Relief;
    let sx = screen_x(w);
    let rocky = rockiness(w, sx);

    // Broad swells everywhere.
    var h = 0.18 * (fbm(w / 28.0, 3) - 0.35);

    // Dunes on the day side, their crests across the ground wind, steep on
    // the lee (day-ward) side.
    let dune_zone = smoothstep(1.35, 0.55, sx) * (1.0 - rocky * 0.8);
    if (dune_zone > 0.0) {
        let u = w.x * 0.085 + 2.4 * fbm(w * vec2f(0.012, 0.006), 3) + 0.5 * fbm(w * 0.05, 2);
        let f = fract(u);
        let prof = smoothstep(0.0, 0.78, f) * smoothstep(1.0, 0.86, f);
        h += 0.32 * dune_zone * prof * (0.5 + fbm(w / 40.0 + 8.0, 2));
    }

    // Old lava breaking through as flat-topped slabs, twilight-ward, and
    // stones and boulders in two sizes, plus cobbles close up. Whatever
    // stands highest is the surface.
    let ground = h;
    var top = h;
    let slab_zone = smoothstep(0.9, 1.6, sx) * smoothstep(0.3, 0.7, rocky);
    if (slab_zone > 0.0) {
        let n = fbm(w / 16.0 + 9.0, 4);
        let slab = smoothstep(0.56, 0.6, n) * slab_zone;
        let lift = slab * (0.22 + 0.25 * fbm(w / 6.0 + 2.0, 2));
        if (slab > 0.0 && lift > top) {
            top = lift;
            r.rock = 2.0;
            r.crown = slab;
            r.id = floor(n * 40.0);
        }
    }
    let base = max(ground, 0.0);
    let b = stone(w, 9.0, 0.16 * rocky, 1.0, 11.0);
    if (b.x > 0.0 && base + b.x * 9.0 > top) {
        top = base + b.x * 9.0;
        r.rock = 1.0;
        r.id = b.y;
        r.crown = b.z;
    }
    let s = stone(w, 2.1, 0.38 * rocky, 0.4, 3.0);
    if (s.x > 0.0 && base + s.x * 2.1 > top) {
        top = base + s.x * 2.1;
        r.rock = 1.0;
        r.id = s.y + 20.0;
        r.crown = s.z;
    }
    if (lod < 0.5) {
        let c = stone(w, 0.75, 0.5 * rocky + 0.1, 0.0, 7.0);
        if (c.x > 0.0 && base + c.x * 0.75 > top) {
            top = base + c.x * 0.75;
            r.rock = 1.0;
            r.id = c.y + 40.0;
            r.crown = c.z;
        }
    }
    // Vent chimneys: crusted cones over the hot ground, open at the top.
    let heat = vent_field(sx);
    if (heat > 0.15) {
        let vq = w / 12.0;
        let vc = floor(vq);
        let vr = pcg2(vc + 91.0);
        if (vr.x < 0.5 * heat) {
            let vo = pcg2(vc + 37.0);
            let rad = 0.11 + 0.1 * vr.y;
            let centre = vc + rad + (1.0 - 2.0 * rad) * vo;
            let d = length(vq - centre) / rad + 0.15 * (vnoise((vq - centre) / rad * 3.0 + vr * 30.0) - 0.5);
            if (d < 1.0) {
                let cone = pow(1.0 - d, 1.2) * (0.75 + 0.5 * vr.y) - 0.45 * smoothstep(0.3, 0.12, d);
                if (base + cone > top) {
                    top = base + cone;
                    r.rock = 3.0;
                    r.id = vr.y * 50.0;
                    r.crown = 1.0 - d;
                }
            }
        }
    }
    h = top;

    // The river: a shallow bed under the water, a low bank of wet shingle.
    let e = river_across(w);
    if (e < 1.8) {
        let bank = smoothstep(1.0, 1.8, e);
        h = mix(min(h, 0.05), h, bank);
        let wet = pan_wet(vec2f(sx, HORIZON - CAM_H * FOCAL / max(w.y, 1.0)));
        if (e < 1.0 && wet > 0.5) {
            h = min(h, 0.0) - 0.45 * (1.0 - e * e) - 0.03;
            r.water = 1.0;
        } else {
            r.water = smoothstep(1.8, 1.0, e) * 0.5;
        }
    }
    r.h = h;
    return r;
}

fn relief_h(w: vec2f, lod: f32) -> f32 {
    return relief(w, lod).h;
}

// Marches a ray over the plain. Returns t, or -1 if it reached the back.
fn march_plain(ro: vec3f, rd: vec3f) -> f32 {
    let down = max(-rd.y, 1e-4);
    var t = (ro.y - PLAIN_TOP) / down;
    let t_end = (ro.y + 0.6) / down;
    var prev = t;
    for (var i = 0; i < 220; i++) {
        let pos = ro + rd * t;
        let lod = step(450.0, t);
        let r = relief(pos.xz, lod);
        let surface = max(r.h, select(-9.0, -0.03, r.water > 0.99));
        let gap = pos.y - surface;
        if (gap < 0.0) {
            var a = prev;
            var b = t;
            for (var k = 0; k < 6; k++) {
                let m = 0.5 * (a + b);
                let q = ro + rd * m;
                let rq = relief(q.xz, lod);
                if (q.y - max(rq.h, select(-9.0, -0.03, rq.water > 0.99)) < 0.0) {
                    b = m;
                } else {
                    a = m;
                }
            }
            return b;
        }
        if (t > t_end) {
            break;
        }
        prev = t;
        // Never step past a stone: no further than the ground drops, and no
        // further than a stone is wide, growing with distance.
        t += clamp(gap * 0.6 / down, 0.05, 0.3 + t * 0.0012);
    }
    return t_end;
}

// Bare ground: patches of dark gravel, rust sand and ochre crust by day,
// basalt grit through the twilight, near-black volcanic sand by night, with
// the minerals of each region showing through in their own colours.
// `foot` is the size of a pixel on the ground, to fade detail too fine to show.
fn soil_albedo(w: vec2f, night: f32, foot: f32) -> vec3f {
    let soil = fbm(w / 22.0, 4);
    let geo = fbm(w / 160.0 + 5.0, 3);
    let speck = mix(0.5, fbm(w * 3.0, 3), smoothstep(0.6, 0.15, foot));
    let rust = vec3f(0.42, 0.16, 0.06);
    let ochre = vec3f(0.55, 0.33, 0.12);
    let gravel = vec3f(0.09, 0.07, 0.07);
    let bleached = vec3f(0.62, 0.45, 0.3);
    var day_ground = mix(mix(gravel, rust, smoothstep(0.35, 0.6, soil)), ochre, smoothstep(0.62, 0.75, soil) * 0.6);
    // Pale gypsum flats and dark hematite sands in broad regions.
    day_ground = mix(day_ground, bleached, smoothstep(0.62, 0.75, geo) * 0.55);
    day_ground = mix(day_ground, vec3f(0.2, 0.05, 0.05), smoothstep(0.36, 0.26, geo) * 0.5);
    var twi_ground = mix(vec3f(0.075, 0.05, 0.065), vec3f(0.17, 0.08, 0.1), smoothstep(0.4, 0.7, soil));
    // Olivine sand, glassy green-black, and violet ash.
    twi_ground = mix(twi_ground, vec3f(0.05, 0.085, 0.06), smoothstep(0.6, 0.72, geo) * 0.7);
    twi_ground = mix(twi_ground, vec3f(0.15, 0.07, 0.17), smoothstep(0.34, 0.24, geo) * 0.6);
    var night_ground = mix(vec3f(0.04, 0.042, 0.055), vec3f(0.08, 0.085, 0.11), smoothstep(0.4, 0.7, soil));
    night_ground = mix(night_ground, vec3f(0.11, 0.12, 0.16), smoothstep(0.62, 0.75, geo) * 0.6);
    return band3(day_ground, twi_ground, night_ground, night) * (0.7 + 0.6 * speck);
}

// Where vent chimneys stand: the vent fields, a little wider than the hot
// ground itself.
fn vent_field(x: f32) -> f32 {
    let a = exp(-pow((x - VOLCANO_X - 0.09) / 0.1, 2.0));
    let b = 0.7 * exp(-pow((x - 2.86) / 0.1, 2.0));
    let c = 0.9 * exp(-pow((x - 3.24) / 0.1, 2.0));
    return max(a, max(b, c));
}

// Hot ground over the vent fields the simulation knows (vesper-terrain VENTS).
fn vent_heat(x: f32) -> f32 {
    let a = exp(-pow((x - VOLCANO_X - 0.09) / 0.05, 2.0));
    let b = 0.7 * exp(-pow((x - 2.86) / 0.05, 2.0));
    let c = 0.9 * exp(-pow((x - 3.24) / 0.05, 2.0));
    return max(a, max(b, c));
}

// Rock that stones are made of, by region: red sandstone and cream
// limestone by day, purple-black basalt with copper-green stains in the
// twilight, blue-grey slate under the night.
fn stone_albedo(id: f32, night: f32, grain: f32) -> vec3f {
    let k = fract(id * 0.618);
    let day = mix(mix(vec3f(0.32, 0.11, 0.05), vec3f(0.5, 0.36, 0.22), smoothstep(0.55, 0.8, k)),
        vec3f(0.07, 0.045, 0.04), smoothstep(0.3, 0.12, k));
    let twi = mix(mix(vec3f(0.08, 0.055, 0.07), vec3f(0.17, 0.1, 0.12), smoothstep(0.5, 0.8, k)),
        vec3f(0.06, 0.13, 0.1), smoothstep(0.18, 0.05, k));
    let nite = mix(vec3f(0.06, 0.065, 0.085), vec3f(0.13, 0.14, 0.17), smoothstep(0.5, 0.85, k));
    return band3(day, twi, nite, night) * (0.7 + 0.6 * grain);
}

// Crusts that grow on stone: rosettes in orange, teal, magenta and gold,
// thickest in the twilight, and glowing faintly where the night falls.
fn lichen(w: vec2f, id: f32, crown: f32, night: f32) -> vec4f {
    let zone = smoothstep(0.15, 0.45, night) * (1.0 - 0.6 * smoothstep(0.75, 1.0, night));
    if (zone <= 0.0 || crown <= 0.0) {
        return vec4f(0.0);
    }
    let q = w * 2.6 + id * 3.1;
    let spread = smoothstep(0.52, 0.66, fbm(q, 3) + 0.25 * crown - 0.12);
    let k = fract(id * 0.371);
    var col = mix(vec3f(0.55, 0.16, 0.02), vec3f(0.02, 0.32, 0.3), smoothstep(0.25, 0.35, k));
    col = mix(col, vec3f(0.42, 0.03, 0.3), smoothstep(0.6, 0.7, k));
    col = mix(col, vec3f(0.5, 0.38, 0.03), smoothstep(0.85, 0.92, k));
    // Concentric growth rings in each rosette.
    let rings = 0.75 + 0.25 * sin(fbm(q, 3) * 60.0);
    return vec4f(col * rings, spread * zone * smoothstep(0.0, 0.3, crown) * step(0.35, fract(id * 0.73)));
}

// Voronoi edge distance: how close w is to a crack between cells.
fn cracks(q: vec2f, salt: f32) -> f32 {
    let c0 = floor(q);
    let f0 = fract(q);
    var d1 = 9.0;
    var d2 = 9.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2f(f32(i), f32(j));
            let d = length(f0 - o - hash22(c0 + o + salt));
            if (d < d1) {
                d2 = d1;
                d1 = d;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return d2 - d1;
}

// The plain at screen point p, seen in perspective.
fn ground(p: vec2f, gt: f32) -> vec4f {
    let night = nightness(p.x);
    let day = daylight(p.x);
    let ro = vec3f(0.0, CAM_H, 0.0);
    let rd = view_ray(p);
    let t = march_plain(ro, rd);
    let pos = ro + rd * t;
    let w = pos.xz;
    let z = w.y;
    let lod = step(450.0, t);
    let rel = relief(w, lod);
    let foot = z / (FOCAL * 2160.0);
    let pan = smoothstep(0.95, 0.4, p.x);

    // How much of this pixel is open water, from a few samples of the flat
    // water surface, so the shore is smooth rather than stepped.
    var cover = 0.0;
    let px = 1.0 / P.world_size.y;
    for (var i = 0; i < 4; i++) {
        let off = (vec2f(f32(i & 1), f32(i >> 1u)) - 0.5) * px * 0.5;
        let q = p + off;
        let e = river_across(plain_point(q));
        cover += select(0.0, 0.25, e < 1.0 && pan_wet(q) > 0.5);
    }
    var water = vec3f(0.0);
    if (cover > 0.0) {
        // The colour seen into the water. The frame pass lays the sky and
        // land reflected on its surface over this.
        let wf = plain_point(p);
        let e = min(river_across(wf), 1.0);
        let depth_in = (1.0 - e * e) * (1.0 - pan * 0.8);
        let bed = band3(vec3f(0.3, 0.16, 0.07), vec3f(0.04, 0.08, 0.08), vec3f(0.005, 0.03, 0.05), night)
            * (0.75 + 0.5 * fbm(wf * 1.5, 3));
        let deep = band3(vec3f(0.03, 0.1, 0.08), vec3f(0.005, 0.06, 0.08), vec3f(0.0, 0.04, 0.09), night);
        water = mix(bed, deep, smoothstep(0.0, 0.8, depth_in));
        // Shallow and bright over salt in the pan.
        water = mix(water, vec3f(0.5, 0.32, 0.18) * (0.3 + 0.7 * day), pan * 0.5);
        // Glacier meltwater carries a living glow, strongest in the channel.
        water += vec3f(0.0, 0.1, 0.16) * smoothstep(0.4, 0.9, night) * depth_in * 0.6;
        if (cover > 0.99) {
            return vec4f(water, water_code(1.0));
        }
    }

    // Surface normal from the relief, plus grain finer than it.
    let e = 0.02 + t * 0.0002;
    let hx = relief_h(w + vec2f(e, 0.0), lod) - relief_h(w - vec2f(e, 0.0), lod);
    let hz = relief_h(w + vec2f(0.0, e), lod) - relief_h(w - vec2f(0.0, e), lod);
    let fine_fade = smoothstep(0.5, 0.05, foot);
    let fine = noised(w * 2.2) * fine_fade;
    let mid = noised(w * 0.45 + 5.0);
    var g = vec2f(hx, hz) / (2.0 * e) + fine.yz * 0.1 + mid.yz * 0.05;
    // Wind ripples in the sand, across the ground wind.
    let ripple_zone = smoothstep(1.4, 0.6, p.x) * step(rel.rock, 0.5);
    if (ripple_zone > 0.0) {
        // They bend and fork, and only lie on patches of loose sand.
        let warp = fbm(w * vec2f(0.12, 0.25), 3);
        let rp = w.x * (7.0 + 4.0 * warp) + 6.0 * fbm(w * vec2f(0.25, 0.5) + 2.0, 2) + w.y * (warp - 0.5) * 2.0;
        let patch_sand = smoothstep(0.42, 0.62, fbm(w * 0.05 + 11.0, 3));
        let ridge = cos(rp) + 0.35 * cos(rp * 2.1 + 1.3);
        g.x += ridge * 0.1 * ripple_zone * patch_sand * smoothstep(0.25, 0.04, foot);
    }
    let n = normalize(vec3f(-g.x, 1.0, -g.y));

    let grain = fbm(w * 1.7, 3);
    var alb = soil_albedo(w, night, foot);
    var glow = vec3f(0.0);
    if (rel.rock > 2.5) {
        // A vent chimney: dark scoria crusted sulphur-yellow and white
        // toward the lip, glowing inside its throat.
        let heat_here = vent_field(p.x);
        var crust = mix(vec3f(0.06, 0.035, 0.03), vec3f(0.5, 0.4, 0.06), smoothstep(0.45, 0.75, rel.crown + 0.2 * grain));
        crust = mix(crust, vec3f(0.62, 0.6, 0.5), smoothstep(0.7, 0.8, rel.crown + 0.15 * grain) * 0.5);
        alb = crust * (0.75 + 0.5 * grain);
        let throat = smoothstep(0.8, 0.9, rel.crown);
        alb = mix(alb, vec3f(0.02, 0.01, 0.01), throat);
        // The lip is lit from below by the throat, and hot rock glows dully
        // down the flanks in cracks.
        let flank_cracks = smoothstep(0.6, 0.75, fbm(w * 3.0 + rel.id, 3)) * smoothstep(0.2, 0.6, rel.crown);
        glow += vec3f(5.0, 1.2, 0.15) * throat * heat_here * (0.6 + 0.4 * grain)
            + vec3f(1.6, 0.4, 0.06) * smoothstep(0.6, 0.85, rel.crown) * heat_here * 0.5
            + vec3f(2.0, 0.45, 0.06) * flank_cracks * heat_here * 0.35;
    } else if (rel.rock > 1.5) {
        // Slab: dark ropy basalt, split into plates.
        let plates = cracks(w / 2.5, 21.0);
        var slab = band3(vec3f(0.12, 0.06, 0.05), vec3f(0.06, 0.045, 0.06), vec3f(0.05, 0.05, 0.065), night)
            * (0.75 + 0.5 * grain);
        slab *= mix(0.25, 1.0, smoothstep(0.02, 0.08, plates));
        alb = mix(alb, slab, smoothstep(0.0, 0.25, rel.crown));
    } else if (rel.rock > 0.5) {
        var st = stone_albedo(rel.id, night, grain);
        // Desert varnish: dark streaks down the sides of sunburnt stones.
        let varnish = smoothstep(0.45, 0.7, fbm(vec2f(w.x * 6.0 + rel.id, pos.y * 1.5), 2)) * (1.0 - rel.crown);
        st = mix(st, st * 0.35, varnish * 0.7 * (1.0 - night));
        alb = mix(alb, st, smoothstep(0.0, 0.06, rel.crown));
        let li = lichen(w, rel.id, rel.crown, night);
        alb = mix(alb, li.rgb, li.a);
        glow += li.rgb * li.a * smoothstep(0.55, 0.95, night) * vec3f(0.3, 0.9, 1.2) * 0.25;
        // Frost on the tops of stones under the night.
        let frost_top = smoothstep(0.6, 0.9, night) * smoothstep(0.5, 0.9, n.y) * smoothstep(0.3, 0.8, rel.crown);
        alb = mix(alb, vec3f(0.5, 0.56, 0.7), frost_top * 0.6);
    }

    // Glow moss: colonies of tiny luminous life carpeting the ground under
    // the night, each speck its own colour, thickest in the hollows.
    let moss_zone = smoothstep(0.55, 0.85, night) * step(rel.rock, 0.5);
    if (moss_zone > 0.0) {
        let colony = smoothstep(0.55, 0.75, fbm(w / 14.0 + 31.0, 3)) * moss_zone;
        if (colony > 0.0) {
            let mq = w * 4.0;
            let mc = floor(mq);
            let mr = pcg2(mc + 5.0);
            let speck = step(0.55, mr.x) * smoothstep(0.35, 0.05, length(fract(mq) - 0.25 - 0.5 * pcg2(mc + 9.0)));
            let hue = fbm(w / 40.0 + 2.0, 2) + 0.3 * mr.y;
            let mcol = mix(vec3f(0.05, 0.9, 0.8), vec3f(0.9, 0.15, 0.85), smoothstep(0.45, 0.75, hue));
            // Specks too small to see blend into a faint glowing film,
            // brightest where the colony is densest.
            let fine = smoothstep(0.06, 0.03, foot);
            let film = 0.05 + 0.1 * smoothstep(0.6, 0.9, fbm(w / 5.0 + 3.0, 3));
            glow += mcol * colony * mix(film, speck * 0.6, fine);
            alb = mix(alb, vec3f(0.02, 0.06, 0.06), colony * 0.5);
        }
    }

    // Small mineral crystals in jewel tones, here and there.
    let cq = w * 9.0;
    let ccell = floor(cq);
    let crystal = step(0.996, hash21(ccell)) * smoothstep(0.35, 0.1, length(fract(cq) - 0.5)) * fine_fade
        * smoothstep(0.4, 0.7, fbm(w / 9.0, 2));
    let jewel = band3(vec3f(0.05, 0.7, 0.55), vec3f(0.6, 0.08, 0.7), vec3f(0.03, 0.3, 0.7), hash21(ccell + 2.0));
    alb = mix(alb, jewel, crystal * 0.7);

    // Wet, darkened banks with moss mats in alien colours.
    let bank = rel.water * (1.0 - pan);
    alb *= 1.0 - bank * 0.6;
    let moss = band3(vec3f(0.06, 0.16, 0.05), vec3f(0.14, 0.03, 0.17), vec3f(0.01, 0.08, 0.13), night);
    let mat = smoothstep(0.48, 0.7, fbm(w * 0.5, 3)) * smoothstep(0.1, 0.5, bank);
    alb = mix(alb, moss * (0.7 + 0.6 * grain), mat * 0.85);

    // The salt pan: a cracked white crust where the river has boiled dry.
    let e_r = river_across(w);
    let crust = pan * smoothstep(1.4, 0.9, e_r + fbm(w * 0.5, 2) * 0.3);
    if (crust > 0.0) {
        let crack = smoothstep(0.07, 0.0, cracks(w / 1.3, 0.0));
        var salt = vec3f(0.72, 0.52, 0.36) * (0.85 + 0.25 * fbm(w * 2.0, 2));
        salt = mix(salt, salt * 0.25, crack);
        alb = mix(alb, salt, crust * step(rel.rock, 0.5) * 0.9);
    }

    // Over the vent fields the ground is split by fissures glowing with heat,
    // crusted yellow with sulphur and stained green with copper.
    let heat = vent_heat(p.x) * smoothstep(0.4, 0.7, fbm(w / 25.0 + 3.0, 3));
    if (heat > 0.01) {
        let edge = cracks(w / 11.0, 40.0);
        // (Far off the cracks would only smear into streaks, so they fade.)
        let fissure = smoothstep(0.02, 0.0, edge) * step(rel.rock, 0.5) * smoothstep(0.4, 0.65, fbm(w / 6.0, 2))
            * smoothstep(600.0, 300.0, z);
        let sulphur = smoothstep(0.2, 0.06, edge) * heat * smoothstep(0.5, 0.7, fbm(w / 3.0, 2));
        alb = mix(alb, vec3f(0.42, 0.33, 0.04), sulphur * 0.6);
        glow += vec3f(3.0, 0.7, 0.1) * fissure * heat;
    }

    // Light: the low sun with real shadows from stones and dunes, the sky,
    // and the stones' own occlusion of the ground around them.
    let l = normalize(SUN_DIR);
    let sun = sun_light(p.x);
    var shade = 1.0;
    let dif = max(dot(n, l), 0.0);
    if (dif > 0.0 && max(sun.r, sun.g) > 0.01) {
        var st = 0.06;
        for (var i = 0; i < 18; i++) {
            let q = pos + n * 0.01 + l * st;
            let hh = relief_h(q.xz, 1.0);
            shade = min(shade, 10.0 * (q.y - hh) / st);
            if (shade < 0.02 || q.y > PLAIN_TOP) {
                break;
            }
            st += clamp((q.y - hh) * 0.8, 0.05, 0.6);
        }
        shade = smoothstep(0.0, 1.0, clamp(shade, 0.0, 1.0));
    }
    var occ = 0.0;
    for (var i = 0; i < 4; i++) {
        let a = f32(i) * 1.5708 + 0.4;
        let q = w + vec2f(cos(a), sin(a)) * 0.35;
        occ += max(relief_h(q, lod) - pos.y, 0.0);
    }
    let ao = 1.0 - clamp(occ * 0.6, 0.0, 0.55);
    let sky = (sky_light(p.x) * (0.6 + 0.4 * n.y) + night_sky_light(p.x, n)) * ao;
    var col = alb * (sun * dif * shade * 0.95 + sky + sun * 0.07 * ao) + glow;
    // Wet ground mirrors a little of the bright sky.
    col += haze_color(vec2f(p.x, 2.0 * HORIZON - p.y)) * bank * 0.1;

    // A little of the air between here and the eye.
    col = mix(col, haze_color(vec2f(p.x, HORIZON)), clamp(z / 9000.0, 0.0, 0.2));
    // The nearest ground falls a little into shadow.
    col *= 0.6 + 0.4 * smoothstep(0.0, 0.1, p.y);
    if (cover > 0.0) {
        return vec4f(mix(col, water, cover), water_code(cover));
    }
    return vec4f(col, M_ROCK);
}

@fragment
fn fs_bake(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    let gt = ground_top(p.x);
    if (p.y < gt) {
        return ground(p, gt);
    }

    let ro = vec3f(0.0, CAM_H, 0.0);
    let rd = view_ray(p);
    // Start where the ray passes the plain's back edge.
    let t0 = plain_z(gt) / rd.z;
    let t = march(ro, rd, t0);
    if (t < 0.0) {
        return vec4f(sky_color(p), M_SKY);
    }
    return shade_land(p, ro, rd, t);
}
