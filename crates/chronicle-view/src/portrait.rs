//! Portraits of species, painted by the wallpaper's own renderer from the
//! likenesses the chronicle keeps, so a species here looks exactly as it does
//! on the desktop, standing where it was seen.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use eframe::egui_wgpu::{RenderState, wgpu};
use std::collections::HashMap;
use vesper_chronicle::likeness::{Likeness, Part, Sheet};
use vesper_chronicle::names::WORLD_W;
use vesper_chronicle::SpeciesId;
use vesper_render::{LifeBatch, Look, Membrane, Organism, OutputConfig, OutputMode, Renderer, Seg, Tissue, WorldView};

/// Portraits painted per frame at most; the rest wait for the next.
const PER_FRAME: u32 = 3;
/// Share of the picture the subject fills, and the closest the view goes
/// (world units tall), so a speck isn't blown up into a blur.
const FILL: f32 = 0.62;
const CLOSEST: f32 = 0.002;

pub enum Portrait {
    Ready(TextureHandle),
    /// Will be painted on a later frame.
    Pending,
    /// No GPU, or nothing to paint.
    None,
}

pub struct Portraits {
    renderer: Option<Renderer>,
    cache: HashMap<(SpeciesId, u64, [u32; 2]), Option<TextureHandle>>,
    painted: u32,
}

impl Portraits {
    pub fn new(gpu: Option<&RenderState>) -> Self {
        let renderer = gpu.map(|g| Renderer::new(g.device.clone(), g.queue.clone()));
        if renderer.is_none() {
            log::warn!("no GPU for the chronicle's portraits");
        }
        Self { renderer, cache: HashMap::new(), painted: 0 }
    }

    /// Call once a frame, before any [`Portraits::get`].
    pub fn begin_frame(&mut self) {
        self.painted = 0;
    }

    /// The portrait of `species` at `size` pixels, painting it if need be.
    pub fn get(&mut self, ctx: &egui::Context, species: SpeciesId, l: &Likeness, size: [u32; 2]) -> Portrait {
        let key = (species, l.tick.to_bits(), size);
        if let Some(t) = self.cache.get(&key) {
            return t.clone().map_or(Portrait::None, Portrait::Ready);
        }
        let Some(renderer) = &mut self.renderer else { return Portrait::None };
        if self.painted >= PER_FRAME {
            ctx.request_repaint();
            return Portrait::Pending;
        }
        self.painted += 1;
        // An older portrait of the same species at this size is stale now.
        self.cache.retain(|k, _| !(k.0 == species && k.2 == size));
        let texture = paint(renderer, l, size).map(|img| {
            ctx.load_texture(format!("portrait-{}-{}x{}", species.0, size[0], size[1]), img, TextureOptions::LINEAR)
        });
        self.cache.insert(key, texture.clone());
        texture.map_or(Portrait::None, Portrait::Ready)
    }
}

/// Renders `l` where it was seen, framed around it, into an image.
fn paint(r: &mut Renderer, l: &Likeness, size: [u32; 2]) -> Option<ColorImage> {
    let b = l.bounds();
    if l.is_empty() || !b.iter().all(|v| v.is_finite()) {
        return None;
    }
    let (w, h) = (size[0] as f32, size[1] as f32);
    let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
    let view_h = (bh.max(bw * h / w) / FILL).clamp(CLOSEST, 1.0);
    let view_w = view_h * w / h;
    let ppu = h / view_h;
    let left = ((b[0] + b[2] - view_w) / 2.0).clamp(0.0, (WORLD_W - view_w).max(0.0));
    let bottom = ((b[1] + b[3] - view_h) / 2.0).clamp(0.0, 1.0 - view_h);

    let segs: Vec<Seg> = l.parts.iter().map(seg).collect();
    let membranes: Vec<Membrane> = l.sheets.iter().map(membrane).collect();
    let mut batch = LifeBatch::new();
    batch.add(&Organism {
        segs: &segs,
        membranes: &membranes,
        look: &Look { genes: l.look },
        z: l.z,
        alive: 1.0,
        signal: 0.0,
        symbol: 0,
        bleeding: 0.0,
        infection: None,
        clip_y: None,
        seed: l.seed,
    });
    r.set_creatures(&mut batch);
    r.set_plants(&mut LifeBatch::new());

    // The light of where and when it was seen, without the weather of that moment.
    let mut view = WorldView::at([WORLD_W * ppu, ppu], l.wall, 1.0);
    view.storm = 0.0;
    view.lightning = 0.0;
    view.ash = 0.0;
    view.dust = 0.0;
    view.cold = 0.0;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut out = r.create_output(OutputConfig {
        origin_px: [left * ppu, (1.0 - bottom - view_h) * ppu],
        size_px: size,
        format,
        mode: OutputMode::Sdr,
    });
    let tex = r.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("portrait"),
        size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    r.render(&mut out, &view, &tex.create_view(&Default::default()));
    let mut px = read_back(r, &tex, size[0], size[1])?;
    for a in px.iter_mut().skip(3).step_by(4) {
        *a = 255;
    }
    Some(ColorImage::from_rgba_unmultiplied([size[0] as usize, size[1] as usize], &px))
}

fn read_back(r: &Renderer, tex: &wgpu::Texture, w: u32, h: u32) -> Option<Vec<u8>> {
    let device = r.device();
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("portrait readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue().submit(Some(enc.finish()));
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
        log::warn!("portrait readback: {e}");
        return None;
    }
    let data = slice.get_mapped_range().ok()?;
    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let s = (row * padded) as usize;
        out.extend_from_slice(&data[s..s + unpadded as usize]);
    }
    Some(out)
}

fn seg(p: &Part) -> Seg {
    Seg {
        a: p.a,
        b: p.b,
        ra: p.ra,
        rb: p.rb,
        tissue: tissue(p.tissue),
        damage: p.damage,
        side: p.side as f32,
        sway: 0.0,
    }
}

fn membrane(m: &Sheet) -> Membrane {
    Membrane { a: m.a, b: m.b, c: m.c, tissue: tissue(m.tissue), side: 0.0 }
}

fn tissue(t: vesper_sim::Tissue) -> Tissue {
    use vesper_sim::Tissue as S;
    match t {
        S::Frame => Tissue::Frame,
        S::Muscle => Tissue::Muscle,
        S::LightFilm => Tissue::LightFilm,
        S::Gut => Tissue::Gut,
        S::Float => Tissue::Float,
        S::Sense => Tissue::Sense,
        S::Glow => Tissue::Glow,
        S::Armor => Tissue::Armor,
        S::Spine => Tissue::Spine,
        S::Store => Tissue::Store,
        S::Brood => Tissue::Brood,
        S::Mouth => Tissue::Mouth,
    }
}
