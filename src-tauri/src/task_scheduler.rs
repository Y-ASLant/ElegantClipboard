//! 管理员模式下的免 UAC 提权（任务计划程序 COM API）
//!
//! 通过 COM API 注册无触发器的计划任务（`RunLevel = HighestAvailable`），
//! 仅作为免 UAC 提权工具：`Run()` 可在不弹出 UAC 的情况下以管理员权限启动程序。
//! 自启动始终使用 `tauri_plugin_autostart`（注册表 `Run`）。

const TASK_NAME: &str = "ElegantClipboard_AdminElevation";

#[cfg(target_os = "windows")]
use windows::core::BSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::VARIANT_BOOL;
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::TaskScheduler::*;
#[cfg(target_os = "windows")]
use windows::Win32::System::Variant::VARIANT;
#[cfg(target_os = "windows")]
use windows_core::Interface;

/// 连接本地任务计划服务并获取根文件夹
#[cfg(target_os = "windows")]
fn get_task_root() -> Result<(ITaskService, ITaskFolder), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let service: ITaskService =
            CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("TaskService 创建失败: {e}"))?;

        service
            .Connect(
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
            )
            .map_err(|e| format!("TaskService 连接失败: {e}"))?;

        let root = service
            .GetFolder(&BSTR::from("\\"))
            .map_err(|e| format!("获取根文件夹失败: {e}"))?;

        Ok((service, root))
    }
}

/// 创建以最高权限运行的计划任务（用于免 UAC 提权）
#[cfg(target_os = "windows")]
pub fn create_elevation_task() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    unsafe {
        let (service, root) = get_task_root()?;

        // 删除已有任务
        let _ = root.DeleteTask(&BSTR::from(TASK_NAME), 0);

        let task = service
            .NewTask(0)
            .map_err(|e| format!("创建任务定义失败: {e}"))?;

        // 设置
        let settings = task.Settings().map_err(|e| e.to_string())?;
        settings.SetMultipleInstances(TASK_INSTANCES_PARALLEL).map_err(|e| e.to_string())?;
        settings.SetDisallowStartIfOnBatteries(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;
        settings.SetStopIfGoingOnBatteries(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;
        settings.SetAllowDemandStart(VARIANT_BOOL(-1)).map_err(|e| e.to_string())?;
        settings.SetEnabled(VARIANT_BOOL(-1)).map_err(|e| e.to_string())?;
        settings.SetStartWhenAvailable(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;
        settings.SetRunOnlyIfNetworkAvailable(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;

        let idle = settings.IdleSettings().map_err(|e| e.to_string())?;
        idle.SetStopOnIdleEnd(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;
        idle.SetRestartOnIdle(VARIANT_BOOL(0)).map_err(|e| e.to_string())?;

        // 主体
        let principal = task.Principal().map_err(|e| e.to_string())?;
        principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN).map_err(|e| e.to_string())?;
        principal.SetRunLevel(TASK_RUNLEVEL_HIGHEST).map_err(|e| e.to_string())?;

        // 操作
        let actions = task.Actions().map_err(|e| e.to_string())?;
        let action: IAction = actions.Create(TASK_ACTION_EXEC).map_err(|e| e.to_string())?;
        let exec_action: IExecAction = action.cast::<IExecAction>().map_err(|e| e.to_string())?;
        exec_action
            .SetPath(&BSTR::from(exe.to_string_lossy().as_ref()))
            .map_err(|e| e.to_string())?;

        // 描述
        let info = task.RegistrationInfo().map_err(|e| e.to_string())?;
        info.SetDescription(&BSTR::from("ElegantClipboard Admin Elevation Helper"))
            .map_err(|e| e.to_string())?;

        // 注册
        root.RegisterTaskDefinition(
            &BSTR::from(TASK_NAME),
            &task,
            TASK_CREATE_OR_UPDATE.0,
            &VARIANT::default(),
            &VARIANT::default(),
            TASK_LOGON_INTERACTIVE_TOKEN,
            &VARIANT::default(),
        )
        .map_err(|e| format!("注册任务失败: {e}"))?;

        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn create_elevation_task() -> Result<(), String> {
    Err("仅限 Windows".into())
}

/// 通过计划任务启动程序（免 UAC 提权）
#[cfg(target_os = "windows")]
pub fn run_elevation_task() -> bool {
    unsafe {
        let (_, root) = match get_task_root() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let task = match root.GetTask(&BSTR::from(TASK_NAME)) {
            Ok(t) => t,
            Err(_) => return false,
        };
        task.Run(&VARIANT::default()).is_ok()
    }
}

#[cfg(not(target_os = "windows"))]
pub fn run_elevation_task() -> bool {
    false
}

/// 删除计划任务
#[cfg(target_os = "windows")]
pub fn delete_elevation_task() -> Result<(), String> {
    unsafe {
        let (_, root) = get_task_root()?;
        match root.DeleteTask(&BSTR::from(TASK_NAME), 0) {
            Ok(()) => Ok(()),
            Err(e) if e.code().0 as u32 == 0x80070002 => Ok(()), // 任务不存在
            Err(e) => Err(format!("删除计划任务失败: {e}")),
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
    unsafe {
        let (_, root) = match get_task_root() {
            Ok(v) => v,
            Err(_) => return false,
        };
        root.GetTask(&BSTR::from(TASK_NAME)).is_ok()
    }
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

    unsafe {
        let (_, root) = match get_task_root() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let task = match root.GetTask(&BSTR::from(TASK_NAME)) {
            Ok(t) => t,
            Err(_) => return false,
        };
        let def = match task.Definition() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let actions = match def.Actions() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let mut count = 0i32;
        if actions.Count(&mut count).is_err() || count == 0 {
            return false;
        }
        // COM 集合是 1-based 索引
        let action = match actions.get_Item(1) {
            Ok(a) => a,
            Err(_) => return false,
        };
        let exec: IExecAction = match action.cast::<IExecAction>() {
            Ok(e) => e,
            Err(_) => return false,
        };
        let mut path = BSTR::default();
        if exec.Path(&mut path).is_err() {
            return false;
        }
        path.to_string().to_lowercase() == current_exe
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevation_task_path_valid() -> bool {
    false
}

/// 清理旧版 ONLOGON 自启动计划任务（迁移用）
#[cfg(target_os = "windows")]
pub fn delete_legacy_autostart_task() {
    unsafe {
        if let Ok((_, root)) = get_task_root() {
            let _ = root.DeleteTask(&BSTR::from("ElegantClipboard_AutoStart"), 0);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn delete_legacy_autostart_task() {}
