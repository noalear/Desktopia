//! Keeping the history on disk.
//!
//! A chronicle folder holds `snapshot.json`, the whole history at some moment,
//! and `log-N.jsonl`, every fact since, one JSON object per line. N is the
//! snapshot's generation. A checkpoint writes a new snapshot and starts an empty
//! log, so the log stays short and loading stays fast. Writes happen on a
//! background thread so the simulation never waits on the disk.
//!
//! The simulation reports many deaths a second, nearly all of them unremarkable.
//! Those are logged as one [`Tally`] line before the next fact that matters,
//! and the log checkpoints itself once it grows past [`LOG_LIMIT`].

use crate::facts::Fact;
use crate::history::{History, Tally};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

const SNAPSHOT: &str = "snapshot.json";
const VERSION: u32 = 1;
/// A log this big is folded into a new snapshot.
pub const LOG_LIMIT: u64 = 4 << 20;

/// One log line: a fact, or a tally of routine deaths.
#[derive(Serialize)]
struct RecordOut<'a> {
    tick: f64,
    wall: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    fact: Option<&'a Fact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tally: Option<&'a Tally>,
}

#[derive(Deserialize)]
struct RecordIn {
    tick: f64,
    wall: f64,
    #[serde(default)]
    fact: Option<Fact>,
    #[serde(default)]
    tally: Option<Tally>,
}

#[derive(Serialize)]
struct SnapshotOut<'a> {
    version: u32,
    generation: u64,
    history: &'a History,
}

#[derive(Deserialize)]
struct SnapshotIn {
    version: u32,
    generation: u64,
    history: History,
}

fn log_path(dir: &Path, generation: u64) -> PathBuf {
    dir.join(format!("log-{generation}.jsonl"))
}

fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

fn read_snapshot(dir: &Path) -> Option<SnapshotIn> {
    let path = dir.join(SNAPSHOT);
    let text = fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<SnapshotIn>(&text) {
        Ok(s) if s.version == VERSION => Some(s),
        Ok(s) => {
            log::warn!("chronicle snapshot is version {}, expected {VERSION}; starting over", s.version);
            None
        }
        Err(e) => {
            log::warn!("can't read {}: {e}", path.display());
            None
        }
    }
}

/// Applies every complete line of `bytes` to `history`, stopping early at a line
/// later than `until`. Returns how many bytes were used and whether any line
/// was left out because of `until`.
fn replay(history: &mut History, bytes: &[u8], until: Option<f64>) -> (usize, bool) {
    let mut used = 0;
    while let Some(len) = bytes[used..].iter().position(|&b| b == b'\n') {
        let line = &bytes[used..used + len];
        if !line.is_empty() {
            match serde_json::from_slice::<RecordIn>(line) {
                Ok(r) if until.is_some_and(|t| r.tick > t) => return (used, true),
                Ok(r) => {
                    if let Some(t) = &r.tally {
                        history.apply_tally(r.tick, r.wall, t);
                    }
                    if let Some(f) = &r.fact {
                        history.apply(r.tick, r.wall, f);
                    }
                }
                Err(e) => log::warn!("skipping a chronicle line: {e}"),
            }
        }
        used += len + 1;
    }
    (used, false)
}

enum Msg {
    Line(String),
    /// Write this snapshot, then start log `generation`.
    Checkpoint {
        generation: u64,
        snapshot: String,
    },
}

struct Writer {
    tx: Sender<Msg>,
    thread: JoinHandle<()>,
}

impl Writer {
    fn start(dir: PathBuf, generation: u64) -> Option<Self> {
        let file = OpenOptions::new().create(true).append(true).open(log_path(&dir, generation));
        let file = match file {
            Ok(f) => f,
            Err(e) => {
                log::error!("chronicle can't write to {}: {e}", dir.display());
                return None;
            }
        };
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("chronicle".into())
            .spawn(move || write_loop(dir, generation, BufWriter::new(file), rx))
            .ok()?;
        Some(Self { tx, thread })
    }

    fn finish(self) {
        drop(self.tx);
        let _ = self.thread.join();
    }
}

