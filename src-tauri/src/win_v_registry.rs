//! 通过注册表禁用/启用系统 Win+V 热键
//!
//! 修改注册表彼底禁用 Windows 内置剪贴板历史，比键盘钉更可靠。

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
const EXPLORER_ADVANCED_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
#[cfg(windows)]
const DISABLED_HOTKEYS_VALUE: &str = "DisabledHotkeys";

/// 将 'V' 加入 DisabledHotkeys 注册表値，禁用系统 Win+V 热键
#[cfg(windows)]
pub fn disable_win_v_hotkey(restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (reg_key, _) = hkcu
        .create_subkey(EXPLORER_ADVANCED_PATH)
        .map_err(|e| format!("无法打开注册表项: {}", e))?;

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    // 若不存在则追加 'V'
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

/// 从 DisabledHotkeys 注册表値移除 'V'，恢复系统 Win+V 热键
#[cfg(windows)]
pub fn enable_win_v_hotkey(restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_key = match hkcu.open_subkey_with_flags(EXPLORER_ADVANCED_PATH, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(_) => return Ok(()), // 注册表项不存在，无需处理
    };

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    // 从値中移除 'V'
    let new_value = current_value.replace('V', "");

    if new_value.is_empty() {
        // 如果为空则整体删除该属性値
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

/// 重启 Explorer 以使注册表修改生效
#[cfg(windows)]
fn restart_explorer_process() -> Result<(), String> {
    use std::process::Command;

    // 关闭 Explorer
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output();

    // 等待片刻
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // 启动 Explorer
    if Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .spawn()
        .is_err()
    {
        Command::new("explorer.exe")
            .spawn()
            .map_err(|e| format!("无法启动Explorer进程: {}", e))?;
    }

    // 等待 Explorer 启动
    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}

/// 检查系统 Win+V 热键是否已被禁用
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

// 非 Windows 平台占位实现
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
