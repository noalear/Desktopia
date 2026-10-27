//! Process-wide setup: one instance only, low priority, DPI awareness, and the
//! Start with Windows switch.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{
    BELOW_NORMAL_PRIORITY_CLASS, CreateMutexW, GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetPriorityClass,
    SetProcessInformation,
};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::core::{HSTRING, PCWSTR, w};

/// Holds the single-instance mutex for the life of the process.
pub struct Instance(#[allow(dead_code)] HANDLE);

/// None if Desktopia is already running.
pub fn single_instance() -> Option<Instance> {
    unsafe {
        let h = CreateMutexW(None, true, w!("Local\\Desktopia.Vesper")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return None;
        }
        Some(Instance(h))
    }
}

/// Physical pixels everywhere, low CPU priority, and efficiency mode so the
/// scheduler favours everything else.
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
