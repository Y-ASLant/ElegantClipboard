//! Windows 管理员权限检测与 UAC 重启。

use std::{env, io, mem::size_of};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};
use windows::core::PCWSTR;

pub fn is_elevated() -> io::Result<bool> {
    let mut token = HANDLE::default();
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(io::Error::other)?;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .map_err(io::Error::other)
    };
    unsafe {
        let _ = CloseHandle(token);
    }
    result.map(|()| elevation.TokenIsElevated != 0)
}

/// 通过 ShellExecute "runas" 启动新的提权实例（会弹 UAC）。
/// `extra_arg` 附加到启动参数（用于 `--wait-for-instance`）。
pub fn relaunch_elevated(extra_arg: Option<&str>) -> io::Result<()> {
    let executable = env::current_exe()?;
    let executable = wide(executable.as_os_str());
    let operation = wide("runas");
    let arguments = extra_arg.map(wide);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(executable.as_ptr()),
            arguments
                .as_ref()
                .map_or(PCWSTR::null(), |a| PCWSTR(a.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result.0 as usize) <= 32 {
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }
    Ok(())
}

fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt as _;
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
