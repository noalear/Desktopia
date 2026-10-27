//! Finding the layer behind the desktop icons, and one window per monitor in it.
//!
//! Sending Progman the undocumented message 0x052C makes Explorer split the
//! desktop into an icon layer (SHELLDLL_DefView) and a wallpaper layer
//! (WorkerW) behind it. Windows placed in the WorkerW draw under the icons.
//! Up to Windows 11 23H2 the WorkerW is a top-level window right after the
//! one that hosts the icons; from 24H2 it is a child of Progman.

use std::sync::Once;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MapWindowPoints, RDW_ALLCHILDREN, RDW_ERASE,
    RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, EnumWindows, FindWindowExW, FindWindowW,
    IsWindow, RegisterClassW, SMTO_NORMAL, SendMessageTimeoutW, WINDOW_EX_STYLE, WM_ERASEBKGND, WM_NCHITTEST,
    WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_DISABLED, WS_VISIBLE,
};
use windows::core::{BOOL, PCWSTR, w};

const CLASS: PCWSTR = w!("DesktopiaWallpaper");
const HTTRANSPARENT: isize = -1;

/// A monitor, in physical screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub rect: RECT,
    /// The part not taken by the taskbar.
    pub work: RECT,
}

impl Monitor {
    pub fn width(&self) -> u32 {
        (self.rect.right - self.rect.left).max(1) as u32
    }
    pub fn height(&self) -> u32 {
        (self.rect.bottom - self.rect.top).max(1) as u32
    }
}

/// All monitors, left to right.
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(hm: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(hm, &mut info.monitorInfo) }.as_bool() {
            list.push(Monitor { rect: info.monitorInfo.rcMonitor, work: info.monitorInfo.rcWork });
        }
        true.into()
    }
    let mut list: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|m| (m.rect.left, m.rect.top));
    list
}

/// The bounding box of all monitors.
pub fn span(monitors: &[Monitor]) -> RECT {
    let mut r = monitors.first().map(|m| m.rect).unwrap_or_default();
    for m in monitors {
        r.left = r.left.min(m.rect.left);
        r.top = r.top.min(m.rect.top);
        r.right = r.right.max(m.rect.right);
        r.bottom = r.bottom.max(m.rect.bottom);
    }
    r
}

/// The window to draw the wallpaper into, behind the icons.
pub fn wallpaper_layer() -> Option<HWND> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).ok()?;
        // Ask Explorer to create the WorkerW layer. Both forms are in use in the
        // wild; neither does harm when the layer already exists.
        for (wp, lp) in [(0xD, 0x1), (0, 0)] {
            SendMessageTimeoutW(progman, 0x052C, WPARAM(wp), LPARAM(lp), SMTO_NORMAL, 1000, None);
        }

        // Windows 11 24H2 and later: icons and WorkerW are both children of Progman.
        let icons_in_progman = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), PCWSTR::null()).is_ok();
        if icons_in_progman && let Ok(worker) = FindWindowExW(Some(progman), None, w!("WorkerW"), PCWSTR::null()) {
            log::info!("desktop layout: WorkerW inside Progman (24H2 or later)");
            return Some(worker);
        }

        // Older layout: the WorkerW that follows the top-level window hosting the icons.
        unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> BOOL {
            let found = unsafe { &mut *(data.0 as *mut Option<HWND>) };
            let hosts_icons =
                unsafe { FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null()) }.is_ok();
            if hosts_icons && let Ok(worker) = unsafe { FindWindowExW(None, Some(hwnd), w!("WorkerW"), PCWSTR::null()) }
            {
                *found = Some(worker);
                return false.into();
            }
            true.into()
        }
        let mut found: Option<HWND> = None;
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
        if let Some(worker) = found {
            log::info!("desktop layout: top-level WorkerW behind the icons");
            return Some(worker);
        }

        log::warn!("no WorkerW found; drawing into Progman");
        Some(progman)
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // The swap chain covers every pixel; skipping the erase avoids a flash.
        WM_ERASEBKGND => LRESULT(1),
        // Clicks fall through to the desktop.
        WM_NCHITTEST => LRESULT(HTTRANSPARENT),
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn register_class() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}

/// A wallpaper window covering one monitor, inside the wallpaper layer.
pub struct WallpaperWindow {
    pub hwnd: HWND,
    pub monitor: Monitor,
}

impl WallpaperWindow {
    pub fn create(layer: HWND, monitor: Monitor) -> windows::core::Result<Self> {
        register_class();
        // The layer's client area may not start at the screen origin.
        let mut corners = [
            POINT { x: monitor.rect.left, y: monitor.rect.top },
            POINT { x: monitor.rect.right, y: monitor.rect.bottom },
        ];
        unsafe {
            MapWindowPoints(None, Some(layer), &mut corners);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS,
                w!("Desktopia"),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_DISABLED,
                corners[0].x,
                corners[0].y,
                corners[1].x - corners[0].x,
                corners[1].y - corners[0].y,
                Some(layer),
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?
        };
        Ok(Self { hwnd, monitor })
    }

    /// False once Explorer has torn down the layer (and with it this window).
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd)) }.as_bool()
    }
}

impl Drop for WallpaperWindow {
    fn drop(&mut self) {
        if self.alive() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

/// Makes Explorer repaint its own wallpaper where ours was.
pub fn repaint_layer(layer: HWND) {
    unsafe {
        let _ = RedrawWindow(
            Some(layer),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
}
