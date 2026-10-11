use super::super::{
    capture::lock_capture_state,
    file_actions::{item_paths_for_action, plain_text_for_copy},
};
use super::{Event, Worker};
use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::{History, database::ClipboardItem};
use clipboard_rs::{
    Clipboard, ClipboardContent, ClipboardContext, RustImageData, common::RustImage,
};
use std::{path::Path, thread};

impl Worker {
    pub(super) fn handle_merge(&mut self, ids: Vec<i64>, for_paste: bool) -> Result<()> {
        let item_count = ids.len();
        let merged = self.history.merge_content(&ids, &self.staged_dir)?;
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_merged_clipboard(&self.clipboard, &merged, &mut state.ignored_sequence)?;
        drop(state);
        self.send_event(Event::Merged {
            for_paste,
            clipboard_sequence,
            item_count,
        })
    }

    pub(super) fn handle_copy(&mut self, id: i64, for_paste: bool, mode: CopyMode) -> Result<()> {
        let item = self.history.item(id)?;
        if mode == CopyMode::PlainText {
            plain_text_for_copy(&item)?;
        }
        let path_text = (mode == CopyMode::Paths)
            .then(|| item_paths_for_action(&item, &self.staged_dir).map(|paths| paths.join("\n")))
            .transpose()?;
        let is_rich =
            mode == CopyMode::Original && matches!(item.content_type.as_str(), "html" | "rtf");
        let files = if mode == CopyMode::Original && item.content_type == "files" {
            let paths = History::file_paths_for_copy(&item, &self.staged_dir)?;
            if paths.iter().any(|path| !Path::new(path).exists()) {
                bail!("源文件或文件夹已不存在，无法复制");
            }
            Some(paths)
        } else {
            None
        };
        let image = if mode == CopyMode::Original && item.content_type == "image" {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            if !Path::new(path).is_file() {
                bail!("图片文件已丢失，无法复制");
            }
            let image =
                RustImageData::from_path(path).map_err(|error| anyhow!("打开图片失败：{error}"))?;
            // clipboard-rs clears the clipboard before converting to PNG/BMP.
            // Check both conversions before touching the user's current contents.
            image
                .to_png()
                .map_err(|error| anyhow!("图片 PNG 编码失败：{error}"))?;
            image
                .to_bitmap()
                .map_err(|error| anyhow!("图片位图编码失败：{error}"))?;
            Some(image)
        } else {
            None
        };
        let mut state = lock_capture_state(&self.state)?;
        let mut rich_preserved = false;
        if let Some(path_text) = path_text {
            self.clipboard
                .set_text(path_text)
                .map_err(|error| anyhow!("复制路径失败：{error}"))?;
        } else if let Some(files) = files {
            self.clipboard
                .set_files(files)
                .map_err(|error| anyhow!("复制文件失败：{error}"))?;
        } else if let Some(image) = image {
            self.clipboard
                .set_image(image)
                .map_err(|error| anyhow!("复制图片失败：{error}"))?;
        } else if is_rich {
            // Record partial self-writes as ignored before an error is
            // returned, so the listener cannot add them to history.
            let rtf = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
            write_rich_clipboard(&item, rtf.as_deref(), &mut state.ignored_sequence)?;
            state.ignored_sequence =
                verified_rich_sequence(&self.clipboard, &item, rtf.as_deref())?;
            rich_preserved = item
                .html_content
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                || rtf.is_some();
        } else {
            let text = item.text_content.context("记录没有可复制的文本")?;
            self.clipboard
                .set_text(text)
                .map_err(|error| anyhow!("复制失败：{error}"))?;
        }
        let clipboard_sequence = if is_rich {
            state.ignored_sequence
        } else {
            let sequence = unsafe { GetClipboardSequenceNumber() };
            state.ignored_sequence = sequence;
            sequence
        };
        if unsafe { GetClipboardSequenceNumber() } != clipboard_sequence {
            bail!("复制过程中剪贴板已被其他应用修改，请重试");
        }
        drop(state);
        self.send_event(Event::Copied {
            id,
            for_paste,
            clipboard_sequence,
            message: match item.content_type.as_str() {
                _ if mode == CopyMode::Paths => "路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::PlainText => "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "image" => "图片已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "files" => "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" if rich_preserved => "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" => "富文本格式未写回，已按纯文本复制",
                _ => "已复制，可切换到目标应用按 Ctrl+V 粘贴",
            }
            .into(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyMode {
    Original,
    PlainText,
    Paths,
}

fn write_rich_clipboard(
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
    ignored_sequence: &mut u32,
) -> Result<()> {
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    if text.is_none() && html.is_none() && rtf.is_none() {
        bail!("富文本记录没有可写回的有效格式");
    }

    // clipboard-rs::set suppresses failures of individual formats. Keep one
    // clipboard open so every requested format is written or reported as failed.
    let mut touched = false;
    let result = (|| {
        let clipboard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
        clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
        touched = true;
        if let Some(text) = text {
            clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入纯文本格式失败：{error}"))?;
        }
        if let Some(html) = html {
            let format = clipboard_win::register_format("HTML Format")
                .context("注册 HTML 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_html_with(format, html, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入 HTML 格式失败：{error}"))?;
        }
        if let Some(rtf) = rtf {
            let format = clipboard_win::register_format("Rich Text Format")
                .context("注册 RTF 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_without_clear(format, rtf)
                .map_err(|error| anyhow!("写入 RTF 格式失败：{error}"))?;
        }
        drop(clipboard);
        Ok(())
    })();
    if touched {
        // Capture the final sequence after the clipboard guard has closed, even
        // when a later format failed and left a partial write behind.
        *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    }
    result
}

fn rich_clipboard_matches(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
) -> bool {
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html_matches = html.is_none_or(|expected| {
        clipboard
            .get_html()
            .ok()
            .is_some_and(|actual| actual.contains(expected))
    });
    let rtf_matches = rtf.is_none_or(|expected| {
        clipboard
            .get_buffer("Rich Text Format")
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    let text_matches = text.is_none_or(|expected| {
        clipboard
            .get_text()
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    html_matches && rtf_matches && text_matches
}

fn verified_rich_sequence(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
) -> Result<u32> {
    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        if !rich_clipboard_matches(clipboard, item, rtf) {
            bail!("富文本写回后格式校验失败，请重试");
        }
        let after = unsafe { GetClipboardSequenceNumber() };
        if before == after {
            return Ok(after);
        }
        thread::yield_now();
    }
    bail!("富文本写回后剪贴板持续变化，请重试")
}

fn write_merged_clipboard(
    clipboard: &ClipboardContext,
    merged: &clipboard_core::MergedContent,
    ignored_sequence: &mut u32,
) -> Result<u32> {
    let mut contents = Vec::with_capacity(2);
    if !merged.files.is_empty() {
        contents.push(ClipboardContent::Files(merged.files.clone()));
    }
    if let Some(text) = &merged.text {
        contents.push(ClipboardContent::Text(text.clone()));
    }
    let write_result = clipboard
        .set(contents)
        .map_err(|error| anyhow!("写入合并内容失败：{error}"));
    *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    write_result?;
    if *ignored_sequence == 0 {
        bail!("无法确认合并内容的剪贴板序列号，请重试");
    }

    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        let text_matches = merged.text.as_ref().is_none_or(|expected| {
            clipboard
                .get_text()
                .ok()
                .is_some_and(|actual| actual == expected.as_str())
        });
        let files_match = merged.files.is_empty()
            || clipboard
                .get_files()
                .ok()
                .is_some_and(|actual| actual.as_slice() == merged.files.as_slice());
        if !text_matches || !files_match {
            bail!("合并内容写回后格式校验失败，请重试");
        }
        let after = unsafe { GetClipboardSequenceNumber() };
        if before == after {
            *ignored_sequence = after;
            return Ok(after);
        }
        thread::yield_now();
    }
    bail!("合并内容写回后剪贴板持续变化，请重试")
}
