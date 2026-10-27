//! Desktopia: Vesper as a living wallpaper behind the Windows desktop icons.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod config;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod governor;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod logger;
#[cfg(windows)]
mod system;
#[cfg(windows)]
mod wallpaper;

#[cfg(not(windows))]
fn main() {
    eprintln!("Desktopia runs on Windows. Use the preview tool to render Vesper elsewhere.");
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
mod app {
    use crate::config::{self, Config};
    use crate::governor::Governor;
    use crate::host::{self, Event, Host, MenuState};
    use crate::wallpaper::{Gpu, Presented, Wallpaper};
    use crate::{logger, system};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use vesper_render::{WorldView, rhythm};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE, SetWaitableTimer, TIMER_ALL_ACCESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
        TranslateMessage, WM_QUIT,
    };
    use windows::core::PCWSTR;

    /// How long to wait after a display change before rebuilding; they come in bursts.
    const REBUILD_SETTLE: Duration = Duration::from_millis(700);
    /// How often to check that Explorer hasn't pulled the windows out from under us.
    const HEALTH_EVERY: Duration = Duration::from_secs(1);
    /// How often to look for HDR being switched on or off.
    const MODES_EVERY: Duration = Duration::from_secs(5);
    /// How long to sleep between checks while nothing is being drawn.
    const IDLE_WAIT: Duration = Duration::from_millis(250);
    /// Where the storm sits for now, in world units; the simulation will move it.
    const STORM_X: f32 = 2.45;

    pub fn run() {
        logger::init();
        let Some(_instance) = system::single_instance() else {
            log::info!("already running");
            return;
        };
        system::be_polite();
        let cfg = config::load();
        log::info!("config: {cfg:?}");

        let host = match Host::new() {
            Ok(h) => h,
            Err(e) => {
                log::error!("can't create the host window: {e}");
                return;
            }
        };
        let timer = unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
        }
        .unwrap_or_default();

        let mut app = App {
            cfg,
            host,
            gpu: None,
            wallpaper: None,
            governor: Governor::new(),
            rebuild_at: Some(Instant::now()),
            failures: 0,
            last_health: Instant::now(),
            last_modes: Instant::now(),
            next_frame: Instant::now(),
            autostart: system::autostart_enabled(),
            last_tip: String::new(),
        };
        app.sync_menu();
        app.run(timer);

        drop(app);
        if !timer.is_invalid() {
            unsafe {
                let _ = CloseHandle(timer);
            }
        }
        log::info!("bye");
        log::logger().flush();
    }

    struct App {
        cfg: Config,
        host: Host,
        // The wallpaper's swap chains must go before the GPU.
        wallpaper: Option<Wallpaper>,
        gpu: Option<Gpu>,
        governor: Governor,
        rebuild_at: Option<Instant>,
        failures: u32,
        last_health: Instant,
        last_modes: Instant,
        next_frame: Instant,
        autostart: bool,
        last_tip: String,
    }

    impl App {
        fn run(&mut self, timer: HANDLE) {
            loop {
                if !pump_messages() {
                    return;
                }
                for e in host::drain_events() {
                    if !self.handle(e) {
                        return;
                    }
                }
                self.maintain();

                let fps = self.frame();
                let wait = match fps {
                    0 => IDLE_WAIT,
                    _ => self.next_frame.saturating_duration_since(Instant::now()),
                };
                sleep_until_message(timer, wait);
            }
        }

        /// Returns false to quit.
        fn handle(&mut self, e: Event) -> bool {
            match e {
                Event::Rebuild => self.schedule_rebuild(),
                Event::ExplorerRestarted => {
                    log::info!("Explorer restarted");
                    self.host.add_tray_icon();
                    self.schedule_rebuild();
                }
                Event::SettingsChanged => {
                    // Taskbar moves only change work areas; anything bigger needs new windows.
                    if let Some(w) = self.wallpaper.as_mut() {
                        let now = crate::desktop::monitors();
                        let same =
                            now.len() == w.monitors.len() && now.iter().zip(&w.monitors).all(|(a, b)| a.rect == b.rect);
                        if same {
                            w.monitors = now;
                            self.governor.poke();
                        } else {
                            self.schedule_rebuild();
                        }
                    }
                }
                Event::Locked(on) => {
                    self.governor.locked = on;
                    self.governor.poke();
                }
                Event::DisplayOn(on) => {
                    self.governor.display_off = !on;
                    self.governor.poke();
                }
                Event::TogglePause => {
                    self.governor.user_paused = !self.governor.user_paused;
                    self.governor.poke();
                    self.sync_menu();
                }
                Event::ToggleAutostart => {
                    system::set_autostart(!self.autostart);
                    self.autostart = system::autostart_enabled();
                    self.sync_menu();
                }
                Event::Quit => return false,
            }
            true
        }

        fn schedule_rebuild(&mut self) {
            self.rebuild_at = Some(Instant::now() + REBUILD_SETTLE);
        }

        fn sync_menu(&self) {
            host::set_menu_state(MenuState { paused: self.governor.user_paused, autostart: self.autostart });
        }

        /// Rebuilds when asked or when the windows have died, and follows HDR changes.
        fn maintain(&mut self) {
            let now = Instant::now();
            if now - self.last_health >= HEALTH_EVERY {
                self.last_health = now;
                if self.wallpaper.as_ref().is_some_and(|w| !w.alive()) && self.rebuild_at.is_none() {
                    log::warn!("wallpaper windows are gone (Explorer restarted?)");
                    self.wallpaper = None;
                    self.host.add_tray_icon();
                    self.schedule_rebuild();
                }
            }
            if self.rebuild_at.is_some_and(|t| now >= t) {
                self.rebuild_at = None;
                self.wallpaper = None;
                match Wallpaper::build(&mut self.gpu, &self.cfg) {
                    Ok(w) => {
                        self.wallpaper = Some(w);
                        self.failures = 0;
                        self.governor.poke();
                    }
                    Err(e) => {
                        // Explorer may still be starting; back off and try again.
                        self.failures += 1;
                        let delay = Duration::from_secs(2u64.pow(self.failures.min(5)));
                        log::error!("can't build the wallpaper ({e}); retrying in {delay:?}");
                        self.rebuild_at = Some(now + delay);
                    }
                }
                self.last_modes = now;
            }
            if now - self.last_modes >= MODES_EVERY {
                self.last_modes = now;
                if let (Some(w), Some(gpu)) = (self.wallpaper.as_mut(), self.gpu.as_ref()) {
                    w.refresh_modes(gpu, &self.cfg);
                }
            }
        }

        /// Draws the monitors that can be seen, if a frame is due. Returns the
        /// frame rate in effect, 0 when nothing should be drawn.
        fn frame(&mut self) -> u32 {
            let (Some(wallpaper), Some(gpu)) = (self.wallpaper.as_mut(), self.gpu.as_ref()) else {
                return 0;
            };
            let decision = self.governor.decide(&self.cfg, &wallpaper.monitors).clone();
            let tip = match decision.fps {
                0 => format!("Desktopia: resting ({})", decision.reason),
                fps => format!("Desktopia: {fps} fps ({})", decision.reason),
            };
            if tip != self.last_tip {
                self.host.set_tooltip(&tip);
                self.last_tip = tip;
            }
            if decision.fps == 0 {
                return 0;
            }

            let now = Instant::now();
            if now < self.next_frame {
                return decision.fps;
            }
            let interval = Duration::from_secs_f64(1.0 / decision.fps as f64);
            self.next_frame += interval;
            if self.next_frame < now {
                self.next_frame = now + interval;
            }

            let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let world = WorldView {
                world_size_px: wallpaper.world_size_px,
                time: rhythm::shader_time(unix),
                band_shift: rhythm::band_shift(unix),
                lightning: rhythm::lightning(unix),
                storm_x: STORM_X,
                exposure: self.cfg.exposure,
            };
            let mut lost = false;
            for i in 0..wallpaper.screens.len() {
                if decision.visible.get(i).copied().unwrap_or(true)
                    && let Presented::Lost = wallpaper.present(gpu, i, &world)
                {
                    lost = true;
                }
            }
            if lost && self.rebuild_at.is_none() {
                log::warn!("swap chain lost; rebuilding");
                self.schedule_rebuild();
            }
            decision.fps
        }
    }

    /// Handles every waiting message. Returns false on WM_QUIT.
    fn pump_messages() -> bool {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return false;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        true
    }

    /// Sleeps for `wait`, or until a message arrives.
    fn sleep_until_message(timer: HANDLE, wait: Duration) {
        if wait.is_zero() {
            return;
        }
        unsafe {
            if !timer.is_invalid() {
                // Negative due time is relative, in 100 ns units.
                let due = -((wait.as_nanos() / 100).max(1) as i64);
                if SetWaitableTimer(timer, &due, 0, None, None, false).is_ok() {
                    MsgWaitForMultipleObjectsEx(Some(&[timer]), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                    return;
                }
            }
            MsgWaitForMultipleObjectsEx(None, wait.as_millis() as u32, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
    }
}
