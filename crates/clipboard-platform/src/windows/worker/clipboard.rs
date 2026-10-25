use super::super::{
    capture::lock_capture_state,
    file_actions::{item_paths_for_action, plain_text_for_copy},
};
use super::clipboard_write::{
    rich_clipboard_matches, verified_copy_sequence, write_extras_session, write_image_with_file,
    write_rich_clipboard,
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

    pub(super) fn handle_copy(
        &mut self,
        id: i64,
        request_id: Option<u64>,
        mode: CopyMode,
    ) -> Result<()> {
        let mut item = self.history.item(id)?;
        let plain_text = (mode == CopyMode::PlainText)
            .then(|| plain_text_for_copy(&item).map(|text| text.into_owned()))
            .transpose()?;
        if mode == CopyMode::Original
            && item.content_type == "html"
            && item.text_content.as_deref().is_none_or(str::is_empty)
        {
            item.text_content = Some(clipboard_core::html_text::html_to_text(
                item.html_content.as_deref().context("记录没有 HTML 内容")?,
            )?);
        }
        let path_text = (mode == CopyMode::Paths)
            .then(|| item_paths_for_action(&item, &self.staged_dir).map(|paths| paths.join("\n")))
            .transpose()?;
        let is_rich =
            mode == CopyMode::Original && matches!(item.content_type.as_str(), "html" | "rtf");
        let files = if mode == CopyMode::Files {
            if item.content_type != "image" {
                bail!("该记录不是图片，无法作为文件复制");
            }
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            if !Path::new(path).is_file() {
                bail!("图片文件已丢失，无法复制");
            }
            Some(vec![path.to_owned()])
        } else if mode == CopyMode::Original && item.content_type == "files" {
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
            // 写回全程使用已编码的字节；clipboard-win 会话内不再二次转换。
            image
                .to_png()
                .map_err(|error| anyhow!("图片 PNG 编码失败：{error}"))?;
            let png = image
                .to_png()
                .map_err(|error| anyhow!("图片 PNG 编码失败：{error}"))?
                .get_bytes()
                .to_vec();
            let pixels = image
                .to_rgba8()
                .map_err(|error| anyhow!("读取图片像素失败：{error}"))?;
            Some((png, pixels))
        } else {
            None
        };
        // 额外原生格式：仅 Original 模式携带（纯文本/路径复制不注入）。
        let extras = if mode == CopyMode::Original {
            self.history.native_formats_for_copy(id)?
        } else {
            Vec::new()
        };
        let mut state = lock_capture_state(&self.state)?;
        let mut rich_preserved = false;
        // A setter can fail after clearing or writing one image format. Record
        // its sequence before propagating errors to suppress partial self-capture.
        let clipboard_sequence = if let Some(files) = files {
            let result = self.clipboard.set_files(files.clone());
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
            result.map_err(|error| anyhow!("复制文件失败：{error}"))?;
            verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || self.clipboard.get_files().ok().as_ref() == Some(&files),
            )?
        } else if let Some((png, pixels)) = image {
            // 图片写回：CF_HDROP（临时文件）+ CF_DIB + PNG，纯图片可直接以文件粘贴。
            let (width, height) = (
                item.image_width.unwrap_or(0).max(0) as u32,
                item.image_height.unwrap_or(0).max(0) as u32,
            );
            let pixel_refs: Vec<[u8; 4]> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|px| [px[0], px[1], px[2], px[3]])
                .collect();
            write_image_with_file(
                &item,
                &self.data_dir,
                &png,
                &pixel_refs,
                width,
                height,
                &mut state.ignored_sequence,
            )?;
            let sequence = verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || {
                    // 校验：PNG 格式字节一致（HDROP 与 DIB 由写回函数内部保证）。
                    // clipboard_rs 每次调用自开自关，无需外层 guard。
                    self.clipboard
                        .get_buffer("PNG")
                        .ok()
                        .is_some_and(|actual| actual == png)
                },
            )?;
            state.ignored_sequence = sequence;
            sequence
        } else if is_rich {
            // Record partial self-writes as ignored before an error is
            // returned, so the listener cannot add them to history.
            let rtf = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
            let (_, extras_failed) =
                write_rich_clipboard(&item, rtf.as_deref(), &extras, &mut state.ignored_sequence)?;
            state.ignored_sequence =
                verified_rich_sequence(&self.clipboard, &item, rtf.as_deref(), &extras)?;
            rich_preserved = item
                .html_content
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                || rtf.is_some();
            if !extras_failed.is_empty() {
                send_event_status(
                    &self.events,
                    format!("部分原生格式未写回：{}", extras_failed.join("、")),
                )?;
            }
            state.ignored_sequence
        } else {
            let text = plain_text
                .or(path_text)
                .or(item.text_content)
                .context("记录没有可复制的文本")?;
            let result = self.clipboard.set_text(text.clone());
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
            result.map_err(|error| anyhow!("复制文本失败：{error}"))?;
            // 纯文本路径的额外格式：独立短会话 NoClear 追加（不 empty）。
            let extras_failed = if extras.is_empty() {
                Vec::new()
            } else {
                write_extras_session(&extras)
                    .unwrap_or_else(|_| extras.iter().map(|(name, _)| name.clone()).collect())
            };
            let sequence = verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || {
                    self.clipboard.get_text().ok().as_ref() == Some(&text)
                        && (extras.is_empty() || extras_verified_via_rs(&self.clipboard, &extras))
                },
            )?;
            if !extras.is_empty() {
                state.ignored_sequence = sequence;
                if !extras_failed.is_empty() {
                    send_event_status(
                        &self.events,
                        format!("部分原生格式未写回：{}", extras_failed.join("、")),
                    )?;
                }
            }
            sequence
        };
        state.ignored_sequence = clipboard_sequence;
        if unsafe { GetClipboardSequenceNumber() } != clipboard_sequence {
            bail!("复制过程中剪贴板已被其他应用修改，请重试");
        }
        drop(state);
        self.send_event(Event::Copied {
            id,
            request_id,
            clipboard_sequence,
            message: match item.content_type.as_str() {
                _ if mode == CopyMode::Files => "图片文件已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::Paths => "路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::PlainText => "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "image" => "图片已复制（含文件格式），可切换到目标应用按 Ctrl+V 粘贴",
                "files" => "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" if rich_preserved => "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" => "富文本格式未写回，已按纯文本复制",
                _ => "已复制，可切换到目标应用按 Ctrl+V 粘贴",
            }
            .into(),
        })
    }
}

