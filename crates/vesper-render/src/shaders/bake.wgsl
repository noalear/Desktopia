// Bake pass: the slow-changing landscape, rendered rarely into an fp16 texture.
//
// Behind the plain lies a real heightfield seen through the camera in
// common.wgsl: ray marched, lit by the low sun with soft shadows and sky
// light, and fogged by Vesper's thick air. The plain in front is the
// simulation's ground, textured in perspective. Baking happens only when
// the season moves the light, so this pass can afford to be expensive.
//
// Alpha carries a material code the frame pass animates:
// 0 rock, 0.19..0.31 lava (how far down the flow, see lava_code), 0.5 water,
// 0.75 ice, 1 sky.

const M_ROCK = 0.0;
const M_WATER = 0.5;
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

// Value noise with analytic derivatives, -1..1.
fn noised(p: vec2f) -> vec3f {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    let a = hash21(i);
    let b = hash21(i + vec2f(1.0, 0.0));
    let c = hash21(i + vec2f(0.0, 1.0));
    let d = hash21(i + vec2f(1.0, 1.0));
    let k1 = b - a;
    let k2 = c - a;
    let k4 = a - b - c + d;
    let v = a + k1 * u.x + k2 * u.y + k4 * u.x * u.y;
    return vec3f(2.0 * v - 1.0, 2.0 * du * vec2f(k1 + k4 * u.y, k2 + k4 * u.x));
}

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

// The ice sheet on the night edge. Its snout ends in a low cliff just
// behind the plain on the right, and its surface climbs away into the
// distance as a long, crevassed slope. To the west its margin pulls back
// and thins into the hills. 0 where there is no ice.
fn ice_h(xz: vec2f, octaves: i32) -> f32 {
    let sx = screen_x(xz);
    let mask = smoothstep(2.5, 3.0, sx);
    if (mask <= 0.0) {
        return 0.0;
    }
    let x_edge = (3.05 - CAM_X) * 560.0 / FOCAL;
    let back = max(x_edge - xz.x, 0.0) * 1.4;
    // The snout wanders, bulging and notched where blocks have calved.
    let front = 560.0 + back + 45.0 * fbm(vec2f(xz.x * 0.005, 1.3), 3) + 12.0 * fbm(vec2f(xz.x * 0.03, 7.3), 3)
        + 3.0 * fbm(vec2f(xz.x * 0.2, 2.0), 2);
    let into = xz.y - front;
    let cliff = smoothstep(-3.0, 3.0, into);
    let up = max(into, 0.0);
    // Broken seracs along the brink, smoothing into the slope behind.
    let seracs = 7.0 * ridged(vec2f(xz.x * 0.04, xz.y * 0.06), min(octaves, 4)) * exp(-up * 0.008);
    let slope = up * 0.07 * (1.0 - 0.45 * smoothstep(800.0, 6000.0, up));
    let surface = 22.0 + 8.0 * fbm(vec2f(xz.x * 0.01, 4.2), 3) + slope + 0.02 * up * eroded(xz / 900.0, min(octaves, 5))
        + 3.0 * eroded(xz / 120.0, min(octaves, 4));
    return mask * cliff * (surface + seracs);
}

fn land_h(xz: vec2f, octaves: i32) -> f32 {
    let z = xz.y;
    let u = view_coords(xz);
    let sx = u.x;

    // Rolling foothills just past the plain, rising out of a low basin.
    let roll = eroded(u * vec2f(2.4, 1.8) + 11.0, octaves);
    var h = z * 0.032 * clamp(0.3 + 0.7 * roll, 0.0, 1.2) * smoothstep(540.0, 900.0, z);

    // Foothills and mid ranges, a kilometre or two out to the volcano's distance.
    let mid_e = 0.035 + 0.075 * ridged(vec2f(sx * 2.3, 8.1), 3)
        * smoothstep(-0.2, 0.6, eroded(u * vec2f(1.1, 0.9) + 3.0, 3));
    let mid_shape = 0.5 + 0.55 * eroded(u * vec2f(3.2, 2.4) + 7.0, octaves);
    // (Fading from far below zero, so the blend never lifts the land where they are absent.)
    let mid = mix(-300.0, elev_h(mid_e * mid_shape, z), smoothstep(1200.0, 4500.0, z));
    h = smax(h, mid, 30.0);

    // Day-side mesas: soft rock eroded into flat-topped buttes and terraces.
    let mesa_zone = smoothstep(1.1, 0.6, sx) * smoothstep(1500.0, 3500.0, z);
    if (mesa_zone > 0.0) {
        let tier = z * 0.018;
        let k = h / tier;
        let stepped = (floor(k) + smoothstep(0.55, 0.95, fract(k))) * tier;
        h = mix(h, stepped, mesa_zone * 0.85);
    }

    // The great far ranges, cresting along far_top.
    let far_e = (far_top(sx) - HORIZON) * (0.72 + 0.3 * eroded(u * vec2f(2.6, 1.4) + 1.0, octaves));
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
    return vec3f(3.4, 1.45, 0.5) * day * day * 1.15;
}

