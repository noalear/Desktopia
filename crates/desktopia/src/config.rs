//! Settings from `%APPDATA%\Desktopia\config.toml`. A missing file or key falls
//! back to the defaults, and a commented default file is written on first run.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Frame cap while the desktop is in view.
    pub fps: u32,
    /// Frame cap on battery power.
    pub battery_fps: u32,
    /// Frame cap while the CPU is busy with something else.
    pub busy_fps: u32,
    /// Total CPU use, in percent, above which Desktopia slows down.
    pub busy_cpu_percent: f32,
    /// Use HDR on monitors that have it switched on in Windows.
    pub hdr: bool,
    /// Brightness multiplier for the whole scene.
    pub exposure: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self { fps: 30, battery_fps: 10, busy_fps: 10, busy_cpu_percent: 80.0, hdr: true, exposure: 1.0 }
    }
}

const DEFAULT_FILE: &str = "\
# Desktopia settings. Delete this file to get the defaults back.

# Frame cap while the desktop is in view.
fps = 30

# Frame cap on battery power.
battery_fps = 10

# Frame cap while other programs keep the CPU busy, and what counts as busy.
busy_fps = 10
busy_cpu_percent = 80

# Use HDR on monitors that have HDR switched on in Windows.
hdr = true

# Brightness multiplier for the whole scene.
exposure = 1.0
";

pub fn dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(|v| PathBuf::from(v).join("Desktopia"))
}

pub fn load() -> Config {
    let Some(dir) = dir("APPDATA") else {
        return Config::default();
    };
    let path = dir.join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::warn!("ignoring {}: {e}", path.display());
                Config::default()
            }
        },
        Err(_) => {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&path, DEFAULT_FILE);
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_file_parses_to_defaults() {
        let cfg: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        let def = super::Config::default();
        assert_eq!(cfg.fps, def.fps);
        assert_eq!(cfg.busy_cpu_percent, def.busy_cpu_percent);
        assert_eq!(cfg.exposure, def.exposure);
    }
}
