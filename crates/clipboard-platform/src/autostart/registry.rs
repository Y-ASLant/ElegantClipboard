use super::to_wide;
use anyhow::{Result, bail};
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

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Value {
    pub kind: REG_VALUE_TYPE,
    pub bytes: Vec<u8>,
}

impl Value {
    pub(super) fn string(&self) -> Option<String> {
        if self.kind != REG_SZ || !self.bytes.len().is_multiple_of(2) {
            return None;
        }
        let mut units: Vec<_> = self
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        if units.last() == Some(&0) {
            units.pop();
        }
        if units.contains(&0) {
            return None;
        }
        String::from_utf16(&units).ok()
    }
}

pub(super) struct Key(HKEY);

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

impl Key {
    pub(super) fn open(path: &str, write: bool) -> Result<Option<Self>> {
        let path = to_wide(path);
        let mut key = HKEY(std::ptr::null_mut());
        let status = if write {
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    w!(""),
                    REG_OPTION_NON_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    None,
                    &mut key,
                    None,
                )
            }
        } else {
            unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    KEY_QUERY_VALUE,
                    &mut key,
                )
            }
        };
        if status == ERROR_FILE_NOT_FOUND && !write {
            return Ok(None);
        }
        check(status, "打开开机启动设置")?;
        Ok(Some(Self(key)))
    }

    pub(super) fn read(&self, name: &str) -> Result<Option<Value>> {
        let name = to_wide(name);
        let mut kind = REG_VALUE_TYPE(0);
        let mut size = 0;
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status, "读取开机启动设置")?;
        if size > 1024 * 1024 {
            bail!("开机启动设置过大");
        }
        let mut bytes = vec![0; size as usize];
        check(
            unsafe {
                RegQueryValueExW(
                    self.0,
                    PCWSTR(name.as_ptr()),
                    None,
                    Some(&mut kind),
                    Some(bytes.as_mut_ptr()),
                    Some(&mut size),
                )
            },
            "读取开机启动命令",
        )?;
        bytes.truncate(size as usize);
        Ok(Some(Value { kind, bytes }))
    }

    pub(super) fn write_verified(&self, name: &str, value: &Value) -> Result<()> {
        let wide = to_wide(name);
        check(
            unsafe {
                RegSetValueExW(
                    self.0,
                    PCWSTR(wide.as_ptr()),
                    None,
                    value.kind,
                    Some(&value.bytes),
                )
            },
            "保存开机启动设置",
        )?;
        if self.read(name)?.as_ref() != Some(value) {
            bail!("无法验证已保存的开机启动设置");
        }
        Ok(())
    }

    pub(super) fn remove(&self, name: &str) -> Result<()> {
        let name = to_wide(name);
        let status = unsafe { RegDeleteValueW(self.0, PCWSTR(name.as_ptr())) };
        if status != ERROR_FILE_NOT_FOUND {
            check(status, "删除旧开机启动设置")?;
        }
        Ok(())
    }
}
