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
    /// CPU use by other programs, in percent of the whole machine, above
    /// which Desktopia draws more slowly.
    pub busy_cpu_percent: f32,
    /// CPU use by other programs above which a faster world drops back to
    /// real time, until the PC has been quiet for half a minute.
    pub fast_cpu_percent: f32,
    /// Use HDR on monitors that have it switched on in Windows.
    pub hdr: bool,
    /// Brightness multiplier for the whole scene.
    pub exposure: f32,
    /// Keys that open the chronicle, like "Win+Alt+V". Empty turns them off.
    pub chronicle_hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fps: 30,
            battery_fps: 10,
            busy_fps: 10,
            busy_cpu_percent: 80.0,
            fast_cpu_percent: 20.0,
            hdr: true,
            exposure: 1.0,
            chronicle_hotkey: "Win+Alt+V".into(),
        }
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

# When the world runs faster than real time (Speed in the tray menu), it drops
# back to real time while other programs use more CPU than this, and for
# fullscreen games, presentations and battery power.
fast_cpu_percent = 20

# Use HDR on monitors that have HDR switched on in Windows.
hdr = true

# Brightness multiplier for the whole scene.
exposure = 1.0

# Keys that open the chronicle, like \"Win+Alt+V\" or \"Ctrl+Shift+F12\". Empty turns them off.
chronicle_hotkey = \"Win+Alt+V\"
";

/// Modifier flags as `RegisterHotKey` takes them.
pub const MOD_ALT: u32 = 1;
pub const MOD_CONTROL: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
pub const MOD_WIN: u32 = 8;

/// Reads "Win+Alt+V" as modifier flags and a virtual-key code. Keys can be a
/// letter, a digit, F1 to F24, Space, Pause or ScrollLock.
pub fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0;
    let mut key = None;
    for part in spec.split('+').map(|p| p.trim().to_ascii_lowercase()) {
        match part.as_str() {
            "" => return None,
            "win" | "windows" | "super" => mods |= MOD_WIN,
            "alt" => mods |= MOD_ALT,
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "space" => key = Some(0x20),
            "pause" => key = Some(0x13),
            "scrolllock" => key = Some(0x91),
            p if p.len() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric()) => {
                key = Some(p.to_ascii_uppercase().as_bytes()[0] as u32)
            }
            p if p.starts_with('f') => match p[1..].parse::<u32>() {
                Ok(n @ 1..=24) => key = Some(0x6f + n),
                _ => return None,
            },
            _ => return None,
        }
    }
    // A bare letter would steal typing everywhere.
    match key {
        Some(k) if mods != 0 || (0x70..=0x87).contains(&k) || k == 0x13 || k == 0x91 => Some((mods, k)),
        _ => None,
    }
}

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
        assert_eq!(cfg.fast_cpu_percent, def.fast_cpu_percent);
        assert_eq!(cfg.exposure, def.exposure);
        assert_eq!(cfg.chronicle_hotkey, def.chronicle_hotkey);
    }

    #[test]
    fn hotkeys_parse() {
        use super::*;
        assert_eq!(parse_hotkey("Win+Alt+V"), Some((MOD_WIN | MOD_ALT, 'V' as u32)));
        assert_eq!(parse_hotkey("ctrl + shift + f12"), Some((MOD_CONTROL | MOD_SHIFT, 0x7b)));
        assert_eq!(parse_hotkey("F9"), Some((0, 0x78)));
        assert_eq!(parse_hotkey("V"), None);
        assert_eq!(parse_hotkey("Win+Banana"), None);
        assert_eq!(parse_hotkey(""), None);
    }
}
