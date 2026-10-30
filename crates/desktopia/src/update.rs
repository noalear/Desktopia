//! Keeps Desktopia up to date from the "latest" GitHub release, which CI
//! rebuilds on every merge to main.
//!
//! A low-priority thread asks GitHub which commit that release was built from,
//! a few minutes after start and then every few hours. If it isn't this
//! build's commit, it downloads the new desktopia.exe next to this one, runs it
//! once with `--version` to be sure it is whole and is the build it claims to
//! be, then swaps the files: Windows lets a running exe be renamed, so this one
//! becomes `desktopia.exe.old` and the new one takes its name. The main loop
//! then restarts into it when nobody is looking. The world, the chronicle and
//! the settings live in the user's folders, not next to the exe, so they carry
//! over untouched.
//!
//! The HTTP work is done by the curl.exe that ships with Windows, so the
//! wallpaper carries no TLS stack of its own.

use serde::Deserialize;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The commit this build was made from, stamped by CI; empty for a build made
/// anywhere else, which never replaces itself.
pub const BUILD: &str = env!("DESKTOPIA_BUILD");

/// How long after start to look first, so startup has the PC to itself.
const FIRST_CHECK: Duration = Duration::from_secs(3 * 60);
/// How often to look after that.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How soon to try again after a check that failed.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// How long a new build may take to answer `--version`.
const VERSION_WAIT: Duration = Duration::from_secs(60);

/// Why updates are off when the settings turn them off; a check can still be asked for.
const TURNED_OFF: &str = "auto_update is false in the settings";

/// The first seven characters of a commit, as GitHub shows them.
pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Updates won't run, and why.
    Off(&'static str),
    /// Not checked yet.
    Waiting,
    Checking,
    /// Fetching the build from this commit.
    Downloading(String),
    UpToDate,
    /// The build from this commit is in place; restarting will run it.
    Ready(String),
    /// The repository is private and no token was given.
    NeedsToken,
    Failed(String),
}

impl Status {
    /// One line for the tray menu.
    pub fn describe(&self) -> String {
        match self {
            Status::Off(why) => format!("Updates are off: {why}"),
            Status::Waiting => format!("Build {}; checks for updates soon", short(BUILD)),
            Status::Checking => "Checking for updates\u{2026}".into(),
            Status::Downloading(sha) => format!("Downloading build {}\u{2026}", short(sha)),
            Status::UpToDate => format!("Up to date (build {})", short(BUILD)),
            Status::Ready(sha) => format!("Build {} is ready; restarts when you're away", short(sha)),
            Status::NeedsToken => "Updates need a GitHub token (update_token in the settings)".into(),
            Status::Failed(why) => format!("Last update check failed: {why}"),
        }
    }

    /// Whether asking for a check now would do anything.
    pub fn can_check(&self) -> bool {
        match self {
            Status::Waiting | Status::UpToDate | Status::NeedsToken | Status::Failed(_) => true,
            Status::Off(why) => *why == TURNED_OFF,
            Status::Checking | Status::Downloading(_) | Status::Ready(_) => false,
        }
    }
}

/// The update thread, and what it last found.
pub struct Updater {
    status: Arc<Mutex<Status>>,
    wake: Option<Sender<()>>,
    exe: Option<PathBuf>,
}

