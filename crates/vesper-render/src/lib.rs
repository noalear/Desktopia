//! Renders Vesper. Platform-neutral: the Windows host and the preview tool both
//! hand it a wgpu device and a texture view per output.
//!
//! Each output (one per monitor) owns a baked landscape texture that is redrawn
//! rarely. Every frame, a cheap pass animates the landscape into an HDR scene
//! texture, creatures and plants are drawn into it as instanced tissue, the
//! brightest light is blurred into a bloom at low resolution, and a last pass
//! tone maps the scene for that display, SDR or HDR.

use std::borrow::Cow;

pub mod life;

pub use life::{Infection, LifeBatch, Look, Membrane, Organism, Seg, Tissue};
pub use vesper_terrain::{SKY_CELLS, SKY_COLS, SKY_ROWS};

const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
const BAKE_WGSL: &str = include_str!("shaders/bake.wgsl");
const FRAME_WGSL: &str = include_str!("shaders/frame.wgsl");
const LIFE_WGSL: &str = include_str!("shaders/life.wgsl");
const COMPOSITE_WGSL: &str = include_str!("shaders/composite.wgsl");
const WEATHER_WGSL: &str = include_str!("shaders/weather.wgsl");
const BLOOM_WGSL: &str = include_str!("shaders/bloom.wgsl");

const BAKED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// How a display wants its pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputMode {
    /// 8-bit SDR. The target must be a non-sRGB format; the shader encodes and dithers.
    Sdr,
    /// scRGB on an `Rgba16Float` target: 1.0 = 80 nits.
    Hdr { paper_white_nits: f32, peak_nits: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct OutputConfig {
    /// Top-left of this output inside the world span, in pixels.
    pub origin_px: [f32; 2],
    /// Size of this output in pixels.
    pub size_px: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub mode: OutputMode,
}

/// The part of the world state the renderer needs this frame.
#[derive(Clone, Copy, Debug)]
pub struct WorldView {
    /// Size of the whole span across all monitors, in pixels.
    pub world_size_px: [f32; 2],
    /// Seconds, wrapped to keep shader precision.
    pub time: f32,
    /// Seasonal slide of the twilight band, in world units.
    pub band_shift: f32,
    /// Current lightning flash strength, 0..1, and where the bolt falls.
    pub lightning: f32,
    pub lightning_x: f32,
    /// World x of the storm cell.
    pub storm_x: f32,
    /// 0 clear .. 1 full storm.
    pub storm: f32,
    /// 0 quiet, about 0.3 rumbling, 1 erupting.
    pub volcano: f32,
    /// How far lava has run each way from the volcano, world units.
    pub lava: f32,
    /// 0..1 ash in the sky.
    pub ash: f32,
    /// The river's slow drift (radians) and flood (0 normal .. 1 twice as wide).
    pub river_phase: f32,
    pub river_flood: f32,
    /// A cold fall spilling off the glacier: 0..1 strength and its front's x.
    pub cold: f32,
    pub cold_x: f32,
    /// A dust storm rolling off the day side: 0..1 strength and its centre's x.
    pub dust: f32,
    pub dust_x: f32,
    /// Wind along x near the ground (day-ward, negative) and up high
    /// (night-ward, positive), world units per second.
    pub wind_low: f32,
    pub wind_high: f32,
    /// How far the high wind has carried the cloud deck, world units. The
    /// host integrates `wind_high` into this; it may wrap like `time`.
    pub cloud_drift: f32,
    /// How far the ground wind has carried what it drags along (fog wisps,
    /// dust), world units, integrated by the host from `wind_low`.
    pub ground_drift: f32,
    /// How far the river's current has carried its ripples, integrated by
    /// the host from [`rhythm::river_speed`]. Wraps like `time`.
    pub river_flow: f32,
    /// How far the ground wind has blown falling snow and ash sideways,
    /// integrated by the host as [`rhythm::fall_drift`] and wrapped at 1 so
    /// it keeps its precision. The flakes are drawn from this rather than
    /// leaned with the wind, which warped them whenever the wind changed.
    pub fall_drift: f32,
    pub exposure: f32,
    /// The weather column by column, as the simulation has it.
    pub sky: Sky,
}

/// The weather along the band, laid out as [`vesper_terrain::SKY_ROWS`]
/// describes.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    pub rows: [[f32; 4]; SKY_ROWS],
}

impl Default for Sky {
    fn default() -> Self {
        Self { rows: [[0.0; 4]; SKY_ROWS] }
    }
}

impl Sky {
    /// Copies rows handed over by the simulation.
    pub fn from_rows(rows: &[[f32; 4]]) -> Self {
        let mut s = Self::default();
        let n = rows.len().min(SKY_ROWS);
        s.rows[..n].copy_from_slice(&rows[..n]);
        s
    }
}

