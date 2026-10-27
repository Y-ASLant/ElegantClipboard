use super::super::{
    CapturedClipboard,
    capture::lock_capture_state,
    file_actions::{item_paths_for_action, plain_text_for_copy},
    read_content::RTF_FORMAT,
};
use super::{Event, Worker, clipboard_content::rtf_for_copy, image_tmp::ensure_temp_png};
use crate::sequence::current_sequence;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::History;
use clipboard_rs::{
    Clipboard, ClipboardContent, ClipboardContext, RustImageData, common::RustImage,
};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

#[cfg(target_os = "linux")]
const PNG_FORMAT: &str = "image/png";
#[cfg(target_os = "macos")]
const PNG_FORMAT: &str = "public.png";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyMode {
    Original,
    PlainText,
    Paths,
    Files,
    /// 富文本条目的单一格式复制（HTML 或 RTF），缺少该格式时报错不降级。
    Html,
    Rtf,
    /// 文本转换后的纯文本复制（见决策 0003）。
    Transform(clipboard_core::TextTransform),
}

impl CopyMode {
    pub(super) fn from(format: crate::service::protocol::CopyFormat) -> Self {
        match format {
            crate::service::protocol::CopyFormat::PlainText => Self::PlainText,
            crate::service::protocol::CopyFormat::Html => Self::Html,
            crate::service::protocol::CopyFormat::Rtf => Self::Rtf,
        }
    }
}

