//! Process-wide setup: one instance only, low priority, DPI awareness, and the
//! Start with Windows switch.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    WPARAM,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{
    BELOW_NORMAL_PRIORITY_CLASS, CreateMutexW, GetCurrentProcess, GetCurrentThread, OpenProcess,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_TERMINATE, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, SetThreadPriority,
    THREAD_PRIORITY_LOWEST, TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId, PostMessageW};
use windows::core::{HSTRING, PCWSTR, w};

/// Holds the single-instance mutex for the life of the process.
pub struct Instance(#[allow(dead_code)] HANDLE);

/// Makes this the only running copy. A copy that is already running is asked
/// to quit, and stopped if it doesn't, so starting a new build replaces the old.
pub fn single_instance() -> Option<Instance> {
    unsafe {
        let h = CreateMutexW(None, true, w!("Local\\Desktopia.Vesper")).ok()?;
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return Some(Instance(h));
        }
        log::info!("another copy is running; replacing it");
        if let Ok(old) = FindWindowW(w!("DesktopiaHost"), PCWSTR::null()) {
            let _ = PostMessageW(Some(old), crate::host::WM_QUIT_REQUEST, WPARAM(0), LPARAM(0));
            if WaitForSingleObject(h, 5000) == WAIT_TIMEOUT {
                // Builds from before WM_QUIT_REQUEST don't know the message.
                let mut pid = 0u32;
                GetWindowThreadProcessId(old, Some(&mut pid));
                if pid != 0
                    && let Ok(p) = OpenProcess(PROCESS_TERMINATE, false, pid)
                {
                    let _ = TerminateProcess(p, 0);
                    let _ = CloseHandle(p);
                }
            } else {
                return Some(Instance(h));
            }
        }
        // An abandoned mutex counts as acquired.
        match WaitForSingleObject(h, 5000) {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Some(Instance(h)),
            _ => None,
        }
    }
}

/// Physical pixels everywhere, low CPU priority, and efficiency mode so the
/// scheduler favours everything else.
/// For the simulation thread: it should only ever use time nothing else wants.
pub fn lower_thread_priority() {
    unsafe {
        if let Err(e) = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) {
            log::warn!("sim thread priority: {e}");
        }
    }
}

pub fn be_polite() {
    unsafe {
        if let Err(e) = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) {
            log::warn!("DPI awareness: {e}");
        }
        let process = GetCurrentProcess();
        if let Err(e) = SetPriorityClass(process, BELOW_NORMAL_PRIORITY_CLASS) {
            log::warn!("priority: {e}");
        }
        let eco = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };
        if let Err(e) = SetProcessInformation(
            process,
            ProcessPowerThrottling,
            &eco as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        ) {
            log::warn!("efficiency mode: {e}");
        }
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Desktopia");

fn run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, access, &mut key) }.ok().ok()?;
    Some(key)
}

pub fn autostart_enabled() -> bool {
    let Some(key) = run_key(KEY_QUERY_VALUE) else { return false };
    let found = unsafe { RegQueryValueExW(key, RUN_VALUE, None, None, None, None) }.is_ok();
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

pub fn set_autostart(on: bool) {
    let Some(key) = run_key(KEY_SET_VALUE) else {
        log::warn!("can't open the Run key");
        return;
    };
    unsafe {
        let result = if on {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
            let cmd = HSTRING::from(format!("\"{exe}\""));
            let bytes = std::slice::from_raw_parts(cmd.as_ptr() as *const u8, (cmd.len() + 1) * 2);
            RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, RUN_VALUE)
        };
        if result.is_err() {
            log::warn!("autostart {on}: {result:?}");
        }
        let _ = RegCloseKey(key);
    }
}
