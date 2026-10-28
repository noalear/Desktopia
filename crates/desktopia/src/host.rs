//! The hidden window that receives system notifications, and the tray icon.

use std::cell::RefCell;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
use windows::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, GetCursorPos, HICON, ICONINFO, IDYES, MB_DEFBUTTON2, MB_ICONWARNING,
    MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MF_CHECKED, MF_SEPARATOR, MF_STRING, MessageBoxW, PBT_POWERSETTINGCHANGE,
    PostMessageW, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CONTEXTMENU,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_LBUTTONUP, WM_NULL, WM_POWERBROADCAST, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use windows::core::{HSTRING, PCWSTR, w};

const WM_TRAY: u32 = WM_APP + 1;
/// Sent by a newer copy that is starting up.
pub const WM_QUIT_REQUEST: u32 = WM_APP + 2;
const CMD_PAUSE: u32 = 1;
const CMD_AUTOSTART: u32 = 2;
const CMD_QUIT: u32 = 3;
const CMD_CHRONICLE: u32 = 4;
const CMD_NEW_WORLD: u32 = 5;
const HOTKEY_CHRONICLE: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Monitors or DPI changed: rebuild the wallpaper windows.
    Rebuild,
    /// Explorer restarted: the tray icon and the desktop windows are gone.
    ExplorerRestarted,
    /// A system setting changed, maybe the taskbar's size or place.
    SettingsChanged,
    Locked(bool),
    DisplayOn(bool),
    TogglePause,
    ToggleAutostart,
    /// The tray icon, its menu or the hotkey asked for the chronicle.
    OpenChronicle,
    /// Asked from the tray to give up this world and start another.
    NewWorld,
    Quit,
}

/// What the tray menu shows; kept up to date by the main loop.
#[derive(Clone, Default)]
pub struct MenuState {
    pub paused: bool,
    pub autostart: bool,
    /// The chronicle hotkey as typed in the settings, if it was registered.
    pub hotkey: Option<String>,
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static MENU: RefCell<MenuState> = RefCell::new(MenuState::default());
    static TASKBAR_CREATED: RefCell<u32> = const { RefCell::new(0) };
}

fn push(e: Event) {
    EVENTS.with(|q| q.borrow_mut().push(e));
}

