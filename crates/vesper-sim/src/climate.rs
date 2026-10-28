//! The air over the band: its warmth, the water in it and what falls out of
//! it, as columns along x stepped once a second.
//!
//! Vesper's sun never moves, so its weather is one great overturning loop.
//! Air heated on the day side rises and flows night-ward aloft, cools, sinks
//! over the ice and comes back day-ward along the ground. Water boiled off the
//! salt pan and the river rides that loop: the day side's thermals carry it
//! up, and so does the cold ground wind wherever it wedges under warmer air;
//! it condenses into cloud as it cools on its way night-ward, falls as rain
//! over the twilight and as snow over the night side, and the snow feeds the
//! ice that feeds the river. Storms grow where moist air is lifted hardest.
//!
//! Everything else is a departure from that loop that the columns carry along
//! and only slowly forget: cold snaps spilling off the ice, heat building over
//! dry ground, droughts in a dry spell, snow lying where it fell, ash from the
//! volcano drifting night-ward and a veil of it cooling the whole world for
//! days after a great eruption. Nothing here kills on its own; it is the
//! creatures and plants that live under it that wear down (see `life` and
//! `ecology`).

use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use vesper_terrain::{self as terrain, SKY_COLS, WORLD_W};

/// Columns of air along the band.
pub const N: usize = SKY_COLS;
/// Width of one column, world units.
pub const DX: f32 = WORLD_W / N as f32;
/// Below this air temperature what falls is snow, and lying snow keeps.
pub const FREEZE: f32 = -0.45;
/// At most this many storm cells at once.
const MAX_CELLS: usize = 5;

/// Water the air can hold at a temperature: about five times as much at the
/// day edge as in the twilight, and almost none over the ice.
pub fn saturation(t: f32) -> f32 {
    0.25 * (1.2 * t).exp()
}

/// The x of column `i`'s centre.
pub fn col_x(i: usize) -> f32 {
    (i as f32 + 0.5) * DX
}

/// Linear interpolation of a column field at x; `none` if the field is empty.
pub fn sample(v: &[f32], x: f32, none: f32) -> f32 {
    if v.is_empty() {
        return none;
    }
    let f = (x / DX - 0.5).clamp(0.0, (v.len() - 1) as f32);
    let i = (f as usize).min(v.len() - 1);
    let j = (i + 1).min(v.len() - 1);
    let t = f - i as f32;
    v[i] + (v[j] - v[i]) * t
}

/// One convecting storm cell: a tower of cloud drawing up the moist air
/// under it, raining it out, throwing down cold gusts and lightning.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub x: f32,
    pub v: f32,
    /// 0..~1.3, how hard it is convecting now.
    pub power: f32,
    /// The power it can reach, set by how unstable the air was where it rose.
    pub peak: f32,
    pub age: f32,
    /// Seconds of growing, of full strength, and of dying back.
    pub grow: f32,
    pub hold: f32,
    pub fade: f32,
    /// Half-width of its rain, world units.
    pub radius: f32,
    /// Already reported to the chronicle.
    pub reported: bool,
}

impl Cell {
    fn shape(&self, x: f32, widen: f32) -> f32 {
        let d = (x - self.x) / (self.radius * widen);
        (-d * d).exp()
    }

    /// 0..1 through its life: growing, mature, dying.
    pub fn life01(&self) -> f32 {
        (self.age / (self.grow + self.hold + self.fade)).clamp(0.0, 1.0)
    }

    pub fn dying(&self) -> bool {
        self.age > self.grow + self.hold
    }
}

/// A trough in the high wind, drifting night-ward, lifting the air under it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wave {
    pub x: f32,
    pub v: f32,
    pub strength: f32,
    pub width: f32,
    pub age: f32,
    pub life: f32,
}

impl Wave {
    pub fn at(&self, x: f32) -> f32 {
        let envelope = (std::f32::consts::PI * self.age / self.life).sin().max(0.0);
        let d = (x - self.x) / self.width;
        self.strength * envelope * (-d * d).exp()
    }
}

