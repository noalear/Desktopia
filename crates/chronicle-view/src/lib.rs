//! The chronicle window. It runs as its own process, opened from the tray or the
//! hotkey, and follows the files the wallpaper's chronicle writes. Closing it
//! ends the process, so it costs nothing while closed; while open it reads new
//! lines every couple of seconds and only repaints when something changed.

mod app;
mod pages;
mod theme;
mod tree;
mod widgets;

pub use app::{ChronicleApp, Page, Source};

/// The window title, which the wallpaper uses to find an open window.
pub const TITLE: &str = "Vesper Chronicle";

/// Opens the window and returns when it is closed.
pub fn run(source: Source) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1360.0, 880.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(icon()),
        ..Default::default()
    };
    eframe::run_native(TITLE, options, Box::new(move |cc| Ok(Box::new(ChronicleApp::new(&cc.egui_ctx, source)))))
}

/// A small twilight disc: amber sun edge fading to violet night.
fn icon() -> eframe::egui::IconData {
    let n = 64u32;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = ((x as f32 + 0.5) / n as f32 - 0.5, (y as f32 + 0.5) / n as f32 - 0.5);
            let d = (fx * fx + fy * fy).sqrt();
            let t = (fx + 0.5).clamp(0.0, 1.0);
            let r = 244.0 * (1.0 - t) + 70.0 * t;
            let g = 172.0 * (1.0 - t) + 40.0 * t;
            let b = 72.0 * (1.0 - t) + 150.0 * t;
            let a = ((0.48 - d) * 64.0).clamp(0.0, 1.0);
            rgba.extend([r as u8, g as u8, b as u8, (a * 255.0) as u8]);
        }
    }
    eframe::egui::IconData { rgba, width: n, height: n }
}
