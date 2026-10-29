//! The GPU side of the wallpaper: one swap chain per monitor window, each
//! configured for SDR or HDR depending on what Windows reports for its display.

use crate::config::Config;
use crate::desktop::{self, Monitor, WallpaperWindow};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use std::num::NonZeroIsize;
use std::time::Instant;
use vesper_render::{Output, OutputConfig, OutputMode, Renderer, WorldView};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub renderer: Renderer,
}

/// One monitor's window, swap chain and renderer output. Field order matters:
/// the swap chain must go before the window it presents to.
pub struct Screen {
    surface: wgpu::Surface<'static>,
    output: Output,
    surface_config: wgpu::SurfaceConfiguration,
    window: WallpaperWindow,
}

pub struct Wallpaper {
    pub layer: HWND,
    pub monitors: Vec<Monitor>,
    pub screens: Vec<Screen>,
    /// The span of all monitors, in pixels.
    pub world_size_px: [f32; 2],
}

pub enum Presented {
    Ok,
    /// The window or swap chain is gone; the wallpaper must be rebuilt.
    Lost,
}

impl Gpu {
    /// Opens the GPU and compiles the renderer's shaders. That takes seconds,
    /// so it runs off the window thread. DXC first; Windows' own FXC is the
    /// fallback, which works everywhere but needs over a minute for our shaders.
    pub fn open() -> Result<Self, String> {
        Self::open_with(wgpu::Dx12Compiler::StaticDxc).or_else(|e| {
            log::warn!("can't use DXC ({e}); falling back to FXC");
            Self::open_with(wgpu::Dx12Compiler::Fxc)
        })
    }

    pub fn open_with(compiler: wgpu::Dx12Compiler) -> Result<Self, String> {
        let start = Instant::now();
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        desc.backends = wgpu::Backends::DX12;
        desc.backend_options.dx12.shader_compiler = compiler.clone();
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        log::info!("GPU: {} ({:?}, driver {})", info.name, info.backend, info.driver_info);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("desktopia"),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        // A shader the compiler rejects shows up here rather than as a blank wallpaper.
        let invalid = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let renderer = Renderer::new(device.clone(), queue);
        let internal = pollster::block_on(internal.pop());
        if let Some(e) = pollster::block_on(invalid.pop()).or(internal) {
            return Err(format!("shaders didn't compile: {e}"));
        }
        device.on_uncaptured_error(std::sync::Arc::new(|e| log::error!("GPU error: {e}")));
        log::info!("GPU ready in {:.1} s ({compiler:?})", start.elapsed().as_secs_f32());
        Ok(Gpu { instance, adapter, renderer })
    }
}

impl Wallpaper {
    /// Creates the windows and swap chains.
    pub fn build(gpu: &Gpu, cfg: &Config) -> Result<Self, String> {
        let layer = desktop::wallpaper_layer().ok_or("no desktop window found")?;
        let monitors = desktop::monitors();
        if monitors.is_empty() {
            return Err("no monitors".into());
        }
        let span = desktop::span(&monitors);
        log::info!(
            "{} monitors, span {}x{} at ({}, {})",
            monitors.len(),
            span.right - span.left,
            span.bottom - span.top,
            span.left,
            span.top
        );

        let mut windows_and_surfaces = Vec::new();
        for m in &monitors {
            let window = WallpaperWindow::create(layer, *m).map_err(|e| format!("window: {e}"))?;
            let surface = create_surface(&gpu.instance, window.hwnd)?;
            if !gpu.adapter.is_surface_supported(&surface) {
                return Err("the GPU can't draw to this monitor".into());
            }
            windows_and_surfaces.push((window, surface));
        }

        let screens = windows_and_surfaces
            .into_iter()
            .map(|(window, surface)| {
                let origin =
                    [(window.monitor.rect.left - span.left) as f32, (window.monitor.rect.top - span.top) as f32];
                let (surface_config, mode) = choose_config(gpu, &surface, &window.monitor, cfg);
                surface.configure(gpu.renderer.device(), &surface_config);
                let output = gpu.renderer.create_output(OutputConfig {
                    origin_px: origin,
                    size_px: [surface_config.width, surface_config.height],
                    format: surface_config.format,
                    mode,
                });
                log::info!(
                    "monitor at ({}, {}) {}x{}: {:?} as {:?}",
                    window.monitor.rect.left,
                    window.monitor.rect.top,
                    surface_config.width,
                    surface_config.height,
                    mode,
                    surface_config.format
                );
                Screen { surface, output, surface_config, window }
            })
            .collect();

        Ok(Self {
            layer,
            monitors,
            screens,
            world_size_px: [(span.right - span.left) as f32, (span.bottom - span.top) as f32],
        })
    }

    /// False if Explorer has destroyed any of the windows.
    pub fn alive(&self) -> bool {
        self.screens.iter().all(|s| s.window.alive())
    }