impl Worker {
    pub(super) fn handle_merge(&mut self, ids: Vec<i64>, request_id: Option<u64>) -> Result<()> {
        self.clipboard_stash = None;
        let item_count = ids.len();
        let merged = self.history.merge_content(&ids, &self.staged_dir)?;
        let mut contents = Vec::with_capacity(2);
        if !merged.files.is_empty() {
            contents.push(ClipboardContent::Files(merged.files));
        }
        if let Some(text) = merged.text {
            contents.push(ClipboardContent::Text(text));
        }
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_verified(&self.clipboard, contents, &mut state.ignored_sequence)?;
        drop(state);
        self.send_event(Event::Merged {
            for_paste: request_id.is_some(),
            request_id,
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
        self.clipboard_stash = None;
        let item = self.history.item(id)?;
        let mut contents = Vec::new();
        let mut rich = false;
        match mode {
            CopyMode::PlainText => contents.push(ClipboardContent::Text(
                plain_text_for_copy(&item)?.into_owned(),
            )),
            CopyMode::Transform(transform) => contents.push(ClipboardContent::Text(
                transform.apply(&plain_text_for_copy(&item)?),
            )),
            CopyMode::Html | CopyMode::Rtf => {
                if mode == CopyMode::Html {
                    let html = item
                        .html_content
                        .as_deref()
                        .filter(|value| !value.is_empty())
                        .context("该记录没有 HTML 格式内容")?;
                    contents.push(ClipboardContent::Html(html.to_owned()));
                } else {
                    contents.push(rtf_for_copy(item.rtf_content.as_deref())?);
                }
            }
            CopyMode::Paths => contents.push(ClipboardContent::Text(
                item_paths_for_action(&item, &self.staged_dir)?.join("\n"),
            )),
            CopyMode::Files => {
                if item.content_type != "image" {
                    bail!("该记录不是图片，无法作为文件复制");
                }
                contents.push(ClipboardContent::Files(item_paths_for_action(
                    &item,
                    &self.staged_dir,
                )?));
            }
            CopyMode::Original => match item.content_type.as_str() {
                "files" => {
                    let files = History::file_paths_for_copy(&item, &self.staged_dir)?;
                    if files.iter().any(|path| !Path::new(path).exists()) {
                        bail!("源文件或文件夹已不存在，无法复制");
                    }
                    contents.push(ClipboardContent::Files(files));
                }
                "image" => {
                    let path = item.image_path.as_deref().context("图片文件路径缺失")?;
                    let image = RustImageData::from_path(path)
                        .map_err(|error| anyhow!("打开图片失败：{error}"))?;
                    let png = image
                        .to_png()
                        .map_err(|error| anyhow!("图片 PNG 编码失败：{error}"))?
                        .get_bytes()
                        .to_vec();
                    let temp = ensure_temp_png(
                        &self.data_dir,
                        item.content_hash.trim_start_matches("image:"),
                        &png,
                    )?;
                    let path = temp.to_str().context("图片路径不是有效 UTF-8")?.to_owned();
                    contents.push(ClipboardContent::Files(vec![path]));
                    contents.push(ClipboardContent::Other(PNG_FORMAT.into(), png));
                }
                "html" | "rtf" => {
                    if let Some(text) = item.text_content.as_ref().filter(|text| !text.is_empty()) {
                        contents.push(ClipboardContent::Text(text.clone()));
                    } else if let Some(html) = &item.html_content {
                        contents.push(ClipboardContent::Text(
                            clipboard_core::html_text::html_to_text(html)?,
                        ));
                    }
                    if let Some(html) = item.html_content.as_ref().filter(|html| !html.is_empty()) {
                        contents.push(ClipboardContent::Html(html.clone()));
                        rich = true;
                    }
                    if let Some(rtf) = item
                        .rtf_content
                        .as_deref()
                        .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
                    {
                        contents.push(ClipboardContent::Other(RTF_FORMAT.into(), rtf));
                        rich = true;
                    }
                }
                _ => contents.push(ClipboardContent::Text(
                    item.text_content.clone().context("记录没有可复制的文本")?,
                )),
            },
        }
        if mode == CopyMode::Original
            && matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf")
        {
            contents.extend(
                self.history
                    .native_formats_for_copy(id)?
                    .into_iter()
                    .map(|(name, data)| ClipboardContent::Other(name, data)),
            );
        }
        let stash = (request_id.is_some() && mode == CopyMode::Original)
            .then(|| self.snapshot_clipboard_for_paste())
            .flatten();
        if contents.is_empty() {
            bail!("记录没有可写回的剪贴板内容");
        }
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_verified(&self.clipboard, contents, &mut state.ignored_sequence)?;
        drop(state);
        let message = match mode {
            CopyMode::Html => "HTML 已复制",
            CopyMode::Rtf => "RTF 已复制",
            CopyMode::Transform(_) => "转换后的文本已复制",
            _ if rich => "富文本已复制",
            _ => "内容已复制",
        };
        self.send_event(Event::Copied {
            id,
            request_id,
            clipboard_sequence,
            message: message.into(),
        })?;
        self.remember_paste_stash(request_id, clipboard_sequence, stash);
        Ok(())
    }

    pub(super) fn handle_restore_stash(
        &mut self,
        request_id: u64,
        clipboard_sequence: u32,
    ) -> Result<()> {
        let result = (|| {
            let Some(stash) = self.take_stash_for_restore(request_id, clipboard_sequence) else {
                return Ok(false);
            };
            let contents = restore_contents(&stash.content, &stash.extra)?;
            let mut state = lock_capture_state(&self.state)?;
            // Unix backends expose no compare-and-set primitive. Recheck after
            // preparation, immediately before transferring clipboard ownership.
            if !stash.is_current(current_sequence(&self.clipboard)?) {
                return Ok(false);
            }
            let written = self
                .clipboard
                .set(contents)
                .map_err(|error| anyhow!("恢复原剪贴板失败：{error}"));
            state.ignored_sequence = current_sequence(&self.clipboard).unwrap_or(0);
            written.map(|_| true)
        })();
        self.send_event(Event::ClipboardRestored(
            result.map_err(|error: anyhow::Error| error.to_string()),
        ))
    }
}

fn restore_contents(
    content: &CapturedClipboard,
    extra: &[clipboard_core::NativeFormatCapture],
) -> Result<Vec<ClipboardContent>> {
    let mut contents = match content {
        CapturedClipboard::Text(text) => vec![ClipboardContent::Text(text.clone())],
        CapturedClipboard::Rich { html, rtf, text } => {
            let mut contents = Vec::with_capacity(3);
            if let Some(text) = text {
                contents.push(ClipboardContent::Text(text.clone()));
            }
            if let Some(html) = html {
                contents.push(ClipboardContent::Html(html.clone()));
            }
            if let Some(rtf) = rtf {
                contents.push(ClipboardContent::Other(RTF_FORMAT.into(), rtf.clone()));
            }
            contents
        }
        CapturedClipboard::Files(paths) => vec![ClipboardContent::Files(paths.clone())],
        CapturedClipboard::Image { png, .. } => {
            let image =
                RustImageData::from_bytes(png).map_err(|error| anyhow!("恢复图片失败：{error}"))?;
            vec![ClipboardContent::Image(image)]
        }
    };
    contents.extend(
        extra
            .iter()
            .map(|format| ClipboardContent::Other(format.name.clone(), format.data.clone())),
    );
    #[cfg(target_os = "linux")]
    let contents = super::super::file_list::encode_contents(contents)?;
    Ok(contents)
}

// Keep only digests while the backend takes ownership of the payload; do not duplicate images.
enum Expected {
    Text(u64),
    Html(u64),
    Files(u64),
    Buffer(String, u64),
}
fn digest(value: impl Hash) -> u64 {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}

fn write_verified(
    clipboard: &ClipboardContext,
    contents: Vec<ClipboardContent>,
    ignored: &mut u32,
) -> Result<u32> {
    #[cfg(target_os = "linux")]
    let contents = super::super::file_list::encode_contents(contents)?;
    let expected = contents
        .iter()
        .map(|content| match content {
            ClipboardContent::Text(text) => Ok(Expected::Text(digest(text))),
            ClipboardContent::Html(html) => Ok(Expected::Html(digest(html))),
            ClipboardContent::Files(files) => Ok(Expected::Files(digest(files))),
            ClipboardContent::Other(name, bytes) => {
                Ok(Expected::Buffer(name.clone(), digest(bytes)))
            }
            _ => bail!("无效的剪贴板写回格式"),
        })
        .collect::<Result<Vec<_>>>()?;
    let written = clipboard
        .set(contents)
        .map_err(|error| anyhow!("写回剪贴板失败：{error}"));
    // Suppress partial writes as well as successful writes.
    *ignored = current_sequence(clipboard).unwrap_or(0);
    written?;
    for _ in 0..4 {
        let before = current_sequence(clipboard)?;
        for expected in &expected {
            let (actual, wanted) = match expected {
                Expected::Text(hash) => (clipboard.get_text().map(digest), *hash),
                Expected::Html(hash) => (clipboard.get_html().map(digest), *hash),
                Expected::Files(hash) => (clipboard.get_files().map(digest), *hash),
                Expected::Buffer(name, hash) => (clipboard.get_buffer(name).map(digest), *hash),
            };
            if actual.map_err(|error| anyhow!("读取写回内容失败：{error}"))? != wanted {
                bail!("剪贴板写回后内容校验失败，请重试");
            }
        }
        if before != 0 && current_sequence(clipboard)? == before {
            *ignored = before;
            return Ok(before);
        }
        std::thread::yield_now();
    }
    bail!("复制过程中剪贴板已被其他应用修改，请重试")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn restoring_files_encodes_reserved_path_characters_and_rejects_relative_paths() -> Result<()> {
        let path = "/tmp/中文 file#%?\n.txt";
        let contents = restore_contents(&CapturedClipboard::Files(vec![path.into()]), &[])?;
        let uri_bytes = contents
            .iter()
            .find_map(|content| match content {
                ClipboardContent::Other(name, bytes) if name == "text/uri-list" => Some(bytes),
                _ => None,
            })
            .context("missing URI list")?;
        let uri = url::Url::parse(std::str::from_utf8(uri_bytes)?)?;
        assert_eq!(uri.to_file_path().unwrap(), Path::new(path));
        assert!(uri.as_str().contains("%23%25%3F%0A"));
        assert!(
            !contents
                .iter()
                .any(|content| matches!(content, ClipboardContent::Files(_)))
        );
        assert!(
            restore_contents(&CapturedClipboard::Files(vec!["relative.txt".into()]), &[]).is_err()
        );
        Ok(())
    }
}
