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

@fragment
fn fs_composite(in: VOut) -> @location(0) vec4f {
    let p = world_pos(in.uv);
    var col = textureLoad(scene, vec2i(in.pos.xy), 0).rgb;
    col += motes(p, 0.03, 0.0008, 0.0022, 0.0) * 0.6;
    return finish(col, in.pos.xy);
}