fn send_event_status(events: &async_channel::Sender<Event>, message: String) -> Result<()> {
    events
        .send_blocking(Event::Status(message))
        .map_err(|_| anyhow!("窗口已关闭"))
}

/// 通过 clipboard_rs 读回额外格式校验（纯文本路径，会话由 rs 管理）。
fn extras_verified_via_rs(_clipboard: &ClipboardContext, extras: &[(String, Vec<u8>)]) -> bool {
    for (name, expected) in extras {
        let Some(format) = clipboard_win::register_format(name) else {
            return false;
        };
        // 独立短会话：raw 读取需要已打开的剪贴板（clipboard_rs 的会话此刻已关闭）。
        let Ok(clipboard) = clipboard_win::Clipboard::new_attempts(10) else {
            return false;
        };
        let mut actual = Vec::new();
        let read = clipboard_win::raw::get_vec(format.get(), &mut actual).is_ok();
        drop(clipboard);
        if !read || &actual != expected {
            return false;
        }
    }
    true
}

fn verified_rich_sequence(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
    extras: &[(String, Vec<u8>)],
) -> Result<u32> {
    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        if !rich_clipboard_matches(clipboard, item, rtf)
            || !extras_verified_via_rs(clipboard, extras)
        {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyMode {
    Original,
    PlainText,
    Paths,
    Files,
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

#[cfg(test)]
mod tests {
    use super::super::clipboard_write::verified_copy_sequence;

    #[test]
    fn copy_confirmation_requires_matching_content_and_a_nonzero_sequence() {
        assert!(verified_copy_sequence(|| 42, || false).is_err());
        assert!(verified_copy_sequence(|| 0, || true).is_err());
        assert_eq!(verified_copy_sequence(|| 42, || true).unwrap(), 42);
    }
}
