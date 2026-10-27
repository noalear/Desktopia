//! Feeds a dump of simulation facts through a chronicle on disk, checks that
//! reopening it gives the same history, and prints what it wrote:
//! `cargo run --release -p vesper-chronicle --example feed -- facts.jsonl out-dir`.
//!
//! Each input line is `{"tick": .., "wall": .., "fact": ..}`.

use std::io::BufRead;
use std::time::Instant;
use vesper_chronicle::{Chronicle, Fact};

#[derive(serde::Deserialize)]
struct Line {
    tick: f64,
    fact: Fact,
}

fn folder_size(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |d| d.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).expect("facts.jsonl");
    let dir = std::path::PathBuf::from(args.get(2).map_or("feed-out", |s| s.as_str()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = std::io::BufReader::new(std::fs::File::open(input).expect("open facts"));
    let mut chronicle = Chronicle::open(&dir);
    let (mut facts, mut bytes, mut busy) = (0u64, 0u64, std::time::Duration::ZERO);
    for line in file.lines() {
        let line = line.unwrap();
        bytes += line.len() as u64 + 1;
        let Ok(l) = serde_json::from_str::<Line>(&line) else { continue };
        facts += 1;
        let t = Instant::now();
        chronicle.record(l.tick, l.fact);
        busy += t.elapsed();
    }
    let live = serde_json::to_string(chronicle.history()).unwrap();
    let h = chronicle.history().clone();
    let log = folder_size(&dir);
    chronicle.checkpoint();
    drop(chronicle);
    let snapshot = folder_size(&dir);
    let t = Instant::now();
    let reopened = Chronicle::open(&dir);
    let load = t.elapsed();
    let same = serde_json::to_string(reopened.history()).unwrap() == live;

    for e in &h.entries {
        println!("[{:>6.2}h] {:?}/{} {}: {}", e.tick / 3600.0, e.kind, e.weight, e.title, h.plain(&e.text));
    }
    println!(
        "\n{facts} facts ({:.1} MB) over {:.2}h; log {:.2} MB, snapshot {:.2} MB; {:.1} µs per fact; reload {:.0} ms; same after reload: {same}",
        bytes as f64 / 1e6,
        h.now / 3600.0,
        log as f64 / 1e6,
        snapshot as f64 / 1e6,
        busy.as_secs_f64() * 1e6 / facts.max(1) as f64,
        load.as_secs_f64() * 1e3,
    );
    println!(
        "{} species ({} living), {} notables, {} groups, {} wars, {} outbreaks, {} eras, {} entries, {} deaths",
        h.species.len(),
        h.living_species(),
        h.notables.len(),
        h.groups.len(),
        h.wars.len(),
        h.outbreaks.len(),
        h.eras.len(),
        h.entries.len(),
        h.deaths
    );
    if !same {
        let _ = std::fs::write(dir.join("live.json"), &live);
        let _ = std::fs::write(dir.join("reloaded.json"), serde_json::to_string(reopened.history()).unwrap());
        panic!("reopening the chronicle gave a different history; see live.json and reloaded.json");
    }
}
