use anyhow::{Result, anyhow};
use clipboard_rs::{Clipboard, ClipboardContext};

/// Honor the producer's local-history policy before reading any content.
/// CanUploadToCloudClipboard alone does not forbid local history.
pub(super) fn excludes_history(clipboard: &ClipboardContext) -> Result<bool> {
    use ::windows::{
        Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW},
        core::w,
    };
    let exclude =
        unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) };
    let history = unsafe { RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")) };
    if exclude == 0 || history == 0 {
        return Err(anyhow!("无法确认剪贴板隐私标记，本次复制未保存"));
    }
    if unsafe { IsClipboardFormatAvailable(exclude) }.is_ok() {
        return Ok(true);
    }
    if unsafe { IsClipboardFormatAvailable(history) }.is_err() {
        return Ok(false);
    }
    let value = clipboard
        .get_buffer("CanIncludeInClipboardHistory")
        .map_err(|error| anyhow!("无法读取剪贴板历史权限，本次复制未保存：{error}"))?;
    Ok(!allows_history(&value))
}

fn allows_history(value: &[u8]) -> bool {
    value
        .get(..4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .is_some_and(|bytes| u32::from_le_bytes(bytes) == 1)
}

#[cfg(test)]
mod tests {
    use super::allows_history;

    #[test]
    fn explicit_history_permission_requires_a_valid_allow_value() {
        assert!(allows_history(&1u32.to_le_bytes()));
        assert!(allows_history(&[1, 0, 0, 0, 0, 0, 0, 0]));
        for value in [
            vec![],
            vec![1],
            0u32.to_le_bytes().to_vec(),
            2u32.to_le_bytes().to_vec(),
        ] {
            assert!(!allows_history(&value));
        }
    }
}