fn sky_light(x: f32) -> vec3f {
    return band3(vec3f(0.45, 0.2, 0.14), vec3f(0.16, 0.06, 0.22), vec3f(0.02, 0.03, 0.075), nightness(x));
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
    let n = normalize(vec3f(
        terrain_h(xz - vec2f(e, 0.0), hi) - terrain_h(xz + vec2f(e, 0.0), hi),
        2.0 * e,
        terrain_h(xz - vec2f(0.0, e), hi) - terrain_h(xz + vec2f(0.0, e), hi)));

    let land = land_h(xz, 6);
    let ice = ice_h(xz, 6);
    let on_ice = ice > land + 0.5;
    let vol = volcano_h(xz, 4);
    let on_volcano = vol > land - 5.0 && vol > 0.0;
    let flat = smoothstep(0.55, 0.85, n.y);
    let grain = fbm(xz / max(t * 0.0008, 0.5), 4);

    // Rock: rust and ochre sandstone banded by strata on the day edge,
    // purple-grey basalt through the twilight, slate under the night.
    let strata = fbm(vec2f(pos.y * 0.09 + fbm(xz * 0.004, 2) * 3.0, 0.5), 3);
    let sand_rock = mix(vec3f(0.36, 0.15, 0.065), vec3f(0.55, 0.3, 0.13), strata) * (0.75 + 0.5 * grain);
    let basalt = vec3f(0.11, 0.075, 0.09) * (0.7 + 0.6 * grain);
    let slate = vec3f(0.06, 0.065, 0.085) * (0.7 + 0.6 * grain);
    var alb = band3(sand_rock, basalt, slate, night);
    // Loose dust and scree on gentle ground, darker in the twilight.
    let dust = band3(vec3f(0.52, 0.26, 0.11), vec3f(0.16, 0.09, 0.12), vec3f(0.08, 0.085, 0.11), night);
    alb = mix(alb, dust * (0.85 + 0.3 * grain), flat * 0.7);
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
    let sky = sky_light(sx) * (0.55 + 0.45 * n.y);
    // Light bounced off the sunlit land, and the sun's glow through the air
    // reaching faces turned away from it.
    let bounce = sun * 0.06 * clamp(0.5 - 0.5 * n.y, 0.0, 1.0);
    let back = vec3f(0.25, 0.08, 0.03) * day * clamp(dot(n, normalize(vec3f(l.x, 0.0, l.z))) * -0.5 + 0.5, 0.0, 1.0) * 0.4;
    var col = alb * (sun * dif * shadow + sky + bounce + back);

    var mat = M_ROCK;
    if (on_ice) {
        col = shade_ice(p, pos, n, sun * dif * shadow, night);
        mat = M_ICE;
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

// The ice cliff and the snowfields behind it. The cliff is pale,
// weathered grey-blue ice broken by deep-blue splits and banded with grit;
// the slope above is old snow cut by crevasses. Kept dim: it glows faintly
// in the night rather than outshining the life on the plain.
fn shade_ice(p: vec2f, pos: vec3f, n: vec3f, sun: vec3f, night: f32) -> vec3f {
    let face = smoothstep(0.55, 0.25, n.y);
    let sky = sky_light(p.x);
    // The twilight sky toward the day lights faces turned toward it.
    let warm = sky_light(2.0) * max(-n.x, 0.0) * 0.35;

    // The cliff.
    let tone = fbm(vec2f(pos.x * 0.045, pos.y * 0.05), 4);
    let striae = fbm(vec2f(pos.x * 0.7, pos.y * 0.06), 3);
    let layers = fbm(vec2f(pos.x * 0.008, pos.y * 0.2 + pos.x * 0.012 + tone * 2.5), 3);
    var ice = mix(vec3f(0.42, 0.5, 0.6), vec3f(0.05, 0.2, 0.38), smoothstep(0.45, 0.7, tone));
    ice *= 0.88 + 0.2 * striae;
    let split = abs(fbm(vec2f(pos.x * 0.06, pos.y * 0.012), 3) - 0.5);
    let fissure = smoothstep(0.025, 0.004, split) * smoothstep(2.0, 8.0, pos.y);
    ice = mix(ice, vec3f(0.004, 0.02, 0.045), fissure * 0.85);
    let grit = smoothstep(0.58, 0.75, layers) * 0.35 + smoothstep(9.0, 1.0, pos.y) * 0.7;
    ice = mix(ice, vec3f(0.05, 0.045, 0.05), clamp(grit, 0.0, 0.85));
    let glow = vec3f(0.0, 0.03, 0.06) * smoothstep(0.012, 0.0, split) * smoothstep(2.0, 8.0, pos.y) * night;
    let cliff = ice * (sky * (1.6 + 0.6 * n.y) + warm + sun * 0.5) + glow;

    // The slope: wind-packed snow, with transverse crevasses opening blue-black.
    let drift = fbm(pos.xz * vec2f(0.02, 0.006), 4);
    let gap = abs(fbm(vec2f(pos.x * 0.004 + drift * 0.3, pos.z * 0.025), 3) - 0.5);
    let crevasse = smoothstep(0.02, 0.006, gap) * smoothstep(40.0, 300.0, pos.z - 560.0)
        * smoothstep(0.45, 0.65, fbm(pos.xz * 0.004, 2));
    var snow = vec3f(0.78, 0.84, 0.98) * (0.85 + 0.3 * drift);
    snow = mix(snow, vec3f(0.02, 0.1, 0.2), crevasse * 0.8);
    // Broad swells of wind-scoured, bluer ice between the drifts.
    snow = mix(snow, vec3f(0.35, 0.5, 0.7), smoothstep(0.5, 0.7, fbm(pos.xz * vec2f(0.003, 0.001), 3)) * 0.5);
    let top = snow * (sky * (0.55 + 0.6 * n.y) + warm * 0.4 + sun);

    var col = mix(top, cliff, face);

    // A meltwater cave at the foot where the river is born, glowing blue
    // from deep inside.
    let cx = (p.x - 3.36) / 0.03;
    let cy = (p.y - ground_top(p.x)) / 0.03;
    let rim = 0.12 * (fbm(vec2f(p.x * 160.0, p.y * 160.0), 3) - 0.5);
    let arch = length(vec2f(cx, max(cy, 0.0))) + rim;
    let cave = smoothstep(1.0, 0.92, arch) * step(-0.2, cy) * face;
    let inside = vec3f(0.0, 0.012, 0.03) + vec3f(0.03, 0.3, 0.5) * smoothstep(1.1, 0.0, length(vec2f(cx * 0.8, cy + 0.4))) * 1.2;
    col = mix(col, inside, cave);
    col += vec3f(0.02, 0.15, 0.25) * smoothstep(1.25, 1.0, arch) * (1.0 - cave) * face * 0.12;
    return col;
}

fn march(ro: vec3f, rd: vec3f, t0: f32) -> f32 {
    var t = t0;
    var prev = t0;
    for (var i = 0; i < 400; i++) {
        let pos = ro + rd * t;
        let h = pos.y - terrain_h(pos.xz, 6);
        if (h < 0.0006 * t) {
            // Refine between the last two samples.
            var a = prev;
            var b = t;
            for (var k = 0; k < 6; k++) {
                let m = 0.5 * (a + b);
                let q = ro + rd * m;
                if (q.y - terrain_h(q.xz, 6) < 0.0006 * m) {
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
        t += max(h * 0.6, t * 0.009);
    }
    return -1.0;
}

// ---------------------------------------------------------------- the plain

// Bare ground: patches of dark gravel, rust sand and ochre crust by day,
// basalt grit through the twilight, near-black volcanic sand by night.
// `foot` is the size of a pixel on the ground, to fade detail too fine to show.
fn soil_albedo(w: vec2f, night: f32, foot: f32) -> vec3f {
    let soil = fbm(w / 22.0, 4);
    let speck = mix(0.5, fbm(w * 3.0, 3), smoothstep(0.6, 0.15, foot));
    let rust = vec3f(0.4, 0.17, 0.07);
    let ochre = vec3f(0.5, 0.31, 0.13);
    let gravel = vec3f(0.085, 0.07, 0.07);
    let day_ground = mix(mix(gravel, rust, smoothstep(0.35, 0.6, soil)), ochre, smoothstep(0.62, 0.75, soil) * 0.6);
    let twi_ground = mix(vec3f(0.07, 0.05, 0.065), vec3f(0.16, 0.08, 0.1), smoothstep(0.4, 0.7, soil));
    let night_ground = mix(vec3f(0.035, 0.038, 0.05), vec3f(0.07, 0.075, 0.095), smoothstep(0.4, 0.7, soil));
    return band3(day_ground, twi_ground, night_ground, night) * (0.7 + 0.6 * speck);
}

// Hot ground over the vent fields the simulation knows (vesper-terrain VENTS).
fn vent_heat(x: f32) -> f32 {
    let a = exp(-pow((x - VOLCANO_X - 0.09) / 0.05, 2.0));
    let b = 0.7 * exp(-pow((x - 2.86) / 0.05, 2.0));
    let c = 0.9 * exp(-pow((x - 3.24) / 0.05, 2.0));
    return max(a, max(b, c));
}

// The simulation's ground, seen in perspective: dark gravel and rust sand
// strewn with stones, wet along the river, crusted with salt on the day edge.
fn ground(p: vec2f, gt: f32) -> vec4f {
    let night = nightness(p.x);
    let day = daylight(p.x);
    let rv = river(p.x);
    let dist = abs(p.y - rv.x);
    let pan = smoothstep(0.95, 0.4, p.x);
    let wet = pan_wet(p);
    let w = plain_point(p);
    let rd = view_ray(p);
    let z = w.y;

    if (dist < rv.y && wet > 0.5) {
        // The colour seen into the water. The frame pass lays the sky and
        // land reflected on its surface over this.
        let depth_in = (1.0 - dist / max(rv.y, 1e-4)) * (1.0 - pan * 0.8);
        let bed = band3(vec3f(0.3, 0.16, 0.07), vec3f(0.03, 0.06, 0.08), vec3f(0.005, 0.03, 0.05), night);
        let deep = band3(vec3f(0.04, 0.1, 0.08), vec3f(0.01, 0.05, 0.08), vec3f(0.0, 0.04, 0.08), night);
        var col = mix(bed, deep, depth_in);
        // Shallow and bright over salt in the pan.
        col = mix(col, vec3f(0.5, 0.32, 0.18) * (0.3 + 0.7 * day), pan * 0.5);
        col += vec3f(0.01, 0.12, 0.2) * night * depth_in * 0.4; // glacier meltwater glow
        return vec4f(col, M_WATER);
    }

    // Stones: a scattering of rounded rocks on a loose cell grid.
    let sq = w / 1.7;
    let cell = floor(sq);
    let fq = fract(sq);
    var stone_h = 0.0;
    var stone_g = vec2f(0.0);
    var stone_id = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let o = vec2f(f32(i), f32(j));
            let id = cell + o;
            let r = pcg2(id + 71.0);
            if (r.x > 0.32) {
                continue;
            }
            let c = o + 0.2 + 0.6 * pcg2(id + 13.0);
            let rad = 0.12 + 0.3 * r.y * r.y;
            let d = (fq - c) / rad;
            let dd = dot(d, d);
            if (dd < 1.0) {
                let hh = sqrt(1.0 - dd) * rad * (0.6 + 0.5 * r.y);
                if (hh > stone_h) {
                    stone_h = hh;
                    stone_g = -d / max(sqrt(1.0 - dd), 0.08) * (0.6 + 0.5 * r.y);
                    stone_id = r.x * 7.0 + r.y;
                }
            }
        }
    }
    let on_stone = step(0.001, stone_h);

    // Surface normal from gravel and ripples in the sand, plus the stones.
    let fine = noised(w * 2.2);
    let mid = noised(w * 0.45 + 5.0);
    let g = fine.yz * 0.12 + mid.yz * 0.08 + stone_g * on_stone;
    let n = normalize(vec3f(-g.x, 1.0, -g.y));

    var alb = soil_albedo(w, night, z / (FOCAL * 2160.0));
    let stone_col = band3(vec3f(0.12, 0.07, 0.05), vec3f(0.08, 0.06, 0.08), vec3f(0.04, 0.045, 0.06), night)
        * (0.7 + 0.6 * fract(stone_id * 3.7));
    alb = mix(alb, stone_col, on_stone);

    // Small mineral crystals in jewel tones, here and there.
    let cq = w * 9.0;
    let ccell = floor(cq);
    let crystal = step(0.996, hash21(ccell)) * smoothstep(0.35, 0.1, length(fract(cq) - 0.5)) * (1.0 - on_stone);
    let jewel = band3(vec3f(0.05, 0.7, 0.55), vec3f(0.6, 0.08, 0.7), vec3f(0.03, 0.3, 0.7), hash21(ccell + 2.0));
    alb = mix(alb, jewel, crystal * 0.7);

    // Wet, darkened banks with moss, and a sheen of reflected sky.
    let bank = smoothstep(rv.y + 0.014, rv.y, dist) * (1.0 - pan);
    alb *= 1.0 - bank * 0.5;
    let moss = band3(vec3f(0.06, 0.16, 0.05), vec3f(0.12, 0.04, 0.16), vec3f(0.01, 0.07, 0.12), night);
    alb = mix(alb, moss, smoothstep(0.5, 0.75, fbm(w * 0.8, 3)) * bank * 0.8);

    // The salt pan: a cracked white crust where the river has boiled dry.
    let crust = pan * smoothstep(rv.y * 1.35, rv.y * 0.9, dist + fbm(w * 0.5, 2) * 0.008);
    if (crust > 0.0) {
        let q = w / 1.3;
        let c0 = floor(q);
        let f0 = fract(q);
        var d1 = 9.0;
        var d2 = 9.0;
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let o = vec2f(f32(i), f32(j));
                let d = length(f0 - o - hash22(c0 + o));
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
        let crack = smoothstep(0.07, 0.0, d2 - d1);
        var salt = vec3f(0.72, 0.52, 0.36) * (0.85 + 0.25 * fbm(w * 2.0, 2));
        salt = mix(salt, salt * 0.25, crack);
        alb = mix(alb, salt, crust * (1.0 - on_stone) * 0.9);
    }

    // Over the vent fields the ground is split by fissures glowing with heat.
    let heat = vent_heat(p.x) * smoothstep(0.4, 0.7, fbm(w / 25.0 + 3.0, 3));
    var glow = vec3f(0.0);
    if (heat > 0.01) {
        let q = w / 11.0;
        let c0 = floor(q);
        let f0 = fract(q);
        var d1 = 9.0;
        var d2 = 9.0;
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let o = vec2f(f32(i), f32(j));
                let d = length(f0 - o - hash22(c0 + o + 40.0));
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
        let fissure = smoothstep(0.035, 0.0, d2 - d1) * (1.0 - on_stone) * smoothstep(0.4, 0.65, fbm(w / 6.0, 2));
        // Crusted with sulphur and scorched dark around the cracks.
        alb = mix(alb, vec3f(0.35, 0.3, 0.05), smoothstep(0.25, 0.1, d2 - d1) * heat * 0.4);
        glow = vec3f(4.0, 0.9, 0.12) * fissure * heat + vec3f(0.6, 0.12, 0.02) * smoothstep(0.35, 0.0, d2 - d1) * heat * 0.25;
    }

    let l = normalize(SUN_DIR);
    let sun = sun_light(p.x);
    // Stones cast short shadows away from the low sun.
    var shade = 1.0;
    if (on_stone < 0.5) {
        let back = w - normalize(l.xz) * 0.35;
        let bc = floor(back / 1.7);
        let r = pcg2(bc + 71.0);
        if (r.x <= 0.32) {
            let c = bc + 0.2 + 0.6 * pcg2(bc + 13.0);
            let rad = (0.12 + 0.3 * r.y * r.y) * 1.7;
            shade = mix(1.0, 0.25, smoothstep(rad, rad * 0.6, length(back - c * 1.7)));
        }
    }
    let dif = max(dot(n, l), 0.0) * shade;
    let sky = sky_light(p.x) * (0.6 + 0.4 * n.y);
    var col = alb * (sun * dif * 0.9 + sky + sun * 0.08) + glow;
    // Wet ground mirrors a little of the bright sky.
    col += haze_color(vec2f(p.x, 2.0 * HORIZON - p.y)) * bank * 0.12;

    // A little of the air between here and the eye.
    col = mix(col, haze_color(vec2f(p.x, HORIZON)), clamp(z / 9000.0, 0.0, 0.2));
    // The nearest ground falls a little into shadow.
    col *= 0.6 + 0.4 * smoothstep(0.0, gt * 0.6, p.y);
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
