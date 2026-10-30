//! Decides how fast to draw, and which monitors to draw at all, so Desktopia
//! stays out of the way of whatever the PC is actually doing.
//!
//! Everything stops for an exclusive-fullscreen game, presentation mode, a
//! locked session or a sleeping display. A monitor covered by a maximized or
//! fullscreen window is skipped. The frame cap drops on battery and when the
//! CPU is busy with other work.
//!
//! It also says when a world set to run faster than real time must slow back
//! down: for a fullscreen game or app, a presentation, battery power, or
//! other programs using the CPU, and for half a minute after.

use crate::config::Config;
use crate::desktop::Monitor;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{FILETIME, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetClassNameW, GetWindowLongW, IsIconic, IsWindowVisible, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

const CHECK_EVERY: Duration = Duration::from_millis(500);
const CPU_EVERY: Duration = Duration::from_secs(2);
/// How long the PC must have been quiet before a faster world speeds up again.
const HOLD_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    /// Frames per second; 0 means draw nothing.
    pub fps: u32,
    /// Per monitor, whether any of it can be seen.
    pub visible: Vec<bool>,
    /// Why, for the log and the tray tooltip.
    pub reason: &'static str,
}

pub struct Governor {
    pub user_paused: bool,
    pub locked: bool,
    pub display_off: bool,
    last_check: Option<Instant>,
    decision: Decision,
    cpu: CpuMeter,
    /// Why the world must run in real time, and when that was last seen.
    hold: Option<(&'static str, Instant)>,
    last_hold_check: Option<Instant>,
}

impl Governor {
    pub fn new() -> Self {
        Self {
            user_paused: false,
            locked: false,
            display_off: false,
            last_check: None,
            decision: Decision { fps: 0, visible: Vec::new(), reason: "starting" },
            cpu: CpuMeter::new(),
            hold: None,
            last_hold_check: None,
        }
    }

    /// Forces a fresh look at the system on the next call.
    pub fn poke(&mut self) {
        self.last_check = None;
    }

    pub fn decide(&mut self, cfg: &Config, monitors: &[Monitor]) -> &Decision {
        let now = Instant::now();
        let due =
            self.last_check.is_none_or(|t| now - t >= CHECK_EVERY) || self.decision.visible.len() != monitors.len();
        if due {
            self.last_check = Some(now);
            let d = self.evaluate(cfg, monitors);
            if d.fps != self.decision.fps || d.visible != self.decision.visible {
                log::info!("governor: {} fps, visible {:?} ({})", d.fps, d.visible, d.reason);
            }
            self.decision = d;
        }
        &self.decision
    }

    /// Why the world may not run faster than real time now, if it may not.
    pub fn speed_hold(&mut self, cfg: &Config) -> Option<&'static str> {
        let now = Instant::now();
        if self.last_hold_check.is_none_or(|t| now - t >= CHECK_EVERY) {
            self.last_hold_check = Some(now);
            if let Some(reason) = self.busy(cfg) {
                if self.hold.is_none_or(|(r, _)| r != reason) {
                    log::info!("governor: world held at real time ({reason})");
                }
                self.hold = Some((reason, now));
            } else if self.hold.is_some_and(|(_, seen)| now - seen >= HOLD_AFTER) {
                log::info!("governor: the PC is quiet; the world may run fast again");
                self.hold = None;
            }
        }
        self.hold.map(|(r, _)| r)
    }

