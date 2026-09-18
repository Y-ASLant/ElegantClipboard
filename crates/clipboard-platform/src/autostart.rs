//! Current-user Windows startup entry for one GPUI data directory.

use anyhow::{Context, Result, bail};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
            REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW,
            RegQueryValueExW, RegSetValueExW,
        },
    },
    core::{PCWSTR, w},
};

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn check(status: WIN32_ERROR, operation: &str) -> Result<()> {
    if status != ERROR_SUCCESS {
        bail!("{operation}失败：Windows 错误 {}", status.0);
    }
    Ok(())
}

fn name(data_dir: &Path) -> Result<Vec<u16>> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let normalized = canonical.to_string_lossy().to_lowercase();
    let hash = blake3::hash(normalized.as_bytes()).to_hex();
    Ok(format!("ElegantClipboard-GPUI-{}", &hash[..16])
        .encode_utf16()
        .chain(Some(0))
        .collect())
}

fn command(exe: &Path, data_dir: &Path) -> String {
    format!(
        "\"{}\" --start-hidden --data-dir \"{}\"",
        exe.display(),
        data_dir.display()
    )
}

fn open_run(write: bool) -> Result<Option<Key>> {
    let mut key = HKEY(std::ptr::null_mut());
    let status = if write {
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                w!(""),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        }
    } else {
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key) }
    };
    if status == ERROR_FILE_NOT_FOUND && !write {
        return Ok(None);
    }
    check(status, "打开开机启动设置")?;
    Ok(Some(Key(key)))
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    let Some(key) = open_run(false)? else {
        return Ok(false);
    };
    let value_name = name(data_dir)?;
    let mut kind = REG_VALUE_TYPE(0);
    let mut size = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    check(status, "读取开机启动设置")?;
    if kind != REG_SZ || size > 4096 || !size.is_multiple_of(2) {
        return Ok(false);
    }
    let mut bytes = vec![0u8; size as usize];
    check(
        unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(value_name.as_ptr()),
                None,
                None,
                Some(bytes.as_mut_ptr()),
                Some(&mut size),
            )
        },
        "读取开机启动命令",
    )?;
    let utf16: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let saved = String::from_utf16(utf16.split(|unit| *unit == 0).next().unwrap_or(&[]))?;
    let expected = command(&std::env::current_exe()?, &data_dir.canonicalize()?);
    Ok(saved == expected)
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let key = open_run(true)?.context("无法打开开机启动设置")?;
    let value_name = name(data_dir)?;
    if enabled {
        let command = command(&std::env::current_exe()?, &data_dir.canonicalize()?);
        let bytes: Vec<u8> = command
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        check(
            unsafe {
                RegSetValueExW(
                    key.0,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(&bytes),
                )
            },
            "保存开机启动设置",
        )?;
    } else {
        let status = unsafe { RegDeleteValueW(key.0, PCWSTR(value_name.as_ptr())) };
        if status != ERROR_FILE_NOT_FOUND {
            check(status, "关闭开机启动")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_unicode_paths_and_starts_hidden() {
        let actual = command(
            Path::new("C:\\Program Files\\剪贴板\\app.exe"),
            Path::new("D:\\历史 数据"),
        );
        assert_eq!(
            actual,
            "\"C:\\Program Files\\剪贴板\\app.exe\" --start-hidden --data-dir \"D:\\历史 数据\""
        );
    }
}