impl WorldView {
    /// The world at a wall-clock moment, with weather and the volcano's cycle
    /// taken from [`rhythm`] so they carry on across restarts. This is the
    /// fallback when no simulation is running; with one, the host copies the
    /// simulation's environment over these fields one for one.
    pub fn at(world_size_px: [f32; 2], unix_secs: f64, exposure: f32) -> Self {
        let w = rhythm::weather(unix_secs);
        let volcano = rhythm::volcano(unix_secs);
        let (cold, cold_x) = rhythm::cold_fall(unix_secs);
        let (dust, dust_x) = rhythm::dust_storm(unix_secs);
        let time = rhythm::shader_time(unix_secs);
        Self {
            world_size_px,
            time,
            band_shift: rhythm::band_shift(unix_secs),
            lightning: w.lightning,
            lightning_x: w.lightning_x,
            storm_x: w.storm_x,
            storm: w.storm,
            volcano,
            lava: rhythm::lava(unix_secs),
            ash: rhythm::ash(unix_secs),
            river_phase: 0.0,
            river_flood: 0.0,
            cold,
            cold_x,
            dust,
            dust_x,
            wind_low: rhythm::CALM_LOW * (1.0 + w.storm),
            wind_high: rhythm::CALM_HIGH * (1.0 + w.storm),
            cloud_drift: time * rhythm::CALM_HIGH * 0.5,
            ground_drift: time * rhythm::CALM_LOW,
            river_flow: time * rhythm::river_speed(0.0),
            fall_drift: rhythm::fall_drift(0.0, rhythm::CALM_LOW, unix_secs),
            exposure,
            sky: rhythm::sky(&w),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    world_size: [f32; 2],
    view_origin: [f32; 2],
    view_size: [f32; 2],
    time: f32,
    band_shift: f32,
    exposure: f32,
    mode: u32,
    paper_white: f32,
    peak: f32,
    lightning: f32,
    storm_x: f32,
    storm: f32,
    volcano: f32,
    river_phase: f32,
    river_flood: f32,
    lightning_x: f32,
    lava: f32,
    ash: f32,
    cold: f32,
    cold_x: f32,
    dust: f32,
    dust_x: f32,
    wind_low: f32,
    wind_high: f32,
    cloud_drift: f32,
    /// Where in the pixel this bake pass samples, in pixels from its centre.
    bake_jitter: [f32; 2],
    ground_drift: f32,
    river_flow: f32,
    fall_drift: f32,
    /// Keeps the block a multiple of 16 bytes.
    _pad: [f32; 3],
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bake_layout: wgpu::BindGroupLayout,
    frame_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    bake_pipeline: wgpu::RenderPipeline,
    frame_pipeline: wgpu::RenderPipeline,
    composite_module: wgpu::ShaderModule,
    composite_pipeline_layout: wgpu::PipelineLayout,
    bloom: BloomPipelines,
    /// The bake again, averaged into what is already there (see `BAKE_SAMPLES`).
    refine_pipeline: wgpu::RenderPipeline,
    life_pipelines: [wgpu::RenderPipeline; life::GROUPS],
    sampler: wgpu::Sampler,
    /// Finish every bake within the frame that starts it (for stills).
    instant_bake: bool,
    plants: LifeBuffer,
    creatures: LifeBuffer,
}

struct BloomPipelines {
    down: wgpu::RenderPipeline,
    down2: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
}

/// One output's bloom: two levels, each with a ping-pong pair for blurring.
struct Bloom {
    _textures: Vec<wgpu::Texture>,
    /// Quarter size: result, scratch. Sixteenth size: result, scratch.
    views: [wgpu::TextureView; 4],
    /// Reading the scene, quarter result, quarter scratch, sixteenth result,
    /// sixteenth scratch.
    groups: [wgpu::BindGroup; 5],
}

/// Instances for one kind of life, kept on the GPU between frames.
struct LifeBuffer {
    buffer: Option<wgpu::Buffer>,
    ranges: [std::ops::Range<u32>; life::GROUPS],
}

pub struct Output {
    config: OutputConfig,
    params: wgpu::Buffer,
    sky: wgpu::Buffer,
    /// Two baked landscapes: the one on show, and one being baked a strip
    /// at a time behind it (see `BAKE_STRIPS`).
    _baked: [wgpu::Texture; 2],
    baked_views: [wgpu::TextureView; 2],
    _scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    bake_group: wgpu::BindGroup,
    /// Frame-pass groups reading each baked landscape.
    frame_groups: [wgpu::BindGroup; 2],
    front: usize,
    composite_group: wgpu::BindGroup,
    composite_pipeline: wgpu::RenderPipeline,
    bloom: Bloom,
    baked_band_shift: Option<f32>,
    /// A bake under way in the back texture: the band shift it bakes and the
    /// next strip to draw.
    baking: Option<(f32, u32)>,
    /// A finished bake gathering its extra samples.
    refining: Option<Refine>,
}

/// Progress of the extra samples going into a fresh bake: which texture,
/// which sample, the next strip, and the band shift it shows once done.
#[derive(Clone, Copy)]
struct Refine {
    tex: usize,
    sample: u32,
    strip: u32,
    shift: f32,
}

impl Output {
    pub fn config(&self) -> &OutputConfig {
        &self.config
    }

    /// Forces the landscape to be baked again, whole, on the next frame.
    pub fn invalidate(&mut self) {
        self.baked_band_shift = None;
        self.baking = None;
        self.refining = None;
    }

    fn bake_jitter(&self) -> [f32; 2] {
        self.refining.map_or([0.0; 2], |r| BAKE_JITTER[r.sample as usize])
    }
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bake_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bake"),
            entries: &[uniform_entry],
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                uniform_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: SKY_BINDING, ..uniform_entry },
            ],
        });

        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                uniform_entry,
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(3),
                texture_entry(4),
                wgpu::BindGroupLayoutEntry { binding: SKY_BINDING, ..uniform_entry },
            ],
        });

        let bake_module = shader(&device, "bake", BAKE_WGSL);
        let frame_module = shader(&device, "frame", &with_weather(FRAME_WGSL));
        let life_module = shader(&device, "life", &with_weather(LIFE_WGSL));
        let composite_module = shader(&device, "composite", &with_weather(COMPOSITE_WGSL));
        let bloom_module = shader(&device, "bloom", BLOOM_WGSL);

        let bake_pipeline_layout = pipeline_layout(&device, "bake", &bake_layout);
        let frame_pipeline_layout = pipeline_layout(&device, "frame", &frame_layout);
        let bake_pipeline =
            fullscreen_pipeline(&device, "bake", &bake_pipeline_layout, &bake_module, "fs_bake", BAKED_FORMAT);
        // Each extra sample is blended in with weight 1/(n+1), a running
        // mean. Alpha holds the material code, which can't be averaged, so
        // it keeps the first sample's.
        let refine_pipeline = fullscreen_pipeline_blended(
            &device,
            "bake refine",
            &bake_pipeline_layout,
            &bake_module,
            "fs_bake",
            BAKED_FORMAT,
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Constant,
                    dst_factor: wgpu::BlendFactor::OneMinusConstant,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        );
        let frame_pipeline =
            fullscreen_pipeline(&device, "frame", &frame_pipeline_layout, &frame_module, "fs_frame", SCENE_FORMAT);
        let life_pipelines = life_pipelines(&device, &frame_pipeline_layout, &life_module);
        let composite_pipeline_layout = pipeline_layout(&device, "composite", &composite_layout);
        let bloom_pipeline =
            |entry| fullscreen_pipeline(&device, "bloom", &frame_pipeline_layout, &bloom_module, entry, BLOOM_FORMAT);
        let bloom = BloomPipelines {
            down: bloom_pipeline("fs_down"),
            down2: bloom_pipeline("fs_down2"),
            blur_h: bloom_pipeline("fs_blur_h"),
            blur_v: bloom_pipeline("fs_blur_v"),
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("baked"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let empty = || LifeBuffer { buffer: None, ranges: Default::default() };
        Self {
            device,
            queue,
            bake_layout,
            frame_layout,
            composite_layout,
            bake_pipeline,
            refine_pipeline,
            frame_pipeline,
            composite_module,
            composite_pipeline_layout,
            bloom,
            life_pipelines,
            sampler,
            instant_bake: false,
            plants: empty(),
            creatures: empty(),
        }
    }

    /// Replaces the plants. They change slowly, so call this only when they do;
    /// the wind is animated on the GPU.
    pub fn set_plants(&mut self, batch: &mut LifeBatch) {
        upload_life(&self.device, &self.queue, &mut self.plants, batch, "plants");
    }

    /// Replaces the creatures. Call once per frame, before rendering the outputs.
    pub fn set_creatures(&mut self, batch: &mut LifeBatch) {
        upload_life(&self.device, &self.queue, &mut self.creatures, batch, "creatures");
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn create_output(&self, config: OutputConfig) -> Output {
        let device = &self.device;
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky"),
            size: (SKY_ROWS * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let make_baked = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("baked landscape"),
                size: wgpu::Extent3d {
                    width: config.size_px[0].max(1),
                    height: config.size_px[1].max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: BAKED_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let baked = [make_baked(), make_baked()];
        let baked_views: [wgpu::TextureView; 2] =
            std::array::from_fn(|i| baked[i].create_view(&wgpu::TextureViewDescriptor::default()));
        let size = wgpu::Extent3d {
            width: config.size_px[0].max(1),
            height: config.size_px[1].max(1),
            depth_or_array_layers: 1,
        };
        let scene = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("life depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let bake_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bake"),
            layout: &self.bake_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() }],
        });
        let frame_groups: [wgpu::BindGroup; 2] = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &self.frame_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&baked_views[i]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: SKY_BINDING, resource: sky.as_entire_binding() },
                ],
            })
        });
        let bloom = self.create_bloom(&params, &sky, &scene_view, config.size_px);
        let composite_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&scene_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&bloom.views[0]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&bloom.views[2]) },
                wgpu::BindGroupEntry { binding: SKY_BINDING, resource: sky.as_entire_binding() },
            ],
        });
        let composite_pipeline = fullscreen_pipeline(
            device,
            "composite",
            &self.composite_pipeline_layout,
            &self.composite_module,
            "fs_composite",
            config.format,
        );
        Output {
            config,
            params,
            sky,
            _baked: baked,
            baked_views,
            _scene: scene,
            scene_view,
            _depth: depth,
            depth_view,
            bake_group,
            frame_groups,
            front: 0,
            composite_group,
            composite_pipeline,
            bloom,
            baked_band_shift: None,
            baking: None,
            refining: None,
        }
    }

    /// Keeps the baked landscape up to date with the season. The first bake
    /// is drawn whole. Later ones, when the band has slid a little, are drawn
    /// a strip per frame into the back texture, so a heavy bake never stalls
    /// the GPU for a whole frame. Either way the fresh bake then gathers
    /// BAKE_SAMPLES - 1 more samples per pixel, a strip per frame, and a
    /// re-bake is shown once it has them all.
    fn bake(&self, encoder: &mut wgpu::CommandEncoder, output: &mut Output, band_shift: f32) {
        let size = output.config.size_px;
        if let Some(r) = output.refining {
            let weight = 1.0 / (r.sample + 1) as f64;
            let view = &output.baked_views[r.tex];
            bake_strips(encoder, view, &self.refine_pipeline, &output.bake_group, r.strip..r.strip + 1, size, weight);
            let mut next = Refine { strip: r.strip + 1, ..r };
            if next.strip == BAKE_STRIPS {
                next.strip = 0;
                next.sample += 1;
            }
            if next.sample < BAKE_SAMPLES {
                output.refining = Some(next);
            } else {
                output.front = r.tex;
                output.baked_band_shift = Some(r.shift);
                output.refining = None;
            }
            return;
        }
        let Some(shown) = output.baked_band_shift else {
            let front = output.front;
            let view = &output.baked_views[front];
            bake_strips(encoder, view, &self.bake_pipeline, &output.bake_group, 0..BAKE_STRIPS, size, 1.0);
            output.baked_band_shift = Some(band_shift);
            output.refining = Some(Refine { tex: front, sample: 1, strip: 0, shift: band_shift });
            return;
        };
        if output.baking.is_none() && (shown - band_shift).abs() > 0.005 {
            output.baking = Some((band_shift, 0));
        }
        let Some((shift, next)) = output.baking else { return };
        let back = 1 - output.front;
        bake_strips(
            encoder,
            &output.baked_views[back],
            &self.bake_pipeline,
            &output.bake_group,
            next..next + 1,
            size,
            1.0,
        );
        if next + 1 < BAKE_STRIPS {
            output.baking = Some((shift, next + 1));
        } else {
            output.baking = None;
            output.refining = Some(Refine { tex: back, sample: 1, strip: 0, shift });
        }
    }

    /// Makes every bake finish within the frame that starts it, extra
    /// samples and all. Slow; for stills, not the wallpaper.
    pub fn set_instant_bake(&mut self, on: bool) {
        self.instant_bake = on;
    }

    fn create_bloom(
        &self,
        params: &wgpu::Buffer,
        sky: &wgpu::Buffer,
        scene: &wgpu::TextureView,
        size: [u32; 2],
    ) -> Bloom {
        let device = &self.device;
        let make = |div: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bloom"),
                size: wgpu::Extent3d {
                    width: (size[0] / div).max(1),
                    height: (size[1] / div).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: BLOOM_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = vec![make(4), make(4), make(16), make(16)];
        let views: [wgpu::TextureView; 4] =
            std::array::from_fn(|i| textures[i].create_view(&wgpu::TextureViewDescriptor::default()));
        let group = |src: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: &self.frame_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: SKY_BINDING, resource: sky.as_entire_binding() },
                ],
            })
        };
        let groups = [group(scene), group(&views[0]), group(&views[1]), group(&views[2]), group(&views[3])];
        Bloom { _textures: textures, views, groups }
    }

    /// Blurs the scene's brightest light into the output's bloom textures.
    fn draw_bloom(&self, encoder: &mut wgpu::CommandEncoder, output: &Output) {
        let (b, p) = (&output.bloom, &self.bloom);
        draw_fullscreen(encoder, "bloom down", &b.views[0], &p.down, &b.groups[0]);
        draw_fullscreen(encoder, "bloom blur", &b.views[1], &p.blur_h, &b.groups[1]);
        draw_fullscreen(encoder, "bloom blur", &b.views[0], &p.blur_v, &b.groups[2]);
        draw_fullscreen(encoder, "bloom down", &b.views[2], &p.down2, &b.groups[1]);
        draw_fullscreen(encoder, "bloom blur", &b.views[3], &p.blur_h, &b.groups[3]);
        draw_fullscreen(encoder, "bloom blur", &b.views[2], &p.blur_v, &b.groups[4]);
    }

    /// Draws one frame of `output` into `target`, re-baking the landscape first
    /// if the season has moved on since the last bake.
    pub fn render(&self, output: &mut Output, world: &WorldView, target: &wgpu::TextureView) {
        if self.instant_bake {
            loop {
                self.queue.write_buffer(&output.params, 0, bytemuck::bytes_of(&params(output, world)));
                let mut encoder =
                    self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bake") });
                self.bake(&mut encoder, output, world.band_shift);
                self.queue.submit(Some(encoder.finish()));
                if output.baking.is_none() && output.refining.is_none() {
                    break;
                }
            }
        }
        self.queue.write_buffer(&output.params, 0, bytemuck::bytes_of(&params(output, world)));
        self.queue.write_buffer(&output.sky, 0, bytemuck::cast_slice(&world.sky.rows));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("vesper") });

        self.bake(&mut encoder, output, world.band_shift);
        draw_fullscreen(
            &mut encoder,
            "frame",
            &output.scene_view,
            &self.frame_pipeline,
            &output.frame_groups[output.front],
        );
        self.draw_life(&mut encoder, output);
        self.draw_bloom(&mut encoder, output);
        draw_fullscreen(&mut encoder, "composite", target, &output.composite_pipeline, &output.composite_group);
        self.queue.submit(Some(encoder.finish()));
    }
}