impl Updater {
    /// Starts checking in the background, unless this build can't be updated.
    /// `on_start` runs first on the new thread, to lower its priority.
    pub fn start(on_start: Option<Box<dyn FnOnce() + Send>>) -> Self {
        let exe = std::env::current_exe().ok();
        let off = |why| Self { status: Arc::new(Mutex::new(Status::Off(why))), wake: None, exe: None };
        if BUILD.is_empty() {
            return off("this copy wasn't built by GitHub");
        }
        let Some(exe) = exe else {
            return off("can't tell where desktopia.exe is");
        };
        tidy(&exe);
        let status = Arc::new(Mutex::new(Status::Waiting));
        let (wake, woken) = mpsc::channel();
        let shared = status.clone();
        let target = exe.clone();
        let started = std::thread::Builder::new().name("updater".into()).spawn(move || {
            if let Some(f) = on_start {
                f();
            }
            let set = |s: Status| *shared.lock().unwrap() = s;
            let mut wait = FIRST_CHECK;
            loop {
                let asked = match woken.recv_timeout(wait) {
                    Ok(()) => true,
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                let cfg = crate::config::load();
                if !cfg.auto_update && !asked {
                    set(Status::Off(TURNED_OFF));
                    wait = CHECK_EVERY;
                    continue;
                }
                set(Status::Checking);
                match update(&cfg.update_repo, cfg.update_token.get(), &target, &set) {
                    Ok(None) => {
                        log::info!("update: build {} is the latest", short(BUILD));
                        set(Status::UpToDate);
                        wait = CHECK_EVERY;
                    }
                    Ok(Some(sha)) => {
                        log::info!("update: build {} is in place at {}", short(&sha), target.display());
                        set(Status::Ready(sha));
                        return;
                    }
                    Err(Problem::NeedsToken) => {
                        log::warn!("update: the release can't be seen without a token");
                        set(Status::NeedsToken);
                        wait = CHECK_EVERY;
                    }
                    Err(Problem::Failed(why)) => {
                        log::warn!("update: {why}");
                        set(Status::Failed(why));
                        wait = RETRY_AFTER;
                    }
                }
            }
        });
        if let Err(e) = started {
            log::error!("can't start the update thread: {e}");
            return off("the update thread didn't start");
        }
        Self { status, wake: Some(wake), exe: Some(exe) }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// Checks now instead of waiting, even if updates are off in the settings.
    pub fn check_now(&self) {
        if let Some(w) = &self.wake {
            let _ = w.send(());
        }
    }

    /// The exe to start in place of this one, once an update is ready.
    pub fn exe(&self) -> Option<&Path> {
        self.exe.as_deref()
    }
}

enum Problem {
    NeedsToken,
    Failed(String),
}

impl From<String> for Problem {
    fn from(s: String) -> Self {
        Problem::Failed(s)
    }
}

#[derive(Deserialize)]
struct Release {
    target_commitish: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    /// The API address of the file; asking it for octet-stream gives the bytes.
    url: String,
    #[serde(default)]
    state: String,
}

/// Brings the newest build into place at `exe`. Returns its commit, or None
/// if this build is already the newest.
fn update(repo: &str, token: &str, exe: &Path, set: &dyn Fn(Status)) -> Result<Option<String>, Problem> {
    let api = format!("https://api.github.com/repos/{}/releases/tags/latest", repo.trim());
    let (code, body) = curl(&api, "application/vnd.github+json", token, None)?;
    match code {
        200 => {}
        404 | 401 if token.is_empty() => return Err(Problem::NeedsToken),
        401 => return Err("GitHub didn't accept update_token".to_string().into()),
        404 => return Err("there's no latest release, or the token can't read the repository".to_string().into()),
        403 | 429 => return Err("GitHub asked to wait (rate limit)".to_string().into()),
        c => return Err(format!("GitHub answered {c}").into()),
    }
    let release: Release = serde_json::from_slice(&body).map_err(|e| format!("can't read GitHub's answer: {e}"))?;
    let Some(asset) = wanted(&release, BUILD)? else { return Ok(None) };
    let sha = release.target_commitish.trim().to_string();

    set(Status::Downloading(sha.clone()));
    log::info!("update: downloading build {} ({} MB)", short(&sha), asset.size / 1_000_000);
    let download = beside(exe, ".download");
    let (code, _) = curl(&asset.url, "application/octet-stream", token, Some(&download))?;
    let check = || -> Result<(), String> {
        if code != 200 {
            return Err(format!("the download answered {code}"));
        }
        let bytes = std::fs::read(&download).map_err(|e| format!("can't read the download: {e}"))?;
        if bytes.len() as u64 != asset.size || !bytes.starts_with(b"MZ") {
            return Err(format!("the download is broken ({} of {} bytes)", bytes.len(), asset.size));
        }
        let says = version_of(&download)?;
        if says != sha {
            return Err(format!("the download says it is build {}, not {}", short(&says), short(&sha)));
        }
        Ok(())
    };
    if let Err(e) = check() {
        let _ = std::fs::remove_file(&download);
        return Err(e.into());
    }
    swap(exe, &download).map_err(|e| format!("can't put the new build in place: {e}"))?;
    Ok(Some(sha))
}

/// The release's exe, if the release was built from another commit than `build`.
fn wanted<'a>(release: &'a Release, build: &str) -> Result<Option<&'a Asset>, String> {
    let sha = release.target_commitish.trim();
    if sha.eq_ignore_ascii_case(build) {
        return Ok(None);
    }
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("the release doesn't name its commit ({sha:?})"));
    }
    release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("desktopia.exe") && a.state == "uploaded" && a.size > 0)
        .map(Some)
        .ok_or_else(|| "the release has no finished desktopia.exe yet".into())
}