pub fn drain_events() -> Vec<Event> {
    EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

pub fn set_menu_state(s: MenuState) {
    MENU.with(|m| *m.borrow_mut() = s);
}

pub struct Host {
    hwnd: HWND,
    icon: HICON,
}

impl Host {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: w!("DesktopiaHost"),
                ..Default::default()
            };
            RegisterClassW(&wc);
            TASKBAR_CREATED.with(|t| *t.borrow_mut() = RegisterWindowMessageW(w!("TaskbarCreated")));
            // A top-level hidden window rather than a message-only one: only
            // top-level windows get broadcasts like TaskbarCreated and
            // WM_DISPLAYCHANGE.
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("DesktopiaHost"),
                w!("Desktopia"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;
            if let Err(e) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
                log::warn!("no lock notifications: {e}");
            }
            if let Err(e) = RegisterPowerSettingNotification(
                HANDLE(hwnd.0),
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            ) {
                log::warn!("no display power notifications: {e}");
            }
            let host = Self { hwnd, icon: make_icon() };
            host.add_tray_icon();
            Ok(host)
        }
    }

    fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        copy_wide(&mut nid.szTip, "Desktopia");
        nid
    }

    /// Adds the tray icon, or adds it back after Explorer restarts.
    pub fn add_tray_icon(&self) {
        unsafe {
            let nid = self.tray_data();
            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
            }
        }
    }

    /// Registers the keys that open the chronicle. Returns false if they are
    /// malformed or another program already has them.
    pub fn register_hotkey(&self, spec: &str) -> bool {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_CHRONICLE);
        }
        let Some((mods, vk)) = crate::config::parse_hotkey(spec) else {
            if !spec.trim().is_empty() {
                log::warn!("can't read the chronicle hotkey {spec:?}");
            }
            return false;
        };
        let mods = HOT_KEY_MODIFIERS(mods) | MOD_NOREPEAT;
        match unsafe { RegisterHotKey(Some(self.hwnd), HOTKEY_CHRONICLE, mods, vk) } {
            Ok(()) => true,
            Err(e) => {
                log::warn!("chronicle hotkey {spec} is taken: {e}");
                false
            }
        }
    }

    pub fn set_tooltip(&self, text: &str) {
        let mut nid = self.tray_data();
        nid.uFlags = NIF_TIP;
        copy_wide(&mut nid.szTip, text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            let nid = self.tray_data();
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_CHRONICLE);
            let _ = DestroyIcon(self.icon);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(n).chain(std::iter::repeat(0))) {
        *d = c;
    }
    dst[n] = 0;
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let taskbar_created = TASKBAR_CREATED.with(|t| *t.borrow());
    match msg {
        WM_QUIT_REQUEST => {
            push(Event::Quit);
            LRESULT(0)
        }
        WM_TRAY => {
            let mouse = (lp.0 & 0xffff) as u32;
            if mouse == WM_RBUTTONUP || mouse == WM_CONTEXTMENU {
                unsafe { show_menu(hwnd) };
            } else if mouse == WM_LBUTTONUP {
                push(Event::OpenChronicle);
            }
            LRESULT(0)
        }
        WM_HOTKEY if wp.0 as i32 == HOTKEY_CHRONICLE => {
            push(Event::OpenChronicle);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            push(Event::Rebuild);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            push(Event::SettingsChanged);
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            match wp.0 as u32 {
                WTS_SESSION_LOCK => push(Event::Locked(true)),
                WTS_SESSION_UNLOCK => push(Event::Locked(false)),
                _ => {}
            }
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if wp.0 as u32 == PBT_POWERSETTINGCHANGE && lp.0 != 0 {
                let setting = unsafe { &*(lp.0 as *const POWERBROADCAST_SETTING) };
                if setting.PowerSetting == GUID_CONSOLE_DISPLAY_STATE {
                    // 0 off, 1 on, 2 dimmed.
                    push(Event::DisplayOn(setting.Data[0] != 0));
                }
            }
            LRESULT(1)
        }
        m if m == taskbar_created && m != 0 => {
            push(Event::ExplorerRestarted);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let state = MENU.with(|m| m.borrow().clone());
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let check = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
        let open = match &state.hotkey {
            Some(keys) => HSTRING::from(format!("Open the chronicle\t{keys}")),
            None => HSTRING::from("Open the chronicle"),
        };
        let _ = AppendMenuW(menu, MF_STRING, CMD_CHRONICLE as usize, &open);
        let _ = SetMenuDefaultItem(menu, CMD_CHRONICLE, 0);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, check(state.paused), CMD_PAUSE as usize, w!("Pause"));
        let _ = AppendMenuW(menu, check(state.autostart), CMD_AUTOSTART as usize, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, CMD_NEW_WORLD as usize, w!("New world\u{2026}"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT as usize, w!("Quit Desktopia"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Without this the menu won't close when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match cmd.0 as u32 {
            CMD_PAUSE => push(Event::TogglePause),
            CMD_AUTOSTART => push(Event::ToggleAutostart),
            CMD_QUIT => push(Event::Quit),
            CMD_CHRONICLE => push(Event::OpenChronicle),
            CMD_NEW_WORLD => push(Event::NewWorld),
            _ => {}
        }
    }
}

/// Asks before the world is given up; nothing changes unless the answer is yes.
pub fn confirm_new_world() -> bool {
    let text =
        w!("Start a new world?\n\nEvery creature, plant and species alive on Vesper now will be gone, and life \
         begins again from its first founders. The old world's save and chronicle are kept in the Desktopia folder \
         (world-past and chronicle-past) but won't be shown again.");
    let flags = MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_SETFOREGROUND | MB_TOPMOST;
    unsafe { MessageBoxW(None, text, w!("Desktopia"), flags) == IDYES }
}

/// The tray icon: Vesper's day, twilight and night across a small disc.
fn make_icon() -> HICON {
    const S: i32 = 32;
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: S,
                biHeight: -S,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let Ok(color) = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            return HICON::default();
        };
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (S * S) as usize);
        for y in 0..S {
            for x in 0..S {
                let (fx, fy) = ((x as f32 + 0.5) / S as f32 * 2.0 - 1.0, (y as f32 + 0.5) / S as f32 * 2.0 - 1.0);
                let r = (fx * fx + fy * fy).sqrt();
                let alpha = ((1.0 - r) * S as f32 * 0.5).clamp(0.0, 1.0);
                let t = (fx + 1.0) * 0.5;
                let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
                let (cr, cg, cb) = if t < 0.5 {
                    let k = t * 2.0;
                    (lerp(255.0, 200.0, k), lerp(170.0, 40.0, k), lerp(40.0, 150.0, k))
                } else {
                    let k = (t - 0.5) * 2.0;
                    (lerp(200.0, 30.0, k), lerp(40.0, 110.0, k), lerp(150.0, 230.0, k))
                };
                // Premultiplied BGRA.
                let a = alpha * 255.0;
                px[(y * S + x) as usize] = ((a as u32) << 24)
                    | (((cr * alpha) as u32) << 16)
                    | (((cg * alpha) as u32) << 8)
                    | ((cb * alpha) as u32);
            }
        }
        let mask = CreateBitmap(S, S, 1, 1, None);
        let info = ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}
