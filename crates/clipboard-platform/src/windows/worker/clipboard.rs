use super::super::{
    capture::lock_capture_state,
    file_actions::{item_paths_for_action, plain_text_for_copy},
};
use super::clipboard_write::{
    rich_clipboard_matches, verified_copy_sequence, write_extras_session, write_image_with_file,
    write_rich_clipboard,
};
use super::{Event, Worker, clipboard_content::rtf_for_copy};
use crate::service::read_content::RTF_FORMAT;
use ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::{History, database::ClipboardItem};
use clipboard_rs::{
    Clipboard, ClipboardContent, ClipboardContext, RustImageData, common::RustImage,
};
use std::{path::Path, thread};

use crate::service::protocol::{CopyFormat, CopyMode, CopyPurpose};

impl Worker {
    pub(super) fn handle_merge(&mut self, ids: Vec<i64>, purpose: CopyPurpose) -> Result<()> {
        self.clipboard_stash = None;
        let item_count = ids.len();
        let merged = self.history.merge_content(&ids)?;
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_merged_clipboard(&self.clipboard, &merged, &mut state.ignored_sequence)?;
        drop(state);
        self.send_event(Event::Merged {
            purpose,
            clipboard_sequence,
            item_count,
        })
    }

    pub(super) fn handle_copy(
        &mut self,
        id: i64,
        mode: CopyMode,
        purpose: CopyPurpose,
    ) -> Result<()> {
        self.clipboard_stash = None;
        let mut item = self.history.item(id)?;
        let plain_text = match mode {
            CopyMode::PlainText => Some(plain_text_for_copy(&item).map(|text| text.into_owned())),
            CopyMode::Transform(transform) => {
                // 转换基于纯文本表示；与 PlainText 同样要求文本类条目。
                Some(plain_text_for_copy(&item).map(|text| transform.apply(&text)))
            }
            _ => None,
        };
        let plain_text = plain_text.transpose()?;
        if mode == CopyMode::Original
            && item.content_type == "html"
            && item.text_content.as_deref().is_none_or(str::is_empty)
        {
            item.text_content = Some(clipboard_core::html_text::html_to_text(
                item.html_content.as_deref().context("记录没有 HTML 内容")?,
            )?);
        }
        let path_text = (mode == CopyMode::Paths)
            .then(|| item_paths_for_action(&item).map(|paths| paths.join("\n")))
            .transpose()?;
        let single_format = match mode {
            CopyMode::Html => Some(CopyFormat::Html),
            CopyMode::Rtf => Some(CopyFormat::Rtf),
            _ => None,
        };
        if let Some(format) = single_format {
            let (label, available) = match format {
                CopyFormat::Html => (
                    "HTML 格式",
                    item.html_content
                        .as_deref()
                        .is_some_and(|value| !value.is_empty()),
                ),
                CopyFormat::Rtf => (
                    "RTF 格式",
                    item.rtf_content
                        .as_deref()
                        .is_some_and(|value| !value.is_empty()),
                ),
            };
            if !available {
                bail!("该记录没有{label}内容");
            }
        }
        let is_rich = mode == CopyMode::Original && item.kind().is_some_and(|kind| kind.is_rich());
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
            let paths = History::file_paths_for_copy(&item)?;
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
            // PNG 字节直接读文件，无需解码后重编码；decode 仅为 CF_DIB 像素服务。
            let png = std::fs::read(path).map_err(|error| anyhow!("读取图片文件失败：{error}"))?;
            let image =
                RustImageData::from_path(path).map_err(|error| anyhow!("打开图片失败：{error}"))?;
            let pixels = image
                .to_rgba8()
                .map_err(|error| anyhow!("读取图片像素失败：{error}"))?;
            Some((png, pixels))
        } else {
            None
        };
        // 快照只在此次写回及成功事件均完成后才绑定到请求。
        let stash = (purpose.request_id().is_some() && mode == CopyMode::Original)
            .then(|| self.snapshot_clipboard_for_paste())
            .flatten();
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
            let (width, height) = pixels.dimensions();
            write_image_with_file(
                &item,
                &self.data_dir,
                &png,
                &pixels,
                width,
                height,
                &mut state.ignored_sequence,
            )?;
            verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || {
                    // 校验：PNG 格式字节一致（HDROP 与 DIB 由写回函数内部保证）。
                    // clipboard_rs 每次调用自开自关，无需外层 guard。
                    self.clipboard
                        .get_buffer("PNG")
                        .ok()
                        .is_some_and(|actual| actual == png)
                },
            )?
        } else if is_rich {
            // Record partial self-writes as ignored before an error is
            // returned, so the listener cannot add them to history.
            let rtf = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
            let extras = self.history.native_formats_for_copy(id)?;
            let extras_failed =
                write_rich_clipboard(&item, rtf.as_deref(), &extras, &mut state.ignored_sequence)?;
            let sequence = verified_rich_sequence(&self.clipboard, &item, rtf.as_deref(), &extras)?;
            rich_preserved = item
                .html_content
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                || rtf.is_some();
            if !extras_failed.is_empty() {
                self.send_event(Event::Status(format!(
                    "部分原生格式未写回：{}",
                    extras_failed.join("、")
                )))?;
            }
            sequence
        } else if let Some(format) = single_format {
            // 单一格式写回：HTML 或 RTF，缺少该格式时已在上方报错。
            // HTML 经 clipboard-rs 写回会包上 CF_HTML 片段头，校验用包含语义；
            // RTF 写原始字节，逐字节比对。
            let label = match format {
                CopyFormat::Html => "HTML 格式",
                CopyFormat::Rtf => "RTF 格式",
            };
            let (contents, expected_html) = if format == CopyFormat::Html {
                let html = item.html_content.clone().unwrap_or_default();
                (vec![ClipboardContent::Html(html.clone())], Some(html))
            } else {
                (vec![rtf_for_copy(item.rtf_content.as_deref())?], None)
            };
            let result = self.clipboard.set(contents);
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
            result.map_err(|error| anyhow!("复制{label}失败：{error}"))?;
            let matches = |clipboard: &ClipboardContext| {
                if let Some(expected) = &expected_html {
                    crate::service::read_html::read_html(clipboard)
                        .ok()
                        .is_some_and(|actual| actual.contains(expected.as_str()))
                } else {
                    // RTF 以 NUL 终止；写回与读回都可能带尾部 NUL，两侧都剥离后比对。
                    let expected = item
                        .rtf_content
                        .as_deref()
                        .and_then(clipboard_core::rich::decode_rtf_for_clipboard);
                    let actual = clipboard.get_buffer(RTF_FORMAT).ok();
                    match (actual, expected) {
                        (Some(actual), Some(expected)) => {
                            let actual = actual.strip_suffix([0u8].as_slice()).unwrap_or(&actual);
                            let expected =
                                expected.strip_suffix([0u8].as_slice()).unwrap_or(&expected);
                            actual == expected
                        }
                        _ => false,
                    }
                }
            };
            verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || matches(&self.clipboard),
            )?
        } else {
            let text = plain_text
                .or(path_text)
                .or(item.text_content)
                .context("记录没有可复制的文本")?;
            let result = self.clipboard.set_text(text.clone());
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
            result.map_err(|error| anyhow!("复制文本失败：{error}"))?;
            // 原生格式仅补充 Original；纯文本、转换和路径复制必须保持纯文本。
            let extras = if mode == CopyMode::Original {
                self.history.native_formats_for_copy(id)?
            } else {
                Vec::new()
            };
            let extras_failed = if extras.is_empty() {
                Vec::new()
            } else {
                write_extras_session(&extras)
                    .unwrap_or_else(|_| extras.iter().map(|(name, _)| name.clone()).collect())
            };
            state.ignored_sequence = unsafe { GetClipboardSequenceNumber() };
            let sequence = verified_copy_sequence(
                || unsafe { GetClipboardSequenceNumber() },
                || {
                    self.clipboard.get_text().ok().as_ref() == Some(&text)
                        && (extras.is_empty() || extras_verified_via_rs(&extras))
                },
            )?;
            if !extras_failed.is_empty() {
                self.send_event(Event::Status(format!(
                    "部分原生格式未写回：{}",
                    extras_failed.join("、")
                )))?;
            }
            sequence
        };
        // 确认结果必须仍是 setter 记录的写后序列；外部复制即使内容相同也不接管其序列。
        if clipboard_sequence != state.ignored_sequence
            || unsafe { GetClipboardSequenceNumber() } != clipboard_sequence
        {
            bail!("复制过程中剪贴板已被其他应用修改，请重试");
        }
        drop(state);
        self.send_event(Event::Copied {
            id,
            purpose,
            clipboard_sequence,
            message: match item.content_type.as_str() {
                _ if mode == CopyMode::Files => "图片文件已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::Paths => "路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::PlainText => "纯文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::Html => "HTML 已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if mode == CopyMode::Rtf => "RTF 已复制，可切换到目标应用按 Ctrl+V 粘贴",
                _ if matches!(mode, CopyMode::Transform(_)) => {
                    "转换后的文本已复制，可切换到目标应用按 Ctrl+V 粘贴"
                }
                "image" => "图片已复制（含文件格式），可切换到目标应用按 Ctrl+V 粘贴",
                "files" => "文件路径已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" if rich_preserved => "富文本已复制，可切换到目标应用按 Ctrl+V 粘贴",
                "html" | "rtf" => "富文本格式未写回，已按纯文本复制",
                _ => "已复制，可切换到目标应用按 Ctrl+V 粘贴",
            }
            .into(),
        })?;
        self.remember_paste_stash(purpose.request_id(), clipboard_sequence, stash);
        Ok(())
    }
}

/// 额外格式读回校验：单个短会话内逐条 raw 读取比对
/// （clipboard_rs 的会话此刻已关闭，raw 读取需要已打开剪贴板）。
fn extras_verified_via_rs(extras: &[(String, Vec<u8>)]) -> bool {
    if extras.is_empty() {
        return true;
    }
    let Ok(clipboard) = clipboard_win::Clipboard::new_attempts(10) else {
        return false;
    };
    let verified = extras.iter().all(|(name, expected)| {
        let Some(format) = clipboard_win::register_format(name) else {
            return false;
        };
        let mut actual = Vec::new();
        clipboard_win::raw::get_vec(format.get(), &mut actual).is_ok_and(|_| actual == *expected)
    });
    drop(clipboard);
    verified
}

fn verified_rich_sequence(
    clipboard: &ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
    extras: &[(String, Vec<u8>)],
) -> Result<u32> {
    for _ in 0..4 {
        let before = unsafe { GetClipboardSequenceNumber() };
        if !rich_clipboard_matches(clipboard, item, rtf) || !extras_verified_via_rs(extras) {
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
