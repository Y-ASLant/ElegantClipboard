//! Windows Registry management for disabling/enabling system Win+V hotkey
//! 
//! This approach completely disables the Windows built-in clipboard history
//! by modifying the registry, which is more reliable than keyboard hooks.

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
const EXPLORER_ADVANCED_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
#[cfg(windows)]
const DISABLED_HOTKEYS_VALUE: &str = "DisabledHotkeys";

/// Disable system Win+V hotkey by adding 'V' to DisabledHotkeys registry
#[cfg(windows)]
pub fn disable_win_v_hotkey(restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (reg_key, _) = hkcu
        .create_subkey(EXPLORER_ADVANCED_PATH)
        .map_err(|e| format!("无法打开注册表项: {}", e))?;

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    // Add 'V' if not already present
    if !current_value.contains('V') {
        let new_value = format!("{}V", current_value);
        reg_key
            .set_value(DISABLED_HOTKEYS_VALUE, &new_value)
            .map_err(|e| format!("无法设置注册表值: {}", e))?;
    }

    if restart_explorer {
        restart_explorer_process()?;
    }

    Ok(())
}

/// Enable system Win+V hotkey by removing 'V' from DisabledHotkeys registry
#[cfg(windows)]
pub fn enable_win_v_hotkey(restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_key = match hkcu.open_subkey_with_flags(EXPLORER_ADVANCED_PATH, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(_) => return Ok(()), // Key doesn't exist, nothing to do
    };

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    // Remove 'V' from the value
    let new_value = current_value.replace('V', "");

    if new_value.is_empty() {
        // If empty, delete the value entirely
        let _ = reg_key.delete_value(DISABLED_HOTKEYS_VALUE);
    } else if new_value != current_value {
        reg_key
            .set_value(DISABLED_HOTKEYS_VALUE, &new_value)
            .map_err(|e| format!("无法更新注册表值: {}", e))?;
    }

    if restart_explorer {
        restart_explorer_process()?;
    }

    Ok(())
}

/// Restart Windows Explorer to apply registry changes
#[cfg(windows)]
fn restart_explorer_process() -> Result<(), String> {
    use std::process::Command;

    // Kill explorer
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output();

    // Wait a moment
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // Start explorer
    if Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .spawn()
        .is_err()
    {
        Command::new("explorer.exe")
            .spawn()
            .map_err(|e| format!("无法启动Explorer进程: {}", e))?;
    }

    // Wait for explorer to start
    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}

/// Check if system Win+V hotkey is disabled
#[cfg(windows)]
pub fn is_win_v_hotkey_disabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_key = match hkcu.open_subkey(EXPLORER_ADVANCED_PATH) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    current_value.contains('V')
}

// Non-Windows stubs
#[cfg(not(windows))]
pub fn disable_win_v_hotkey(_restart_explorer: bool) -> Result<(), String> {
    Err("Win+V registry modification is only available on Windows".to_string())
}

#[cfg(not(windows))]
pub fn enable_win_v_hotkey(_restart_explorer: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn is_win_v_hotkey_disabled() -> bool {
    false
}