    /// What the PC is busy with that a fast world would get in the way of.
    fn busy(&mut self, cfg: &Config) -> Option<&'static str> {
        if let Ok(state) = unsafe { SHQueryUserNotificationState() } {
            match state {
                QUNS_RUNNING_D3D_FULL_SCREEN => return Some("fullscreen game"),
                QUNS_PRESENTATION_MODE => return Some("presentation mode"),
                QUNS_BUSY => return Some("fullscreen app"),
                _ => {}
            }
        }
        if on_battery() {
            return Some("on battery");
        }
        if self.cpu.sample().is_some_and(|load| load > cfg.fast_cpu_percent) {
            return Some("PC busy");
        }
        None
    }

    fn evaluate(&mut self, cfg: &Config, monitors: &[Monitor]) -> Decision {
        let none = vec![false; monitors.len()];
        let stop = |reason| Decision { fps: 0, visible: none.clone(), reason };
        if self.user_paused {
            return stop("paused from the tray");
        }
        if self.locked {
            return stop("session locked");
        }
        if self.display_off {
            return stop("display off");
        }
        if let Ok(state) = unsafe { SHQueryUserNotificationState() } {
            if state == QUNS_RUNNING_D3D_FULL_SCREEN {
                return stop("fullscreen game");
            }
            if state == QUNS_PRESENTATION_MODE {
                return stop("presentation mode");
            }
        }
        let visible = uncovered(monitors);
        if !visible.iter().any(|v| *v) {
            return Decision { fps: 0, visible, reason: "desktop covered" };
        }

        let mut fps = cfg.fps.max(1);
        let mut reason = "desktop in view";
        if on_battery() {
            fps = fps.min(cfg.battery_fps.max(1));
            reason = "on battery";
        }
        if let Some(load) = self.cpu.sample()
            && load > cfg.busy_cpu_percent
        {
            fps = fps.min(cfg.busy_fps.max(1));
            reason = "CPU busy";
        }
        Decision { fps, visible, reason }
    }
}

fn on_battery() -> bool {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return false;
    }
    // ACLineStatus 0 is offline; SystemStatusFlag 1 is battery saver.
    s.ACLineStatus == 0 || s.SystemStatusFlag == 1
}

/// CPU use by other programs across all cores, from the change in system
/// times less Desktopia's own, so a fast world doesn't count as the PC being busy.
struct CpuMeter {
    last: Option<(Instant, u64, u64)>,
    load: Option<f32>,
}

impl CpuMeter {
    fn new() -> Self {
        Self { last: None, load: None }
    }

    fn sample(&mut self) -> Option<f32> {
        let now = Instant::now();
        if self.last.is_some_and(|(t, _, _)| now - t < CPU_EVERY) {
            return self.load;
        }
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        let (mut created, mut exited, mut own_kernel, mut own_user) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let own =
            unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut own_kernel, &mut own_user) }
                .map_or(0, |()| ft(own_kernel) + ft(own_user));
        // Kernel time includes idle time.
        let total = ft(kernel) + ft(user);
        let busy = (total - ft(idle)).saturating_sub(own);
        if let Some((_, b0, t0)) = self.last {
            let dt = total.saturating_sub(t0);
            if dt > 0 {
                self.load = Some(100.0 * busy.saturating_sub(b0) as f32 / dt as f32);
            }
        }
        self.last = Some((now, busy, total));
        self.load
    }
}

/// For each monitor, whether some of it is not covered by a single window.
fn uncovered(monitors: &[Monitor]) -> Vec<bool> {
    struct Scan<'a> {
        monitors: &'a [Monitor],
        covered: Vec<bool>,
    }
    unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
        let scan = unsafe { &mut *(data.0 as *mut Scan) };
        if let Some(r) = covering_rect(hwnd) {
            for (i, m) in scan.monitors.iter().enumerate() {
                // A maximized window stops at the taskbar, so compare with the work area.
                if contains(&r, &m.work) {
                    scan.covered[i] = true;
                }
            }
        }
        true.into()
    }
    let mut scan = Scan { monitors, covered: vec![false; monitors.len()] };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut scan as *mut _ as isize));
    }
    scan.covered.iter().map(|c| !c).collect()
}

/// The on-screen bounds of a window that could hide the desktop, or None for
/// hidden, minimized, cloaked, see-through and shell windows.
fn covering_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        // Overlays (GPU driver, game bars, widgets) are layered or click-through.
        let see_through = WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0;
        if ex & see_through != 0 {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return None;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf) as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return None;
        }
        let mut r = RECT::default();
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
        Some(r)
    }
}

/// Whether `outer` covers `inner`, give or take a couple of pixels.
fn contains(outer: &RECT, inner: &RECT) -> bool {
    const SLACK: i32 = 2;
    outer.left <= inner.left + SLACK
        && outer.top <= inner.top + SLACK
        && outer.right >= inner.right - SLACK
        && outer.bottom >= inner.bottom - SLACK
}
