//! 管理员启动配置

use crate::config::AppConfig;
use std::path::PathBuf;

fn get_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

// ─── 偏好设置（配置文件） ─────────────────────────────────────────────────────

/// 检查是否启用了管理员启动（读取配置文件）
pub fn is_admin_launch_enabled() -> bool {
    AppConfig::load().run_as_admin.unwrap_or(false)
}

/// 启用管理员启动
/// 保存偏好，若已提权则同时创建计划任务以便后续免 UAC 提权
pub fn enable_admin_launch() -> Result<(), String> {
    let mut config = AppConfig::load();
    config.run_as_admin = Some(true);
    config.save()?;

    // 已提权时创建/更新计划任务，后续重启可跳过 UAC 弹窗
    #[cfg(target_os = "windows")]
    if is_running_as_admin() {
        let _ = crate::task_scheduler::create_elevation_task();
    }

    Ok(())
}

/// 禁用管理员启动
/// 保存偏好、删除计划任务并清理旧版注册表项
pub fn disable_admin_launch() -> Result<(), String> {
    let mut config = AppConfig::load();
    config.run_as_admin = Some(false);
    config.save()?;

    let _ = crate::task_scheduler::delete_elevation_task();

    // 清理旧版兼容性注册表项
    #[cfg(target_os = "windows")]
    cleanup_compat_flags();

    Ok(())
}

// ─── 权限检查 ─────────────────────────────────────────────────────────────────

/// 检查当前进程是否已提权（管理员）
#[cfg(target_os = "windows")]
pub fn is_running_as_admin() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = Default::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_running_as_admin() -> bool {
    false
}

// ─── 自提权 ───────────────────────────────────────────────────────────────────

/// 等待新的提权实例启动（轮询进程列表，最多等待 `secs` 秒）
/// 防止计划任务 Queued 状态或 UAC 进程初始化崩溃导致应用"消失"
#[cfg(target_os = "windows")]
fn wait_for_new_instance(secs: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{OpenProcess, OpenProcessToken};

    let Ok(exe_path) = get_exe_path() else {
        return false;
    };
    let exe_name = exe_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let my_pid = std::process::id();

    for _ in 0..(secs as u64 * 5) {
        std::thread::sleep(std::time::Duration::from_millis(200));

        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                continue;
            };
            if snapshot == INVALID_HANDLE_VALUE {
                continue;
            }

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            if Process32FirstW(snapshot, &mut entry).is_err() {
                let _ = CloseHandle(snapshot);
                continue;
            }

            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                if name.to_lowercase() == exe_name && entry.th32ProcessID != my_pid {
                    if let Ok(h) = OpenProcess(
                        windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
                        false,
                        entry.th32ProcessID,
                    ) {
                        let mut token = Default::default();
                        if OpenProcessToken(h, TOKEN_QUERY, &mut token).is_ok() {
                            let mut elevation = TOKEN_ELEVATION::default();
                            let mut len = 0u32;
                            let ok = GetTokenInformation(
                                token,
                                TokenElevation,
                                Some(&mut elevation as *mut _ as *mut _),
                                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                                &mut len,
                            );
                            let _ = CloseHandle(token);
                            if ok.is_ok() && elevation.TokenIsElevated != 0 {
                                let _ = CloseHandle(h);
                                let _ = CloseHandle(snapshot);
                                return true;
                            }
                        }
                        let _ = CloseHandle(h);
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
            let _ = CloseHandle(snapshot);
        }
    }
    false
}

/// 尝试启动一个新的提权实例
/// 优先使用计划任务（免 UAC 弹窗），失败则回退到 UAC 提示
/// 返回 `true` 表示新实例已启动（调用方应立即退出以释放单例锁）
///
/// 注意：调用方必须在此函数返回 true 后**立即**退出，否则单例插件会拒绝新实例。
#[cfg(target_os = "windows")]
pub fn self_elevate() -> bool {
    use crate::task_scheduler;

    // 优先尝试计划任务路径（免 UAC）
    if task_scheduler::is_elevation_task_exists()
        && task_scheduler::is_elevation_task_path_valid()
        && task_scheduler::run_elevation_task()
    {
        // 验证提权进程是否真正启动（防止 Queued 状态导致的假成功）
        if wait_for_new_instance(5) {
            return true;
        }
        tracing::warn!("计划任务声称成功但未检测到提权进程，回退到 UAC");
    }

    // 回退到 UAC 弹窗提权
    if elevate_with_uac() {
        // 验证新进程存活（防止初始化崩溃导致两个实例都退出）
        if wait_for_new_instance(3) {
            return true;
        }
        tracing::warn!("UAC 提权声称成功但未检测到提权进程");
    }
    false
}

#[cfg(not(target_os = "windows"))]
pub fn self_elevate() -> bool {
    false
}

/// 通过 ShellExecute "runas" 启动新实例（会弹出 UAC 提示）
#[cfg(target_os = "windows")]
fn elevate_with_uac() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    let exe_path = match get_exe_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let op: Vec<u16> = OsStr::new("runas").encode_wide().chain(Some(0)).collect();
    let file: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
        .0 as usize
            > 32
    }
}

// ─── 重启 ─────────────────────────────────────────────────────────────────────

/// 重启应用
/// 管理员模式：通过计划任务/UAC 提权启动新实例
/// 已提权但未启用管理员模式：通过 explorer.exe 降权启动
/// 普通用户：返回 false，由调用方使用 app.restart()
#[cfg(target_os = "windows")]
pub fn restart_app() -> bool {
    if is_admin_launch_enabled() {
        return self_elevate();
    }

    // 仅在当前已提权时才需要通过 explorer.exe 降权启动，
    // 未提权时直接返回 false，让调用方使用 app.restart()
    if is_running_as_admin() {
        return launch_via_explorer();
    }

    false
}

#[cfg(not(target_os = "windows"))]
pub fn restart_app() -> bool {
    false
}

/// 通过 explorer.exe 启动，确保新进程不继承管理员权限
#[cfg(target_os = "windows")]
fn launch_via_explorer() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    let exe_path = match get_exe_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let explorer: Vec<u16> = OsStr::new("explorer.exe")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let file: Vec<u16> = exe_path.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        ShellExecuteW(
            None,
            PCWSTR::null(),
            PCWSTR(explorer.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
        .0 as usize
            > 32
    }
}

// ─── 旧版兼容清理 ─────────────────────────────────────────────────────────────

/// 清理旧版 `AppCompatFlags\Layers` 中的 `RUNASADMIN` 注册表项
/// 不存在时安全跳过
#[cfg(target_os = "windows")]
pub fn cleanup_compat_flags() {
    use winreg::RegKey;
    use winreg::enums::*;

    const COMPAT_LAYERS_PATH: &str =
        r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

    let exe_path = match get_exe_path() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return,
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(COMPAT_LAYERS_PATH, KEY_ALL_ACCESS) {
        let _ = key.delete_value(&exe_path);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn cleanup_compat_flags() {}
