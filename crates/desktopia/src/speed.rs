//! How fast Vesper's time runs, chosen from the tray and kept in
//! `%LOCALAPPDATA%\Desktopia\speed.txt` so it lasts across restarts.

use std::path::PathBuf;
use vesper_sim::runner::FLAT_OUT;

/// The choices on the tray menu, in sim seconds per real second.
pub const SPEEDS: [f32; 5] = [1.0, 2.0, 5.0, 10.0, FLAT_OUT];

pub fn label(speed: f32) -> &'static str {
    match speed {
        s if s.is_infinite() => "As fast as it can",
        s if s >= 10.0 => "10\u{d7} faster",
        s if s >= 5.0 => "5\u{d7} faster",
        s if s >= 2.0 => "2\u{d7} faster",
        _ => "Real time",
    }
}

/// How many threads besides its own the world may borrow when one core
/// isn't enough: a quarter of the machine at most, never more than three.
pub fn helpers() -> usize {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    (threads / 4).min(3)
}

fn path() -> Option<PathBuf> {
    crate::config::dir("LOCALAPPDATA").map(|d| d.join("speed.txt"))
}

/// The speed last chosen, or real time.
pub fn load() -> f32 {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map_or(1.0, |t| parse(&t))
}

pub fn store(speed: f32) {
    if let Some(p) = path()
        && let Err(e) = std::fs::write(&p, format(speed))
    {
        log::warn!("can't keep the speed in {}: {e}", p.display());
    }
}

fn format(speed: f32) -> String {
    if speed.is_infinite() { "max".into() } else { format!("{speed}") }
}

/// Reads a stored speed, falling back to real time for anything that isn't
/// one of the menu's choices.
fn parse(text: &str) -> f32 {
    let speed = match text.trim() {
        "max" => FLAT_OUT,
        t => t.parse().unwrap_or(1.0),
    };
    if SPEEDS.contains(&speed) { speed } else { 1.0 }
}

/// How fast the world has really been going, for the tooltip.
pub fn describe_rate(rate: f32) -> String {
    if rate < 9.95 { format!("{rate:.1}\u{d7}") } else { format!("{rate:.0}\u{d7}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_survive_the_file() {
        for s in SPEEDS {
            assert_eq!(parse(&format(s)), s);
        }
        assert_eq!(parse("7"), 1.0);
        assert_eq!(parse("banana"), 1.0);
        assert_eq!(parse(" max\n"), FLAT_OUT);
    }

    #[test]
    fn labels_are_distinct() {
        let mut seen: Vec<_> = SPEEDS.iter().map(|s| label(*s)).collect();
        seen.dedup();
        assert_eq!(seen.len(), SPEEDS.len());
    }
}
