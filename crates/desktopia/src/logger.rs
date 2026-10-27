//! A small file logger: `%LOCALAPPDATA%\Desktopia\<name>.log`, started fresh
//! each run, with the previous run kept as `<name>.old.log`.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct FileLogger {
    file: Mutex<Option<File>>,
    start: Instant,
}

impl log::Log for FileLogger {
    fn enabled(&self, meta: &log::Metadata) -> bool {
        // wgpu is chatty at info; keep its warnings and errors only.
        meta.level() <= log::Level::Warn || !meta.target().starts_with("wgpu")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{:>9.3} {:<5} {}: {}\n",
            self.start.elapsed().as_secs_f64(),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.write_all(line.as_bytes());
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.flush();
        }
    }
}

pub fn init(name: &str) {
    let file = crate::config::dir("LOCALAPPDATA").and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("{name}.log"));
        let _ = std::fs::rename(&path, dir.join(format!("{name}.old.log")));
        File::create(path).ok()
    });
    let logger = Box::leak(Box::new(FileLogger { file: Mutex::new(file), start: Instant::now() }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    log::info!("Desktopia {} {name} starting (unix time {unix})", env!("CARGO_PKG_VERSION"));

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        log::logger().flush();
        default_hook(info);
    }));
}
