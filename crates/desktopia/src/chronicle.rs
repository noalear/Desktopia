//! The chronicle window runs in its own process (`desktopia.exe --chronicle`),
//! so the wallpaper carries none of its weight and closing it frees everything.
//! The wallpaper starts it, or brings it forward if it is already open.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::{CreateMutexW, NORMAL_PRIORITY_CLASS};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, MB_ICONERROR, MB_OK, MessageBoxW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Where the wallpaper keeps the chronicle.
pub fn dir() -> Option<PathBuf> {
    crate::config::dir("LOCALAPPDATA").map(|d| d.join("chronicle"))
}

/// Brings an open chronicle window forward. Returns false if none is open.
fn focus_existing() -> bool {
    let title = HSTRING::from(chronicle_view::TITLE);
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR::null(), &title) }) else { return false };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    true
}

/// From the wallpaper: shows the chronicle, starting its process if needed.
pub fn show() {
    if focus_existing() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    // A window people look at shouldn't inherit the wallpaper's low priority.
    let started = Command::new(exe).arg("--chronicle").creation_flags(NORMAL_PRIORITY_CLASS.0).spawn();
    if let Err(e) = started {
        log::error!("can't start the chronicle: {e}");
    }
}

/// The chronicle process itself. `demo` shows a made-up world instead of the
/// real one.
pub fn window(demo: bool) {
    crate::logger::init("chronicle");
    // One window at a time; a second launch just brings the first forward.
    let _guard = unsafe { CreateMutexW(None, true, w!("Local\\Desktopia.Chronicle")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        for _ in 0..30 {
            if focus_existing() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return;
    }
    let source = match (demo, dir()) {
        (false, Some(d)) => chronicle_view::Source::Folder(d),
        _ => chronicle_view::Source::Demo,
    };
    if let Err(e) = chronicle_view::run(source) {
        log::error!("chronicle window: {e}");
        let text = HSTRING::from(format!("The chronicle couldn't open its window.\n\n{e}"));
        unsafe {
            MessageBoxW(None, &text, w!("Desktopia"), MB_OK | MB_ICONERROR);
        }
    }
}
