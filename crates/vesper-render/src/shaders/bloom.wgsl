// Bloom: the brightest light in the scene (the sun, glow cells, lava, vent
// throats) spills softly into the air around it, the way it does in an eye
// or a lens. Worked out at a quarter and a sixteenth of the screen's size,
// so it costs next to nothing.

@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

fn texel() -> vec2f {
    return 1.0 / vec2f(textureDimensions(src));
}

// Four bilinear taps cover the 4x4 block of source pixels under each output
// pixel. Only light above a soft threshold passes.
@fragment
fn fs_down(in: VOut) -> @location(0) vec4f {
    let t = texel();
    var c = vec3f(0.0);
    c += textureSampleLevel(src, samp, in.uv + vec2f(-1.0, -1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(1.0, -1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(-1.0, 1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(1.0, 1.0) * t, 0.0).rgb;
    c *= 0.25 * P.exposure;
    let l = luma(c);
    let knee = 0.8;
    let over_knee = max(l - knee, 0.0);
    // Very bright points (the sun) are tamed so they don't flood the screen.
    let keep = over_knee * over_knee / (over_knee + 0.5) / max(l, 1e-4);
    return vec4f(min(c * keep, vec3f(20.0)), 1.0);
}

// Second step down, from a quarter to a sixteenth.
@fragment
fn fs_down2(in: VOut) -> @location(0) vec4f {
    let t = texel();
    var c = vec3f(0.0);
    c += textureSampleLevel(src, samp, in.uv + vec2f(-1.0, -1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(1.0, -1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(-1.0, 1.0) * t, 0.0).rgb;
    c += textureSampleLevel(src, samp, in.uv + vec2f(1.0, 1.0) * t, 0.0).rgb;
    return vec4f(c * 0.25, 1.0);
}

// A 9-tap Gaussian in five bilinear taps.
fn blur(uv: vec2f, dir: vec2f) -> vec4f {
    let t = texel() * dir;
    var c = textureSampleLevel(src, samp, uv, 0.0).rgb * 0.2270270;
    c += textureSampleLevel(src, samp, uv + t * 1.3846154, 0.0).rgb * 0.3162162;
    c += textureSampleLevel(src, samp, uv - t * 1.3846154, 0.0).rgb * 0.3162162;
    c += textureSampleLevel(src, samp, uv + t * 3.2307692, 0.0).rgb * 0.0702703;
    c += textureSampleLevel(src, samp, uv - t * 3.2307692, 0.0).rgb * 0.0702703;
    return vec4f(c, 1.0);
}

@fragment
fn fs_blur_h(in: VOut) -> @location(0) vec4f {
    return blur(in.uv, vec2f(1.0, 0.0));
}

@fragment
fn fs_blur_v(in: VOut) -> @location(0) vec4f {
    return blur(in.uv, vec2f(0.0, 1.0));
}
