use anyhow::{Context, Result, bail};
use clipboard_core::{History, MAX_IMAGE_BYTES, database::ClipboardItem};
use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};

/// QQ reads a dropped image after `DoDragDrop` returns. Keep that file alive.
const IMAGE_DROP_RETENTION: Duration = Duration::from_secs(30);

/// Image drags offer a temporary PNG file first, then a ready 24-bit CF_DIB and PNG bytes.
#[derive(Clone, Debug)]
pub struct PreparedExternalDrag {
    pub(super) formats: Arc<Vec<(u16, FormatData)>>,
    image_file: Option<Arc<ImageDropFile>>,
}

#[derive(Debug)]
pub(super) enum FormatData {
    Bytes(Vec<u8>),
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
        }
    }
}

/// A PNG written only for this drag. The stored history file is never offered.
#[derive(Debug)]
struct ImageDropFile {
    path: tempfile::TempPath,
}

impl ImageDropFile {
    fn create(png: &[u8]) -> Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("elegant-clipboard-drag-")
            .suffix(".png")
            .tempfile()
            .context("无法创建拖放图片临时文件")?;
        file.write_all(png).context("无法写入拖放图片临时文件")?;
        file.flush().context("无法保存拖放图片临时文件")?;
        Ok(Self {
            path: file.into_temp_path(),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// Dropped targets may open the path after OLE returns. Cancelled drags delete immediately.
pub(super) fn retain_dropped_image(payload: &PreparedExternalDrag) {
    let Some(file) = payload.image_file.clone() else {
        return;
    };
    let now = Instant::now();
    let mut retained = RETAINED_IMAGE_DROPS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    retained.retain(|entry| entry.expires_at > now);
    retained.push(RetainedImageDrop {
        expires_at: now + IMAGE_DROP_RETENTION,
        _file: file,
    });
}

struct RetainedImageDrop {
    expires_at: Instant,
    _file: Arc<ImageDropFile>,
}

static RETAINED_IMAGE_DROPS: Mutex<Vec<RetainedImageDrop>> = Mutex::new(Vec::new());

/// Decodes the stored PNG into CF_DIB before the drop target asks for it.
pub fn prepare_external_drag(item: &ClipboardItem) -> Result<PreparedExternalDrag> {
    let mut formats = Vec::new();
    let mut image_file = None;
    match item.content_type.as_str() {
        "files" => {
            let paths = History::file_paths_for_copy(item)?;
            formats.push((CF_HDROP.0, file_drop(&paths)?.into()));
        }
        "image" => {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES as u64 {
                bail!("拖放图片无效或超过 50 MiB");
            }
            let png = std::fs::read(path).with_context(|| format!("读取拖放图片失败：{path}"))?;
            let file = ImageDropFile::create(&png)?;
            let offered = file
                .path()
                .to_str()
                .context("拖放图片临时路径不是 Unicode")?
                .to_owned();
            image_file = Some(Arc::new(file));
            // QQ prefers a file it can open after the drop returns.
            formats.push((CF_HDROP.0, file_drop(&[offered])?.into()));
            formats.push((CF_DIB.0, crate::cf_dib::from_png(&png)?.into()));
            formats.push((registered_format("PNG")?, png.into()));
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
        image_file,
    })
}

#[cfg(test)]
impl PreparedExternalDrag {
    pub(super) fn formats_only(formats: Vec<(u16, FormatData)>) -> Self {
        Self {
            formats: Arc::new(formats),
            image_file: None,
        }
    }
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
