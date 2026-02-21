//! 管理员模式下的免 UAC 提权（任务计划程序）
//!
//! 计划任务用 `/SC ONCE /RL HIGHEST` 注册，仅作为免 UAC 提权工具：
//! `schtasks /Run` 可在不弹出 UAC 的情况下以管理员权限启动程序。
//! 自启动始终使用 `tauri_plugin_autostart`（注册表 `Run`）。

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;

const TASK_NAME: &str = "ElegantClipboard_AdminElevation";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 创建以最高权限运行的一次性计划任务（用于免 UAC 提权）
#[cfg(target_os = "windows")]
pub fn create_elevation_task() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    // 先删除可能存在的旧任务
    let _ = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &format!("\"{}\"", exe.to_string_lossy()),
            "/SC",
            "ONCE",
            "/ST",
            "00:00",
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
pub fn create_elevation_task() -> Result<(), String> {
    Err("仅限 Windows".into())
}

/// 通过计划任务启动程序（免 UAC 提权）
#[cfg(target_os = "windows")]
pub fn run_elevation_task() -> bool {
    Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn run_elevation_task() -> bool {
    false
}

/// 删除计划任务
/// 成功或任务不存在时返回 Ok，删除失败（如权限不足）时返回 Err
#[cfg(target_os = "windows")]
pub fn delete_elevation_task() -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("cannot find") || stderr.contains("找不到") {
            Ok(())
        } else {
            Err(format!("删除计划任务失败: {}", stderr.trim()))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn delete_elevation_task() -> Result<(), String> {
    Ok(())
}

/// 检查计划任务是否存在
#[cfg(target_os = "windows")]
pub fn is_elevation_task_exists() -> bool {
    Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevation_task_exists() -> bool {
    false
}

/// 校验计划任务中的 exe 路径是否与当前进程路径一致
#[cfg(target_os = "windows")]
pub fn is_elevation_task_path_valid() -> bool {
    let current_exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_lowercase(),
        Err(_) => return false,
    };

    let output = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME, "/FO", "LIST", "/V"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(o) = output {
        if o.status.success() {
            let stdout = String::from_utf8_lossy(&o.stdout).to_lowercase();
            return stdout.contains(&current_exe);
        }
    }
    false
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevation_task_path_valid() -> bool {
    false
}

/// 清理旧版 ONLOGON 自启动计划任务（迁移用）
#[cfg(target_os = "windows")]
pub fn delete_legacy_autostart_task() {
    let _ = Command::new("schtasks")
        .args(["/Delete", "/TN", "ElegantClipboard_AutoStart", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(target_os = "windows"))]
pub fn delete_legacy_autostart_task() {}