/// `exe` with `suffix` added to its file name.
fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(exe.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

/// Puts `new` where `exe` is, keeping the old one as `.old` until next start.
fn swap(exe: &Path, new: &Path) -> std::io::Result<()> {
    let old = beside(exe, ".old");
    match std::fs::remove_file(&old) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::rename(exe, &old)?;
    // An antivirus may still be looking at the new file; give it a moment.
    let mut tries = 0;
    while let Err(e) = std::fs::rename(new, exe) {
        tries += 1;
        if tries == 5 || !new.exists() {
            let _ = std::fs::rename(&old, exe);
            return Err(e);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Clears what the last update left behind. The old exe may still be running
/// an open chronicle window; then it goes next time.
fn tidy(exe: &Path) {
    for suffix in [".old", ".download"] {
        let _ = std::fs::remove_file(beside(exe, suffix));
    }
}

/// A command that never flashes a console window.
fn quiet(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    }
    c
}

/// The curl that comes with Windows 10 and later, by its full path.
fn curl_exe() -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(root) if cfg!(windows) => PathBuf::from(root).join("System32").join("curl.exe"),
        _ => PathBuf::from("curl"),
    }
}

/// A string as curl's config file quotes it.
fn quote(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// What curl is told for one request. It goes through stdin, so the token
/// never shows on a command line.
fn curl_config(url: &str, accept: &str, token: &str, output: Option<&Path>) -> String {
    let mut c = String::new();
    let mut line = |k: &str, v: &str| c.push_str(&format!("{k} = {}\n", quote(v)));
    line("url", url);
    line("header", &format!("Accept: {accept}"));
    line("header", "X-GitHub-Api-Version: 2022-11-28");
    line("user-agent", "Desktopia-updater");
    if !token.trim().is_empty() {
        // curl keeps this header to api.github.com, not the storage host it redirects to.
        line("header", &format!("Authorization: Bearer {}", token.trim()));
    }
    line("proto", "=https");
    line("proto-redir", "=https");
    line("connect-timeout", "30");
    line("max-time", if output.is_some() { "3600" } else { "60" });
    // Give up on a download that stalls for a minute.
    line("speed-limit", "1024");
    line("speed-time", "60");
    match output {
        Some(path) => {
            line("output", &path.to_string_lossy());
            line("write-out", "%{http_code}");
        }
        None => line("write-out", "\\n%{http_code}"),
    }
    c.push_str("location\nsilent\nshow-error\n");
    if cfg!(windows) {
        // Schannel fails outright when it can't reach a revocation server.
        c.push_str("ssl-revoke-best-effort\n");
    }
    c
}

/// Fetches `url`, into `output` if given. Returns the HTTP status and, without
/// an output file, the body.
fn curl(url: &str, accept: &str, token: &str, output: Option<&Path>) -> Result<(u32, Vec<u8>), String> {
    let mut child = quiet(&curl_exe())
        .args(["--disable", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("can't run curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(curl_config(url, accept, token, output).as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("can't reach GitHub ({})", err.trim().trim_start_matches("curl: ")));
    }
    let mut body = out.stdout;
    let split = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let code = String::from_utf8_lossy(&body[split..]).trim().parse().unwrap_or(0);
    body.truncate(split.saturating_sub(1));
    Ok((code, body))
}

/// Runs a downloaded build with `--version`, which only prints its commit.
fn version_of(exe: &Path) -> Result<String, String> {
    let mut child = quiet(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("the new build won't start: {e}"))?;
    let deadline = Instant::now() + VERSION_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the new build didn't answer".into());
            }
        }
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
    }
    Ok(out.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "440ac66d4c21d0bd2624212e9a6c36e4639be4c1";

    fn release(json: &str) -> Release {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn only_another_commit_is_wanted() {
        let r = release(&format!(
            r#"{{"target_commitish":"{SHA}","name":"Latest build","assets":[
                {{"name":"desktopia.exe","size":39508992,"state":"uploaded",
                  "url":"https://api.github.com/repos/noalear/Desktopia/releases/assets/690788906"}}]}}"#
        ));
        assert!(wanted(&r, SHA).unwrap().is_none());
        assert!(wanted(&r, &SHA.to_uppercase()).unwrap().is_none());
        let a = wanted(&r, "54a3d15000000000000000000000000000000000").unwrap().unwrap();
        assert_eq!(a.size, 39508992);
        assert!(a.url.ends_with("/690788906"));
    }

    #[test]
    fn half_published_releases_are_skipped() {
        let r = release(&format!(
            r#"{{"target_commitish":"{SHA}","assets":[{{"name":"desktopia.exe","size":5,"state":"starter","url":"u"}}]}}"#
        ));
        assert!(wanted(&r, "").is_err());
        let r = release(&format!(r#"{{"target_commitish":"{SHA}","assets":[]}}"#));
        assert!(wanted(&r, "").is_err());
        let r = release(r#"{"target_commitish":"main","assets":[]}"#);
        assert!(wanted(&r, "").is_err());
    }

    #[test]
    fn config_quotes_paths_and_tokens() {
        let c = curl_config(
            "https://x/y",
            "application/octet-stream",
            " tok\"en\n",
            Some(Path::new(r"C:\Users\noale\Downloads\desktopia.exe.download")),
        );
        assert!(c.contains(r#"output = "C:\\Users\\noale\\Downloads\\desktopia.exe.download""#));
        assert!(c.contains(r#"header = "Authorization: Bearer tok\"en""#));
        let c = curl_config("https://x/y", "application/json", "", None);
        assert!(!c.contains("Authorization"));
        assert!(c.contains(r#"write-out = "\\n%{http_code}""#));
    }

    #[test]
    fn swap_keeps_the_old_build_aside() {
        let dir = std::env::temp_dir().join(format!("desktopia-swap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("desktopia.exe");
        let new = beside(&exe, ".download");
        std::fs::write(&exe, "old").unwrap();
        std::fs::write(beside(&exe, ".old"), "older").unwrap();
        std::fs::write(&new, "new").unwrap();
        swap(&exe, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(beside(&exe, ".old")).unwrap(), "old");
        assert!(!new.exists());
        // A failed swap leaves the running build where it was.
        assert!(swap(&exe, &new).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        tidy(&exe);
        assert!(!beside(&exe, ".old").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
