//! Renders Vesper. Platform-neutral: the Windows host and the preview tool both
//! hand it a wgpu device and a texture view per output.
//!
//! Each output (one per monitor) owns a baked landscape texture that is redrawn
//! rarely, plus a cheap per-frame pass that animates on top and tone maps for
//! that display, SDR or HDR.

use std::borrow::Cow;

const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
const BAKE_WGSL: &str = include_str!("shaders/bake.wgsl");
const FRAME_WGSL: &str = include_str!("shaders/frame.wgsl");

const BAKED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

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
    /// Current lightning flash strength, 0..1.
    pub lightning: f32,
    /// World x of the storm cell.
    pub storm_x: f32,
    pub exposure: f32,
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
    _pad: [f32; 2],
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bake_layout: wgpu::BindGroupLayout,
    frame_layout: wgpu::BindGroupLayout,
    bake_pipeline: wgpu::RenderPipeline,
    frame_module: wgpu::ShaderModule,
    frame_pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
}

pub struct Output {
    config: OutputConfig,
    params: wgpu::Buffer,
    _baked: wgpu::Texture,
    baked_view: wgpu::TextureView,
    bake_group: wgpu::BindGroup,
    frame_group: wgpu::BindGroup,
    frame_pipeline: wgpu::RenderPipeline,
    baked_band_shift: Option<f32>,
}

impl Output {
    pub fn config(&self) -> &OutputConfig {
        &self.config
    }

    /// Forces the landscape to be baked again on the next frame.
    pub fn invalidate(&mut self) {
        self.baked_band_shift = None;
    }
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
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
            ],
        });

        let bake_module = shader(&device, "bake", BAKE_WGSL);
        let frame_module = shader(&device, "frame", FRAME_WGSL);

        let bake_pipeline_layout = pipeline_layout(&device, "bake", &bake_layout);
        let frame_pipeline_layout = pipeline_layout(&device, "frame", &frame_layout);
        let bake_pipeline =
            fullscreen_pipeline(&device, "bake", &bake_pipeline_layout, &bake_module, "fs_bake", BAKED_FORMAT);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("baked"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { device, queue, bake_layout, frame_layout, bake_pipeline, frame_module, frame_pipeline_layout, sampler }
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
        let baked = device.create_texture(&wgpu::TextureDescriptor {
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
        });
        let baked_view = baked.create_view(&wgpu::TextureViewDescriptor::default());
        let bake_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bake"),
            layout: &self.bake_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() }],
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &self.frame_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&baked_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let frame_pipeline = fullscreen_pipeline(
            device,
            "frame",
            &self.frame_pipeline_layout,
            &self.frame_module,
            "fs_frame",
            config.format,
        );
        Output {
            config,
            params,
            _baked: baked,
            baked_view,
            bake_group,
            frame_group,
            frame_pipeline,
            baked_band_shift: None,
        }
    }

    /// Draws one frame of `output` into `target`, re-baking the landscape first
    /// if the season has moved on since the last bake.
    pub fn render(&self, output: &mut Output, world: &WorldView, target: &wgpu::TextureView) {
        let cfg = output.config;
        let (mode, paper_white, peak) = match cfg.mode {
            OutputMode::Sdr => (0, 1.0, 1.0),
            OutputMode::Hdr { paper_white_nits, peak_nits } => {
                (1, paper_white_nits / 80.0, peak_nits.max(paper_white_nits) / 80.0)
            }
        };
        let params = Params {
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
            _pad: [0.0; 2],
        };
        self.queue.write_buffer(&output.params, 0, bytemuck::bytes_of(&params));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("vesper") });

        let stale = match output.baked_band_shift {
            None => true,
            Some(b) => (b - world.band_shift).abs() > 0.002,
        };
        if stale {
            draw_fullscreen(&mut encoder, "bake", &output.baked_view, &self.bake_pipeline, &output.bake_group);
            output.baked_band_shift = Some(world.band_shift);
        }
        draw_fullscreen(&mut encoder, "frame", target, &output.frame_pipeline, &output.frame_group);
        self.queue.submit(Some(encoder.finish()));
    }
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
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
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
/// twilight band and the storm's lightning. Driven by wall-clock time so the
/// season carries on across restarts.
pub mod rhythm {
    /// Length of one seasonal wobble, in days.
    pub const SEASON_DAYS: f64 = 4.0;
    const BAND_AMPLITUDE: f64 = 0.08;
    /// Shader time wraps at this many seconds to keep f32 precision.
    pub const TIME_WRAP: f64 = 65536.0;

    pub fn band_shift(unix_secs: f64) -> f32 {
        let phase = unix_secs / 86400.0 / SEASON_DAYS * std::f64::consts::TAU;
        (BAND_AMPLITUDE * phase.sin()) as f32
    }

    pub fn shader_time(secs: f64) -> f32 {
        (secs % TIME_WRAP) as f32
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

    fn hash(x: f64) -> f64 {
        let s = (x * 12.9898 + 78.233).sin() * 43758.5453;
        s - s.floor()
    }
}