fn write_loop(dir: PathBuf, mut generation: u64, mut log: BufWriter<File>, rx: Receiver<Msg>) {
    let mut handle = |msg: Msg, log: &mut BufWriter<File>| match msg {
        Msg::Line(line) => {
            let _ = log.write_all(line.as_bytes());
            let _ = log.write_all(b"\n");
        }
        Msg::Checkpoint { generation: next, snapshot } => {
            let _ = log.flush();
            let tmp = dir.join("snapshot.tmp");
            if let Err(e) = fs::write(&tmp, snapshot) {
                log::error!("chronicle snapshot: {e}");
                return;
            }
            // The window may have the old snapshot open for a moment.
            let mut renamed = false;
            for _ in 0..20 {
                if fs::rename(&tmp, dir.join(SNAPSHOT)).is_ok() {
                    renamed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            if !renamed {
                log::error!("chronicle snapshot: can't replace {SNAPSHOT}");
                return;
            }
            match File::create(log_path(&dir, next)) {
                Ok(f) => {
                    *log = BufWriter::new(f);
                    let _ = fs::remove_file(log_path(&dir, generation));
                    generation = next;
                }
                Err(e) => log::error!("chronicle log: {e}"),
            }
        }
    };
    while let Ok(msg) = rx.recv() {
        handle(msg, &mut log);
        // Batch whatever else is waiting, then flush once.
        while let Ok(msg) = rx.try_recv() {
            handle(msg, &mut log);
        }
        let _ = log.flush();
    }
    let _ = log.flush();
}

/// The wallpaper's side: takes facts, keeps the history, writes it down.
pub struct Chronicle {
    history: History,
    dir: Option<PathBuf>,
    generation: u64,
    writer: Option<Writer>,
    /// Routine deaths not yet written, and the tick of the latest.
    tally: Tally,
    tally_tick: f64,
    /// Bytes in the current log.
    logged: u64,
}

impl Chronicle {
    /// A chronicle that keeps nothing on disk.
    pub fn in_memory() -> Self {
        Self {
            history: History::default(),
            dir: None,
            generation: 0,
            writer: None,
            tally: Tally::default(),
            tally_tick: 0.0,
            logged: 0,
        }
    }

    /// Opens, or starts, the chronicle kept in `dir`.
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        Self::load(dir.into(), None)
    }

    fn load(dir: PathBuf, until: Option<f64>) -> Self {
        if let Err(e) = fs::create_dir_all(&dir) {
            log::error!("chronicle can't create {}: {e}", dir.display());
        }
        let (mut history, generation) = match read_snapshot(&dir) {
            Some(s) => (s.history, s.generation),
            None => (History::default(), 0),
        };
        if until.is_some_and(|t| history.now > t) {
            log::warn!("chronicle snapshot is newer than the world save; keeping it");
        }
        let path = log_path(&dir, generation);
        let bytes = fs::read(&path).unwrap_or_default();
        let (used, dropped) = replay(&mut history, &bytes, until);
        if used < bytes.len() && !dropped {
            // A torn last line from a crash: cut it so new lines start clean.
            if let Ok(f) = OpenOptions::new().write(true).open(&path) {
                let _ = f.set_len(used as u64);
            }
        }
        let writer = Writer::start(dir.clone(), generation);
        let mut c = Self {
            history,
            dir: Some(dir),
            generation,
            writer,
            tally: Tally::default(),
            tally_tick: 0.0,
            logged: used as u64,
        };
        if dropped || c.logged > LOG_LIMIT {
            c.checkpoint();
        }
        c
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn record(&mut self, tick: f64, fact: Fact) {
        let wall = unix_now();
        if let Fact::Death { who, species, cause, life } = &fact
            && self.history.is_routine(*who, life)
        {
            self.tally.add(*who, *species, *cause, life.x);
            self.tally_tick = tick;
        } else {
            self.flush_tally(wall);
            self.write(&RecordOut { tick, wall, fact: Some(&fact), tally: None });
        }
        self.history.apply(tick, wall, &fact);
        if self.logged > LOG_LIMIT {
            self.checkpoint();
        }
    }

    fn write(&mut self, record: &RecordOut) {
        if let Some(w) = &self.writer
            && let Ok(line) = serde_json::to_string(record)
        {
            self.logged += line.len() as u64 + 1;
            let _ = w.tx.send(Msg::Line(line));
        }
    }

    /// Writes the routine deaths counted since the last line, stamped no
    /// earlier than any of them so replaying gives the same history.
    fn flush_tally(&mut self, wall: f64) {
        if self.tally.is_empty() {
            return;
        }
        let tally = std::mem::take(&mut self.tally);
        self.write(&RecordOut { tick: self.tally_tick, wall, fact: None, tally: Some(&tally) });
    }

    pub fn record_all(&mut self, facts: impl IntoIterator<Item = (f64, Fact)>) {
        for (tick, fact) in facts {
            self.record(tick, fact);
        }
    }

    /// Writes the whole history and starts a fresh log. Call it whenever the
    /// world is saved, so the two always agree after a crash.
    pub fn checkpoint(&mut self) {
        // The history already holds the unwritten tally, so the snapshot does too.
        self.tally = Tally::default();
        let Some(w) = &self.writer else { return };
        let generation = self.generation + 1;
        let snapshot = SnapshotOut { version: VERSION, generation, history: &self.history };
        match serde_json::to_string(&snapshot) {
            Ok(snapshot) => {
                if w.tx.send(Msg::Checkpoint { generation, snapshot }).is_ok() {
                    self.generation = generation;
                    self.logged = 0;
                }
            }
            Err(e) => log::error!("chronicle snapshot: {e}"),
        }
    }

    /// Forgets everything after `tick`. Call it right after loading a world
    /// save, with the save's tick, so history the world lost is lost here too.
    pub fn rewind_to(&mut self, tick: f64) {
        if self.history.now <= tick {
            return;
        }
        let Some(dir) = self.dir.clone() else { return };
        self.flush_tally(self.history.wall);
        if let Some(w) = self.writer.take() {
            w.finish();
        }
        *self = Self::load(dir, Some(tick));
    }

    /// Starts a new world: the old chronicle moves to `chronicle-past` beside it.
    pub fn new_world(&mut self) {
        let Some(dir) = self.dir.clone() else {
            self.history = History::default();
            return;
        };
        self.flush_tally(self.history.wall);
        if let Some(w) = self.writer.take() {
            w.finish();
        }
        if dir.join(SNAPSHOT).exists() || log_path(&dir, self.generation).exists() {
            let past = dir.with_file_name("chronicle-past");
            let _ = fs::create_dir_all(&past);
            let to = past.join(format!("{}", unix_now() as u64));
            if let Err(e) = fs::rename(&dir, &to) {
                log::error!("can't archive the old chronicle: {e}");
                let _ = fs::remove_dir_all(&dir);
            }
        }
        *self = Self::load(dir, None);
    }
}

impl Drop for Chronicle {
    fn drop(&mut self) {
        self.flush_tally(self.history.wall);
        if let Some(w) = self.writer.take() {
            w.finish();
        }
    }
}

/// The window's side: follows a chronicle folder another process is writing.
pub struct Reader {
    dir: PathBuf,
    snapshot_stamp: Option<(SystemTime, u64)>,
    generation: u64,
    offset: u64,
    pub history: History,
}

impl Reader {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let mut r =
            Self { dir: dir.into(), snapshot_stamp: None, generation: 0, offset: 0, history: History::default() };
        r.poll();
        r
    }

    /// A reader over a history built in memory, for demos and tests.
    pub fn fixed(history: History) -> Self {
        Self { dir: PathBuf::new(), snapshot_stamp: None, generation: 0, offset: 0, history }
    }

    /// Whether the folder has anything in it yet.
    pub fn exists(&self) -> bool {
        self.dir.join(SNAPSHOT).exists() || log_path(&self.dir, 0).exists()
    }

    /// Catches up with the files. Returns true if the history changed.
    pub fn poll(&mut self) -> bool {
        if self.dir.as_os_str().is_empty() {
            return false;
        }
        let mut changed = false;
        let stamp = fs::metadata(self.dir.join(SNAPSHOT)).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
        if stamp.is_some()
            && stamp != self.snapshot_stamp
            && let Some(s) = read_snapshot(&self.dir)
        {
            self.history = s.history;
            self.generation = s.generation;
            self.offset = 0;
            self.snapshot_stamp = stamp;
            changed = true;
        }
        let Ok(mut f) = File::open(log_path(&self.dir, self.generation)) else { return changed };
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return changed;
        }
        let mut bytes = Vec::new();
        if f.read_to_end(&mut bytes).is_err() || bytes.is_empty() {
            return changed;
        }
        let (used, _) = replay(&mut self.history, &bytes, None);
        self.offset += used as u64;
        changed || used > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vesper-chronicle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn arose(id: u64) -> Fact {
        Fact::SpeciesArose { id: SpeciesId(id), parent: None, traits: crate::demo::seed_traits(id) }
    }

    #[test]
    fn survives_reopening_and_checkpoints() {
        let dir = temp_dir("reopen");
        {
            let mut c = Chronicle::open(&dir);
            c.record(1.0, arose(1));
            c.checkpoint();
            c.record(2.0, arose(2));
        }
        let c = Chronicle::open(&dir);
        assert_eq!(c.history().species.len(), 2);
        let r = Reader::new(&dir);
        assert_eq!(r.history.species.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewind_forgets_the_future() {
        let dir = temp_dir("rewind");
        {
            let mut c = Chronicle::open(&dir);
            c.record(1.0, arose(1));
            c.checkpoint();
            c.record(5.0, arose(2));
            c.record(9.0, arose(3));
        }
        let mut c = Chronicle::open(&dir);
        c.rewind_to(6.0);
        assert_eq!(c.history().species.len(), 2);
        drop(c);
        assert_eq!(Chronicle::open(&dir).history().species.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_follows_a_live_writer() {
        let dir = temp_dir("follow");
        let mut c = Chronicle::open(&dir);
        c.record(1.0, arose(1));
        c.checkpoint();
        drop(c);
        let mut r = Reader::new(&dir);
        assert_eq!(r.history.species.len(), 1);
        let mut c = Chronicle::open(&dir);
        c.record(2.0, arose(2));
        drop(c);
        assert!(r.poll());
        assert_eq!(r.history.species.len(), 2);
        let mut c = Chronicle::open(&dir);
        c.checkpoint();
        c.record(3.0, arose(3));
        drop(c);
        assert!(r.poll());
        assert_eq!(r.history.species.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ordinary_deaths_are_tallied_and_replay_the_same() {
        let dir = temp_dir("tally");
        let live = {
            let mut c = Chronicle::open(&dir);
            c.record(0.0, arose(1));
            c.record(0.0, arose(2));
            for i in 0..500u64 {
                let species = SpeciesId(1 + i % 2);
                let cause = if i % 3 == 0 {
                    DeathCause::Starvation
                } else {
                    DeathCause::Predation { killer: CreatureId(9000 + i), killer_species: SpeciesId(2) }
                };
                let mut life = crate::demo::blank_life(CreatureId(100 + i), species);
                life.x = i as f32 / 500.0;
                c.record(1.0 + i as f64, Fact::Death { who: life.id, species, cause, life });
                if i == 250 {
                    c.record(251.5, Fact::Nature { kind: NatureKind::Storm, x: 1.0, strength: 0.5 });
                }
            }
            serde_json::to_string(c.history()).unwrap()
        };
        let lines = fs::read_to_string(log_path(&dir, 0)).unwrap().lines().count();
        assert_eq!(lines, 5, "two species, a tally, the storm, a tally");
        let reopened = Chronicle::open(&dir);
        assert_eq!(reopened.history().deaths, 500);
        assert_eq!(serde_json::to_string(reopened.history()).unwrap(), live);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn torn_last_line_is_cut() {
        let dir = temp_dir("torn");
        {
            let mut c = Chronicle::open(&dir);
            c.record(1.0, arose(1));
        }
        let mut f = OpenOptions::new().append(true).open(log_path(&dir, 0)).unwrap();
        f.write_all(b"{\"tick\":2.0,\"wa").unwrap();
        drop(f);
        {
            let mut c = Chronicle::open(&dir);
            c.record(3.0, arose(3));
        }
        assert_eq!(Chronicle::open(&dir).history().species.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