    /// Picks up HDR being switched on or off, or the SDR brightness slider
    /// moving, without rebuilding the windows.
    pub fn refresh_modes(&mut self, gpu: &Gpu, cfg: &Config) {
        for s in &mut self.screens {
            let (config, mode) = choose_config(gpu, &s.surface, &s.window.monitor, cfg);
            if config.format != s.surface_config.format
                || config.color_space != s.surface_config.color_space
                || mode != s.output.config().mode
            {
                log::info!("display mode changed: {:?} as {:?}", mode, config.format);
                s.surface.configure(gpu.renderer.device(), &config);
                let out = *s.output.config();
                s.output = gpu.renderer.create_output(OutputConfig { format: config.format, mode, ..out });
                s.surface_config = config;
            }
        }
    }

    pub fn present(&mut self, gpu: &Gpu, index: usize, world: &WorldView) -> Presented {
        let s = &mut self.screens[index];
        let device = gpu.renderer.device();
        let texture = match s.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                // Draw this one, then reconfigure for the next.
                let view = t.texture.create_view(&Default::default());
                gpu.renderer.render(&mut s.output, world, &view);
                gpu.renderer.queue().present(t);
                s.surface.configure(device, &s.surface_config);
                return Presented::Ok;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Presented::Ok;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                s.surface.configure(device, &s.surface_config);
                return Presented::Ok;
            }
            wgpu::CurrentSurfaceTexture::Lost => return Presented::Lost,
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("swap chain validation error on monitor {index}");
                return Presented::Lost;
            }
        };
        let view = texture.texture.create_view(&Default::default());
        gpu.renderer.render(&mut s.output, world, &view);
        gpu.renderer.queue().present(texture);
        Presented::Ok
    }
}

impl Drop for Wallpaper {
    fn drop(&mut self) {
        // Swap chains and windows go first (via the field drop), then Explorer
        // gets to repaint its own wallpaper.
        self.screens.clear();
        desktop::repaint_layer(self.layer);
    }
}

fn create_surface(instance: &wgpu::Instance, hwnd: HWND) -> Result<wgpu::Surface<'static>, String> {
    let mut handle = Win32WindowHandle::new(NonZeroIsize::new(hwnd.0 as isize).ok_or("null window")?);
    handle.hinstance = unsafe { GetModuleHandleW(None) }.ok().and_then(|h| NonZeroIsize::new(h.0 as isize));
    // SAFETY: the window outlives the surface; `Screen` drops the surface first.
    unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::Win32(handle),
        })
    }
    .map_err(|e| format!("surface: {e}"))
}

/// HDR scRGB on displays with HDR switched on, otherwise 8-bit SDR in a
/// non-sRGB format (the shader encodes and dithers).
fn choose_config(
    gpu: &Gpu,
    surface: &wgpu::Surface,
    monitor: &Monitor,
    cfg: &Config,
) -> (wgpu::SurfaceConfiguration, OutputMode) {
    let caps = surface.get_capabilities(&gpu.adapter);
    let hdr_info = surface.display_hdr_info(&gpu.adapter);
    let hdr_on = hdr_info.coarse.and_then(|c| c.high_dynamic_range) == Some(true);
    let scrgb_ok = caps.format_capabilities.iter().any(|f| {
        f.format == wgpu::TextureFormat::Rgba16Float
            && f.color_spaces.contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR)
    });

    let (format, color_space, mode) = if cfg.hdr && hdr_on && scrgb_ok {
        let lum = hdr_info.luminance.unwrap_or_default();
        // SDR content on an HDR display is shown at the SDR white level, so
        // using it as paper white makes both monitors agree.
        let paper_white = lum.sdr_white_nits.filter(|v| v.is_finite() && *v > 40.0).unwrap_or(200.0);
        let peak = lum.max_nits.filter(|v| v.is_finite() && *v > paper_white).unwrap_or(paper_white * 4.0).min(4000.0);
        (
            wgpu::TextureFormat::Rgba16Float,
            wgpu::SurfaceColorSpace::ExtendedSrgbLinear,
            OutputMode::Hdr { paper_white_nits: paper_white, peak_nits: peak },
        )
    } else {
        let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .into_iter()
            .find(|f| caps.formats.contains(f))
            .unwrap_or_else(|| caps.formats.first().copied().unwrap_or(wgpu::TextureFormat::Bgra8Unorm));
        (format, wgpu::SurfaceColorSpace::Auto, OutputMode::Sdr)
    };

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space,
        width: monitor.width(),
        height: monitor.height(),
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 1,
        alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
        view_formats: vec![],
    };
    (config, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Every shader compiles with DXC, in seconds. The build machine has no GPU,
    /// so this runs on Windows' software renderer; there DXC took 8 s and FXC 41 s.
    #[test]
    fn shaders_compile_quickly() {
        let start = Instant::now();
        match Gpu::open_with(wgpu::Dx12Compiler::StaticDxc) {
            Ok(_) => {}
            Err(e) if e.starts_with("no GPU") => return eprintln!("skipped: {e}"),
            Err(e) => panic!("{e}"),
        }
        let took = start.elapsed();
        assert!(took < Duration::from_secs(30), "the shaders took {took:?} to compile");
    }
}