impl Renderer {
    fn draw_life(&self, encoder: &mut wgpu::CommandEncoder, output: &Output) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("life"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &output.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Life shares the frame pass's uniforms and weather.
        pass.set_bind_group(0, &output.frame_groups[output.front], &[]);
        // Each group draws plants first, then creatures, far to near.
        for (g, pipeline) in self.life_pipelines.iter().enumerate() {
            pass.set_pipeline(pipeline);
            for lb in [&self.plants, &self.creatures] {
                let Some(buf) = &lb.buffer else { continue };
                if lb.ranges[g].is_empty() {
                    continue;
                }
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, lb.ranges[g].clone());
            }
        }
    }
}

fn upload_life(device: &wgpu::Device, queue: &wgpu::Queue, lb: &mut LifeBuffer, batch: &mut LifeBatch, label: &str) {
    let (prims, ranges) = batch.finish();
    lb.ranges = ranges;
    if prims.is_empty() {
        return;
    }
    let bytes: &[u8] = bytemuck::cast_slice(&prims);
    let fits = lb.buffer.as_ref().is_some_and(|b| b.size() >= bytes.len() as u64);
    if !fits {
        // Grow with headroom so a growing population doesn't reallocate every frame.
        let size = (bytes.len() as u64 * 3 / 2).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        lb.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    queue.write_buffer(lb.buffer.as_ref().unwrap(), 0, bytes);
}

fn life_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
) -> [wgpu::RenderPipeline; life::GROUPS] {
    let make = |label: &str, vs: &str, fs: &str, write_depth: bool| {
        // Premultiplied alpha. Glow writes alpha 0, which makes it additive.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<life::Prim>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &life::PRIM_ATTRIBS,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write_depth),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    [
        make("life shadow", "vs_shadow", "fs_shadow", false),
        make("life body", "vs_seg", "fs_body", true),
        make("life inner", "vs_seg", "fs_inner", false),
        make("life membrane", "vs_membrane", "fs_membrane", false),
        make("life glow", "vs_glow", "fs_glow", false),
    ]
}

