//! Renders Vesper headlessly to PNG files, for checking the look without
//! installing the wallpaper.
//!
//! Usage: preview [--scale S] [--time SECONDS] [--frames N] [--step SECONDS]
//!                [--band SHIFT] [--lightning L] [--out DIR]

use vesper_render::{OutputConfig, OutputMode, Renderer, WorldView, rhythm};

/// Kyle's layout: two 3840x2160 monitors side by side, the right one HDR.
const MONITOR: [u32; 2] = [3840, 2160];

struct Args {
    scale: f32,
    time: f64,
    frames: u32,
    step: f64,
    band: Option<f32>,
    lightning: Option<f32>,
    out: std::path::PathBuf,
}

fn parse_args() -> Args {
    let mut a = Args {
        scale: 0.25,
        time: 120.0,
        frames: 1,
        step: 1.0,
        band: None,
        lightning: None,
        out: "preview-out".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut val = || it.next().expect("missing value");
        match flag.as_str() {
            "--scale" => a.scale = val().parse().unwrap(),
            "--time" => a.time = val().parse().unwrap(),
            "--frames" => a.frames = val().parse().unwrap(),
            "--step" => a.step = val().parse().unwrap(),
            "--band" => a.band = Some(val().parse().unwrap()),
            "--lightning" => a.lightning = Some(val().parse().unwrap()),
            "--out" => a.out = val().into(),
            other => panic!("unknown flag {other}"),
        }
    }
    a
}

fn main() {
    let args = parse_args();
    std::fs::create_dir_all(&args.out).unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("no adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let renderer = Renderer::new(device, queue);

    let w = (MONITOR[0] as f32 * args.scale) as u32;
    let h = (MONITOR[1] as f32 * args.scale) as u32;
    let world_size = [w as f32 * 2.0, h as f32];

    let sdr = wgpu::TextureFormat::Rgba8Unorm;
    let hdr = wgpu::TextureFormat::Rgba16Float;
    let mut left = renderer.create_output(OutputConfig {
        origin_px: [0.0, 0.0],
        size_px: [w, h],
        format: sdr,
        mode: OutputMode::Sdr,
    });
    let mut right = renderer.create_output(OutputConfig {
        origin_px: [w as f32, 0.0],
        size_px: [w, h],
        format: sdr,
        mode: OutputMode::Sdr,
    });
    let mut right_hdr = renderer.create_output(OutputConfig {
        origin_px: [w as f32, 0.0],
        size_px: [w, h],
        format: hdr,
        mode: OutputMode::Hdr { paper_white_nits: 200.0, peak_nits: 1000.0 },
    });

    let left_tex = target(renderer.device(), w, h, sdr);
    let right_tex = target(renderer.device(), w, h, sdr);
    let hdr_tex = target(renderer.device(), w, h, hdr);

    for f in 0..args.frames {
        let t = args.time + f as f64 * args.step;
        let world = WorldView {
            world_size_px: world_size,
            time: rhythm::shader_time(t),
            band_shift: args.band.unwrap_or(0.0),
            lightning: args.lightning.unwrap_or_else(|| rhythm::lightning(t)),
            storm_x: 2.45,
            exposure: 1.0,
        };
        renderer.render(&mut left, &world, &view(&left_tex));
        renderer.render(&mut right, &world, &view(&right_tex));
        renderer.render(&mut right_hdr, &world, &view(&hdr_tex));

        let l = read_rgba8(&renderer, &left_tex, w, h);
        let r = read_rgba8(&renderer, &right_tex, w, h);
        let mut img = image::RgbaImage::new(w * 2, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                img.put_pixel(x, y, image::Rgba([l[i], l[i + 1], l[i + 2], 255]));
                img.put_pixel(x + w, y, image::Rgba([r[i], r[i + 1], r[i + 2], 255]));
            }
        }
        let path = args.out.join(format!("frame-{f:03}.png"));
        img.save(&path).unwrap();

        let stats = hdr_stats(&read_rgba16f(&renderer, &hdr_tex, w, h));
        eprintln!("{} | HDR right: max {:.1} nits, mean {:.1} nits, bad {}", path.display(), stats.0, stats.1, stats.2);
    }
}

fn target(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

fn read_back(r: &Renderer, tex: &wgpu::Texture, w: u32, h: u32, bpp: u32) -> Vec<u8> {
    let device = r.device();
    let unpadded = w * bpp;
    let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
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
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue().submit(Some(enc.finish()));
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |res| res.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = slice.get_mapped_range().unwrap();
    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let s = (row * padded) as usize;
        out.extend_from_slice(&data[s..s + unpadded as usize]);
    }
    out
}

fn read_rgba8(r: &Renderer, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    read_back(r, tex, w, h, 4)
}

fn read_rgba16f(r: &Renderer, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<f32> {
    read_back(r, tex, w, h, 8)
        .chunks_exact(2)
        .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
        .collect()
}

/// Max and mean luminance in nits, plus the count of NaN or negative values.
fn hdr_stats(px: &[f32]) -> (f32, f32, usize) {
    let mut max = 0.0f32;
    let mut sum = 0.0f64;
    let mut bad = 0;
    for c in px.chunks_exact(4) {
        if c[..3].iter().any(|v| !v.is_finite() || *v < 0.0) {
            bad += 1;
            continue;
        }
        let y = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) * 80.0;
        max = max.max(y);
        sum += y as f64;
    }
    (max, (sum / (px.len() / 4) as f64) as f32, bad)
}