/// The air, column by column. All fields are `N` long once the world has
/// stepped once; an empty one reads as calm, average weather.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Atmos {
    /// How much warmer than usual the air near the ground is.
    pub anom: Vec<f32>,
    /// Water vapour in the low air and aloft.
    pub vapour: Vec<f32>,
    pub vapour_hi: Vec<f32>,
    /// Cloud water aloft, and fog or low cloud near the ground.
    pub cloud: Vec<f32>,
    pub fog: Vec<f32>,
    /// What is falling now, 0..1 (1 a downpour), what share of it is snow,
    /// and how much of it is hail.
    pub precip: Vec<f32>,
    pub snowy: Vec<f32>,
    pub hail: Vec<f32>,
    /// Water in the ground, 0 parched .. 1 sodden.
    pub moisture: Vec<f32>,
    /// Snow lying on the ground, water held in it; about 0.3 covers it.
    pub snow: Vec<f32>,
    /// Ash in the air, ash falling now, and ash lying on the ground.
    pub ash: Vec<f32>,
    pub ash_rate: Vec<f32>,
    pub ashfall: Vec<f32>,
    /// Dust in the air.
    pub dust: Vec<f32>,
    /// How hard the wind gusts, 1 calm.
    pub gust: Vec<f32>,
    /// Meltwater running off the snow and ice now.
    pub melt: Vec<f32>,
    /// Water running off into the river from upstream, for floods.
    pub runoff: f32,
    pub cells: Vec<Cell>,
    pub waves: Vec<Wave>,
    /// Fine ash high in the sky over the whole world after a great eruption;
    /// it dims and cools everything until it settles, over a day or two.
    pub veil: f32,
    /// The slow swing between wet and dry spells, -1 dry .. 1 wet.
    pub era: f32,
    /// Seconds the twilight has been in drought, a heat wave, or neither.
    pub dry_for: f32,
    pub hot_for: f32,
    /// Conditions already reported, so each is told once.
    pub told: Told,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Told {
    pub drought: bool,
    pub heat: bool,
    pub veil: bool,
    pub blizzard_at: f64,
}

/// Something the weather did that the chronicle should hear about.
pub enum Happening {
    Storm { x: f32, power: f32 },
    Hail { x: f32, power: f32 },
    Strike { x: f32 },
    Drought { strength: f32 },
    DroughtBreaks { x: f32, strength: f32 },
    Heatwave { strength: f32 },
    Blizzard { x: f32, strength: f32 },
    AshWinter { strength: f32 },
}

impl Atmos {
    pub fn ready(&self) -> bool {
        self.anom.len() == N
    }

    /// Fills every column with the air of a calm spell.
    pub fn init(&mut self, band_shift: f32) {
        let z = || vec![0.0; N];
        self.anom = z();
        self.vapour = (0..N).map(|i| saturation(base(col_x(i), band_shift)) * 0.55).collect();
        self.vapour_hi = (0..N).map(|i| saturation(aloft(base(col_x(i), band_shift))) * 0.6).collect();
        self.cloud = z();
        self.fog = z();
        self.precip = z();
        self.snowy = z();
        self.hail = z();
        self.moisture = (0..N)
            .map(|i| {
                let n = terrain::nightness(col_x(i), band_shift);
                0.15 + 0.45 * n
            })
            .collect();
        self.snow = (0..N).map(|i| if base(col_x(i), band_shift) < FREEZE - 0.2 { 0.5 } else { 0.0 }).collect();
        self.ash = z();
        self.ash_rate = z();
        self.ashfall = z();
        self.dust = z();
        self.gust = vec![1.0; N];
        self.melt = z();
        self.cells.clear();
        self.waves.clear();
    }

    pub fn cover(&self, x: f32) -> f32 {
        1.0 - (-4.0 * sample(&self.cloud, x, 0.0)).exp()
    }
}

