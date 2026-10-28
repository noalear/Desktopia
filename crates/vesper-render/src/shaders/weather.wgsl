// The weather along the band, column by column, as the simulation has it.
// See vesper_terrain::SKY_ROWS for the layout. The host prepends SKY_COLS,
// SKY_CELLS, SKY_ROWS and SKY_BINDING.

struct Sky {
    rows: array<vec4f, SKY_ROWS>,
};

@group(0) @binding(SKY_BINDING) var<uniform> SKY: Sky;

fn sky_row(base: i32, x: f32) -> vec4f {
    let col_w = P.world_size.x / P.world_size.y / f32(SKY_COLS);
    let f = clamp(x / col_w - 0.5, 0.0, f32(SKY_COLS - 1));
    let i = i32(floor(f));
    let j = min(i + 1, SKY_COLS - 1);
    return mix(SKY.rows[base + i], SKY.rows[base + j], f - floor(f));
}

// Cloud cover, what is falling (0..1), the share of it that is snow, fog.
fn sky_air(x: f32) -> vec4f {
    return sky_row(0, x);
}

// Ground wetness, lying snow, ash in the air, dust in the air.
fn sky_ground(x: f32) -> vec4f {
    return sky_row(SKY_COLS, x);
}

// Ash lying on the ground, hail, gusts (0 calm .. 3), cold snap (0..1.5).
fn sky_more(x: f32) -> vec4f {
    return sky_row(2 * SKY_COLS, x);
}

// A storm cell: x, power (0..1.3), radius, age through its life (0..1).
fn sky_cell(k: i32) -> vec4f {
    return SKY.rows[3 * SKY_COLS + k];
}

// The ash veil, the dust front's speed, the cold fall's speed.
fn sky_misc() -> vec4f {
    return SKY.rows[3 * SKY_COLS + SKY_CELLS];
}
