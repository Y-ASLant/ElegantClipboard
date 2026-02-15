//! 管理员模式下的开机自启动（任务计划程序）
//!
//! Windows 会静默跳过注册表 Run 中需要 UAC 提权的程序，
//! 因此管理员模式下改用任务计划程序（HIGHEST 运行级别）。

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;

const TASK_NAME: &str = "ElegantClipboard_AutoStart";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 创建以最高权限运行的自启动计划任务
#[cfg(target_os = "windows")]
pub fn create_autostart_task() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &format!("\"{}\" --hidden", exe.to_string_lossy()),
            "/SC",
            "ONLOGON",
            "/RL",
            "HIGHEST",
            "/F",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "创建计划任务失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn create_autostart_task() -> Result<(), String> {
    Err("仅限 Windows".into())
}

/// 删除自启动计划任务（任务不存在时静默忽略）
#[cfg(target_os = "windows")]
pub fn delete_autostart_task() -> Result<(), String> {
    let _ = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn delete_autostart_task() -> Result<(), String> {
    Ok(())
}

/// 检查自启动计划任务是否存在
#[cfg(target_os = "windows")]
pub fn is_autostart_task_exists() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_autostart_task_exists() -> bool {
    false
}
