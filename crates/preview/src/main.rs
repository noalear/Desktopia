//! Renders Vesper headlessly to PNG files, for checking the look without
//! installing the wallpaper.
//!
//! Usage: preview [--scale S] [--time SECONDS] [--frames N] [--step SECONDS]
//!                [--band SHIFT] [--band-speed S] [--lightning L] [--storm X STRENGTH] [--volcano V]
//!                [--lava REACH] [--ash A] [--cold X STRENGTH] [--dust X STRENGTH]
//!                [--seed N] [--no-life] [--lineup] [--sim MINUTES]
//!                [--follow] [--crop X Y W H] [--acts MINUTES] [--world FILE] [--out DIR]
//!
//! `--crop` renders only a window of the world (x, y of its bottom-left and
//! its size, in monitor heights) at full 4K density times `--scale`, for
//! looking at creatures up close.
//!
//! `--sim` grows a real simulated world for that many sim minutes and draws
//! it exactly as the wallpaper does, in place of the stand-in menagerie;
//! `--follow` then frames the walker with the most limbs, close up.
//!
//! `--world FILE` keeps the grown world: the first run saves it there and
//! later runs load it instead of growing it again. With `--sim`, `--crop`
//! renders only that window of the world, as without it.
//!
//! `--acts` grows a world for that many sim minutes, then has a few of its
//! creatures act out every behaviour the animator knows, `--frames` frames
//! at 15 a second, one sheet per creature (see acts.rs).

mod acts;
#[path = "../../desktopia/src/living.rs"]
#[allow(dead_code)]
mod living;

use vesper_forms::Menagerie;
use vesper_render::{LifeBatch, OutputConfig, OutputMode, Renderer, WorldView};

/// Kyle's layout: two 3840x2160 monitors side by side, the right one HDR.
const MONITOR: [u32; 2] = [3840, 2160];

struct Args {
    scale: f32,
    time: f64,
    frames: u32,
    step: f64,
    band: Option<f32>,
    /// How far the band slides per second of preview time, to watch re-bakes.
    band_speed: f32,
    lightning: Option<f32>,
    storm: Option<[f32; 2]>,
    volcano: Option<f32>,
    lava: Option<f32>,
    ash: Option<f32>,
    cold: Option<[f32; 2]>,
    dust: Option<[f32; 2]>,
    seed: u64,
    life: bool,
    lineup: bool,
    sim: Option<f64>,
    acts: Option<f64>,
    crop: Option<[f32; 4]>,
    /// With --sim: frame one walking creature instead of the whole world.
    follow: bool,
    /// With --sim: where to keep the grown world between runs.
    world: Option<std::path::PathBuf>,
    out: std::path::PathBuf,
}