/// The usual air temperature at a point on the ground, before weather:
/// about +1.3 at the day edge and -1.3 at the night edge.
pub fn base(x: f32, band_shift: f32) -> f32 {
    1.3 - 2.6 * terrain::nightness(x, band_shift)
}

/// Moves a field `u * dt` along x, taking `inflow` in at the upwind edge.
fn advect(v: &mut [f32], scratch: &mut Vec<f32>, u: f32, dt: f32, inflow: f32) {
    scratch.clear();
    scratch.extend_from_slice(v);
    let shift = u * dt / DX;
    for (i, out) in v.iter_mut().enumerate() {
        let from = i as f32 - shift;
        *out = if from < 0.0 {
            if u > 0.0 { inflow } else { scratch[0] }
        } else if from > (N - 1) as f32 {
            if u < 0.0 { inflow } else { scratch[N - 1] }
        } else {
            let k = (from as usize).min(N - 2);
            let t = from - k as f32;
            scratch[k] + (scratch[k + 1] - scratch[k]) * t
        };
    }
}

/// Spreads a field a little into its neighbours.
fn blur(v: &mut [f32], scratch: &mut Vec<f32>, k: f32) {
    scratch.clear();
    scratch.extend_from_slice(v);
    for i in 0..N {
        let l = scratch[i.saturating_sub(1)];
        let r = scratch[(i + 1).min(N - 1)];
        v[i] = scratch[i] + k * (l + r - 2.0 * scratch[i]);
    }
}

/// Everything the step needs from the world outside the air.
pub struct Ground<'a> {
    /// Share of each column that is open water.
    pub water: &'a [f32],
    pub band_shift: f32,
    /// Winds near the ground and aloft, world units per second.
    pub wind_low: f32,
    pub wind_high: f32,
    /// The volcano: how hard it is erupting.
    pub eruption: f32,
    /// A cold fall's strength and front, and a dust front's strength, centre
    /// and speed.
    pub cold: f32,
    pub cold_x: f32,
    pub dust: f32,
    pub dust_x: f32,
    pub time: f64,
}