/// The uniforms for one pass over `output`.
fn params(output: &Output, world: &WorldView) -> Params {
    let cfg = output.config;
    let (mode, paper_white, peak) = match cfg.mode {
        OutputMode::Sdr => (0, 1.0, 1.0),
        OutputMode::Hdr { paper_white_nits, peak_nits } => {
            (1, paper_white_nits / 80.0, peak_nits.max(paper_white_nits) / 80.0)
        }
    };
    Params {
        world_size: world.world_size_px,
        view_origin: cfg.origin_px,
        view_size: [cfg.size_px[0] as f32, cfg.size_px[1] as f32],
        time: world.time,
        band_shift: world.band_shift,
        exposure: world.exposure,
        mode,
        paper_white,
        peak,
        lightning: world.lightning,
        storm_x: world.storm_x,
        storm: world.storm,
        volcano: world.volcano,
        river_phase: world.river_phase,
        river_flood: world.river_flood,
        lightning_x: world.lightning_x,
        lava: world.lava,
        ash: world.ash,
        cold: world.cold,
        cold_x: world.cold_x,
        dust: world.dust,
        dust_x: world.dust_x,
        wind_low: world.wind_low,
        wind_high: world.wind_high,
        cloud_drift: world.cloud_drift,
        bake_jitter: output.bake_jitter(),
        ground_drift: world.ground_drift,
        river_flow: world.river_flow,
        fall_drift: world.fall_drift,
        _pad: [0.0; 3],
    }
}