fn parse_args() -> Args {
    let mut a = Args {
        scale: 0.25,
        time: 120.0,
        frames: 1,
        step: 1.0,
        band: None,
        band_speed: 0.0,
        lightning: None,
        storm: None,
        volcano: None,
        lava: None,
        ash: None,
        cold: None,
        dust: None,
        seed: 7,
        life: true,
        lineup: false,
        sim: None,
        acts: None,
        crop: None,
        follow: false,
        world: None,
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
            "--band-speed" => a.band_speed = val().parse().unwrap(),
            "--lightning" => a.lightning = Some(val().parse().unwrap()),
            "--seed" => a.seed = val().parse().unwrap(),
            "--storm" | "--cold" | "--dust" => {
                let x = val().parse().unwrap();
                let pair = Some([x, it.next().expect("missing value").parse().unwrap()]);
                match flag.as_str() {
                    "--storm" => a.storm = pair,
                    "--cold" => a.cold = pair,
                    _ => a.dust = pair,
                }
            }
            "--volcano" => a.volcano = Some(val().parse().unwrap()),
            "--lava" => a.lava = Some(val().parse().unwrap()),
            "--ash" => a.ash = Some(val().parse().unwrap()),
            "--no-life" => a.life = false,
            "--lineup" => a.lineup = true,
            "--sim" => a.sim = Some(val().parse().unwrap()),
            "--acts" => a.acts = Some(val().parse().unwrap()),
            "--follow" => a.follow = true,
            "--crop" => {
                let mut c = [0.0; 4];
                for v in &mut c {
                    *v = it.next().expect("missing value").parse().unwrap();
                }
                a.crop = Some(c);
            }
            "--world" => a.world = Some(val().into()),
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
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("no adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut renderer = Renderer::new(device, queue);

    if let Some(minutes) = args.acts {
        return acts::render(&mut renderer, &args, minutes);
    }
    if let Some(minutes) = args.sim {
        return render_sim(&mut renderer, &args, minutes);
    }

    let mut world = match (args.lineup, args.crop) {
        (true, Some(c)) => Menagerie::lineup(args.seed, c[0], c[2]),
        _ => Menagerie::new(args.seed),
    };
    let mut batch = LifeBatch::new();
    if args.life {
        world.warm_up(args.time.min(20.0) as f32);
        world.plants_batch(&mut batch);
        renderer.set_plants(&mut batch);
    }

    if let Some(crop) = args.crop {
        return render_crop(&mut renderer, &mut world, &mut batch, &args, crop);
    }

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
        if args.life {
            if f > 0 {
                step_world(&mut world, args.step);
            }
            world.creatures_batch(&mut batch);
            renderer.set_creatures(&mut batch);
        }
        let world = view_at(&args, world_size, t);
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

/// Grows a simulated world and renders it the way the wallpaper does.
fn render_sim(renderer: &mut Renderer, args: &Args, minutes: f64) {
    let epoch = 1_800_000_000.0;
    let saved = args.world.as_ref().and_then(|p| vesper_sim::save::load(p).ok());
    let grow = saved.is_none();
    let mut sim = saved.unwrap_or_else(|| vesper_sim::Sim::new(args.seed, epoch));
    let ticks = if grow { (minutes * 60.0 / 0.2) as u64 } else { 0 };
    for t in 0..ticks {
        sim.step();
        if t % 3000 == 0 {
            eprintln!(
                "sim {:.0} min: {} creatures, {} plants",
                t as f64 * 0.2 / 60.0,
                sim.world().creatures.len(),
                sim.world().plants.len()
            );
        }
    }
    if let (true, Some(p)) = (grow, &args.world) {
        vesper_sim::save::save(&sim, p).unwrap();
    }
    // With --follow, frame the walker with the most limbs, nearer ones first.
    let follow = args.follow.then(|| {
        let w = sim.world();
        w.creatures
            .iter()
            .filter(|c| c.abil.locomotion == Some(vesper_sim::history::Locomotion::Walk))
            .max_by_key(|c| c.body.segments.len() as i64 * 100 - (c.pos[1] * 1000.0) as i64)
            .or(w.creatures.first())
            .map(|c| c.id.0)
    });
    let follow = follow.flatten();
    let mut life = living::Living::run(sim, Default::default());
    let mut batch = LifeBatch::new();
    // Following one creature, the plants would only hide it.
    let show_plants = follow.is_none();
    if show_plants {
        life.plants(&mut batch);
        renderer.set_plants(&mut batch);
    }

    // Either both monitors side by side, or one window of the world (--crop).
    let sdr = wgpu::TextureFormat::Rgba8Unorm;
    let (w, h, world_size, origins) = match args.crop {
        Some([x0, y0, cw, ch]) => {
            let ppu = MONITOR[1] as f32 * args.scale;
            let world_w = MONITOR[0] as f32 * 2.0 / MONITOR[1] as f32;
            let (w, h) = ((cw * ppu) as u32, (ch * ppu) as u32);
            (w, h, [world_w * ppu, ppu], vec![[x0 * ppu, (1.0 - y0 - ch) * ppu]])
        }
        None => {
            let w = (MONITOR[0] as f32 * args.scale) as u32;
            let h = (MONITOR[1] as f32 * args.scale) as u32;
            (w, h, [w as f32 * 2.0, h as f32], vec![[0.0, 0.0], [w as f32, 0.0]])
        }
    };
    let mut outs: Vec<_> = origins
        .iter()
        .map(|&origin_px| {
            renderer.create_output(OutputConfig { origin_px, size_px: [w, h], format: sdr, mode: OutputMode::Sdr })
        })
        .collect();
    let texs: Vec<_> = outs.iter().map(|_| target(renderer.device(), w, h, sdr)).collect();
    for f in 0..args.frames {
        if f > 0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(args.step));
            if show_plants && life.plants(&mut batch) {
                renderer.set_plants(&mut batch);
            }
        }
        let world = life.world_view(view_at(args, world_size, args.time + f as f64 * args.step));
        life.creatures(&mut batch);
        renderer.set_creatures(&mut batch);
        let mut img = image::RgbaImage::new(w * outs.len() as u32, h);
        for i in 0..outs.len() {
            renderer.render(&mut outs[i], &world, &view(&texs[i]));
            let px = read_rgba8(renderer, &texs[i], w, h);
            for y in 0..h {
                for x in 0..w {
                    let k = ((y * w + x) * 4) as usize;
                    img.put_pixel(x + w * i as u32, y, image::Rgba([px[k], px[k + 1], px[k + 2], 255]));
                }
            }
        }
        if let Some((at, len)) = follow.and_then(|id| life.locate(id)) {
            // A window a few body lengths across, centred on it, enlarged.
            let px = h as f32;
            let half = (len * 1.2).max(0.02) * px;
            let cx = at[0] * px;
            let cy = (1.0 - at[1]) * px - half * 0.3;
            let x0 = (cx - half).clamp(0.0, (w * 2) as f32 - 2.0 * half) as u32;
            let y0 = (cy - half).clamp(0.0, h as f32 - 2.0 * half) as u32;
            let side = (2.0 * half) as u32;
            let sub = image::imageops::crop_imm(&img, x0, y0, side, side).to_image();
            img = image::imageops::resize(&sub, 512, 512, image::imageops::FilterType::Triangle);
        }
        let path = args.out.join(format!("sim-{f:03}.png"));
        img.save(&path).unwrap();
        eprintln!("{}", path.display());
    }
}

/// The world at `t` seconds, with any weather the flags pin down.
fn view_at(args: &Args, world_size_px: [f32; 2], t: f64) -> WorldView {
    let mut w = WorldView::at(world_size_px, t, 1.0);
    w.band_shift = args.band.unwrap_or(0.0) + args.band_speed * (t - args.time) as f32;
    if let Some([x, s]) = args.storm {
        w.storm_x = x;
        w.lightning_x = x;
        w.storm = s;
        w.wind_low = vesper_render::rhythm::CALM_LOW * (1.0 + s);
    }
    if let Some([x, s]) = args.cold {
        w.cold_x = x;
        w.cold = s;
    }
    if let Some([x, s]) = args.dust {
        w.dust_x = x;
        w.dust = s;
    }
    if let Some(l) = args.lightning {
        w.lightning = l;
    }
    if let Some(v) = args.volcano {
        w.volcano = v;
        w.lava = vesper_render::rhythm::lava(v);
    }
    if let Some(r) = args.lava {
        w.lava = r;
    }
    if let Some(a) = args.ash {
        w.ash = a;
    }
    w
}

fn step_world(world: &mut Menagerie, secs: f64) {
    let n = (secs * 30.0).ceil().max(1.0) as usize;
    for _ in 0..n {
        world.step((secs / n as f64) as f32);
    }
}

fn render_crop(renderer: &mut Renderer, world: &mut Menagerie, batch: &mut LifeBatch, args: &Args, crop: [f32; 4]) {
    let ppu = MONITOR[1] as f32 * args.scale;
    let [x0, y0, cw, ch] = crop;
    let w = (cw * ppu) as u32;
    let h = (ch * ppu) as u32;
    let world_w = MONITOR[0] as f32 * 2.0 / MONITOR[1] as f32;
    let sdr = wgpu::TextureFormat::Rgba8Unorm;
    let mut out = renderer.create_output(OutputConfig {
        origin_px: [x0 * ppu, (1.0 - y0 - ch) * ppu],
        size_px: [w, h],
        format: sdr,
        mode: OutputMode::Sdr,
    });
    let tex = target(renderer.device(), w, h, sdr);
    for f in 0..args.frames {
        let t = args.time + f as f64 * args.step;
        if args.life {
            if f > 0 {
                step_world(world, args.step);
            }
            world.creatures_batch(batch);
            renderer.set_creatures(batch);
        }
        let view_world = view_at(args, [world_w * ppu, ppu], t);
        renderer.render(&mut out, &view_world, &view(&tex));
        let px = read_rgba8(renderer, &tex, w, h);
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            let i = ((y * w + x) * 4) as usize;
            image::Rgba([px[i], px[i + 1], px[i + 2], 255])
        });
        let path = args.out.join(format!("crop-{f:03}.png"));
        img.save(&path).unwrap();
        eprintln!("{}", path.display());
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
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
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
    read_back(r, tex, w, h, 8).chunks_exact(2).map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32()).collect()
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
