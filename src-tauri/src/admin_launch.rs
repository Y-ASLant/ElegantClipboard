//! Administrator launch configuration for Windows
//! Registry path: HKEY_CURRENT_USER\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers

use std::path::PathBuf;

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

#[cfg(target_os = "windows")]
const COMPAT_LAYERS_PATH: &str = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";
#[cfg(target_os = "windows")]
const RUNASADMIN_VALUE: &str = "~ RUNASADMIN";

fn get_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Check if admin launch is enabled in registry
#[cfg(target_os = "windows")]
pub fn is_admin_launch_enabled() -> bool {
    let exe_path = match get_exe_path() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return false,
    };
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(COMPAT_LAYERS_PATH)
        .and_then(|key| key.get_value::<String, _>(&exe_path))
        .map(|v| v.contains("RUNASADMIN"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_admin_launch_enabled() -> bool { false }

/// Enable admin launch via registry
#[cfg(target_os = "windows")]
pub fn enable_admin_launch() -> Result<(), String> {
    let exe_path = get_exe_path()?.to_string_lossy().to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(COMPAT_LAYERS_PATH).map_err(|e| e.to_string())?;
    key.set_value(&exe_path, &RUNASADMIN_VALUE).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn enable_admin_launch() -> Result<(), String> { Err("Windows only".into()) }

/// Disable admin launch via registry
#[cfg(target_os = "windows")]
pub fn disable_admin_launch() -> Result<(), String> {
    let exe_path = get_exe_path()?.to_string_lossy().to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(COMPAT_LAYERS_PATH, KEY_ALL_ACCESS) {
        let _ = key.delete_value(&exe_path); // Ignore if not exists
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn disable_admin_launch() -> Result<(), String> { Err("Windows only".into()) }

/// Check if current process is elevated
#[cfg(target_os = "windows")]
pub fn is_running_as_admin() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = Default::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let result = GetTokenInformation(
            token, TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_running_as_admin() -> bool { false }

/// Restart app - uses explorer.exe for non-elevated restart
#[cfg(target_os = "windows")]
pub fn restart_app() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    let exe_path = match get_exe_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    if is_admin_launch_enabled() {
        // Direct launch - Windows will elevate based on registry
        let op: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
        let file: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            ShellExecuteW(None, PCWSTR(op.as_ptr()), PCWSTR(file.as_ptr()),
                PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL).0 as usize > 32
        }
    } else {
        // Use explorer.exe to ensure non-elevated restart
        let explorer: Vec<u16> = OsStr::new("explorer.exe").encode_wide().chain(Some(0)).collect();
        let file: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            ShellExecuteW(None, PCWSTR::null(), PCWSTR(explorer.as_ptr()),
                PCWSTR(file.as_ptr()), PCWSTR::null(), SW_SHOWNORMAL).0 as usize > 32
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn restart_app() -> bool { false }