/// Where the weather's columns are bound, in the frame, life and composite passes.
const SKY_BINDING: u32 = 5;

/// A pass's shader with the weather columns and their helpers in front of it.
fn with_weather(body: &str) -> String {
    format!(
        "const SKY_COLS: i32 = {SKY_COLS};\nconst SKY_CELLS: i32 = {SKY_CELLS};\nconst SKY_ROWS: u32 = {SKY_ROWS}u;\nconst SKY_BINDING: u32 = {SKY_BINDING}u;\n{WEATHER_WGSL}\n{body}"
    )
}

fn shader(device: &wgpu::Device, label: &str, body: &str) -> wgpu::ShaderModule {
    let source = format!("{COMMON_WGSL}\n{body}");
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
    })
}

fn pipeline_layout(device: &wgpu::Device, label: &str, group: &wgpu::BindGroupLayout) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(group)],
        ..Default::default()
    })
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    fullscreen_pipeline_blended(device, label, layout, module, fragment_entry, format, None)
}

fn fullscreen_pipeline_blended(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// How many frames a re-bake of the landscape is spread over.
const BAKE_STRIPS: u32 = 12;

/// Samples averaged into each baked pixel. The landscape only changes with
/// the season, so supersampling it costs a few seconds of strips after each
/// bake and nothing after that, and it smooths every edge and fine texture
/// in the land and sky.
const BAKE_SAMPLES: u32 = 8;

/// Where in the pixel each sample falls, in pixels from its centre: the
/// centre, then the rest of the standard 8x pattern.
const BAKE_JITTER: [[f32; 2]; BAKE_SAMPLES as usize] = [
    [0.0, 0.0],
    [-0.0625, 0.1875],
    [0.3125, 0.0625],
    [-0.1875, -0.3125],
    [-0.3125, 0.3125],
    [-0.4375, -0.0625],
    [0.1875, 0.4375],
    [0.4375, -0.4375],
];

/// Bakes some horizontal strips of the landscape (of BAKE_STRIPS), leaving
/// the rest of the texture as it was. `weight` is the blend constant, for
/// the refine pipeline.
fn bake_strips(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    strips: std::ops::Range<u32>,
    size: [u32; 2],
    weight: f64,
) {
    let (w, h) = (size[0].max(1), size[1].max(1));
    let row = |k: u32| (h as u64 * k as u64 / BAKE_STRIPS as u64) as u32;
    let (y0, y1) = (row(strips.start), row(strips.end));
    if y1 <= y0 {
        return;
    }
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bake"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.set_scissor_rect(0, y0, w, y1 - y0);
    pass.set_blend_constant(wgpu::Color { r: weight, g: weight, b: weight, a: weight });
    pass.draw(0..3, 0..1);
}

fn draw_fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

/// Slow world rhythms that don't need a simulation: the seasonal slide of the
/// twilight band, storms, the volcano, cold falls and dust storms. Driven by
/// wall-clock time so they carry on across restarts, and standing in for the
/// simulation when it is not running.
pub mod rhythm {
    /// Length of one seasonal wobble, in days.
    pub const SEASON_DAYS: f64 = vesper_terrain::SEASON_DAYS;
    /// Shader time wraps at this many seconds to keep f32 precision.
    pub const TIME_WRAP: f64 = 65536.0;

    pub fn band_shift(unix_secs: f64) -> f32 {
        vesper_terrain::band_shift(unix_secs)
    }

    pub fn shader_time(secs: f64) -> f32 {
        (secs % TIME_WRAP) as f32
    }

    /// How fast the river's ripples drift for a flood level, per second.
    /// Anything that moves at a speed that changes has to be integrated like
    /// this rather than drawn as `time * speed`, which runs backwards
    /// whenever the speed falls.
    pub fn river_speed(flood: f32) -> f32 {
        0.6 + 0.6 * flood
    }

    /// `drift` carried on `secs` seconds by a ground wind of `wind_low`, for
    /// [`WorldView::fall_drift`](crate::WorldView::fall_drift): half a unit
    /// per world unit of wind, wrapped at 1. Each layer of flakes moves
    /// a fixed number of its own cells per unit, so the wrap never shows.
    pub fn fall_drift(drift: f32, wind_low: f32, secs: f64) -> f32 {
        (drift as f64 + wind_low as f64 * 0.5 * secs).rem_euclid(1.0) as f32
    }

    /// Calm-air wind near the ground and up high, world units per second;
    /// storms double both. Matches the simulation's own.
    pub const CALM_LOW: f32 = -0.004;
    pub const CALM_HIGH: f32 = 0.005;

    pub struct Weather {
        pub storm_x: f32,
        /// 0 clear .. 1 full storm.
        pub storm: f32,
        pub lightning: f32,
        pub lightning_x: f32,
    }

    /// Storms are born every so often where warm and cold air meet, drift
    /// night-ward on the high wind, rain themselves out and fade.
    pub fn weather(secs: f64) -> Weather {
        // Each 40-minute slot may hold one storm that lives up to 35 minutes.
        const SLOT: f64 = 2400.0;
        const LIFE: f64 = 2100.0;
        let mut best = Weather { storm_x: 2.4, storm: 0.0, lightning: 0.0, lightning_x: 2.4 };
        // A storm from the previous slot may still be alive.
        for k in [-1.0, 0.0] {
            let slot = (secs / SLOT).floor() + k;
            if hash(slot * 1.31 + 0.2) < 0.35 {
                continue;
            }
            let born = slot * SLOT + hash(slot + 7.3) * (SLOT - 300.0);
            let age = secs - born;
            if !(0.0..LIFE).contains(&age) {
                continue;
            }
            let grow = (age / 360.0).min(1.0);
            let fade = ((LIFE - age) / 600.0).min(1.0);
            let strength = (grow * fade * (0.6 + 0.4 * hash(slot + 2.9))) as f32;
            if strength > best.storm {
                let x0 = 1.3 + 1.5 * hash(slot + 4.1);
                // About half a monitor per half hour.
                let x = x0 + age / 3600.0 * 0.9;
                best = Weather { storm_x: x as f32, storm: strength, lightning: 0.0, lightning_x: x as f32 };
            }
        }
        if best.storm > 0.5 {
            best.lightning = lightning(secs) * ((best.storm - 0.5) * 2.0).min(1.0);
            // Each strike lands somewhere under the cell.
            let strike = (secs / 50.0).floor();
            best.lightning_x += (0.16 * (hash(strike + 3.3) - 0.5)) as f32;
        }
        best
    }

    /// Lightning strength at a moment. Strikes are rare (one window per
    /// 50 seconds, most of them empty) and each fades over about a second.
    pub fn lightning(secs: f64) -> f32 {
        const WINDOW: f64 = 50.0;
        let slot = (secs / WINDOW).floor();
        let r = hash(slot);
        if r < 0.55 {
            return 0.0;
        }
        let start = hash(slot + 0.5) * (WINDOW - 3.0);
        let local = secs - slot * WINDOW - start;
        if !(0.0..1.4).contains(&local) {
            return 0.0;
        }
        // A quick rise, a second flicker, then a soft fade.
        let fade = (-(local) * 3.0).exp();
        let flicker = if (0.18..0.3).contains(&local) { 0.4 } else { 1.0 };
        (fade * flicker) as f32
    }

    /// The volcano's mood. Long quiet spells with a steam plume, a rumbling
    /// build-up as the crater glows, then an eruption that dies back slowly.
    pub fn volcano(secs: f64) -> f32 {
        mood(secs).0
    }

    /// The volcano's mood, and how many seconds ago the eruption began if
    /// this cycle has erupted.
    fn mood(secs: f64) -> (f32, Option<f64>) {
        // Cycles of 2.5 to 4 hours; about two in three end in an eruption.
        const CYCLE: f64 = 3.0 * 3600.0;
        let slot = (secs / CYCLE).floor();
        let local = (secs - slot * CYCLE) / CYCLE;
        if hash(slot * 3.7 + 1.0) < 0.35 {
            // A cycle that only rumbles.
            return ((0.25 * smooth(0.6, 0.75, local) * smooth(0.95, 0.85, local)) as f32, None);
        }
        let start = 0.55 + 0.25 * hash(slot + 9.1);
        let rumble = smooth(start - 0.15, start, local) * (1.0 - smooth(start, start + 0.1, local)) * 0.3;
        // Erupts within a minute, then dies back over about half an hour.
        let erupt = smooth(start, start + 0.005, local) * (-(local - start).max(0.0) * 10.0).exp();
        let since = (local >= start).then(|| (local - start) * CYCLE);
        (rumble.max(erupt) as f32, since)
    }

    /// How far lava has run from the crater at `secs`. It only ever runs
    /// down: rumbling wells it up in the crater, an eruption sends it down
    /// the cone, and there it stays while it cools.
    pub fn lava(secs: f64) -> f32 {
        match mood(secs) {
            (v, Some(since)) if v >= 0.03 => (0.012 + 0.002 * since).min(0.32) as f32,
            (v, _) => 0.04 * v,
        }
    }

    /// How far lava has run once an eruption of mood `volcano` is under way,
    /// for stills that set the mood by hand.
    pub fn lava_for(volcano: f32) -> f32 {
        let v = volcano as f64;
        (0.04 * v + 0.28 * v * smooth(0.35, 0.6, v)) as f32
    }

    /// Ash thrown up by eruptions over the last hour or so, thinning slowly.
    pub fn ash(secs: f64) -> f32 {
        let mut ash = 0.0f32;
        for k in 0..8 {
            let lag = k as f64 * 300.0;
            let v = volcano(secs - lag);
            if v > 0.5 {
                ash = ash.max(0.8 * (v - 0.3) / 0.7 * (-lag / 1800.0).exp() as f32);
            }
        }
        ash.min(0.8)
    }

    /// Weather columns for the stand-in storm: a cloud deck thickening
    /// toward the night side, the storm's tower and rain, damp ground in the
    /// twilight and snow lying on the night side.
    pub fn sky(w: &Weather) -> super::Sky {
        use super::SKY_COLS;
        let mut sky = super::Sky::default();
        let col_w = 7680.0 / 2160.0 / SKY_COLS as f32;
        for i in 0..SKY_COLS {
            let x = (i as f32 + 0.5) * col_w;
            let d = (x - w.storm_x) / 0.15;
            let under = w.storm * (-d * d).exp();
            let night = smooth(0.25, 3.3, x as f64) as f32;
            let snowy = smooth(2.35, 2.6, x as f64) as f32;
            sky.rows[i] = [(0.15 + 0.4 * night + under).min(1.0), under, snowy, 0.0];
            sky.rows[SKY_COLS + i] = [(0.1 + 0.5 * night + under).min(1.0), snowy, 0.0, 0.0];
            sky.rows[2 * SKY_COLS + i] = [0.0, 0.0, 2.0 * under, 0.0];
        }
        if w.storm > 0.0 {
            sky.rows[3 * SKY_COLS] = [w.storm_x, w.storm, w.storm, 0.5];
        }
        sky
    }

    /// A front of cold air spilling off the glacier and creeping day-ward,
    /// as (strength 0..1, x of its front). One every few hours.
    pub fn cold_fall(secs: f64) -> (f32, f32) {
        front(secs, 5400.0, 0.45, 3.3, -0.0012, 1.2, 11.0)
    }

    /// A wall of dust blown off the scorched day side, rolling night-ward,
    /// as (strength 0..1, x of its centre).
    pub fn dust_storm(secs: f64) -> (f32, f32) {
        front(secs, 7200.0, 0.4, 0.2, 0.0015, 1.8, 23.0)
    }

    /// Something born at `x0` in some slots, travelling at `speed` until it
    /// reaches `x_end`, then dying away over a couple of minutes.
    fn front(secs: f64, slot_len: f64, odds: f64, x0: f64, speed: f64, x_end: f64, salt: f64) -> (f32, f32) {
        let travel = (x_end - x0) / speed;
        let life = travel + 480.0;
        let mut best = (0.0f32, x0 as f32);
        for k in [-1.0, 0.0] {
            let slot = (secs / slot_len).floor() + k;
            if hash(slot * 1.7 + salt) > odds {
                continue;
            }
            let born = slot * slot_len + hash(slot + salt + 0.3) * (slot_len - life).max(0.0);
            let age = secs - born;
            if !(0.0..life).contains(&age) {
                continue;
            }
            let grow = (age / 180.0).min(1.0);
            let fade = (-(age - travel).max(0.0) / 120.0).exp();
            let strength = ((0.4 + 0.6 * hash(slot + salt + 0.7)) * grow * fade) as f32;
            if strength > best.0 {
                best = (strength, (x0 + speed * age.min(travel)) as f32);
            }
        }
        best
    }

    fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn hash(x: f64) -> f64 {
        let s = (x * 12.9898 + 78.233).sin() * 43758.5453;
        s - s.floor()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Over two days, cold falls and dust storms both happen, stay
        /// between where they start and where they die, and fade smoothly.
        #[test]
        fn fronts_come_and_go_in_range() {
            let t0 = 1_800_000_000.0;
            let (mut cold_seen, mut dust_seen) = (false, false);
            let (mut last_cold, mut last_dust) = (0.0f32, 0.0f32);
            for k in 0..(2 * 86400 / 10) {
                let t = t0 + k as f64 * 10.0;
                let (c, cx) = cold_fall(t);
                let (d, dx) = dust_storm(t);
                assert!((0.0..=1.0).contains(&c) && (0.0..=1.0).contains(&d));
                if c > 0.05 {
                    cold_seen = true;
                    assert!((1.19..=3.31).contains(&cx), "cold front at {cx}");
                }
                if d > 0.05 {
                    dust_seen = true;
                    assert!((0.19..=1.81).contains(&dx), "dust storm at {dx}");
                }
                assert!((c - last_cold).abs() < 0.1 && (d - last_dust).abs() < 0.1, "a front jumped at {t}");
                last_cold = c;
                last_dust = d;
            }
            assert!(cold_seen && dust_seen);
        }

        #[test]
        fn lava_and_ash_follow_the_volcano() {
            assert_eq!(lava_for(0.0), 0.0);
            assert!(lava_for(1.0) > 0.25 && lava_for(0.3) < 0.03);
            let t0 = 1_800_000_000.0;
            let mut ran = false;
            let mut last = 0.0;
            for k in 0..(86400 / 60) {
                let t = t0 + k as f64 * 60.0;
                let a = ash(t);
                assert!((0.0..=0.8).contains(&a));
                // Once lava is out of the crater it only runs down the cone
                // until it has cooled.
                let l = lava(t);
                if last > 0.02 && volcano(t) >= 0.03 {
                    assert!(l >= last, "lava ran back up at {t}: {last} -> {l}");
                }
                ran |= l > 0.25;
                last = l;
            }
            assert!(ran, "no eruption sent lava down the cone in a day");
        }
    }
}
