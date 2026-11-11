use super::read_content::retry_clipboard_read;
use anyhow::{Context, Result, anyhow};
use windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
    },
    core::w,
};

/// 独立 HWND 区分同进程的并发会话，避免 OpenClipboard(NULL) 相互关闭。
/// 窗口在读取线程创建和销毁，生命周期覆盖所有读取重试。
struct ReadWindow(HWND);

impl Drop for ReadWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

pub(super) fn read_files() -> Result<Vec<String>> {
    let window = ReadWindow(
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .context("创建剪贴板文件读取窗口失败")?,
    );
    retry_clipboard_read(|| {
        let _session = clipboard_win::Clipboard::new_for(window.0.0)?;
        // get 不会再次打开会话；不能替换成 get_clipboard 或 clipboard-rs getter。
        clipboard_win::get(clipboard_win::formats::FileList)
    })
    .map_err(|error| anyhow!("读取剪贴板文件失败：{error}"))
}
