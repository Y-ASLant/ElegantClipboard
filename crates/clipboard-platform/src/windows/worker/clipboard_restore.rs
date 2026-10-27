use super::{Event, StashedClipboard, Worker, clipboard_write::build_cf_dib};
use crate::service::{CapturedClipboard, capture::lock_capture_state, read_content::RTF_FORMAT};
use anyhow::{Context, Result, anyhow};
use clipboard_rs::{RustImageData, common::RustImage};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

impl Worker {
    pub(super) fn handle_restore_stash(
        &mut self,
        request_id: u64,
        clipboard_sequence: u32,
    ) -> Result<()> {
        let restored = self
            .take_stash_for_restore(request_id, clipboard_sequence)
            .map_or(Ok(false), |stash| self.write_stashed(&stash))
            .map_err(|error| error.to_string());
        self.send_event(Event::ClipboardRestored(restored))
    }

    fn write_stashed(&self, stash: &StashedClipboard) -> Result<bool> {
        // 在打开或清空剪贴板之前完成解码与校验。
        let dib = if let CapturedClipboard::Image { png, .. } = &stash.content {
            let image =
                RustImageData::from_bytes(png).map_err(|error| anyhow!("恢复图片失败：{error}"))?;
            let pixels = image
                .to_rgba8()
                .map_err(|error| anyhow!("读取恢复图片像素失败：{error}"))?;
            Some(build_cf_dib(&pixels, pixels.width(), pixels.height())?)
        } else {
            None
        };
        let mut state = lock_capture_state(&self.state)?;
        let mut touched = false;
        let result = (|| {
            let clipboard = clipboard_win::Clipboard::new_attempts(10)
                .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
            // 与写入持有同一系统锁，避免用户新复制发生在序列检查与清空之间。
            if !stash.is_current(unsafe { GetClipboardSequenceNumber() }) {
                return Ok(false);
            }
            clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
            touched = true;
            write_content_locked(stash, dib.as_deref())?;
            for format in &stash.extra {
                // 额外原生格式保持尽力恢复的既有语义。
                let _ = write_buffer_locked(&format.name, &format.data);
            }
            drop(clipboard);
            Ok(true)
        })();
        if touched {
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
        }
        result
    }
}

fn write_content_locked(stash: &StashedClipboard, dib: Option<&[u8]>) -> Result<()> {
    match &stash.content {
        CapturedClipboard::Text(text) => write_text_locked(text),
        CapturedClipboard::Rich { html, rtf, text } => {
            if let Some(text) = text {
                write_text_locked(text)?;
            }
            if let Some(html) = html {
                let format = clipboard_win::register_format("HTML Format")
                    .context("注册 HTML 剪贴板格式失败")?;
                clipboard_win::raw::set_html_with(
                    format.get(),
                    html,
                    clipboard_win::options::NoClear,
                )
                .map_err(|error| anyhow!("恢复 HTML 格式失败：{error}"))?;
            }
            if let Some(rtf) = rtf {
                write_buffer_locked(RTF_FORMAT, rtf)?;
            }
            Ok(())
        }
        CapturedClipboard::Files(paths) => {
            clipboard_win::raw::set_file_list_with(paths, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("恢复文件列表失败：{error}"))
        }
        CapturedClipboard::Image { png, .. } => {
            clipboard_win::raw::set_without_clear(8, dib.context("恢复图片位图缺失")?)
                .map_err(|error| anyhow!("恢复图片位图失败：{error}"))?;
            write_buffer_locked("PNG", png)
        }
    }
}

fn write_text_locked(text: &str) -> Result<()> {
    clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
        .map_err(|error| anyhow!("恢复文本失败：{error}"))
}

fn write_buffer_locked(name: &str, bytes: &[u8]) -> Result<()> {
    let format = clipboard_win::register_format(name).context("注册恢复剪贴板格式失败")?;
    clipboard_win::raw::set_without_clear(format.get(), bytes)
        .map_err(|error| anyhow!("恢复原生格式失败：{error}"))
}
