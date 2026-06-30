//! Windows RTF 剪贴板读写

/// 从剪贴板读取 RTF（Windows 专用）
#[cfg(target_os = "windows")]
pub fn read_rtf_from_clipboard() -> Option<String> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatA,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::core::PCSTR;

    unsafe {
        let cf_rtf = RegisterClipboardFormatA(PCSTR(c"Rich Text Format".as_ptr().cast()));
        if cf_rtf == 0 {
            return None;
        }

        if OpenClipboard(None).is_err() {
            return None;
        }

        let result = (|| -> Option<String> {
            let handle = GetClipboardData(cf_rtf).ok()?;
            let hglobal = windows::Win32::Foundation::HGLOBAL(handle.0);
            if hglobal.is_invalid() {
                return None;
            }

            let ptr = GlobalLock(hglobal) as *const u8;
            if ptr.is_null() {
                return None;
            }

            // drop guard: 确保 GlobalUnlock 在任何退出路径（含 panic）都执行
            struct GlobalUnlockGuard(windows::Win32::Foundation::HGLOBAL);
            impl Drop for GlobalUnlockGuard {
                fn drop(&mut self) {
                    unsafe {
                        GlobalUnlock(self.0).ok();
                    }
                }
            }
            let _guard = GlobalUnlockGuard(hglobal);

            use windows::Win32::System::Memory::GlobalSize;

            let size = GlobalSize(hglobal) as usize;
            if size == 0 {
                return None;
            }
            let buf = std::slice::from_raw_parts(ptr, size);
            let len = buf.iter().position(|&b| b == 0).unwrap_or(size);
            Some(String::from_utf8_lossy(&buf[..len]).to_string())
        })();

        CloseClipboard().ok();

        result
    }
}


/// 将 RTF（及可选纯文本 fallback）写入剪贴板（Windows 专用）
#[cfg(target_os = "windows")]
pub fn write_rtf_to_clipboard(rtf: &str, plain_text: Option<&str>) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::core::PCSTR;

    unsafe {
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {e}"))?;

        let result = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;

            let cf_rtf = RegisterClipboardFormatA(PCSTR(c"Rich Text Format".as_ptr().cast()));
            if cf_rtf == 0 {
                return Err("Failed to register RTF clipboard format".to_string());
            }

            set_null_terminated_clipboard_data(cf_rtf, rtf.as_bytes())?;

            if let Some(text) = plain_text.filter(|t| !t.is_empty()) {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let byte_len = wide.len() * 2;
                let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len)
                    .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
                let ptr = GlobalLock(hmem) as *mut u8;
                if ptr.is_null() {
                    return Err("GlobalLock failed".to_string());
                }
                std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr, byte_len);
                GlobalUnlock(hmem).map_err(|e| format!("GlobalUnlock failed: {e}"))?;
                // CF_UNICODETEXT = 13
                SetClipboardData(13, Some(HANDLE(hmem.0)))
                    .map_err(|e| format!("SetClipboardData Unicode failed: {e}"))?;
            }

            Ok(())
        })();

        CloseClipboard().ok();
        result
    }
}

#[cfg(target_os = "windows")]
fn set_null_terminated_clipboard_data(format: u32, data: &[u8]) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::SetClipboardData;
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    let size = data.len().saturating_add(1);
    unsafe {
        let hmem =
            GlobalAlloc(GMEM_MOVEABLE, size).map_err(|e| format!("GlobalAlloc failed: {e}"))?;
        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        if !data.is_empty() {
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        *ptr.add(data.len()) = 0;
        GlobalUnlock(hmem).map_err(|e| format!("GlobalUnlock failed: {e}"))?;
        SetClipboardData(format, Some(HANDLE(hmem.0)))
            .map_err(|e| format!("SetClipboardData failed: {e}"))?;
    }
    Ok(())
}

