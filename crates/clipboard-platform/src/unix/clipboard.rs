use super::super::{
    capture::lock_capture_state,
    file_actions::{item_paths_for_action, plain_text_for_copy},
    read_content::RTF_FORMAT,
};
use super::{Event, Worker, image_tmp::ensure_temp_png};
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
}

impl Worker {
    pub(super) fn handle_merge(&mut self, ids: Vec<i64>, for_paste: bool) -> Result<()> {
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
        let item = self.history.item(id)?;
        let mut contents = Vec::new();
        let mut rich = false;
        match mode {
            CopyMode::PlainText => contents.push(ClipboardContent::Text(
                plain_text_for_copy(&item)?.into_owned(),
            )),
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
        if contents.is_empty() {
            bail!("记录没有可写回的剪贴板内容");
        }
        let mut state = lock_capture_state(&self.state)?;
        let clipboard_sequence =
            write_verified(&self.clipboard, contents, &mut state.ignored_sequence)?;
        drop(state);
        self.send_event(Event::Copied {
            id,
            request_id,
            clipboard_sequence,
            message: if rich {
                "富文本已复制"
            } else {
                "内容已复制"
            }
            .into(),
        })
    }
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
