use super::dib::DeferredDib;
use anyhow::{Context, Result, bail};
use clipboard_core::{History, MAX_IMAGE_BYTES, database::ClipboardItem};
use std::{path::Path, sync::Arc};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};

/// Transferable payload: cheap formats are ready; a bounded worker renders DIB.
#[derive(Clone, Debug)]
pub struct PreparedExternalDrag {
    pub(super) formats: Arc<Vec<(u16, FormatData)>>,
}

#[derive(Debug)]
pub(super) enum FormatData {
    Bytes(Vec<u8>),
    Dib(Arc<DeferredDib>),
}

impl From<Vec<u8>> for FormatData {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

impl FormatData {
    pub(super) fn with_bytes<T>(
        &self,
        consume: impl FnOnce(&[u8]) -> windows::core::Result<T>,
    ) -> windows::core::Result<T> {
        match self {
            Self::Bytes(bytes) => consume(bytes),
            Self::Dib(dib) => dib.with_bytes(consume),
        }
    }
}

/// Checks attachment paths, but never decodes images or writes the clipboard.
/// The bounded DIB renderer runs separately; a DIB consumer may wait for its result.
pub fn prepare_external_drag(item: &ClipboardItem) -> Result<PreparedExternalDrag> {
    let mut formats = Vec::new();
    match item.content_type.as_str() {
        "files" => {
            let paths = History::file_paths_for_copy(item)?;
            formats.push((CF_HDROP.0, file_drop(&paths)?.into()));
        }
        "image" => {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            formats.push((CF_HDROP.0, file_drop(&[path.to_owned()])?.into()));
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES as u64 {
                bail!("拖放图片无效或超过 50 MiB");
            }
            formats.push((
                CF_DIB.0,
                FormatData::Dib(DeferredDib::prepare(path.to_owned())),
            ));
        }
        "text" | "url" | "html" | "rtf" => {
            if let Some(text) = item.text_content.as_deref().filter(|text| !text.is_empty()) {
                formats.push((CF_UNICODETEXT.0, unicode_text(text)?.into()));
            }
            if let Some(html) = item.html_content.as_deref().filter(|html| !html.is_empty()) {
                formats.push((registered_format("HTML Format")?, html_format(html)?.into()));
            }
            if let Some(rtf) = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
            {
                formats.push((registered_format("Rich Text Format")?, rtf.into()));
            }
        }
        kind => bail!("记录类型 {kind} 不支持拖放"),
    }
    if formats.is_empty() {
        bail!("记录没有可拖放的内容");
    }
    Ok(PreparedExternalDrag {
        formats: Arc::new(formats),
    })
}

fn registered_format(name: &str) -> Result<u16> {
    Ok(clipboard_win::register_format(name)
        .context("注册拖放格式失败")?
        .get() as u16)
}

fn unicode_text(text: &str) -> Result<Vec<u8>> {
    if text.contains('\0') {
        bail!("拖放文本包含无效的空字符");
    }
    Ok(crate::instance::to_wide_bytes(text))
}

fn file_drop(paths: &[String]) -> Result<Vec<u8>> {
    if paths.is_empty() {
        bail!("拖放文件列表为空");
    }
    // DROPFILES: pFiles, POINT, fNC, fWide. Explicit serialization avoids padding.
    let mut bytes: Vec<u8> = [20u32, 0, 0, 0, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    for path in paths {
        if !Path::new(path).is_absolute() || !Path::new(path).exists() {
            bail!("拖放源文件或文件夹已不存在：{path}");
        }
        bytes.extend(unicode_text(path)?);
    }
    bytes.extend([0, 0]);
    Ok(bytes)
}

fn html_format(fragment: &str) -> Result<Vec<u8>> {
    if fragment.contains('\0') {
        bail!("HTML 拖放内容包含无效的空字符");
    }
    let prefix = "<html><body><!--StartFragment-->";
    let suffix = "<!--EndFragment--></body></html>";
    let header = |start, end, fragment_start, fragment_end| {
        format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{end:010}\r\nStartFragment:{fragment_start:010}\r\nEndFragment:{fragment_end:010}\r\n"
        )
    };
    let start = header(0, 0, 0, 0).len();
    let fragment_start = start + prefix.len();
    let fragment_end = fragment_start + fragment.len();
    let end = fragment_end + suffix.len();
    let mut bytes = header(start, end, fragment_start, fragment_end).into_bytes();
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(fragment.as_bytes());
    bytes.extend_from_slice(suffix.as_bytes());
    bytes.push(0);
    Ok(bytes)
}