/// One second (or `dt`) of weather. Returns what happened worth telling.
pub fn step(a: &mut Atmos, rng: &mut Rng, g: &Ground, dt: f32, out: &mut Vec<Happening>) {
    if !a.ready() {
        a.init(g.band_shift);
    }
    let mut scratch = Vec::with_capacity(N);

    // The slow swing between wet and dry spells: a wandering that forgets
    // itself over about a day, so droughts and wet spells last hours to days.
    let tau_era = 86400.0;
    a.era += -a.era * dt / tau_era + rng.normal() * (2.0 * dt / tau_era).sqrt() * 0.75;
    a.era = a.era.clamp(-1.0, 1.0);
    let wet_era = 1.0 + 0.5 * a.era;
    a.veil *= (-dt / 43_200.0).exp();
    let shift = offset(a.era, a.veil);

    let t_low: Vec<f32> = (0..N).map(|i| base(col_x(i), g.band_shift) + shift + a.anom[i]).collect();

    // ---------------------------------------------------------------- warmth
    for i in 0..N {
        let x = col_x(i);
        let day = terrain::daylight(x, g.band_shift);
        let night = terrain::nightness(x, g.band_shift);
        let cover = 1.0 - (-4.0 * a.cloud[i]).exp();
        let snow = (a.snow[i] / 0.3).min(1.0);
        let rh = a.vapour[i] / saturation(t_low[i]);
        // Where the anomaly settles: clouds shade the day and blanket the
        // night, dry ground bakes under the sun, snow throws the light back,
        // and clear dry nights lose their heat to the sky.
        let settle = 0.09 - 0.35 * cover * day + 0.2 * cover * (1.0 - day) + 0.3 * (1.0 - a.moisture[i]) * day * day
            - 0.15 * snow
            - 0.2 * (1.0 - cover) * (1.0 - day) * (1.0 - rh.min(1.0))
            - 0.25 * a.ash[i] * day;
        let tau = 600.0 + 2400.0 * night;
        a.anom[i] += (settle - a.anom[i]) * dt / tau;
        // Rain drags cold air down with it.
        a.anom[i] -= a.precip[i] * 0.0006 * dt;
    }
    // A cold fall pours its air into the columns behind its front.
    if g.cold > 0.0 {
        for i in 0..N {
            let behind = col_x(i) - g.cold_x;
            if (0.0..0.5).contains(&behind) {
                a.anom[i] -= g.cold * 0.0025 * dt * (1.0 - behind / 0.5);
            }
        }
    }
    // Cold and warm spells move day-ward with the ground wind, slower than it.
    advect(&mut a.anom, &mut scratch, g.wind_low * 0.35, dt, 0.0);
    blur(&mut a.anom, &mut scratch, 0.01 * dt);
    for v in &mut a.anom {
        *v = v.clamp(-1.6, 1.0);
    }

    // ---------------------------------------------------------------- waves
    // Troughs in the high flow drift night-ward across the band, lifting the
    // air under them into broad bands of cloud and steady rain.
    if rng.chance(dt / 3600.0 * (0.6 + 0.4 * wet_era)) && a.waves.len() < 4 {
        a.waves.push(Wave {
            x: rng.range(0.5, 2.0),
            v: rng.range(0.0008, 0.002),
            strength: rng.range(0.3, 1.0),
            width: rng.range(0.15, 0.35),
            age: 0.0,
            life: rng.range(5400.0, 14_400.0),
        });
    }
    for wv in &mut a.waves {
        wv.age += dt;
        wv.x += wv.v * dt;
    }
    a.waves.retain(|wv| wv.age < wv.life && wv.x < WORLD_W + 0.3);
    let ascent: Vec<f32> = (0..N)
        .map(|i| {
            let x = col_x(i);
            let waves: f32 = a.waves.iter().map(|wv| wv.at(x)).sum();
            // Warm air is shoved up ahead of a cold fall's head.
            let head = g.cold * (-((x - g.cold_x + 0.06) / 0.1).powi(2)).exp();
            let towers: f32 = a.cells.iter().map(|c| c.power * c.shape(x, 1.2)).sum();
            (waves + head + towers).min(2.0)
        })
        .collect();

    // ---------------------------------------------------------------- water
    let t_low: Vec<f32> = (0..N).map(|i| base(col_x(i), g.band_shift) + shift + a.anom[i]).collect();
    let mut lifted = vec![0.0; N];
    for i in 0..N {
        let x = col_x(i);
        let day = terrain::daylight(x, g.band_shift);
        let t = t_low[i];
        let qs = saturation(t);
        // Water rising off the river, the pan, wet ground and snow.
        let open = (g.water[i] / 40.0 + a.moisture[i] / 3000.0) * (t > FREEZE) as u8 as f32;
        let surface = open + a.snow[i].min(0.3) / 6000.0;
        let evap = (qs - a.vapour[i]).max(0.0) * surface * wet_era * (0.5 + 0.5 * day);
        a.vapour[i] += evap * dt;
        // Lifting: the day side's thermals, and the cold ground wind wedging
        // under warmer air, steepest across the middle of the band.
        // Most of that lifting comes in passing waves and at the head of a
        // cold fall; between them the air settles and the sky clears.
        let next = t_low[(i + 1).min(N - 1)];
        let front = ((t - next) / DX / 1.3).max(0.0) * (0.15 + ascent[i]);
        let thermals = day.powf(1.5) * (1.0 + a.anom[i].max(0.0));
        let lift = (thermals / 1500.0 + front / 1800.0 + ascent[i] / 3000.0) * dt;
        let up = a.vapour[i] * lift.min(0.5);
        a.vapour[i] -= up;
        a.vapour_hi[i] += up;
        lifted[i] = lift / dt;
        // Over the night side the air aloft sinks, drying as it comes down.
        let night = terrain::nightness(x, g.band_shift);
        let down = a.vapour_hi[i] * night * night / 800.0 * dt;
        a.vapour_hi[i] -= down;
        a.vapour[i] += down;
        // Fog: low air that cools past saturation, and burns off when it warms.
        let excess = a.vapour[i] - qs * 1.02;
        if excess > 0.0 {
            let c = excess * 0.3 * dt.min(3.0);
            a.vapour[i] -= c;
            a.fog[i] += c;
        } else {
            let back = a.fog[i].min(-excess * 0.05 * dt);
            a.fog[i] -= back;
            a.vapour[i] += back;
        }
    }

    // Aloft: condensing into cloud, cloud thinning in dry air, and rain.
    let mut fall = vec![0.0; N];
    let mut downpour = vec![0.0; N];
    for i in 0..N {
        // High up the air's warmth hardly varies along the band (the sun
        // heats the ground, and the ground the air, from below), so air
        // aloft drifting night-ward doesn't cool much on its own. It clouds
        // over where it is already saturated, and thickly only where
        // something lifts it; where it sinks, its cloud dries away.
        let qs_hi = saturation(aloft(t_low[i]));
        let rh_hi = a.vapour_hi[i] / qs_hi;
        let rising = ascent[i] + lifted[i] * 600.0;
        let over = (a.vapour_hi[i] - qs_hi * 1.1).max(0.0) * 0.03 * dt;
        let lift = a.vapour_hi[i] * rising * (rh_hi - 0.5).max(0.0) * dt / 400.0;
        let c = (over + lift).min(a.vapour_hi[i] * 0.5);
        a.vapour_hi[i] -= c;
        a.cloud[i] += c;
        let thin = a.cloud[i] * (1.0 - rising.min(1.0)) * ((1.0 - rh_hi).max(0.0) / 600.0 + 1.0 / 5400.0) * dt;
        a.cloud[i] -= thin;
        a.vapour_hi[i] += thin;
        let rain = (a.cloud[i] - 0.03).max(0.0) * (0.03 + rising) * dt / 300.0;
        a.cloud[i] -= rain;
        fall[i] = rain;
    }

    // ---------------------------------------------------------------- storms
    for c in &mut a.cells {
        c.age += dt;
        let target = if c.age < c.grow {
            c.peak * (c.age / c.grow).powf(0.7)
        } else if c.age < c.grow + c.hold {
            c.peak
        } else {
            c.peak * (1.0 - (c.age - c.grow - c.hold) / c.fade).max(0.0)
        };
        // It starves if the air under it has been rained dry.
        let i = ((c.x / DX) as usize).min(N - 1);
        let fuel = (a.vapour[i] / saturation(t_low[i]) / 0.55).clamp(0.2, 1.2);
        c.power += (target * fuel - c.power) * (dt / 60.0).min(1.0);
        c.radius = 0.06 + 0.07 * c.power;
        // It drifts on the wind between the ground and the cloud tops, and
        // leans toward the moister air on its flanks, where new towers rise.
        let l = a.vapour[((c.x - 0.15) / DX).clamp(0.0, (N - 1) as f32) as usize];
        let r = a.vapour[((c.x + 0.15) / DX).clamp(0.0, (N - 1) as f32) as usize];
        c.v += rng.normal() * 0.00004 * dt + (r - l).clamp(-0.1, 0.1) * 0.00002 * dt;
        c.v = c.v.clamp(-0.0012, 0.0016);
        let steer = (g.wind_low + g.wind_high) * 0.15;
        c.x = (c.x + (c.v + steer) * dt).clamp(0.2, WORLD_W - 0.1);
    }
    a.cells.retain(|c| c.age < c.grow + c.hold + c.fade);
    for c in &mut a.cells {
        // Draw up the moist air under it and rain it out.
        for i in 0..N {
            let s = c.shape(col_x(i), 1.3);
            if s < 0.01 {
                continue;
            }
            let draw = a.vapour[i] * c.power * s * dt / 250.0;
            a.vapour[i] -= draw;
            fall[i] += draw * 0.85;
            downpour[i] += c.power * c.shape(col_x(i), 1.0);
            a.cloud[i] += draw * 0.15 + c.power * s * 0.002 * dt;
            // Cold air thrown down from it spreads along the ground.
            let pool = c.shape(col_x(i), 1.8);
            a.anom[i] -= c.power * pool * 0.0009 * dt;
        }
        if c.power > 0.7 && !c.reported {
            c.reported = true;
            out.push(Happening::Storm { x: c.x, power: c.power });
            if c.peak > 0.95 {
                out.push(Happening::Hail { x: c.x, power: c.power });
            }
        }
        // Lightning, more of it the harder it convects.
        if c.power > 0.45 && rng.chance(dt * 0.03 * (c.power - 0.45)) {
            out.push(Happening::Strike { x: c.x + rng.normal() * c.radius * 0.6 });
        }
    }
    // New towers rise where moist air is lifted hard enough.
    if a.cells.len() < MAX_CELLS {
        for i in 0..N {
            let x = col_x(i);
            // How buoyant ground air would be, lifted to the cloud tops:
            // cooling as it rises, warmed by its own water condensing, against
            // the air already up there.
            let t = t_low[i];
            let rh = (a.vapour[i] / saturation(t)).min(1.0);
            let buoyancy = 0.65 * t - 0.35 + 3.0 * a.vapour[i] + 0.25;
            // Hot air streaming aloft from the scorched far side caps the
            // day side's thermals: it bakes, but seldom storms.
            let cap = terrain::smoothstep(0.35, 1.15, x);
            let cape = buoyancy.max(0.0) * rh * rh * cap;
            let trigger = ascent[i] + ((t - t_low[(i + 1).min(N - 1)]) / DX / 1.3).max(0.0) * 0.6;
            let p = (cape * trigger).min(2.0) * 2.5e-5 * dt * wet_era;
            if p <= 0.0 || !rng.chance(p) || a.cells.iter().any(|c| (c.x - x).abs() < 0.3) {
                continue;
            }
            let peak = (0.3 + cape * 2.5 + rng.range(-0.15, 0.3)).clamp(0.3, 1.3);
            a.cells.push(Cell {
                x,
                v: rng.normal() * 0.0003,
                power: 0.02,
                peak,
                age: 0.0,
                grow: rng.range(400.0, 900.0),
                hold: rng.range(300.0, 1500.0) * peak,
                fade: rng.range(500.0, 900.0),
                radius: 0.06,
                reported: false,
            });
            if a.cells.len() >= MAX_CELLS {
                break;
            }
        }
    }

    // ---------------------------------------------------------------- falling
    for i in 0..N {
        let t = t_low[i];
        let qs = saturation(t);
        let rh = (a.vapour[i] / qs).min(1.2);
        // Rain falling through dry air partly evaporates on the way down,
        // chilling it: the dry side of a storm drops cold gusts, not water.
        let reach = fall[i] * (0.35 + 0.65 * (rh / 0.75).min(1.0));
        let lost = fall[i] - reach;
        a.vapour[i] += lost;
        a.anom[i] -= lost * 1.5;
        let rate = (reach * 900.0 + downpour[i] * (0.35 + 0.65 * (rh / 0.75).min(1.0))).min(1.0);
        a.precip[i] += (rate - a.precip[i]) * (dt / 20.0).min(1.0);
        a.snowy[i] = terrain::smoothstep(FREEZE + 0.15, FREEZE - 0.15, t);
        let mut hail = 0.0;
        for c in &a.cells {
            if c.power > 0.85 {
                hail = f32::max(hail, (c.power - 0.85) * 3.0 * c.shape(col_x(i), 0.7));
            }
        }
        a.hail[i] = hail.min(1.0);
    }

    // ---------------------------------------------------------------- ground
    let mut runoff = 0.0;
    for i in 0..N {
        let x = col_x(i);
        let day = terrain::daylight(x, g.band_shift);
        let t = t_low[i];
        let p = a.precip[i];
        let snowfall = p * a.snowy[i];
        let rain = p - snowfall;
        a.snow[i] += snowfall * dt / 900.0;
        // Melting: warm air eats the snow, faster in sun and in rain.
        let warm = (t - FREEZE).max(0.0);
        let melt = (a.snow[i]).min(warm * (1.0 + day + rain) * dt / 2000.0);
        a.snow[i] -= melt;
        // The ice at the night edge melts in warm spells and feeds the river.
        let ice = if x > terrain::GLACIER_X { (a.anom[i] + 0.1).max(0.0) * 0.002 } else { 0.0 };
        a.melt[i] = melt / dt.max(1e-6) + ice;
        let soak = rain * dt / 500.0 + melt * 0.6;
        let room = 1.0 - a.moisture[i];
        a.moisture[i] += soak.min(room);
        if x > 1.3 {
            runoff += (rain * a.moisture[i] * a.moisture[i] + (soak - room).max(0.0) / dt.max(1e-6) * 500.0 + a.melt[i] * 400.0)
                / (N as f32 * 0.63);
        }
        // Drying: faster under the sun, in warm dry wind, slowed by snow.
        let qs = saturation(t);
        let rh = (a.vapour[i] / qs).min(1.0);
        let dry = qs * (1.0 - rh) * (0.25 + 0.75 * day) * a.gust[i] / 2500.0 + 1.0 / 80_000.0;
        let cover = (a.snow[i] / 0.3).min(1.0);
        a.moisture[i] -= a.moisture[i] * dry * (1.0 - 0.8 * cover) * dt;
        a.moisture[i] = a.moisture[i].clamp(0.0, 1.0);
        // Snow sublimes slowly in dry air, and over bare ground is soon gone.
        a.snow[i] -= a.snow[i] * (1.0 - rh) * dt / 40_000.0;
    }
    a.dry_for = {
        let twilight: f32 = (0..N)
            .filter(|&i| (1.0..2.6).contains(&col_x(i)))
            .map(|i| a.moisture[i])
            .sum::<f32>()
            / (0..N).filter(|&i| (1.0..2.6).contains(&col_x(i))).count() as f32;
        if twilight < 0.12 {
            a.dry_for + dt
        } else if twilight > 0.3 {
            if a.told.drought {
                a.told.drought = false;
                let (i, wettest) = (0..N)
                    .map(|i| (i, a.precip[i]))
                    .fold((0, 0.0), |b, c| if c.1 > b.1 { c } else { b });
                out.push(Happening::DroughtBreaks { x: col_x(i), strength: (0.5 + wettest).min(1.0) });
            }
            0.0
        } else {
            a.dry_for
        }
    };
    if a.dry_for > 6.0 * 3600.0 && !a.told.drought {
        a.told.drought = true;
        out.push(Happening::Drought { strength: (a.dry_for / (12.0 * 3600.0)).min(1.0) });
    }
    let warm_spell: f32 = (0..N).filter(|&i| (0.8..2.4).contains(&col_x(i))).map(|i| a.anom[i]).sum::<f32>()
        / (0..N).filter(|&i| (0.8..2.4).contains(&col_x(i))).count() as f32;
    a.hot_for = if warm_spell > 0.22 { a.hot_for + dt } else { (a.hot_for - dt * 2.0).max(0.0) };
    if a.hot_for > 2.0 * 3600.0 && !a.told.heat {
        a.told.heat = true;
        out.push(Happening::Heatwave { strength: (warm_spell * 2.5).min(1.0) });
    } else if a.hot_for <= 0.0 {
        a.told.heat = false;
    }
    for i in 0..N {
        let blizzard = a.precip[i] * a.snowy[i] * a.gust[i];
        if blizzard > 1.5 && g.time - a.told.blizzard_at > 3.0 * 3600.0 {
            a.told.blizzard_at = g.time;
            out.push(Happening::Blizzard { x: col_x(i), strength: (blizzard / 2.0).min(1.0) });
        }
    }

    // ---------------------------------------------------------------- dust and ash
    for i in 0..N {
        let x = col_x(i);
        let day = terrain::daylight(x, g.band_shift);
        let bare = (1.0 - a.moisture[i]).powi(2) * (1.0 - (a.snow[i] / 0.3).min(1.0)) * (0.3 + 0.7 * day);
        let front = g.dust * (-((x - g.dust_x) / 0.4).powi(2)).exp();
        let lift = (0.0005 * (a.gust[i] - 1.3).max(0.0).powf(1.5) + front * 0.004) * bare;
        a.dust[i] += lift * dt;
        a.dust[i] -= a.dust[i] * (1.0 / 1500.0 + a.precip[i] * 0.01) * dt;
        a.dust[i] = a.dust[i].clamp(0.0, 1.2);
    }
    advect(&mut a.dust, &mut scratch, g.wind_low * 0.6, dt, 0.0);
    let vi = ((terrain::VOLCANO_X / DX) as usize).min(N - 1);
    if g.eruption > 0.0 {
        a.ash[vi] += g.eruption * 0.006 * dt;
        a.veil = (a.veil + g.eruption.powi(6) * 0.0023 * dt).min(1.0);
    }
    if a.veil > 0.25 && !a.told.veil {
        a.told.veil = true;
        out.push(Happening::AshWinter { strength: a.veil });
    } else if a.veil < 0.1 {
        a.told.veil = false;
    }
    for i in 0..N {
        let settle = a.ash[i] * (1.0 / 2400.0 + a.precip[i] * 0.015) * dt;
        a.ash[i] -= settle;
        a.ash_rate[i] = settle / dt.max(1e-6) * 600.0;
        a.ashfall[i] = (a.ashfall[i] + settle * 4.0).min(1.0);
        a.ashfall[i] -= a.ashfall[i] * (1.0 / 86_400.0 + a.precip[i] * 0.0015) * dt;
    }
    advect(&mut a.ash, &mut scratch, g.wind_high * 0.5, dt, 0.0);
    blur(&mut a.ash, &mut scratch, 0.03 * dt);
    for v in &mut a.ash {
        *v = v.clamp(0.0, 1.0);
    }

    // ---------------------------------------------------------------- moving air
    let night_air = saturation(base(WORLD_W, g.band_shift) + shift) * 0.7;
    advect(&mut a.vapour, &mut scratch, g.wind_low * 0.25, dt, night_air);
    advect(&mut a.fog, &mut scratch, g.wind_low * 0.4, dt, 0.0);
    let day_air = saturation(aloft(base(0.0, g.band_shift))) * 0.3;
    advect(&mut a.vapour_hi, &mut scratch, g.wind_high * 0.6, dt, day_air);
    advect(&mut a.cloud, &mut scratch, g.wind_high * 0.6, dt, 0.0);
    blur(&mut a.vapour, &mut scratch, 0.02 * dt);
    blur(&mut a.vapour_hi, &mut scratch, 0.02 * dt);
    // Whatever cloud reaches the night edge snows out on the ice.
    a.cloud[N - 1] *= 0.5;

    // Gusts: storm outflow, the head of a cold fall, a dust front.
    for i in 0..N {
        let x = col_x(i);
        let mut gust = 1.0 + 0.15 * a.era.abs();
        for c in &a.cells {
            gust += 2.4 * c.power * c.shape(x, 1.8);
        }
        gust += 1.6 * g.cold * (-((x - g.cold_x) / 0.1).powi(2)).exp();
        gust += 2.0 * g.dust * (-((x - g.dust_x) / 0.35).powi(2)).exp();
        a.gust[i] = gust;
    }
    a.runoff = runoff;
}

/// The air's warmth high above a point whose ground air is at `t`.
pub fn aloft(t: f32) -> f32 {
    -0.65 + 0.35 * t
}

/// The whole world's warmth shifted by the wet-dry swing and an ash veil.
pub fn offset(era: f32, veil: f32) -> f32 {
    0.06 * era - 0.55 * veil
}
